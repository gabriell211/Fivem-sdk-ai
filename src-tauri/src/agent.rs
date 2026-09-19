use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::AppHandle;
use url::Url;

use crate::{fxserver::{self, FxServerManager}, nui, workspace, AppError, AppResult};

const MAX_AGENT_STEPS: usize = 12;
const MAX_TOOL_TEXT: usize = 120_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    pub workspace: String,
    pub prompt: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

fn endpoint_url(base: &str) -> AppResult<Url> {
    let mut url = Url::parse(base.trim_end_matches('/'))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::InvalidInput("AI endpoint must use HTTP or HTTPS".into()));
    }
    if url.scheme() == "http" {
        let host = url.host_str().unwrap_or_default();
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1") {
            return Err(AppError::InvalidInput("plain HTTP AI endpoints are restricted to localhost".into()));
        }
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url.join("chat/completions")?)
}

fn trim_tool_output(value: String) -> String {
    if value.len() <= MAX_TOOL_TEXT {
        return value;
    }
    let mut end = MAX_TOOL_TEXT;
    while !value.is_char_boundary(end) { end -= 1; }
    format!("{}\n...[truncated by IDE]", &value[..end])
}

async fn execute_tool(app: &AppHandle, manager: &FxServerManager, workspace_path: &str, name: &str, args: Value) -> AppResult<String> {
    match name {
        "list_files" => {
            let files = workspace::list_files(workspace_path)?;
            Ok(trim_tool_output(serde_json::to_string(&files)?))
        }
        "read_file" => {
            let path = args.get("path").and_then(Value::as_str).ok_or_else(|| AppError::InvalidInput("read_file.path is required".into()))?;
            Ok(trim_tool_output(workspace::read_file(workspace_path, path)?))
        }
        "write_file" => {
            let path = args.get("path").and_then(Value::as_str).ok_or_else(|| AppError::InvalidInput("write_file.path is required".into()))?;
            let content = args.get("content").and_then(Value::as_str).ok_or_else(|| AppError::InvalidInput("write_file.content is required".into()))?;
            workspace::write_file(workspace_path, path, content)?;
            Ok(json!({"ok": true, "path": path}).to_string())
        }
        "refresh_resources" => {
            fxserver::send_command(manager, "refresh").await?;
            Ok(json!({"ok": true}).to_string())
        }
        "restart_resource" => {
            let resource = args.get("resource").and_then(Value::as_str).ok_or_else(|| AppError::InvalidInput("restart_resource.resource is required".into()))?;
            fxserver::send_command(manager, &format!("restart {resource}")).await?;
            Ok(json!({"ok": true, "resource": resource}).to_string())
        }
        "run_ingame_test" => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::InvalidInput("run_ingame_test.action is required".into()))?;
            let scenario_args = args
                .get("args")
                .cloned()
                .unwrap_or_else(|| json!({}));

            if action == "screenshot" {
                let evidence = fxserver::capture_screenshot(manager, workspace_path).await?;
                return Ok(json!({
                    "ok": true,
                    "action": "screenshot",
                    "path": evidence.path,
                    "size": evidence.size,
                    "sha256": evidence.sha256,
                    "mime": evidence.mime
                })
                .to_string());
            }

            Ok(fxserver::run_bridge_test(manager, action, scenario_args)
                .await?
                .to_string())
        }
        "server_status" => Ok(serde_json::to_string(&fxserver::status(app, Some(manager)).await?)?),
        "recent_logs" => {
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(120)
                .clamp(1, 500) as usize;
            Ok(trim_tool_output(
                serde_json::to_string(&manager.recent_logs(limit).await)?,
            ))
        },
        "nui_targets" => Ok(trim_tool_output(serde_json::to_string(&nui::targets().await?)?)),
        _ => Err(AppError::InvalidInput(format!("unknown AI tool: {name}"))),
    }
}

