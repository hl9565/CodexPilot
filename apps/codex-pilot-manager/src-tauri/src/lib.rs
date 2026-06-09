use serde::{Deserialize, Serialize};
use serde_json::json;
mod commands;
mod launch_settings;
pub(crate) use launch_settings::*;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

const CODEX_RUNNING_CACHE_TTL: Duration = Duration::from_secs(2);
const MANAGER_INJECT_TIMEOUT: Duration = Duration::from_secs(25);
const MANAGER_LAUNCH_TIMEOUT: Duration = Duration::from_secs(25);

struct ManagerState {
    launch_state: Mutex<LaunchState>,
    codex_process_cache: Mutex<CodexProcessCache>,
}

#[derive(Debug, Clone)]
enum LaunchState {
    Idle,
    Launching,
    Running,
    Failed(String),
}

#[derive(Debug, Default)]
struct CodexProcessCache {
    checked_at: Option<Instant>,
    codex_running: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchSnapshot {
    app_path: Option<String>,
    requested_app_path: String,
    debug_port: u16,
    helper_port: u16,
    auto_launch_on_open: bool,
    auto_sync_sessions_on_launch: bool,
    ready: bool,
    codex_installed: bool,
    state: String,
    action_kind: String,
    action_label: String,
    helper_reachable: bool,
    debug_reachable: bool,
    codex_running: bool,
    detail: String,
    command_preview: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticCheck {
    name: String,
    status: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsSnapshot {
    checks: Vec<DiagnosticCheck>,
    logs: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecycleBinSnapshot {
    entries: Vec<codex_pilot_data::storage::RecycleBinEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecycleBinTokensRequest {
    tokens: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecycleBinBatchFailure {
    token: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecycleBinBatchResponse {
    message: String,
    succeeded_tokens: Vec<String>,
    failed: Vec<RecycleBinBatchFailure>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionZipExportResult {
    zip_path: String,
    manifest: codex_pilot_data::session_zip::SessionZipManifest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionZipInspectResult {
    zip_path: String,
    manifest: codex_pilot_data::session_zip::SessionZipManifest,
    entries: codex_pilot_data::session_zip::SessionZipIncludes,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionZipImportResult {
    mode: String,
    manifest: codex_pilot_data::session_zip::SessionZipManifest,
    restored_session_files: usize,
    restored_archived_session_files: usize,
    restored_state_sqlite: bool,
    safety_backup_zip_path: Option<String>,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionZipImportRequest {
    zip_path: String,
    mode: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionZipExportRequest {
    zip_path: String,
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("CODEX_PILOT_LOG").unwrap_or_else(|_| "info,codex_pilot=debug".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .manage(ManagerState {
            launch_state: Mutex::new(LaunchState::Idle),
            codex_process_cache: Mutex::new(CodexProcessCache::default()),
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                hide_main_window_on_close(window, event);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_version,
            commands::app::backend_status,
            commands::launch::launch_snapshot,
            commands::launch::launch_codex,
            commands::launch::reinject_codex,
            commands::launch::restart_codex_and_inject,
            commands::app::save_launch_preferences,
            commands::app::enhancement_settings_snapshot,
            commands::app::save_enhancement_settings,
            commands::session_sync::provider_sync_snapshot,
            commands::session_sync::sync_provider_sessions,
            commands::sessions::recycle_bin_snapshot,
            commands::sessions::restore_recycle_bin_entries,
            commands::sessions::delete_recycle_bin_entries,
            commands::sessions::export_session_zip,
            commands::sessions::pick_session_zip_save_path,
            commands::sessions::pick_session_zip_file,
            commands::sessions::inspect_session_zip,
            commands::sessions::import_session_zip,
            commands::diagnostics::diagnostics_snapshot,
            commands::diagnostics::collect_diagnostics
        ])
        .build(tauri::generate_context!())
        .expect("error while building CodexPilot Manager");

    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main_window(handle);
        }
    });
}

fn hide_main_window_on_close<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    event: &tauri::WindowEvent,
) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}

fn show_main_window<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn format_provider_counts(counts: &[codex_pilot_data::provider_sync::ProviderCount]) -> String {
    if counts.is_empty() {
        return "无".to_string();
    }
    counts
        .iter()
        .map(|item| format!("{} {}", item.provider, item.count))
        .collect::<Vec<_>>()
        .join("，")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_preferences_round_trip() {
        let root = unique_temp_dir("prefs-round-trip");
        std::fs::create_dir_all(&root).unwrap();
        let app_dir = root.join("Codex.app");
        std::fs::create_dir_all(&app_dir).unwrap();
        let path = root.join("manager-launch.json");

        let prefs = sanitize_launch_preferences(LaunchPreferences {
            app_path: app_dir.to_string_lossy().to_string(),
            debug_port: 9444,
            helper_port: 58444,
            auto_launch_on_open: true,
            auto_sync_sessions_on_launch: true,
        })
        .unwrap();
        save_launch_preferences_to_path(&path, &prefs).unwrap();

        let loaded = load_launch_preferences_from_path(&path).unwrap();
        assert_eq!(loaded.app_path, app_dir.to_string_lossy());
        assert_eq!(loaded.debug_port, 9444);
        assert_eq!(loaded.helper_port, 58444);
        assert!(loaded.auto_launch_on_open);
        assert!(loaded.auto_sync_sessions_on_launch);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn launch_preferences_reject_duplicate_ports() {
        let result = sanitize_launch_preferences(LaunchPreferences {
            app_path: String::new(),
            debug_port: 9444,
            helper_port: 9444,
            auto_launch_on_open: false,
            auto_sync_sessions_on_launch: false,
        });

        assert!(result.unwrap_err().contains("不能相同"));
    }

    #[test]
    fn launch_preferences_migrate_old_default_ports() {
        let prefs = sanitize_launch_preferences(LaunchPreferences {
            app_path: String::new(),
            debug_port: 9333,
            helper_port: 57321,
            auto_launch_on_open: false,
            auto_sync_sessions_on_launch: false,
        })
        .unwrap();

        assert_eq!(
            prefs.debug_port,
            codex_pilot_core::ports::DEFAULT_DEBUG_PORT
        );
        assert_eq!(
            prefs.helper_port,
            codex_pilot_core::ports::DEFAULT_HELPER_PORT
        );
    }

    #[test]
    fn launch_preferences_keep_custom_ports() {
        let prefs = sanitize_launch_preferences(LaunchPreferences {
            app_path: String::new(),
            debug_port: 9444,
            helper_port: 58888,
            auto_launch_on_open: false,
            auto_sync_sessions_on_launch: false,
        })
        .unwrap();

        assert_eq!(prefs.debug_port, 9444);
        assert_eq!(prefs.helper_port, 58888);
    }

    #[test]
    fn enhancement_settings_round_trip() {
        let root = unique_temp_dir("enhancement-settings");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("enhancement-settings.json");
        let settings = EnhancementSettings {
            enabled: false,
            timeline: true,
            inline_actions: false,
            scroll_restore: true,
            plugin_entry_unlock: false,
            force_plugin_install: true,
            fast_global_mode: false,
        };

        save_enhancement_settings_to_path(&path, &settings).unwrap();
        let loaded = load_enhancement_settings_from_path(&path).unwrap();

        assert_eq!(loaded, settings);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codex-pilot-manager-{name}-{}", now_nanos()))
    }
}

#[cfg(test)]
fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_session_zip_import_mode(
    value: &str,
) -> Result<codex_pilot_data::session_zip::SessionZipImportMode, String> {
    match value {
        "merge" => Ok(codex_pilot_data::session_zip::SessionZipImportMode::Merge),
        "overwrite" => Ok(codex_pilot_data::session_zip::SessionZipImportMode::Overwrite),
        _ => Err(format!("不支持的导入模式：{value}")),
    }
}
