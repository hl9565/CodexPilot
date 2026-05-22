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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperProxyRoute {
    Responses { stream: bool },
    Models,
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
    } else if let Some(route) = helper_proxy_route(method, path, request_body) {
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
        if matches!(route, HelperProxyRoute::Responses { stream: true })
            && target.protocol == crate::protocol_proxy::UpstreamProtocol::ChatCompletions
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
        if matches!(route, HelperProxyRoute::Responses { stream: true })
            && target.protocol == crate::protocol_proxy::UpstreamProtocol::AnthropicMessages
        {
            if let Err(error) = crate::protocol_proxy::stream_anthropic_messages_as_responses(
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
        match route {
            HelperProxyRoute::Responses { .. } => {
                match crate::protocol_proxy::handle_responses_proxy_request(&target, request_body)
                    .await
                {
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
            }
            HelperProxyRoute::Models => {
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
            }
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

fn helper_proxy_route(method: &str, path: &str, body: &str) -> Option<HelperProxyRoute> {
    if method == "POST" && crate::protocol_proxy::is_responses_proxy_path(path) {
        return Some(HelperProxyRoute::Responses {
            stream: crate::protocol_proxy::responses_request_wants_stream(body),
        });
    }
    if method == "GET" && crate::protocol_proxy::is_models_proxy_path(path) {
        return Some(HelperProxyRoute::Models);
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use crate::protocol_proxy::{RouteMode, UpstreamProtocol};

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "codex-pilot-helper-{name}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    fn write_provider_profiles(
        root: &std::path::Path,
        base_url: &str,
        protocol: &str,
    ) -> anyhow::Result<()> {
        let state = json!({
            "activeProfileId": "p1",
            "profiles": [{
                "id": "p1",
                "name": "test",
                "baseUrl": base_url,
                "bearerToken": "sk-test",
                "mode": "api",
                "upstreamProtocol": protocol
            }]
        });
        std::fs::create_dir_all(root)?;
        std::fs::write(
            root.join("provider-profiles.json"),
            serde_json::to_vec_pretty(&state)?,
        )?;
        Ok(())
    }

    #[test]
    fn helper_identifies_responses_and_models_routes() {
        let _guard = test_guard();
        assert_eq!(
            helper_proxy_route("POST", "/v1/responses", r#"{"stream":true}"#),
            Some(HelperProxyRoute::Responses { stream: true })
        );
        assert_eq!(
            helper_proxy_route("POST", "/responses/compact", r#"{"stream":false}"#),
            Some(HelperProxyRoute::Responses { stream: false })
        );
        assert_eq!(
            helper_proxy_route("GET", "/v1/models", ""),
            Some(HelperProxyRoute::Models)
        );
        assert_eq!(helper_proxy_route("POST", "/backend/status", ""), None);
    }

    #[test]
    fn helper_loads_chat_proxy_target_from_profiles() {
        let _guard = test_guard();
        let root = unique_temp_dir("chat-target");
        crate::app_paths::set_test_app_state_dir(Some(root.clone()));
        write_provider_profiles(&root, "https://chat.example/v1", "chatCompletions").unwrap();

        let target = load_active_proxy_target().unwrap().unwrap();
        assert_eq!(target.base_url, "https://chat.example/v1");
        assert_eq!(target.api_key, "sk-test");
        assert_eq!(target.protocol, UpstreamProtocol::ChatCompletions);
        assert_eq!(
            crate::protocol_proxy::route_mode_for_protocol(target.protocol),
            RouteMode::LocalProxy
        );

        crate::app_paths::set_test_app_state_dir(None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn helper_loads_anthropic_proxy_target_from_profiles() {
        let _guard = test_guard();
        let root = unique_temp_dir("anthropic-target");
        crate::app_paths::set_test_app_state_dir(Some(root.clone()));
        write_provider_profiles(&root, "https://anthropic.example/v1", "anthropicMessages")
            .unwrap();

        let target = load_active_proxy_target().unwrap().unwrap();
        assert_eq!(target.base_url, "https://anthropic.example/v1");
        assert_eq!(target.protocol, UpstreamProtocol::AnthropicMessages);
        assert_eq!(
            crate::protocol_proxy::route_mode_for_protocol(target.protocol),
            RouteMode::LocalProxy
        );

        crate::app_paths::set_test_app_state_dir(None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn helper_ignores_direct_responses_profiles() {
        let _guard = test_guard();
        let root = unique_temp_dir("direct-target");
        crate::app_paths::set_test_app_state_dir(Some(root.clone()));
        write_provider_profiles(&root, "https://responses.example/v1", "responses").unwrap();

        assert!(load_active_proxy_target().unwrap().is_none());

        crate::app_paths::set_test_app_state_dir(None);
        let _ = std::fs::remove_dir_all(root);
    }
}
