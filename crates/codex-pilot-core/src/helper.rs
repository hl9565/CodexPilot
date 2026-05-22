use anyhow::Context;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct HelperRuntime {
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl HelperRuntime {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

pub async fn start_helper(port: u16) -> anyhow::Result<HelperRuntime> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("failed to bind helper runtime on 127.0.0.1:{port}"))?;
    let (shutdown, mut shutdown_rx) = tokio::sync::oneshot::channel();

    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    if let Ok((stream, _)) = accepted {
                        tokio::spawn(async move {
                            let _ = handle_connection(stream).await;
                        });
                    }
                }
            }
        }
    });

    Ok(HelperRuntime { shutdown, task })
}

async fn handle_connection(mut stream: tokio::net::TcpStream) -> anyhow::Result<()> {
    let mut buffer = vec![0_u8; 65536];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let body_start = request
        .find("\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| request.find("\n\n").map(|index| index + 2))
        .unwrap_or(read);
    let request_body = &request[body_start..];

    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
        stream.write_all(response.as_bytes()).await?;
        stream.shutdown().await?;
        return Ok(());
    }

    let (status, content_type, body) =
        if path == "/backend/status" && matches!(method, "GET" | "POST") {
        (
            "200 OK".to_string(),
            "application/json; charset=utf-8".to_string(),
            serde_json::to_vec(&json!({
                "status": "ok",
                "message": "CodexPilot 后端已连接",
                "version": crate::version::VERSION,
                "transport": "http-helper"
            }))?,
        )
    } else if crate::protocol_proxy::is_responses_proxy_path(path) && method == "POST" {
        let target = match load_active_proxy_target() {
            Ok(Some(target)) => target,
            Ok(None) => {
                write_json_response(
                    &mut stream,
                    "502 Bad Gateway",
                    &json!({
                        "status": "failed",
                        "message": "当前激活配置未启用本地协议代理。"
                    }),
                )
                .await?;
                stream.shutdown().await?;
                return Ok(());
            }
            Err(error) => {
                write_json_response(
                    &mut stream,
                    "502 Bad Gateway",
                    &json!({
                        "status": "failed",
                        "message": error.to_string()
                    }),
                )
                .await?;
                stream.shutdown().await?;
                return Ok(());
            }
        };
        if target.protocol == crate::protocol_proxy::UpstreamProtocol::ChatCompletions
            && crate::protocol_proxy::responses_request_wants_stream(request_body)
        {
            if let Err(error) = crate::protocol_proxy::stream_chat_completions_as_responses(
                &mut stream,
                &target,
                request_body,
            )
            .await
            {
                write_json_response(
                    &mut stream,
                    "502 Bad Gateway",
                    &json!({
                        "status": "failed",
                        "message": error.to_string()
                    }),
                )
                .await?;
            }
            return Ok(());
        }
        match crate::protocol_proxy::handle_responses_proxy_request(&target, request_body).await {
            Ok(result) => (result.status, result.content_type, result.body),
            Err(error) => (
                "502 Bad Gateway".to_string(),
                "application/json; charset=utf-8".to_string(),
                serde_json::to_vec(&json!({
                    "status": "failed",
                    "message": error.to_string()
                }))?,
            ),
        }
    } else if crate::protocol_proxy::is_models_proxy_path(path) && method == "GET" {
        let target = match load_active_proxy_target() {
            Ok(Some(target)) => target,
            Ok(None) => {
                write_json_response(
                    &mut stream,
                    "502 Bad Gateway",
                    &json!({
                        "status": "failed",
                        "message": "当前激活配置未启用本地协议代理。"
                    }),
                )
                .await?;
                stream.shutdown().await?;
                return Ok(());
            }
            Err(error) => {
                write_json_response(
                    &mut stream,
                    "502 Bad Gateway",
                    &json!({
                        "status": "failed",
                        "message": error.to_string()
                    }),
                )
                .await?;
                stream.shutdown().await?;
                return Ok(());
            }
        };
        match crate::protocol_proxy::handle_models_proxy_request(&target).await {
            Ok(result) => (result.status, result.content_type, result.body),
            Err(error) => (
                "502 Bad Gateway".to_string(),
                "application/json; charset=utf-8".to_string(),
                serde_json::to_vec(&json!({
                    "status": "failed",
                    "message": error.to_string()
                }))?,
            ),
        }
    } else {
        (
            "404 Not Found".to_string(),
            "application/json; charset=utf-8".to_string(),
            serde_json::to_vec(&json!({
                "status": "failed",
                "message": "未知后端路径"
            }))?,
        )
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(response.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn write_json_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    body: &serde_json::Value,
) -> anyhow::Result<()> {
    let body = serde_json::to_vec(body)?;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(&body).await?;
    Ok(())
}

fn load_active_proxy_target() -> anyhow::Result<Option<crate::protocol_proxy::ActiveProxyTarget>> {
    let path = crate::app_paths::app_state_dir().join("provider-profiles.json");
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path)?;
    let value = serde_json::from_str::<serde_json::Value>(&contents)?;
    let active_profile_id = value
        .get("activeProfileId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let profiles = value
        .get("profiles")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let profile = profiles
        .iter()
        .find(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some(active_profile_id))
        .or_else(|| profiles.first());
    let Some(profile) = profile else {
        return Ok(None);
    };
    let base_url = profile
        .get("baseUrl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let api_key = profile
        .get("bearerToken")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let protocol = match profile
        .get("upstreamProtocol")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("responses")
    {
        "chatCompletions" => crate::protocol_proxy::UpstreamProtocol::ChatCompletions,
        "anthropicMessages" => crate::protocol_proxy::UpstreamProtocol::AnthropicMessages,
        _ => crate::protocol_proxy::UpstreamProtocol::Responses,
    };
    if crate::protocol_proxy::route_mode_for_protocol(protocol)
        == crate::protocol_proxy::RouteMode::Direct
    {
        return Ok(None);
    }
    if base_url.is_empty() {
        return Ok(None);
    }
    Ok(Some(crate::protocol_proxy::ActiveProxyTarget {
        base_url,
        api_key,
        protocol,
    }))
}
