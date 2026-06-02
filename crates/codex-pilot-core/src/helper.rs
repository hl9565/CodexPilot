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
