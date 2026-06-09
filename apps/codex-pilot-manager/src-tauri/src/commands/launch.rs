use super::super::*;
use crate::commands::diagnostics::append_diagnostic_event;
pub(crate) use crate::commands::launch_helpers::launch_state_label;
use crate::commands::launch_helpers::{
    auto_sync_sessions_after_launch, cached_codex_process_running, clear_backend_status_file,
    launch_action_detail, launch_action_kind, launch_action_label, request_codex_quit,
    with_codex_process_cache_mut, with_launch_state_mut,
};
use tokio::io::AsyncReadExt;

fn emit_launch_state(app: &tauri::AppHandle, new_state: &LaunchState) {
    use tauri::Emitter;
    if let Err(e) = app.emit(
        "launch_state_changed",
        crate::commands::launch_helpers::launch_state_label(new_state),
    ) {
        let _ = codex_pilot_core::diagnostic_log::append(
            "launch.emit_failed",
            serde_json::json!({ "error": e.to_string() }),
        );
    }
}

pub(crate) fn resolve_launcher_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("无法定位管理器：{error}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法定位管理器所在目录".to_string())?;
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let sidecar = dir.join(format!("codex-pilot-launcher{suffix}"));
    if sidecar.is_file() {
        return Ok(sidecar);
    }
    let dev = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target/debug")
        .join(format!("codex-pilot-launcher{suffix}"));
    if dev.is_file() {
        return Ok(dev);
    }
    Err(format!(
        "未找到 launcher，可先运行 cargo build -p codex-pilot-launcher。尝试路径：{}",
        sidecar.display()
    ))
}

#[tauri::command]
pub(crate) async fn launch_snapshot(
    state: tauri::State<'_, ManagerState>,
    app: tauri::AppHandle,
) -> Result<LaunchSnapshot, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    let app_dir = codex_pilot_core::app_paths::resolve_codex_app_dir(options.app_dir.as_deref());
    let codex_installed = app_dir.is_some();
    let command_preview = app_dir
        .as_deref()
        .map(|path| build_codex_command_preview(path, options.debug_port))
        .unwrap_or_else(Vec::new);

    let helper_reachable = codex_pilot_core::ports::can_connect_loopback_port(options.helper_port);
    let debug_reachable = codex_pilot_core::ports::can_connect_loopback_port(options.debug_port);

    // Self-heal：若状态停留在 Running/Launching 但 helper 和调试端口都不可达，说明后端已退出或启动失败。
    // 这里主动降级并发事件，避免 UI 只靠下一轮 polling 感知状态机变化。
    let mut healed_to_idle = false;
    let current = {
        let mut guard = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        if (matches!(*guard, LaunchState::Running) && !helper_reachable)
            || (matches!(*guard, LaunchState::Launching) && !helper_reachable && !debug_reachable)
        {
            *guard = LaunchState::Idle;
            set_cached_codex_process_running(&state, false);
            healed_to_idle = true;
        }
        guard.clone()
    };
    if healed_to_idle {
        emit_launch_state(&app, &LaunchState::Idle);
    }

    let manager_running = matches!(current, LaunchState::Running);
    let codex_running = if helper_reachable {
        set_cached_codex_process_running(&state, true);
        true
    } else {
        cached_codex_process_running(&state).await
    };

    Ok(LaunchSnapshot {
        app_path: app_dir.map(|path| path.to_string_lossy().to_string()),
        requested_app_path: prefs.app_path,
        debug_port: options.debug_port,
        helper_port: options.helper_port,
        auto_launch_on_open: prefs.auto_launch_on_open,
        auto_sync_sessions_on_launch: prefs.auto_sync_sessions_on_launch,
        ready: !command_preview.is_empty(),
        codex_installed,
        state: launch_state_label(&current),
        action_kind: launch_action_kind(
            !command_preview.is_empty(),
            manager_running,
            codex_running,
            &options,
            &current,
        ),
        action_label: launch_action_label(
            !command_preview.is_empty(),
            manager_running,
            codex_running,
            &options,
            &current,
        ),
        helper_reachable,
        debug_reachable,
        codex_running,
        detail: launch_action_detail(
            !command_preview.is_empty(),
            manager_running,
            codex_running,
            &options,
            &current,
        ),
        command_preview,
    })
}

