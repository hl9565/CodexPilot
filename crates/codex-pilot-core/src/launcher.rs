use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Child;

// Codex 冷启动后，CDP 页面 target 通常要几秒才就绪。注入在这个截止时间内持续重试，
// 以熬过首启竞态；上限保持小于 manager 端的启动/注入超时（25s），确保 launcher 能在
// manager 放弃前完成注入并写入运行状态。
const INJECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const INJECTION_RETRY_DELAY_MS: u64 = 500;
const BACKEND_MONITOR_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
// Codex 在自更新重启、单实例接管，或加载完成后关闭 CDP 端口时，调试端口会短暂消失，
// 但进程仍然存活。必须以 Codex 进程是否在运行作为退出判据，并要求连续多次探测都查不到
// 进程才判定真正退出，避免把瞬时空窗误判为退出而拆掉后端、让 manager 弹回启动界面。
const BACKEND_MONITOR_MISSED_PROCESS_PROBES: u8 = 5;

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
    crate::status::clear_status()?;
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
                monitor_backend_until_codex_exits(helper, debug_port, helper_port).await;
                return Ok(());
            }
            Err(error) => {
                helper.shutdown().await;
                return Err(error);
            }
        }
    }
    if is_codex_process_running().await {
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
    monitor_backend_until_codex_exits(helper, debug_port, helper_port).await;
    match child.try_wait() {
        Ok(Some(status)) => {
            let _ = crate::diagnostic_log::append(
                "launcher.wrapper_already_exited",
                serde_json::json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port,
                    "status": status.to_string()
                }),
            );
        }
        Ok(None) => {
            let _ = crate::diagnostic_log::append(
                "launcher.child_still_running_after_debug_port_lost",
                serde_json::json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port
                }),
            );
        }
        Err(error) => {
            let _ = crate::diagnostic_log::append(
                "launcher.wrapper_status_failed",
                serde_json::json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port,
                    "message": error.to_string()
                }),
            );
        }
    }
    Ok(())
}

async fn monitor_backend_until_codex_exits(
    helper: crate::helper::HelperRuntime,
    debug_port: u16,
    helper_port: u16,
) {
    let _ = crate::diagnostic_log::append(
        "launcher.backend_monitor_start",
        serde_json::json!({
            "debug_port": debug_port,
            "helper_port": helper_port
        }),
    );
    let mut missed_process_probes = 0_u8;
    loop {
        // 以 Codex 进程是否存活作为唯一退出判据；调试端口可能在 Codex 自更新重启、
        // 单实例接管或首屏加载完成后短暂消失，但只要进程还在就不应拆掉后端。
        if is_codex_process_running().await {
            missed_process_probes = 0;
        } else {
            missed_process_probes = missed_process_probes.saturating_add(1);
            if missed_process_probes >= BACKEND_MONITOR_MISSED_PROCESS_PROBES {
                let _ = crate::diagnostic_log::append(
                    "launcher.codex_process_exited",
                    serde_json::json!({
                        "debug_port": debug_port,
                        "helper_port": helper_port,
                        "missed_probes": missed_process_probes,
                        "debug_port_reachable": crate::ports::can_connect_loopback_port(debug_port)
                    }),
                );
                break;
            }
        }
        tokio::time::sleep(BACKEND_MONITOR_INTERVAL).await;
    }
    if let Err(error) = crate::status::clear_status() {
        let _ = crate::diagnostic_log::append(
            "launcher.clear_status_failed",
            serde_json::json!({
                "debug_port": debug_port,
                "helper_port": helper_port,
                "message": error.to_string()
            }),
        );
    }
    helper.shutdown().await;
    let _ = crate::diagnostic_log::append(
        "launcher.backend_monitor_stopped",
        serde_json::json!({
            "debug_port": debug_port,
            "helper_port": helper_port
        }),
    );
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
    let mut process = crate::windows_integration::tokio_command(executable);
    process
        .args(&command[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::windows_integration::spawn_hidden(&mut process)
        .with_context(|| format!("failed to launch Codex with {executable}"))
}

pub async fn is_codex_process_running() -> bool {
    tokio::task::spawn_blocking(detect_codex_process_running)
        .await
        .unwrap_or(false)
}

fn detect_codex_process_running() -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut command = crate::windows_integration::std_command("pgrep");
        command
            .args(["-x", "Codex"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::windows_integration::status_hidden(&mut command)
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = crate::windows_integration::std_command("tasklist");
        command
            .args(["/FI", "IMAGENAME eq Codex.exe"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::windows_integration::output_hidden(&mut command)
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("Codex.exe"))
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let mut command = crate::windows_integration::std_command("pgrep");
        command
            .args(["-x", "codex"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::windows_integration::status_hidden(&mut command)
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

pub async fn inject_running_codex(debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
    let script = crate::assets::injection_script(helper_port);
    let retry_delay = std::time::Duration::from_millis(INJECTION_RETRY_DELAY_MS);
    let deadline = std::time::Instant::now() + INJECTION_TIMEOUT;
    loop {
        match inject_bridge(debug_port, helper_port, &script).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                // 没有足够时间再试一次时，返回最近一次的失败原因。
                if std::time::Instant::now() + retry_delay >= deadline {
                    return Err(error);
                }
            }
        }
        tokio::time::sleep(retry_delay).await;
    }
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
        assert!(!command.contains(&"-W".to_string()));
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
