use rand::RngCore;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::Mutex,
    time::{sleep, Duration, Instant},
};
use url::Url;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::{bridge, workspace, AppError, AppResult};

const ARTIFACT_INDEX: &str = "https://runtime.fivem.net/artifacts/fivem/build_server_windows/master/";
const SERVER_DATA_ZIP: &str = "https://github.com/citizenfx/cfx-server-data/archive/refs/heads/master.zip";
const SCREENSHOT_BASIC_ZIP: &str =
    "https://github.com/citizenfx/screenshot-basic/archive/refs/heads/master.zip";
const MAX_LOG_LINES: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeMetadata {
    build: String,
    executable: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FxServerStatus {
    pub installed: bool,
    pub running: bool,
    pub build: Option<String>,
    pub executable: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerLogEvent {
    pub stream: String,
    pub line: String,
}

struct RunningServer {
    child: Child,
    stdin: ChildStdin,
}

#[derive(Debug, Clone)]
struct StoredLog {
    seq: u64,
    line: String,
}

pub struct FxServerManager {
    process: Mutex<Option<RunningServer>>,
    logs: Mutex<VecDeque<StoredLog>>,
    sequence: AtomicU64,
    bridge_token: Mutex<Option<String>>,
}

impl Default for FxServerManager {
    fn default() -> Self {
        Self {
            process: Mutex::new(None),
            logs: Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)),
            sequence: AtomicU64::new(0),
            bridge_token: Mutex::new(None),
        }
    }
}

impl FxServerManager {
    async fn push_log(&self, app: &AppHandle, stream: &str, line: String) {
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let mut logs = self.logs.lock().await;
        if logs.len() >= MAX_LOG_LINES {
            logs.pop_front();
        }
        logs.push_back(StoredLog { seq, line: line.clone() });
        drop(logs);
        let _ = app.emit("fxserver://log", ServerLogEvent { stream: stream.to_owned(), line });
    }

    pub async fn latest_seq(&self) -> u64 {
        self.sequence.load(Ordering::Relaxed)
    }

    async fn lines_after(&self, seq: u64) -> Vec<StoredLog> {
        self.logs.lock().await.iter().filter(|line| line.seq > seq).cloned().collect()
    }
}

fn metadata_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_local_data_dir()?.join("fxserver").join("current.json"))
}

fn read_metadata(app: &AppHandle) -> AppResult<Option<RuntimeMetadata>> {
    let path = metadata_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let metadata: RuntimeMetadata = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(Some(metadata))
}

fn write_metadata(app: &AppHandle, metadata: &RuntimeMetadata) -> AppResult<()> {
    let path = metadata_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(metadata)?)?;
    Ok(())
}

fn safe_extract_zip(bytes: &[u8], destination: &Path) -> AppResult<()> {
    fs::create_dir_all(destination)?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(relative) = file.enclosed_name().map(Path::to_path_buf) else {
            continue;
        };
        let output = destination.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&output)?;
        std::io::copy(&mut file, &mut out)?;
    }
    Ok(())
}

fn find_server_executable(root: &Path) -> Option<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|entry| {
            if !entry.file_type().is_file() {
                return None;
            }
            let name = entry.file_name().to_string_lossy();
            if name.eq_ignore_ascii_case("FXServer.exe") || name.eq_ignore_ascii_case("cfx-server.exe") {
                Some(entry.path().to_path_buf())
            } else {
                None
            }
        })
}

