use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub app_dir: Option<PathBuf>,
    pub debug_port: u16,
    pub helper_port: u16,
}

#[derive(Debug, Deserialize)]
struct HelperStatus {
    status: String,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            app_dir: None,
            debug_port: crate::ports::DEFAULT_DEBUG_PORT,
            helper_port: crate::ports::DEFAULT_HELPER_PORT,
        }
    }
}

pub fn parse_launch_options<I, S>(args: I) -> LaunchOptions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut options = LaunchOptions::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--app-path" => {
                if let Some(value) = iter.next() {
                    let value = value.as_ref().trim();
                    if !value.is_empty() {
                        options.app_dir = Some(PathBuf::from(value));
                    }
                }
            }
            "--debug-port" => {
                if let Some(value) = iter.next() {
                    if let Ok(port) = value.as_ref().parse::<u16>() {
                        options.debug_port = port;
                    }
                }
            }
            "--helper-port" => {
                if let Some(value) = iter.next() {
                    if let Ok(port) = value.as_ref().parse::<u16>() {
                        options.helper_port = port;
                    }
                }
            }
            _ => {}
        }
    }

    options
}

pub async fn launch_and_inject(options: LaunchOptions) -> anyhow::Result<()> {
    let app_dir = crate::app_paths::resolve_codex_app_dir(options.app_dir.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Codex App directory not found"))?;
    let debug_port = crate::ports::select_platform_loopback_port(options.debug_port);
    if helper_status(options.helper_port).await.is_ok() {
        let _ = crate::diagnostic_log::append(
            "launcher.helper_already_running_skip_inject",
            serde_json::json!({
                "debug_port": debug_port,
                "helper_port": options.helper_port
            }),
        );
        crate::status::write_status(&crate::status::BackendStatus {
            status: "running".to_string(),
            version: crate::version::VERSION.to_string(),
        })?;
        return Ok(());
    }
    let helper_port = options.helper_port;
    let _ = crate::diagnostic_log::append(
        "launcher.start",
        serde_json::json!({
            "app_dir": app_dir.to_string_lossy(),
            "debug_port": debug_port,
            "helper_port": helper_port
        }),
    );
    let helper = crate::helper::start_helper(helper_port).await?;
    let mut child = launch_codex(&app_dir, debug_port).await?;
    inject_running_codex(debug_port, helper_port).await?;
    crate::status::write_status(&crate::status::BackendStatus {
        status: "running".to_string(),
        version: crate::version::VERSION.to_string(),
    })?;
    let _ = child.wait().await;
    helper.shutdown().await;
    Ok(())
}

async fn helper_status(port: u16) -> anyhow::Result<HelperStatus> {
    let url = format!("http://127.0.0.1:{port}/backend/status");
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .context("failed to build helper status client")?
        .get(url)
        .send()
        .await
        .context("failed to query helper status")?
        .error_for_status()
        .context("helper status returned an error")?
        .json::<HelperStatus>()
        .await
        .context("failed to parse helper status")
        .and_then(|status| {
            if status.status == "ok" {
                Ok(status)
            } else {
                anyhow::bail!("helper status is not ok")
            }
        })
}

pub fn build_codex_arguments(debug_port: u16) -> Vec<String> {
    vec![
        format!("--remote-debugging-port={debug_port}"),
        format!("--remote-allow-origins=http://127.0.0.1:{debug_port}"),
    ]
}

pub fn build_codex_command(app_dir: &Path, debug_port: u16) -> Vec<String> {
    let mut command = vec![
        crate::app_paths::build_codex_executable(app_dir)
            .to_string_lossy()
            .to_string(),
    ];
    command.extend(build_codex_arguments(debug_port));
    command
}

pub fn build_macos_open_command(app_dir: &Path, debug_port: u16) -> Vec<String> {
    let mut command = vec![
        "open".to_string(),
        "-n".to_string(),
        "-W".to_string(),
        "-a".to_string(),
        app_dir.to_string_lossy().to_string(),
        "--args".to_string(),
    ];
    command.extend(build_codex_arguments(debug_port));
    command
}

async fn launch_codex(app_dir: &Path, debug_port: u16) -> anyhow::Result<Child> {
    let command = if app_dir.extension().and_then(|value| value.to_str()) == Some("app") {
        build_macos_open_command(app_dir, debug_port)
    } else {
        build_codex_command(app_dir, debug_port)
    };
    let executable = command
        .first()
        .ok_or_else(|| anyhow::anyhow!("Codex launch command is empty"))?;
    let _ = crate::diagnostic_log::append(
        "launcher.spawn",
        serde_json::json!({
            "executable": executable,
            "arg_count": command.len().saturating_sub(1)
        }),
    );
    Command::new(executable)
        .args(&command[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch Codex with {executable}"))
}

pub async fn inject_running_codex(debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
    let script = crate::assets::injection_script(helper_port);
    let mut last_error = None;
    for _ in 0..60 {
        match inject_bridge(debug_port, helper_port, &script).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        anyhow::anyhow!("Codex injection failed: CDP port did not become available")
    }))
}

async fn inject_bridge(debug_port: u16, helper_port: u16, script: &str) -> anyhow::Result<()> {
    let websocket_url = crate::cdp::selected_page_websocket_url(debug_port).await?;
    let ctx = crate::routes::BridgeContext::new(debug_port, helper_port);
    crate::bridge::install_bridge(
        &websocket_url,
        crate::bridge::BRIDGE_BINDING_NAME,
        std::sync::Arc::new(move |path, payload| {
            let ctx = ctx.clone();
            Box::pin(
                async move { Ok(crate::routes::handle_bridge_request(ctx, &path, payload).await) },
            )
        }),
        &[script.to_string()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_open_command_forces_new_instance_for_debug_args() {
        let command = build_macos_open_command(Path::new("/Applications/Codex.app"), 9688);

        assert_eq!(command[0], "open");
        assert!(command.contains(&"-n".to_string()));
        assert!(command.contains(&"--remote-debugging-port=9688".to_string()));
    }
}
