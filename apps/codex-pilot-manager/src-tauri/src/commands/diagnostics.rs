use super::super::*;
use crate::commands::launch::resolve_launcher_path;
use crate::commands::launch_helpers::launch_action_kind;

#[tauri::command]
pub(crate) async fn diagnostics_snapshot(
    state: tauri::State<'_, ManagerState>,
) -> Result<DiagnosticsSnapshot, String> {
    let launch_state_snapshot = state
        .launch_state
        .lock()
        .map_err(|_| "启动状态锁已损坏")?
        .clone();

    tauri::async_runtime::spawn_blocking(move || {
        let status_path = codex_pilot_core::status::status_path();
        let status_exists = status_path.exists();
        let prefs = load_launch_preferences();
        let options = launch_options_from_preferences(&prefs);
        let helper_reachable = codex_pilot_core::ports::can_connect_loopback_port(options.helper_port);
        let backend_kind = launch_action_kind(
            true,
            matches!(launch_state_snapshot, LaunchState::Running),
            matches!(launch_state_snapshot, LaunchState::Running) || helper_reachable,
            &options,
            &launch_state_snapshot,
        );
        let backend_check = match backend_kind.as_str() {
            "running" => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: "ok".to_string(),
                detail: format!(
                    "本地连接服务已连接；状态文件路径：{}",
                    status_path.to_string_lossy()
                ),
            },
            "launching" => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: "ok".to_string(),
                detail: format!(
                    "Codex 正在启动中，请稍候。状态文件路径：{}",
                    status_path.to_string_lossy()
                ),
            },
            "reinject" => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: "warning".to_string(),
                detail: format!(
                    "调试端口可达但本地连接服务未响应，可回到启动页点'重新注入'。状态文件路径：{}",
                    status_path.to_string_lossy()
                ),
            },
            "restart" => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: "warning".to_string(),
                detail: "检测到 Codex 在运行但非 CodexPilot 启动，需要在启动页点'重启并注入'。"
                    .to_string(),
            },
            "unavailable" => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: "missing".to_string(),
                detail: "未检测到本地连接服务，且 Codex 应用路径未配置。".to_string(),
            },
            "launch" | _ => DiagnosticCheck {
                name: "后端状态".to_string(),
                status: if status_exists {
                    "warning".to_string()
                } else {
                    "missing".to_string()
                },
                detail: if status_exists {
                    format!(
                        "本地连接服务无响应，但发现旧状态文件：{}。后端可能已退出或端口配置不一致，请回到启动页点'重新注入'。",
                        status_path.to_string_lossy()
                    )
                } else {
                    format!(
                        "未检测到本地连接服务，且状态文件不存在：{}",
                        status_path.to_string_lossy()
                    )
                },
            },
        };
        let provider_sync_check = provider_sync_diagnostic_check();
        DiagnosticsSnapshot {
            checks: vec![
                backend_check,
                DiagnosticCheck {
                    name: "Codex 应用探测".to_string(),
                    status: if codex_pilot_core::app_paths::resolve_codex_app_dir(None).is_some() {
                        "ok"
                    } else {
                        "warning"
                    }
                    .to_string(),
                    detail: "使用 codex-pilot-core 的应用路径探测逻辑。".to_string(),
                },
                DiagnosticCheck {
                    name: "中转设置".to_string(),
                    status: if codex_pilot_core::relay_config::default_relay_provider_config()
                        .configured
                    {
                        "ok"
                    } else {
                        "missing"
                    }
                    .to_string(),
                    detail: codex_pilot_core::app_paths::codex_config_path()
                        .to_string_lossy()
                        .to_string(),
                },
                provider_sync_check,
            ],
            logs: codex_pilot_core::diagnostic_log::read_tail(80).unwrap_or_default(),
        }
    })
    .await
    .map_err(|error| format!("读取诊断信息失败：{error}"))
}

#[tauri::command]
pub(crate) fn collect_diagnostics(state: tauri::State<'_, ManagerState>) -> Result<String, String> {
    append_diagnostic_snapshot(&state)?;
    Ok("诊断快照已写入日志。".to_string())
}