async fn download(client: &reqwest::Client, url: &Url) -> AppResult<Vec<u8>> {
    if url.scheme() != "https" {
        return Err(AppError::InvalidInput("downloads must use HTTPS".into()));
    }
    let response = client.get(url.clone()).send().await?.error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

pub async fn install(app: &AppHandle) -> AppResult<FxServerStatus> {
    if !cfg!(target_os = "windows") {
        return Err(AppError::InvalidInput("the integrated FiveM/FXServer runtime currently targets Windows".into()));
    }

    let client = reqwest::Client::builder().user_agent("FiveM-SDK-AI/0.1").build()?;
    let html = client.get(ARTIFACT_INDEX).send().await?.error_for_status()?.text().await?;
    let recommended = Regex::new(r#"href="([^"]+)"[^>]*>\s*LATEST RECOMMENDED\s*\((\d+)\)"#)
        .map_err(|error| AppError::Process(error.to_string()))?;
    let captures = recommended.captures(&html).ok_or_else(|| AppError::Process("could not resolve LATEST RECOMMENDED FXServer build".into()))?;
    let href = captures.get(1).ok_or_else(|| AppError::Process("artifact link missing".into()))?.as_str();
    let build = captures.get(2).ok_or_else(|| AppError::Process("artifact build missing".into()))?.as_str().to_owned();

    let base = Url::parse(ARTIFACT_INDEX)?;
    let artifact_url = base.join(href)?;
    if artifact_url.host_str() != Some("runtime.fivem.net") || !artifact_url.path().ends_with("server.7z") {
        return Err(AppError::InvalidInput("unexpected FXServer artifact URL".into()));
    }

    let bytes = download(&client, &artifact_url).await?;
    let runtime_root = app.path().app_local_data_dir()?.join("fxserver").join("artifacts").join(&build);
    let staging = runtime_root.with_extension("staging");
    let archive_path = runtime_root.with_extension("download.7z");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&archive_path, &bytes)?;
    fs::create_dir_all(&staging)?;
    sevenz_rust::decompress_file(&archive_path, &staging)
        .map_err(|error| AppError::Process(format!("failed to extract FXServer artifact: {error}")))?;
    let _ = fs::remove_file(&archive_path);
    let executable = find_server_executable(&staging).ok_or_else(|| AppError::Process("FXServer executable not found in artifact".into()))?;

    if runtime_root.exists() {
        fs::remove_dir_all(&runtime_root)?;
    }
    fs::rename(&staging, &runtime_root)?;
    let relative_exe = executable.strip_prefix(&staging).map_err(|_| AppError::Process("invalid executable path".into()))?;
    let installed_exe = runtime_root.join(relative_exe);
    write_metadata(app, &RuntimeMetadata { build: build.clone(), executable: installed_exe.to_string_lossy().to_string() })?;
    status(app, None).await
}

fn copy_server_data(bytes: &[u8], workspace_root: &Path) -> AppResult<()> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(path) = file.enclosed_name().map(Path::to_path_buf) else { continue; };
        let mut components = path.components();
        let _archive_root = components.next();
        let Some(first) = components.next() else { continue; };
        if first.as_os_str() != "resources" { continue; }
        let relative: PathBuf = components.collect();
        let output = workspace_root.join("resources").join(relative);
        if file.is_dir() {
            fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() { fs::create_dir_all(parent)?; }
            let mut out = fs::File::create(output)?;
            std::io::copy(&mut file, &mut out)?;
        }
    }
    Ok(())
}


fn copy_repository_root(bytes: &[u8], destination: &Path) -> AppResult<()> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    fs::create_dir_all(destination)?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(path) = file.enclosed_name().map(Path::to_path_buf) else {
            continue;
        };
        let mut components = path.components();
        let _archive_root = components.next();
        let relative: PathBuf = components.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = destination.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(output)?;
            std::io::copy(&mut file, &mut out)?;
        }
    }
    Ok(())
}

