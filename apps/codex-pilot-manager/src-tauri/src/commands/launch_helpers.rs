use super::super::*;
use crate::commands::diagnostics::append_diagnostic_event;
use crate::commands::launch::set_cached_codex_process_running;

pub(crate) fn with_launch_state_mut<F>(state: &ManagerState, f: F)
where
    F: FnOnce(&mut LaunchState),
{
    match state.launch_state.lock() {
        Ok(mut g) => f(&mut g),
        Err(poisoned) => {
            tracing::error!(
                target = "mutex",
                lock = "launch_state",
                "mutex poisoned, recovering"
            );
            let mut g = poisoned.into_inner();
            f(&mut g);
            state.launch_state.clear_poison();
        }
    }
}

pub(crate) fn with_codex_process_cache_mut<F>(state: &ManagerState, f: F)
where
    F: FnOnce(&mut CodexProcessCache),
{
    match state.codex_process_cache.lock() {
        Ok(mut g) => f(&mut g),
        Err(poisoned) => {
            tracing::error!(
                target = "mutex",
                lock = "codex_process_cache",
                "mutex poisoned, recovering"
            );
            let mut g = poisoned.into_inner();
            f(&mut g);
            state.codex_process_cache.clear_poison();
        }
    }
}

pub(crate) fn clear_backend_status_file() {
    let path = codex_pilot_core::status::status_path();
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) async fn auto_sync_sessions_after_launch(
    launch_action: &str,
    prefs: &LaunchPreferences,
) -> String {
    let success_message = match launch_action {
        "reinject" => "已重新注入 CodexPilot。",
        _ => "已启动 CodexPilot。",
    };

    if !prefs.auto_sync_sessions_on_launch {
        return success_message.to_string();
    }

    let inspection_result =
        tauri::async_runtime::spawn_blocking(|| codex_pilot_data::provider_sync::inspect_provider_sync(None))
            .await;

    let inspection = match inspection_result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let message = format!("自动同步会话检查失败：{error}");
            let _ = append_diagnostic_event(
                "manager.auto_session_sync_failed",
                json!({
                    "launch_action": launch_action,
                    "stage": "inspect",
                    "message": message
                }),
            );
            return format!("{success_message} 但自动同步会话失败：{message}");
        }
        Err(error) => {
            let message = format!("自动同步会话检查任务失败：{error}");
            let _ = append_diagnostic_event(
                "manager.auto_session_sync_failed",
                json!({
                    "launch_action": launch_action,
                    "stage": "inspect_join",
                    "message": message
                }),
            );
            return format!("{success_message} 但自动同步会话失败：{message}");
        }
    };

    let _ = append_diagnostic_event(
        "manager.auto_session_sync_checked",
        json!({
            "launch_action": launch_action,
            "target_provider": inspection.target_provider,
            "rollout_rewrite_needed": inspection.rollout_rewrite_needed,
            "sqlite_provider_rows_needing_sync": inspection.sqlite_provider_rows_needing_sync
        }),
    );

    if inspection.rollout_rewrite_needed == 0 && inspection.sqlite_provider_rows_needing_sync == 0 {
        return match launch_action {
            "reinject" => "已重新注入 CodexPilot，无需同步会话。".to_string(),
            _ => "已启动 CodexPilot，无需同步会话。".to_string(),
        };
    }

    let sync_result =
        tauri::async_runtime::spawn_blocking(|| codex_pilot_data::provider_sync::run_provider_sync(None))
            .await;

    let sync_result = match sync_result {
        Ok(value) => value,
        Err(error) => {
            let message = format!("自动同步会话任务失败：{error}");
            let _ = append_diagnostic_event(
                "manager.auto_session_sync_failed",
                json!({
                    "launch_action": launch_action,
                    "target_provider": inspection.target_provider,
                    "stage": "sync_join",
                    "message": message
                }),
            );
            return format!("{success_message} 但自动同步会话失败：{message}");
        }
    };

    let _ = append_diagnostic_event(
        "manager.auto_session_sync_finished",
        json!({
            "launch_action": launch_action,
            "target_provider": inspection.target_provider,
            "status": format!("{:?}", sync_result.status),
            "message": sync_result.message
        }),
    );

    if sync_result.status != codex_pilot_data::provider_sync::ProviderSyncStatus::Synced {
        return format!(
            "{success_message} 但自动同步会话失败：{}",
            sync_result.message
        );
    }

    match launch_action {
        "reinject" => "已重新注入 CodexPilot，并完成会话同步。".to_string(),
        _ => "已启动 CodexPilot，并完成会话同步。".to_string(),
    }
}

