use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const CDP_HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CDP_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CdpTarget {
    pub id: String,
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "webSocketDebuggerUrl")]
    pub web_socket_debugger_url: Option<String>,
}

pub async fn list_targets(debug_port: u16) -> anyhow::Result<Vec<CdpTarget>> {
    let url = format!("http://127.0.0.1:{debug_port}/json");
    let _ = crate::diagnostic_log::append(
        "cdp.list_targets",
        serde_json::json!({ "debug_port": debug_port }),
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(CDP_HTTP_TIMEOUT)
        .build()
        .context("failed to build CDP HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to query CDP targets")?
        .error_for_status()
        .context("CDP target query failed")?;

    response
        .json::<Vec<CdpTarget>>()
        .await
        .context("failed to deserialize CDP targets")
}

pub fn pick_page_target(targets: &[CdpTarget]) -> anyhow::Result<CdpTarget> {
    let pages = targets.iter().filter(|target| {
        target.target_type == "page"
            && target
                .web_socket_debugger_url
                .as_deref()
                .is_some_and(|url| !url.is_empty())
    });

    let mut first_page = None;
    for target in pages {
        first_page.get_or_insert(target);
        let haystack = format!("{} {}", target.title, target.url).to_lowercase();
        if haystack.contains("codex") {
            return Ok(target.clone());
        }
    }

    if let Some(target) = first_page {
        return Ok(target.clone());
    }

    bail!("No injectable Codex page target found")
}

pub async fn evaluate_script(websocket_url: &str, script: &str) -> anyhow::Result<Value> {
    let (mut socket, _) = tokio::time::timeout(
        CDP_CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async(websocket_url),
    )
    .await
    .with_context(|| {
        format!(
            "timed out connecting CDP websocket after {}s",
            CDP_CONNECT_TIMEOUT.as_secs()
        )
    })?
    .context("failed to connect CDP websocket")?;

    let id = 1_u64;
    socket
        .send(Message::Text(
            json!({
                "id": id,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": script,
                    "awaitPromise": false,
                    "allowUnsafeEvalBlockedByCSP": true
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .context("failed to send CDP Runtime.evaluate command")?;

    loop {
        let message = tokio::time::timeout(CDP_COMMAND_TIMEOUT, socket.next())
            .await
            .with_context(|| {
                format!(
                    "timed out waiting for CDP Runtime.evaluate response after {}s",
                    CDP_COMMAND_TIMEOUT.as_secs()
                )
            })?
            .ok_or_else(|| anyhow::anyhow!("CDP websocket closed before response"))?
            .context("failed to read CDP websocket message")?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).context("failed to parse CDP message")?;
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(error) = value.get("error") {
                bail!("CDP Runtime.evaluate failed: {error}");
            }
            return Ok(value);
        }
    }
}

pub async fn inject_script(debug_port: u16, script: &str) -> anyhow::Result<()> {
    let targets = list_targets(debug_port).await?;
    let target = pick_page_target(&targets)?;
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("selected CDP target has no websocket URL"))?;
    evaluate_script(websocket_url, script).await?;
    Ok(())
}

pub async fn selected_page_websocket_url(debug_port: u16) -> anyhow::Result<String> {
    let targets = list_targets(debug_port).await?;
    let target = pick_page_target(&targets)?;
    let _ = crate::diagnostic_log::append(
        "cdp.selected_target",
        serde_json::json!({
            "id": target.id,
            "title": target.title,
            "url": target.url
        }),
    );
    target
        .web_socket_debugger_url
        .ok_or_else(|| anyhow::anyhow!("selected CDP target has no websocket URL"))
}