pub async fn bootstrap_workspace(workspace_path: &str) -> AppResult<()> {
    let root = workspace::root(workspace_path)?;
    let client = reqwest::Client::builder().user_agent("FiveM-SDK-AI/0.1").build()?;
    let url = Url::parse(SERVER_DATA_ZIP)?;
    let bytes = download(&client, &url).await?;
    copy_server_data(&bytes, &root)?;

    let screenshot_url = Url::parse(SCREENSHOT_BASIC_ZIP)?;
    let screenshot_bytes = download(&client, &screenshot_url).await?;
    let screenshot_resource = root
        .join("resources")
        .join("[sdk-ai]")
        .join("screenshot-basic");
    copy_repository_root(&screenshot_bytes, &screenshot_resource)?;

    bridge::install(workspace_path)?;

    let server_cfg = root.join("server.cfg");
    if !server_cfg.exists() {
        fs::write(server_cfg, r#"endpoint_add_tcp "127.0.0.1:30120"
endpoint_add_udp "127.0.0.1:30120"

ensure mapmanager
ensure chat
ensure spawnmanager
ensure sessionmanager
ensure basic-gamemode
ensure hardcap
ensure rconlog
ensure screenshot-basic
ensure sdkai_bridge

sv_scriptHookAllowed 0
sets locale "pt-BR"
sv_hostname "FiveM SDK AI Dev Server"
sets sv_projectName "FiveM SDK AI Workspace"
sets sv_projectDesc "Local development server managed by FiveM SDK AI"
sv_master1 ""
set onesync on
sv_maxclients 4
"#)?;
    }
    Ok(())
}

fn runtime_cfg(workspace_root: &Path, license_key: &str, bridge_token: &str) -> AppResult<PathBuf> {
    if license_key.is_empty() || license_key.len() > 512 || license_key.chars().any(|ch| matches!(ch, '\n' | '\r' | '"')) {
        return Err(AppError::InvalidInput("invalid Cfx.re license key".into()));
    }
    let dir = workspace_root.join(".sdkai");
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join("screenshots"))?;
    let cfg = dir.join("runtime.cfg");
    fs::write(
        &cfg,
        format!(
            "exec server.cfg\nsv_master1 \"\"\nset onesync on\nsv_maxclients 4\nsv_licenseKey \"{license_key}\"\nset sdkai_token \"{bridge_token}\"\nensure screenshot-basic\nensure sdkai_bridge\n"
        ),
    )?;
    Ok(cfg)
}

pub async fn start(app: AppHandle, manager: Arc<FxServerManager>, workspace_path: &str, license_key: &str) -> AppResult<()> {
    let root = workspace::root(workspace_path)?;
    bridge::install(workspace_path)?;
    let metadata = read_metadata(&app)?.ok_or_else(|| AppError::Process("FXServer is not installed".into()))?;
    let executable = PathBuf::from(metadata.executable);
    if !executable.is_file() {
        return Err(AppError::Process("installed FXServer executable no longer exists".into()));
    }
    if !root.join("server.cfg").is_file() {
        return Err(AppError::InvalidInput("server.cfg not found; use Prepare first".into()));
    }
    let mut token_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut token_bytes);
    let bridge_token = hex::encode(token_bytes);
    let cfg = runtime_cfg(&root, license_key, &bridge_token)?;
    let cfg_relative = cfg.strip_prefix(&root).map_err(|_| AppError::Process("runtime cfg escaped workspace".into()))?;

    let mut process_lock = manager.process.lock().await;
    if let Some(running) = process_lock.as_mut() {
        if running.child.try_wait()?.is_none() {
            return Err(AppError::Process("FXServer is already running".into()));
        }
        *process_lock = None;
    }

    let mut command = Command::new(&executable);
    command.current_dir(&root).arg("+exec").arg(cfg_relative).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| AppError::Process("could not capture FXServer stdin".into()))?;
    let stdout = child.stdout.take().ok_or_else(|| AppError::Process("could not capture FXServer stdout".into()))?;
    let stderr = child.stderr.take().ok_or_else(|| AppError::Process("could not capture FXServer stderr".into()))?;

    *process_lock = Some(RunningServer { child, stdin });
    drop(process_lock);
    *manager.bridge_token.lock().await = Some(bridge_token);

    {
        let app = app.clone();
        let manager = manager.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                manager.push_log(&app, "stdout", line).await;
            }
        });
    }

    {
        let app = app.clone();
        let manager = manager.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                manager.push_log(&app, "stderr", line).await;
            }
        });
    }
    manager.push_log(&app, "system", format!("FXServer {} started", metadata.build)).await;
    Ok(())
}

pub async fn stop(app: &AppHandle, manager: &FxServerManager) -> AppResult<()> {
    let mut process_lock = manager.process.lock().await;
    if let Some(mut running) = process_lock.take() {
        running.child.kill().await?;
        let _ = running.child.wait().await;
        manager.push_log(app, "system", "FXServer stopped".into()).await;
    }
    *manager.bridge_token.lock().await = None;
    Ok(())
}