#[tauri::command]
pub(crate) async fn launch_codex(
    state: tauri::State<'_, ManagerState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    if codex_pilot_core::ports::can_connect_loopback_port(options.helper_port) {
        set_cached_codex_process_running(&state, true);
        append_diagnostic_event(
            "manager.launch_helper_already_running",
            serde_json::json!({
                "debug_port": options.debug_port,
                "helper_port": options.helper_port,
                "debug_port_connectable": codex_pilot_core::ports::can_connect_loopback_port(options.debug_port)
            }),
        )?;
        {
            let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
            *current = LaunchState::Running;
        }
        emit_launch_state(&app, &LaunchState::Running);
        return Ok("CodexPilot 已在运行中。".to_string());
    }
    if codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        return inject_existing_codex(
            &state,
            &app,
            &prefs,
            options.debug_port,
            options.helper_port,
        )
        .await;
    }
    if cached_codex_process_running(&state).await {
        return Err(
            "当前 Codex 不是通过 CodexPilot 启动，无法直接注入。请确认后使用“重启并注入”。"
                .to_string(),
        );
    }
    spawn_launcher(&state, &app, &prefs).await
}

#[tauri::command]
pub(crate) async fn reinject_codex(
    state: tauri::State<'_, ManagerState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    if !codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        return Err("未检测到 Codex 调试端口，无法重新注入。".to_string());
    }
    inject_existing_codex(
        &state,
        &app,
        &prefs,
        options.debug_port,
        options.helper_port,
    )
    .await
}

#[tauri::command]
pub(crate) async fn restart_codex_and_inject(
    state: tauri::State<'_, ManagerState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    request_codex_quit()?;
    set_cached_codex_process_running(&state, false);
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let prefs = load_launch_preferences();
    spawn_launcher(&state, &app, &prefs).await
}

async fn inject_existing_codex(
    state: &tauri::State<'_, ManagerState>,
    app: &tauri::AppHandle,
    prefs: &LaunchPreferences,
    debug_port: u16,
    helper_port: u16,
) -> Result<String, String> {
    {
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        *current = LaunchState::Launching;
    }
    emit_launch_state(app, &LaunchState::Launching);
    match inject_running_codex_for_manager(debug_port, helper_port).await {
        Ok(()) => {
            codex_pilot_core::status::write_status(&codex_pilot_core::status::BackendStatus {
                status: "running".to_string(),
                version: codex_pilot_core::version::VERSION.to_string(),
            })
            .map_err(|error| format!("写入状态失败：{error}"))?;
            {
                let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
                *current = LaunchState::Running;
            }
            emit_launch_state(app, &LaunchState::Running);
            set_cached_codex_process_running(state, true);
            Ok(auto_sync_sessions_after_launch("reinject", prefs).await)
        }
        Err(error) => {
            let message = format!("重新注入失败：{error}");
            with_launch_state_mut(state, |current| {
                *current = LaunchState::Failed(message.clone());
            });
            emit_launch_state(app, &LaunchState::Failed(message.clone()));
            Err(message)
        }
    }
}

async fn inject_running_codex_for_manager(debug_port: u16, helper_port: u16) -> Result<(), String> {
    append_diagnostic_event(
        "manager.inject_existing_start",
        json!({
            "debug_port": debug_port,
            "helper_port": helper_port,
            "timeout_ms": MANAGER_INJECT_TIMEOUT.as_millis()
        }),
    )?;

    let result = tokio::time::timeout(
        MANAGER_INJECT_TIMEOUT,
        codex_pilot_core::launcher::inject_running_codex(debug_port, helper_port),
    )
    .await;

    match result {
        Ok(Ok(())) => {
            append_diagnostic_event(
                "manager.inject_existing_ok",
                json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port
                }),
            )?;
            Ok(())
        }
        Ok(Err(error)) => {
            let message = error.to_string();
            append_diagnostic_event(
                "manager.inject_existing_failed",
                json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port,
                    "message": message
                }),
            )?;
            Err(message)
        }
        Err(_) => {
            let message = format!(
                "注入 CodexPilot 超时，已等待 {} 秒。请查看诊断后手动重试或重启并注入。",
                MANAGER_INJECT_TIMEOUT.as_secs()
            );
            append_diagnostic_event(
                "manager.inject_existing_timeout",
                json!({
                    "debug_port": debug_port,
                    "helper_port": helper_port,
                    "timeout_ms": MANAGER_INJECT_TIMEOUT.as_millis()
                }),
            )?;
            Err(message)
        }
    }
}

