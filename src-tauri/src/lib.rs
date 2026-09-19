mod agent;
mod bridge;
mod fxserver;
mod nui;
mod workspace;

use std::sync::Arc;
use tauri::{AppHandle, State};
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("process error: {0}")]
    Process(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

struct AppState {
    fxserver: Arc<fxserver::FxServerManager>,
}

fn as_command<T>(result: AppResult<T>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}

#[tauri::command]
fn list_workspace_files(workspace: String) -> Result<Vec<workspace::WorkspaceFile>, String> {
    as_command(workspace::list_files(&workspace))
}

#[tauri::command]
fn read_workspace_file(workspace: String, path: String) -> Result<String, String> {
    as_command(workspace::read_file(&workspace, &path))
}

#[tauri::command]
fn write_workspace_file(workspace: String, path: String, content: String) -> Result<(), String> {
    as_command(workspace::write_file(&workspace, &path, &content))
}

#[tauri::command]
async fn bootstrap_workspace(workspace: String) -> Result<(), String> {
    as_command(fxserver::bootstrap_workspace(&workspace).await)
}

#[tauri::command]
async fn install_fxserver(app: AppHandle) -> Result<fxserver::FxServerStatus, String> {
    as_command(fxserver::install(&app).await)
}

#[tauri::command]
async fn fxserver_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<fxserver::FxServerStatus, String> {
    as_command(fxserver::status(&app, Some(&state.fxserver)).await)
}

#[tauri::command]
async fn start_fxserver(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace: String,
    license_key: String,
) -> Result<(), String> {
    as_command(fxserver::start(
        app,
        state.fxserver.clone(),
        &workspace,
        &license_key,
    )
    .await)
}

#[tauri::command]
async fn stop_fxserver(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    as_command(fxserver::stop(&app, &state.fxserver).await)
}

#[tauri::command]
async fn fxserver_command(state: State<'_, AppState>, command: String) -> Result<(), String> {
    as_command(fxserver::send_command(&state.fxserver, &command).await)
}

#[tauri::command]
fn connect_fivem() -> Result<(), String> {
    as_command(fxserver::connect())
}

#[tauri::command]
async fn run_bridge_test(
    state: State<'_, AppState>,
    action: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    as_command(fxserver::run_bridge_test(&state.fxserver, &action, args).await)
}

#[tauri::command]
async fn capture_screenshot(
    state: State<'_, AppState>,
    workspace: String,
) -> Result<fxserver::ScreenshotEvidence, String> {
    as_command(fxserver::capture_screenshot(&state.fxserver, &workspace).await)
}


#[tauri::command]
async fn nui_targets() -> Result<Vec<nui::NuiTarget>, String> {
    as_command(nui::targets().await)
}

#[tauri::command]
fn open_nui_devtools() -> Result<(), String> {
    as_command(nui::open_devtools())
}

#[tauri::command]
async fn run_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    request: agent::AgentRequest,
) -> Result<String, String> {
    as_command(agent::run(app, state.fxserver.clone(), request).await)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            fxserver: Arc::new(fxserver::FxServerManager::default()),
        })
        .invoke_handler(tauri::generate_handler![
            list_workspace_files,
            read_workspace_file,
            write_workspace_file,
            bootstrap_workspace,
            install_fxserver,
            fxserver_status,
            start_fxserver,
            stop_fxserver,
            fxserver_command,
            connect_fivem,
            run_bridge_test,
            capture_screenshot,
            nui_targets,
            open_nui_devtools,
            run_agent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FiveM SDK AI");
}