pub async fn run(app: AppHandle, manager: Arc<FxServerManager>, request: AgentRequest) -> AppResult<String> {
    let _ = workspace::root(&request.workspace)?;
    if request.prompt.trim().is_empty() || request.prompt.len() > 32_000 {
        return Err(AppError::InvalidInput("agent prompt is empty or too large".into()));
    }
    if request.model.trim().is_empty() || request.model.len() > 200 {
        return Err(AppError::InvalidInput("invalid model name".into()));
    }
    let url = endpoint_url(&request.endpoint)?;
    let client = Client::builder().user_agent("FiveM-SDK-AI/0.1").build()?;

    let tools = json!([
      {"type":"function","function":{"name":"list_files","description":"List files inside the current FiveM workspace.","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
      {"type":"function","function":{"name":"read_file","description":"Read a UTF-8 source file inside the workspace.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"write_file","description":"Create or replace a UTF-8 source file inside the workspace. Paths cannot escape the workspace.","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"refresh_resources","description":"Ask FXServer to rescan resource manifests.","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
      {"type":"function","function":{"name":"restart_resource","description":"Restart one FiveM resource after editing it.","parameters":{"type":"object","properties":{"resource":{"type":"string"}},"required":["resource"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"run_ingame_test","description":"Run a real test through the authenticated sdkai_bridge and connected FiveM client. Supports connectivity, player snapshots, teleport validation, vehicle spawning, cleanup, and screenshots. Use args for action-specific data.","parameters":{"type":"object","properties":{"action":{"type":"string","enum":["ping","snapshot","teleport","spawn_vehicle","cleanup","scenario","screenshot"]},"args":{"type":"object","description":"Action arguments. teleport: x,y,z,heading. spawn_vehicle: model,warp. scenario: steps array with safe actions snapshot, teleport, spawn_vehicle, wait, cleanup."}},"required":["action"],"additionalProperties":false}}},
      {"type":"function","function":{"name":"server_status","description":"Return the managed FXServer installation/running status.","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
      {"type":"function","function":{"name":"recent_logs","description":"Read the most recent bounded FXServer log lines for runtime diagnosis.","parameters":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500}},"additionalProperties":false}}},
      {"type":"function","function":{"name":"nui_targets","description":"List CEF/NUI pages currently exposed by the running FiveM client's remote DevTools endpoint.","parameters":{"type":"object","properties":{},"additionalProperties":false}}}
    ]);

    let system = r#"You are the coding and test engine inside FiveM SDK AI. Work only through the provided typed tools; there is no shell tool.
Rules:
- Treat the current directory as a FiveM/FXServer workspace.
- Prefer fxmanifest.lua with fx_version 'cerulean' and game 'gta5'. Lua 5.4 is the current runtime; do not add lua54 'yes' because that manifest setting is deprecated.
- Respect client/server runtime boundaries. Network only events that truly cross boundaries; never trust money, permissions, inventory, coordinates or authorization supplied by a client when the server can verify them.
- NUI callbacks must always return a response. Keep JSON payloads bounded and validate input.
- After source changes, refresh/restart the smallest affected resource.
- If the user asks to test, use run_ingame_test whenever a FiveM client is connected. Prefer a scenario that exercises the changed behavior, then capture screenshot evidence for visual changes. Do not claim an in-game test passed unless the tool returned ok=true.
- Use teleport/spawn_vehicle only for local deterministic test setup. Prefer scenario for reproducible multi-step tests and always include cleanup after entities created by a test.
- Use nui_targets to verify that NUI/CEF pages are actually loaded when working on UI resources.
- Do not modify files outside the workspace and do not invent successful runtime results.
Return a concise engineering summary including files changed, tests actually executed, failures still present, and next concrete step."#;

    let mut messages = vec![json!({"role":"system","content":system}), json!({"role":"user","content":request.prompt})];

    for _ in 0..MAX_AGENT_STEPS {
        let body = json!({
            "model": request.model.clone(),
            "messages": messages.clone(),
            "tools": tools.clone(),
            "tool_choice": "auto",
            "temperature": 0.1
        });
        let mut http = client.post(url.clone()).json(&body);
        if let Some(key) = request.api_key.as_deref().filter(|key| !key.is_empty()) {
            http = http.bearer_auth(key);
        }
        let response: Value = http.send().await?.error_for_status()?.json().await?;
        let message = response.pointer("/choices/0/message").cloned().ok_or_else(|| AppError::Process("AI provider returned no choices[0].message".into()))?;
        let tool_calls = message.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default();
        messages.push(message.clone());

        if tool_calls.is_empty() {
            return Ok(message.get("content").and_then(Value::as_str).unwrap_or("Agent finished without textual output.").to_owned());
        }

        for call in tool_calls {
            let id = call.get("id").and_then(Value::as_str).ok_or_else(|| AppError::Process("AI tool call had no id".into()))?;
            let function = call.get("function").ok_or_else(|| AppError::Process("AI tool call had no function".into()))?;
            let name = function.get("name").and_then(Value::as_str).ok_or_else(|| AppError::Process("AI tool call had no name".into()))?;
            let raw_args = function.get("arguments").and_then(Value::as_str).unwrap_or("{}");
            let args = serde_json::from_str::<Value>(raw_args).map_err(|error| AppError::InvalidInput(format!("invalid tool arguments for {name}: {error}")))?;
            let tool_result = execute_tool(&app, &manager, &request.workspace, name, args).await;
            let content = match tool_result {
                Ok(value) => value,
                Err(error) => json!({"ok": false, "error": error.to_string()}).to_string(),
            };
            messages.push(json!({"role":"tool","tool_call_id":id,"content":content}));
        }
    }

    Err(AppError::Process(format!("agent exceeded {MAX_AGENT_STEPS} tool-call rounds")))
}