fn append_diagnostic_snapshot(state: &tauri::State<'_, ManagerState>) -> Result<(), String> {
    let prefs = load_launch_preferences();
    let launch_state = state
        .launch_state
        .lock()
        .map_err(|_| "启动状态锁已损坏")?
        .clone();
    let options = launch_options_from_preferences(&prefs);
    let app_dir = codex_pilot_core::app_paths::resolve_codex_app_dir(options.app_dir.as_deref());
    let launcher = resolve_launcher_path();
    let provider = codex_pilot_core::relay_config::default_relay_provider_config();
    let status_path = codex_pilot_core::status::status_path();
    let config_path = codex_pilot_core::app_paths::codex_config_path();
    let auth_path = codex_pilot_core::app_paths::codex_auth_path();
    let state_db_path = codex_pilot_core::app_paths::codex_state_db_path();

    append_diagnostic_event(
        "diagnostics.snapshot",
        json!({
            "launch_state": crate::commands::launch::launch_state_label(&launch_state),
            "manager_config_path": manager_config_path().to_string_lossy(),
            "diagnostic_log_path": codex_pilot_core::diagnostic_log::log_path().to_string_lossy()
        }),
    )?;
    append_diagnostic_event(
        "diagnostics.launch",
        json!({
            "requested_app_path": prefs.app_path,
            "resolved_app_path": app_dir.as_ref().map(|path| path.to_string_lossy().to_string()),
            "debug_port": options.debug_port,
            "helper_port": options.helper_port,
            "helper_port_connectable": codex_pilot_core::ports::can_connect_loopback_port(options.helper_port),
            "launcher_path": launcher.as_ref().ok().map(|path| path.to_string_lossy().to_string()),
            "launcher_error": launcher.as_ref().err()
        }),
    )?;
    append_diagnostic_event(
        "diagnostics.provider",
        json!({
            "active_provider": if provider.active { provider.provider } else { "chatgpt".to_string() },
            "configured": provider.configured,
            "authenticated": provider.authenticated,
            "config_path": provider.config_path,
            "account_present": provider.account_label.is_some()
        }),
    )?;
    append_diagnostic_event(
        "diagnostics.files",
        json!({
            "status_path": status_path.to_string_lossy(),
            "status_exists": status_path.exists(),
            "config_path": config_path.to_string_lossy(),
            "config_exists": config_path.exists(),
            "auth_path": auth_path.to_string_lossy(),
            "auth_exists": auth_path.exists(),
            "state_db_path": state_db_path.to_string_lossy(),
            "state_db_exists": state_db_path.exists(),
            "provider_profiles_path": provider_profiles_path().to_string_lossy(),
            "provider_profiles_exists": provider_profiles_path().exists()
        }),
    )
}

pub(crate) fn append_diagnostic_event(
    event: &str,
    detail: serde_json::Value,
) -> Result<(), String> {
    codex_pilot_core::diagnostic_log::append(event, detail)
        .map_err(|error| format!("写入诊断日志失败：{error}"))
}

fn provider_sync_diagnostic_check() -> DiagnosticCheck {
    match codex_pilot_data::provider_sync::inspect_provider_sync_with_target(
        None,
        Some("CodexPilot"),
    ) {
        Ok(inspection) => {
            let pending =
                inspection.rollout_rewrite_needed + inspection.sqlite_provider_rows_needing_sync;
            let rollout = format_provider_counts(&inspection.rollout_providers);
            let sqlite = format_provider_counts(&inspection.sqlite_providers);
            DiagnosticCheck {
                name: "历史会话同步".to_string(),
                status: if pending == 0 { "ok" } else { "warning" }.to_string(),
                detail: format!(
                    "目标 {}。rollout {}/{} 需要同步；SQLite provider {}/{} 行需要同步，总更新项 {}。rollout 分布：{}。SQLite 分布：{}。",
                    inspection.target_provider,
                    inspection.rollout_rewrite_needed,
                    inspection.rollout_files,
                    inspection.sqlite_provider_rows_needing_sync,
                    inspection.sqlite_rows,
                    inspection.sqlite_total_updates_needed,
                    rollout,
                    sqlite
                ),
            }
        }
        Err(error) => DiagnosticCheck {
            name: "历史会话同步".to_string(),
            status: "warning".to_string(),
            detail: format!("检查失败：{error}"),
        },
    }
}
