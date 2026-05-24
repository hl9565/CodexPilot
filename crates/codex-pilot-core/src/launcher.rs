use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

const INJECTION_MAX_ATTEMPTS: usize = 2;
const INJECTION_RETRY_DELAY_MS: u64 = 500;

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
    let debug_port = options.debug_port;
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
    if crate::ports::can_connect_loopback_port(debug_port) {
        let _ = crate::diagnostic_log::append(
            "launcher.debug_port_available_reinject",
            serde_json::json!({
                "app_dir": app_dir.to_string_lossy(),
                "debug_port": debug_port,
                "helper_port": helper_port
            }),
        );
        let helper = crate::helper::start_helper(helper_port).await?;
        let inject_result = inject_running_codex(debug_port, helper_port).await;
        match inject_result {
            Ok(()) => {
                crate::status::write_status(&crate::status::BackendStatus {
                    status: "running".to_string(),
                    version: crate::version::VERSION.to_string(),
                })?;
                return Ok(());
            }
            Err(error) => {
                helper.shutdown().await;
                return Err(error);
            }
        }
    }
    if is_codex_process_running() {
        let _ = crate::diagnostic_log::append(
            "launcher.codex_running_without_debug_port",
            serde_json::json!({
                "app_dir": app_dir.to_string_lossy(),
                "debug_port": debug_port,
                "helper_port": helper_port
            }),
        );
        anyhow::bail!(
            "Codex is already running without a reachable debug port. Restart Codex from CodexPilot before trying again."
        );
    }
    if !crate::ports::can_bind_loopback_port(debug_port) {
        let _ = crate::diagnostic_log::append(
            "launcher.debug_port_unavailable",
            serde_json::json!({
                "debug_port": debug_port,
                "helper_port": helper_port
            }),
        );
        anyhow::bail!(
            "调试端口 {debug_port} 已被占用，无法启动 Codex。请先关闭占用该端口的进程，或在启动设置里更换调试端口。"
        );
    }
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
    let mut process = Command::new(executable);
    process
        .args(&command[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    process
        .spawn()
        .with_context(|| format!("failed to launch Codex with {executable}"))
}

fn is_codex_process_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("pgrep")
            .args(["-x", "Codex"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq Codex.exe"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("Codex.exe"))
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("pgrep")
            .args(["-x", "codex"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

pub async fn inject_running_codex(debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
    let script = crate::assets::injection_script(helper_port);
    let mut last_error = None;
    for attempt in 0..INJECTION_MAX_ATTEMPTS {
        match inject_bridge(debug_port, helper_port, &script).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < INJECTION_MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(INJECTION_RETRY_DELAY_MS))
                        .await;
                }
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

    #[test]
    fn launch_options_keep_requested_debug_port() {
        let options = LaunchOptions {
            app_dir: None,
            debug_port: 9688,
            helper_port: 58888,
        };

        assert_eq!(options.debug_port, 9688);
    }
}
