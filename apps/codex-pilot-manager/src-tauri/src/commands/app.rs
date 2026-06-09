use super::super::*;

#[tauri::command]
pub(crate) async fn backend_status()
-> Result<Option<codex_pilot_core::status::BackendStatus>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let prefs = load_launch_preferences();
        let options = launch_options_from_preferences(&prefs);
        // 仅以 helper 端口是否可达判定后端存活，刻意不看调试端口：Codex 在自更新重启、单实例接管，
        // 或首屏加载完成后关闭 CDP 端口时，调试端口会短暂/持久消失，但后端仍然存活。这里与 launcher
        // 的存活监控（以 Codex 进程为判据）和 launch_snapshot 的 Running 自愈（仅看 helper）保持一致，
        // 避免状态标签在调试端口消失窗口里误报 idle 并反复删除状态文件。
        let helper_reachable =
            codex_pilot_core::ports::can_connect_loopback_port(options.helper_port);
        let status = codex_pilot_core::status::read_status().map_err(|error| error.to_string())?;

        if helper_reachable {
            return Ok(Some(codex_pilot_core::status::BackendStatus {
                status: "running".to_string(),
                version: codex_pilot_core::version::VERSION.to_string(),
            }));
        }

        if status
            .as_ref()
            .map(|value| value.status == "running")
            .unwrap_or(false)
        {
            let _ = codex_pilot_core::status::clear_status();
            return Ok(None);
        }

        Ok(status)
    })
    .await
    .map_err(|error| format!("读取后端状态任务失败：{error}"))?
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    codex_pilot_core::version::VERSION.to_string()
}

#[tauri::command]
pub(crate) async fn save_launch_preferences(request: LaunchPreferences) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let prefs = sanitize_launch_preferences(request)?;
        save_launch_preferences_to_path(&manager_config_path(), &prefs)?;
        Ok("启动偏好已保存。".to_string())
    })
    .await
    .map_err(|error| format!("保存启动偏好任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn enhancement_settings_snapshot() -> EnhancementSettings {
    tauri::async_runtime::spawn_blocking(load_enhancement_settings)
        .await
        .expect("enhancement_settings_snapshot task panicked")
}

#[tauri::command]
pub(crate) async fn save_enhancement_settings(
    request: EnhancementSettings,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = sanitize_enhancement_settings(request);
        save_enhancement_settings_to_path(&enhancement_settings_path(), &settings)?;
        Ok("页面增强设置已保存，重新注入后生效。".to_string())
    })
    .await
    .map_err(|error| format!("保存页面增强设置任务失败：{error}"))?
}