pub fn validate_command(command: &str) -> AppResult<()> {
    if command.len() > 256 || command.chars().any(|ch| matches!(ch, '\n' | '\r' | ';' | '&' | '|')) {
        return Err(AppError::InvalidInput("unsafe server command".into()));
    }
    let parts: Vec<&str> = command.split_whitespace().collect();
    let Some(name) = parts.first().copied() else { return Err(AppError::InvalidInput("empty server command".into())); };
    let resource_name = |value: &str| value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '[' | ']' | '.'));
    let allowed = match name {
        "refresh" | "status" | "sdkai_ping" | "sdkai_snapshot" => parts.len() == 1,
        "ensure" | "restart" | "start" | "stop" => parts.len() == 2 && resource_name(parts[1]),
        _ => false,
    };
    if !allowed {
        return Err(AppError::InvalidInput(format!("server command '{name}' is not in the IDE allowlist")));
    }
    Ok(())
}

pub async fn send_command(manager: &FxServerManager, command: &str) -> AppResult<()> {
    validate_command(command)?;
    let mut process_lock = manager.process.lock().await;
    let running = process_lock.as_mut().ok_or_else(|| AppError::Process("FXServer is not running".into()))?;
    running.stdin.write_all(command.as_bytes()).await?;
    running.stdin.write_all(b"\n").await?;
    running.stdin.flush().await?;
    Ok(())
}

pub async fn run_ingame_test(manager: &FxServerManager, action: &str) -> AppResult<Value> {
    let command = match action {
        "ping" => "sdkai_ping",
        "snapshot" => "sdkai_snapshot",
        _ => return Err(AppError::InvalidInput("unsupported in-game test".into())),
    };
    let start_seq = manager.latest_seq().await;
    send_command(manager, command).await?;
    let deadline = Instant::now() + Duration::from_secs(18);
    while Instant::now() < deadline {
        for line in manager.lines_after(start_seq).await {
            let Some(raw) = line.line.strip_prefix("[SDKAI_EVENT]") else { continue; };
            let Ok(event) = serde_json::from_str::<Value>(raw) else { continue; };
            if event.get("type").and_then(Value::as_str) == Some("test_result") && event.get("action").and_then(Value::as_str) == Some(action) {
                return Ok(event);
            }
        }
        sleep(Duration::from_millis(120)).await;
    }
    Err(AppError::Process(format!("in-game test '{action}' timed out")))
}

pub async fn run_bridge_test(
    manager: &FxServerManager,
    action: &str,
    args: Value,
) -> AppResult<Value> {
    if !matches!(
        action,
        "ping" | "snapshot" | "teleport" | "spawn_vehicle" | "cleanup" | "nui_state" | "screenshot"
    ) {
        return Err(AppError::InvalidInput(format!(
            "unsupported bridge action: {action}"
        )));
    }

    let encoded_args = serde_json::to_vec(&args)?;
    if encoded_args.len() > 16 * 1024 {
        return Err(AppError::InvalidInput(
            "bridge test arguments exceed 16 KiB".into(),
        ));
    }

    let token = manager
        .bridge_token
        .lock()
        .await
        .clone()
        .ok_or_else(|| AppError::Process("FXServer bridge token is unavailable".into()))?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(25))
        .build()?;

    let response = client
        .post("http://127.0.0.1:30120/sdkai_bridge/test")
        .header("x-sdkai-token", token)
        .json(&serde_json::json!({ "action": action, "args": args }))
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json().await?)
}

pub async fn status(app: &AppHandle, manager: Option<&FxServerManager>) -> AppResult<FxServerStatus> {
    let metadata = read_metadata(app)?;
    let mut running = false;
    if let Some(manager) = manager {
        let mut process_lock = manager.process.lock().await;
        if let Some(process) = process_lock.as_mut() {
            running = process.child.try_wait()?.is_none();
            if !running { *process_lock = None; }
        }
    }
    Ok(FxServerStatus {
        installed: metadata.as_ref().is_some_and(|item| Path::new(&item.executable).is_file()),
        running,
        build: metadata.as_ref().map(|item| item.build.clone()),
        executable: metadata.map(|item| item.executable),
    })
}

pub fn connect() -> AppResult<()> {
    open::that("fivem://connect/127.0.0.1:30120").map_err(|error| AppError::Process(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_allowlist_blocks_shell_and_privileged_commands() {
        assert!(validate_command("restart my_resource").is_ok());
        assert!(validate_command("sdkai_snapshot").is_ok());
        assert!(validate_command("quit").is_err());
        assert!(validate_command("exec secrets.cfg").is_err());
        assert!(validate_command("restart x; quit").is_err());
    }
}