pub(crate) fn launch_state_label(state: &LaunchState) -> String {
    match state {
        LaunchState::Idle => "空闲".to_string(),
        LaunchState::Launching => "启动中".to_string(),
        LaunchState::Running => "运行中".to_string(),
        LaunchState::Failed(message) => format!("失败：{message}"),
    }
}

pub(crate) fn launch_action_kind(
    ready: bool,
    manager_running: bool,
    codex_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
    launch_state: &LaunchState,
) -> String {
    if matches!(launch_state, LaunchState::Launching) {
        return "launching".to_string();
    }
    if manager_running || codex_pilot_core::ports::can_connect_loopback_port(options.helper_port) {
        "running".to_string()
    } else if codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        "reinject".to_string()
    } else if codex_running {
        "restart".to_string()
    } else if ready {
        "launch".to_string()
    } else {
        "unavailable".to_string()
    }
}

pub(crate) fn launch_action_label(
    ready: bool,
    manager_running: bool,
    codex_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
    launch_state: &LaunchState,
) -> String {
    match launch_action_kind(ready, manager_running, codex_running, options, launch_state).as_str()
    {
        "launching" => "启动中".to_string(),
        "running" => "已运行".to_string(),
        "reinject" => "重新注入".to_string(),
        "restart" => "重启并注入".to_string(),
        "launch" => "启动 Codex".to_string(),
        _ => "不可启动".to_string(),
    }
}

pub(crate) fn launch_action_detail(
    ready: bool,
    manager_running: bool,
    codex_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
    launch_state: &LaunchState,
) -> String {
    match launch_action_kind(ready, manager_running, codex_running, options, launch_state).as_str()
    {
        "launching" => "CodexPilot 正在启动 Codex，请稍候。".to_string(),
        "running" => "CodexPilot 已连接，无需重复启动。".to_string(),
        "reinject" => "检测到 Codex 调试端口，可以直接重新注入。".to_string(),
        "restart" => "检测到 Codex 已运行，但没有调试端口；需要确认后重启。".to_string(),
        "launch" => "未检测到运行中的 Codex，可以从 CodexPilot 启动并注入。".to_string(),
        _ => "需要检查 Codex 应用路径或启动偏好。".to_string(),
    }
}

pub(crate) fn request_codex_quit() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut command = codex_pilot_core::windows_integration::std_command("osascript");
        command
            .args(["-e", r#"tell application "Codex" to quit"#])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = codex_pilot_core::windows_integration::status_hidden(&mut command)
            .map_err(|error| format!("请求关闭 Codex 失败：{error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("请求关闭 Codex 失败，请手动关闭后再启动。".to_string())
        }
    }
    #[cfg(target_os = "windows")]
    {
        Err("Windows 暂不支持自动请求关闭 Codex，请手动关闭后再启动。".to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("当前平台暂不支持自动请求关闭 Codex，请手动关闭后再启动。".to_string())
    }
}

pub(crate) async fn cached_codex_process_running(state: &ManagerState) -> bool {
    if let Ok(cache) = state.codex_process_cache.lock() {
        if cache
            .checked_at
            .is_some_and(|checked_at| checked_at.elapsed() < CODEX_RUNNING_CACHE_TTL)
        {
            return cache.codex_running;
        }
    }

    let codex_running = codex_pilot_core::launcher::is_codex_process_running().await;
    set_cached_codex_process_running(state, codex_running);
    codex_running
}