async fn spawn_launcher(
    state: &tauri::State<'_, ManagerState>,
    app: &tauri::AppHandle,
    prefs: &LaunchPreferences,
) -> Result<String, String> {
    let mut emitted_idle = false;
    {
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        if matches!(*current, LaunchState::Launching | LaunchState::Running) {
            if codex_pilot_core::ports::can_connect_loopback_port(prefs.helper_port) {
                return Ok("CodexPilot 已在启动或运行中。".to_string());
            }
            *current = LaunchState::Idle;
            emitted_idle = true;
        }
    }
    if emitted_idle {
        emit_launch_state(app, &LaunchState::Idle);
    }
    {
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        *current = LaunchState::Launching;
    }
    emit_launch_state(app, &LaunchState::Launching);

    let launcher = match resolve_launcher_path() {
        Ok(path) => path,
        Err(message) => {
            with_launch_state_mut(state, |current| {
                *current = LaunchState::Failed(message.clone());
            });
            emit_launch_state(app, &LaunchState::Failed(message.clone()));
            return Err(message);
        }
    };
    let mut command = codex_pilot_core::windows_integration::tokio_command(&launcher);
    append_tokio_launcher_args(&mut command, prefs);
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = match codex_pilot_core::windows_integration::spawn_hidden(&mut command) {
        Ok(child) => child,
        Err(error) => {
            let message = format!("启动 CodexPilot 失败：{error}");
            with_launch_state_mut(state, |current| {
                *current = LaunchState::Failed(message.clone());
            });
            emit_launch_state(app, &LaunchState::Failed(message.clone()));
            return Err(message);
        }
    };
    let mut stderr = child.stderr.take();

    clear_backend_status_file();
    match wait_for_backend_launch(prefs.helper_port, &mut child, stderr.as_mut()).await {
        Ok(()) => {
            {
                let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
                *current = LaunchState::Running;
            }
            emit_launch_state(app, &LaunchState::Running);
            set_cached_codex_process_running(state, true);
            Ok(auto_sync_sessions_after_launch("launch", prefs).await)
        }
        Err(message) => {
            with_launch_state_mut(state, |current| {
                *current = LaunchState::Failed(message.clone());
            });
            emit_launch_state(app, &LaunchState::Failed(message.clone()));
            Err(message)
        }
    }
}

async fn wait_for_backend_launch(
    helper_port: u16,
    child: &mut tokio::process::Child,
    mut stderr: Option<&mut tokio::process::ChildStderr>,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + MANAGER_LAUNCH_TIMEOUT;
    loop {
        let helper_reachable = codex_pilot_core::ports::can_connect_loopback_port(helper_port);
        let backend_running = codex_pilot_core::status::read_status()
            .ok()
            .flatten()
            .map(|status| status.status == "running")
            .unwrap_or(false);
        if helper_reachable && backend_running {
            let _ = append_diagnostic_event(
                "manager.launch_backend_ready",
                json!({
                    "helper_port": helper_port
                }),
            );
            return Ok(());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr_text = read_launcher_stderr(stderr.take()).await;
                let detail = concise_launcher_error(&stderr_text);
                let message = if detail.is_empty() {
                    format!("CodexPilot launcher 已退出：{status}")
                } else {
                    format!("CodexPilot launcher 已退出：{status}。{detail}")
                };
                let _ = append_diagnostic_event(
                    "manager.launcher_exited_before_backend_ready",
                    json!({
                        "helper_port": helper_port,
                        "status": status.to_string(),
                        "stderr": stderr_text
                    }),
                );
                return Err(message);
            }
            Ok(None) => {}
            Err(error) => {
                let message = format!("检查 CodexPilot launcher 状态失败：{error}");
                let _ = append_diagnostic_event(
                    "manager.launcher_status_failed",
                    json!({
                        "helper_port": helper_port,
                        "message": error.to_string()
                    }),
                );
                return Err(message);
            }
        }
        if std::time::Instant::now() >= deadline {
            let message = format!(
                "启动 CodexPilot 超时，已等待 {} 秒。请查看诊断后重试。",
                MANAGER_LAUNCH_TIMEOUT.as_secs()
            );
            let _ = append_diagnostic_event(
                "manager.launch_backend_timeout",
                json!({
                    "helper_port": helper_port,
                    "timeout_ms": MANAGER_LAUNCH_TIMEOUT.as_millis(),
                    "helper_reachable": helper_reachable,
                    "backend_running": backend_running
                }),
            );
            return Err(message);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

async fn read_launcher_stderr(stderr: Option<&mut tokio::process::ChildStderr>) -> String {
    let Some(stderr) = stderr else {
        return String::new();
    };
    let mut buffer = String::new();
    let result =
        tokio::time::timeout(Duration::from_secs(1), stderr.read_to_string(&mut buffer)).await;
    match result {
        Ok(Ok(_)) => buffer.trim().to_string(),
        Ok(Err(error)) => format!("读取 launcher 错误输出失败：{error}"),
        Err(_) => "读取 launcher 错误输出超时。".to_string(),
    }
}

fn concise_launcher_error(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next()
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn set_cached_codex_process_running(state: &ManagerState, codex_running: bool) {
    with_codex_process_cache_mut(state, |cache| {
        cache.codex_running = codex_running;
        cache.checked_at = Some(Instant::now());
    });
}
