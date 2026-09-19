use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{AppError, AppResult};

const NUI_DEVTOOLS_LIST: &str = "http://127.0.0.1:13172/json/list";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NuiTarget {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub target_type: String,
    #[serde(default)]
    pub web_socket_debugger_url: String,
}

pub async fn targets() -> AppResult<Vec<NuiTarget>> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(4))
        .build()?;

    let response = client
        .get(NUI_DEVTOOLS_LIST)
        .send()
        .await
        .map_err(|error| {
            AppError::Process(format!(
                "FiveM NUI DevTools is not reachable on 127.0.0.1:13172: {error}"
            ))
        })?
        .error_for_status()?;

    let value: serde_json::Value = response.json().await?;
    let items = value.as_array().ok_or_else(|| {
        AppError::Process("FiveM NUI DevTools returned a non-array target list".into())
    })?;

    let mut targets = Vec::with_capacity(items.len());
    for item in items {
        targets.push(NuiTarget {
            id: item
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            title: item
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            url: item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            target_type: item
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            web_socket_debugger_url: item
                .get("webSocketDebuggerUrl")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
    }

    targets.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(targets)
}

pub fn open_devtools() -> AppResult<()> {
    open::that("http://127.0.0.1:13172/")
        .map_err(|error| AppError::Process(error.to_string()))?;
    Ok(())
}
