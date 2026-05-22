use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tauri::Manager;

struct ManagerState {
    launch_state: Mutex<LaunchState>,
}

#[derive(Debug, Clone)]
enum LaunchState {
    Idle,
    Launching,
    Running,
    Failed(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchSnapshot {
    app_path: Option<String>,
    requested_app_path: String,
    debug_port: u16,
    helper_port: u16,
    auto_launch_on_open: bool,
    ready: bool,
    state: String,
    action_kind: String,
    action_label: String,
    helper_reachable: bool,
    debug_reachable: bool,
    codex_running: bool,
    detail: String,
    command_preview: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchPreferences {
    app_path: String,
    debug_port: u16,
    helper_port: u16,
    #[serde(default)]
    auto_launch_on_open: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct EnhancementSettings {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_true")]
    timeline: bool,
    #[serde(default = "default_true")]
    inline_actions: bool,
    #[serde(default = "default_true")]
    scroll_restore: bool,
}

impl Default for EnhancementSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            timeline: true,
            inline_actions: true,
            scroll_restore: true,
        }
    }
}

fn default_true() -> bool {
    true
}

impl Default for LaunchPreferences {
    fn default() -> Self {
        let options = codex_pilot_core::launcher::LaunchOptions::default();
        Self {
            app_path: String::new(),
            debug_port: options.debug_port,
            helper_port: options.helper_port,
            auto_launch_on_open: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSnapshot {
    active_provider: String,
    mode: String,
    profile: String,
    source: String,
    auth_path: String,
    configured: bool,
    authenticated: bool,
    account_label: Option<String>,
    profiles: Vec<ProviderProfile>,
    active_profile_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSyncSnapshot {
    target_provider: String,
    current_provider: String,
    available_providers: Vec<String>,
    rollout_files: usize,
    rollout_rewrite_needed: usize,
    sqlite_rows: usize,
    sqlite_provider_rows_needing_sync: usize,
    sqlite_total_updates_needed: usize,
    rollout_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
    sqlite_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
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
struct ProviderApplyRequest {
    profile_id: Option<String>,
    mode: Option<ProviderProfileMode>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ProviderProfileMode {
    HybridApi,
    Api,
}

impl ProviderProfileMode {
    fn label(self) -> &'static str {
        match self {
            ProviderProfileMode::HybridApi => "混合中转",
            ProviderProfileMode::Api => "无账号",
        }
    }
}

fn default_provider_profile_mode() -> ProviderProfileMode {
    ProviderProfileMode::HybridApi
}

fn default_upstream_protocol() -> codex_pilot_core::protocol_proxy::UpstreamProtocol {
    codex_pilot_core::protocol_proxy::UpstreamProtocol::Responses
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ProviderProfile {
    id: String,
    name: String,
    base_url: String,
    bearer_token: String,
    #[serde(default = "default_provider_profile_mode")]
    mode: ProviderProfileMode,
    #[serde(default = "default_upstream_protocol")]
    upstream_protocol: codex_pilot_core::protocol_proxy::UpstreamProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfilesState {
    active_profile_id: String,
    profiles: Vec<ProviderProfile>,
}

impl Default for ProviderProfilesState {
    fn default() -> Self {
        Self {
            active_profile_id: "default".to_string(),
            profiles: vec![ProviderProfile {
                id: "default".to_string(),
                name: "默认中转".to_string(),
                base_url: String::new(),
                bearer_token: String::new(),
                mode: ProviderProfileMode::HybridApi,
                upstream_protocol: default_upstream_protocol(),
            }],
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileSaveRequest {
    id: Option<String>,
    name: String,
    base_url: String,
    bearer_token: String,
    mode: ProviderProfileMode,
    #[serde(default = "default_upstream_protocol")]
    upstream_protocol: codex_pilot_core::protocol_proxy::UpstreamProtocol,
    activate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileSaveResponse {
    id: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileIdRequest {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSyncRequest {
    target_provider: Option<String>,
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

#[tauri::command]
fn backend_status() -> Result<Option<codex_pilot_core::status::BackendStatus>, String> {
    codex_pilot_core::status::read_status().map_err(|error| error.to_string())
}

#[tauri::command]
fn app_version() -> String {
    codex_pilot_core::version::VERSION.to_string()
}

#[tauri::command]
fn launch_snapshot(state: tauri::State<'_, ManagerState>) -> Result<LaunchSnapshot, String> {
    let current = state
        .launch_state
        .lock()
        .map_err(|_| "启动状态锁已损坏")?
        .clone();
    launch_snapshot_with_state(current)
}

fn launch_snapshot_with_state(state: LaunchState) -> Result<LaunchSnapshot, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    let app_dir = codex_pilot_core::app_paths::resolve_codex_app_dir(options.app_dir.as_deref());
    let command_preview = app_dir
        .as_deref()
        .map(|path| build_codex_command_preview(path, options.debug_port))
        .unwrap_or_else(Vec::new);

    let manager_running = matches!(state, LaunchState::Running);
    Ok(LaunchSnapshot {
        app_path: app_dir.map(|path| path.to_string_lossy().to_string()),
        requested_app_path: prefs.app_path,
        debug_port: options.debug_port,
        helper_port: options.helper_port,
        auto_launch_on_open: prefs.auto_launch_on_open,
        ready: !command_preview.is_empty(),
        state: launch_state_label(&state),
        action_kind: launch_action_kind(!command_preview.is_empty(), manager_running, &options),
        action_label: launch_action_label(!command_preview.is_empty(), manager_running, &options),
        helper_reachable: codex_pilot_core::ports::can_connect_loopback_port(options.helper_port),
        debug_reachable: codex_pilot_core::ports::can_connect_loopback_port(options.debug_port),
        codex_running: is_codex_process_running(),
        detail: launch_action_detail(!command_preview.is_empty(), manager_running, &options),
        command_preview,
    })
}

#[tauri::command]
async fn launch_codex(state: tauri::State<'_, ManagerState>) -> Result<String, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    if codex_pilot_core::ports::can_connect_loopback_port(options.helper_port) {
        append_diagnostic_event(
            "manager.launch_helper_already_running",
            serde_json::json!({
                "debug_port": options.debug_port,
                "helper_port": options.helper_port,
                "debug_port_connectable": codex_pilot_core::ports::can_connect_loopback_port(options.debug_port)
            }),
        )?;
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        *current = LaunchState::Running;
        return Ok("CodexPilot 已在运行中。".to_string());
    }
    if codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        return inject_existing_codex(&state, options.debug_port, options.helper_port).await;
    }
    if is_codex_process_running() {
        return Err(
            "当前 Codex 不是通过 CodexPilot 启动，无法直接注入。请确认后使用“重启并注入”。"
                .to_string(),
        );
    }
    spawn_launcher(&state, &prefs)
}

#[tauri::command]
async fn reinject_codex(state: tauri::State<'_, ManagerState>) -> Result<String, String> {
    let prefs = load_launch_preferences();
    let options = launch_options_from_preferences(&prefs);
    if !codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        return Err("未检测到 Codex 调试端口，无法重新注入。".to_string());
    }
    inject_existing_codex(&state, options.debug_port, options.helper_port).await
}

#[tauri::command]
async fn restart_codex_and_inject(state: tauri::State<'_, ManagerState>) -> Result<String, String> {
    request_codex_quit()?;
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let prefs = load_launch_preferences();
    spawn_launcher(&state, &prefs)
}

async fn inject_existing_codex(
    state: &tauri::State<'_, ManagerState>,
    debug_port: u16,
    helper_port: u16,
) -> Result<String, String> {
    {
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        *current = LaunchState::Launching;
    }
    match codex_pilot_core::launcher::inject_running_codex(debug_port, helper_port).await {
        Ok(()) => {
            codex_pilot_core::status::write_status(&codex_pilot_core::status::BackendStatus {
                status: "running".to_string(),
                version: codex_pilot_core::version::VERSION.to_string(),
            })
            .map_err(|error| format!("写入状态失败：{error}"))?;
            let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
            *current = LaunchState::Running;
            Ok("已重新注入 CodexPilot。".to_string())
        }
        Err(error) => {
            let message = format!("重新注入失败：{error}");
            if let Ok(mut current) = state.launch_state.lock() {
                *current = LaunchState::Failed(message.clone());
            }
            Err(message)
        }
    }
}

fn spawn_launcher(
    state: &tauri::State<'_, ManagerState>,
    prefs: &LaunchPreferences,
) -> Result<String, String> {
    {
        let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
        if matches!(*current, LaunchState::Launching | LaunchState::Running) {
            if codex_pilot_core::ports::can_connect_loopback_port(prefs.helper_port) {
                return Ok("CodexPilot 已在启动或运行中。".to_string());
            }
            *current = LaunchState::Idle;
        }
        *current = LaunchState::Launching;
    }

    let launcher = match resolve_launcher_path() {
        Ok(path) => path,
        Err(message) => {
            if let Ok(mut current) = state.launch_state.lock() {
                *current = LaunchState::Failed(message.clone());
            }
            return Err(message);
        }
    };
    let mut command = std::process::Command::new(&launcher);
    append_launcher_args(&mut command, &prefs);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if let Err(error) = command.spawn() {
        let message = format!("启动 CodexPilot 失败：{error}");
        if let Ok(mut current) = state.launch_state.lock() {
            *current = LaunchState::Failed(message.clone());
        }
        return Err(message);
    }

    let mut current = state.launch_state.lock().map_err(|_| "启动状态锁已损坏")?;
    *current = LaunchState::Running;
    Ok("已启动 CodexPilot。".to_string())
}

#[tauri::command]
fn save_launch_preferences(request: LaunchPreferences) -> Result<String, String> {
    let prefs = sanitize_launch_preferences(request)?;
    save_launch_preferences_to_path(&manager_config_path(), &prefs)?;
    Ok("启动偏好已保存。".to_string())
}

#[tauri::command]
fn enhancement_settings_snapshot() -> EnhancementSettings {
    load_enhancement_settings()
}

#[tauri::command]
fn save_enhancement_settings(request: EnhancementSettings) -> Result<String, String> {
    let settings = sanitize_enhancement_settings(request);
    save_enhancement_settings_to_path(&enhancement_settings_path(), &settings)?;
    Ok("页面增强设置已保存，重新注入后生效。".to_string())
}

#[tauri::command]
fn provider_snapshot() -> ProviderSnapshot {
    let provider = codex_pilot_core::relay_config::default_relay_provider_config();
    let profiles = load_provider_profiles();
    let active_profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.id == profiles.active_profile_id)
        .or_else(|| profiles.profiles.first());
    ProviderSnapshot {
        active_provider: if provider.active {
            provider.provider
        } else {
            "chatgpt".to_string()
        },
        mode: if provider.active {
            provider.mode
        } else {
            "official".to_string()
        },
        profile: active_profile
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "默认中转".to_string()),
        source: provider.config_path,
        auth_path: codex_pilot_core::app_paths::codex_auth_path()
            .to_string_lossy()
            .to_string(),
        configured: provider.configured,
        authenticated: provider.authenticated,
        account_label: provider.account_label,
        profiles: profiles.profiles,
        active_profile_id: profiles.active_profile_id,
    }
}

#[tauri::command]
async fn apply_provider(request: ProviderApplyRequest) -> Result<String, String> {
    let profiles = load_provider_profiles();
    let profile = profile_by_id(&profiles, request.profile_id.as_deref())?;
    let base_url = profile.base_url;
    let bearer_token = profile.bearer_token;
    let mode = request.mode.unwrap_or(profile.mode);
    let upstream_protocol = profile.upstream_protocol;
    if upstream_protocol == codex_pilot_core::protocol_proxy::UpstreamProtocol::AnthropicMessages {
        return Err("Anthropic Messages 适配尚未实现，当前不能应用该配置。".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let result = match mode {
            ProviderProfileMode::HybridApi => {
                codex_pilot_core::relay_config::apply_relay_provider_config_with_protocol(
                    &base_url,
                    &bearer_token,
                    upstream_protocol,
                )
                .map_err(|error| format!("应用混合中转失败：{error}"))?
            }
            ProviderProfileMode::Api => {
                codex_pilot_core::relay_config::apply_api_provider_config_with_protocol(
                    &base_url,
                    &bearer_token,
                    upstream_protocol,
                )
                .map_err(|error| format!("应用无账号通道失败：{error}"))?
            }
        };
        Ok(result
            .backup_path
            .map(|path| format!("{} 已应用，备份：{path}。", mode.label()))
            .unwrap_or_else(|| format!("{} 已应用。", mode.label())))
    })
    .await
    .map_err(|error| format!("应用运行模式任务失败：{error}"))?
}

#[tauri::command]
fn save_provider_profile(
    request: ProviderProfileSaveRequest,
) -> Result<ProviderProfileSaveResponse, String> {
    let mut state = load_provider_profiles();
    let activate = request.activate;
    let profile = sanitize_provider_profile(request)?;
    let normalized_name = profile.name.trim();
    if state.profiles.iter().any(|item| {
        item.id != profile.id && item.name.trim().eq_ignore_ascii_case(normalized_name)
    }) {
        return Err("配置档名称不能重复。".to_string());
    }
    let id = profile.id.clone();
    if let Some(existing) = state.profiles.iter_mut().find(|item| item.id == id) {
        *existing = profile;
    } else {
        state.profiles.push(profile);
    }
    if activate
        || state.active_profile_id.is_empty()
        || state.active_profile_id == id
        || state.profiles.len() == 1
    {
        state.active_profile_id = id.clone();
    }
    save_provider_profiles_to_path(&provider_profiles_path(), &state)?;
    Ok(ProviderProfileSaveResponse {
        id,
        message: "中转配置档已保存。".to_string(),
    })
}

#[tauri::command]
fn activate_provider_profile(request: ProviderProfileIdRequest) -> Result<String, String> {
    let mut state = load_provider_profiles();
    if !state
        .profiles
        .iter()
        .any(|profile| profile.id == request.id)
    {
        return Err("中转配置档不存在。".to_string());
    }
    state.active_profile_id = request.id;
    save_provider_profiles_to_path(&provider_profiles_path(), &state)?;
    Ok("已切换中转配置档。".to_string())
}

#[tauri::command]
fn delete_provider_profile(request: ProviderProfileIdRequest) -> Result<String, String> {
    let mut state = load_provider_profiles();
    if state.profiles.len() <= 1 {
        return Err("至少保留一个中转配置档。".to_string());
    }
    let before = state.profiles.len();
    state.profiles.retain(|profile| profile.id != request.id);
    if state.profiles.len() == before {
        return Err("中转配置档不存在。".to_string());
    }
    if state.active_profile_id == request.id {
        state.active_profile_id = state
            .profiles
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| "default".to_string());
    }
    save_provider_profiles_to_path(&provider_profiles_path(), &state)?;
    Ok("中转配置档已删除。".to_string())
}

#[tauri::command]
fn clear_provider() -> Result<String, String> {
    let result = codex_pilot_core::relay_config::clear_relay_provider_config()
        .map_err(|error| format!("清除中转失败：{error}"))?;
    Ok(result
        .backup_path
        .map(|path| format!("中转已清除，备份：{path}"))
        .unwrap_or_else(|| "中转已清除。".to_string()))
}

#[tauri::command]
async fn provider_sync_snapshot(
    request: Option<ProviderSyncRequest>,
) -> Result<ProviderSyncSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let current = codex_pilot_core::relay_config::default_relay_provider_config();
        let current_provider = if current.active {
            current.provider
        } else {
            "openai".to_string()
        };
        let target_provider = sanitize_provider_sync_target(
            request
                .and_then(|item| item.target_provider)
                .unwrap_or_else(|| "CodexPilot".to_string()),
        )?;
        let inspection = codex_pilot_data::provider_sync::inspect_provider_sync_with_target(
            None,
            Some(&target_provider),
        )
        .map_err(|error| format!("检查历史会话同步失败：{error}"))?;
        let mut available = vec!["CodexPilot".to_string(), current_provider.clone()];
        available.extend(
            inspection
                .rollout_providers
                .iter()
                .chain(inspection.sqlite_providers.iter())
                .map(|item| item.provider.clone())
                .filter(|item| !item.trim().is_empty()),
        );
        available.sort();
        available.dedup();
        Ok(ProviderSyncSnapshot {
            target_provider: inspection.target_provider,
            current_provider,
            available_providers: available,
            rollout_files: inspection.rollout_files,
            rollout_rewrite_needed: inspection.rollout_rewrite_needed,
            sqlite_rows: inspection.sqlite_rows,
            sqlite_provider_rows_needing_sync: inspection.sqlite_provider_rows_needing_sync,
            sqlite_total_updates_needed: inspection.sqlite_total_updates_needed,
            rollout_providers: inspection.rollout_providers,
            sqlite_providers: inspection.sqlite_providers,
        })
    })
    .await
    .map_err(|error| format!("检查历史会话同步任务失败：{error}"))?
}

#[tauri::command]
async fn sync_provider_sessions(request: ProviderSyncRequest) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target_provider = sanitize_provider_sync_target(
            request
                .target_provider
                .unwrap_or_else(|| "CodexPilot".to_string()),
        )?;
        Ok(provider_sync_message(
            codex_pilot_data::provider_sync::run_provider_sync_with_target(
                None,
                Some(&target_provider),
            ),
        ))
    })
    .await
    .map_err(|error| format!("同步历史会话任务失败：{error}"))?
}

#[tauri::command]
async fn recycle_bin_snapshot() -> Result<RecycleBinSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        adapter
            .list_undo_backups()
            .map(|entries| RecycleBinSnapshot { entries })
            .map_err(|error| format!("读取回收站失败：{error}"))
    })
    .await
    .map_err(|error| format!("读取回收站任务失败：{error}"))?
}

#[tauri::command]
async fn restore_recycle_bin_entries(
    request: RecycleBinTokensRequest,
) -> Result<RecycleBinBatchResponse, String> {
    let tokens = sanitized_recycle_tokens(request.tokens)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        let mut succeeded_tokens = Vec::new();
        let mut failed = Vec::new();
        for token in tokens {
            match adapter.undo(&token) {
                Ok(result) if result.status == codex_pilot_data::storage::DeleteStatus::Undone => {
                    succeeded_tokens.push(token);
                }
                Ok(result) => failed.push(RecycleBinBatchFailure {
                    token,
                    message: format!("{}：{}", result.session_id, result.message),
                }),
                Err(error) => failed.push(RecycleBinBatchFailure {
                    token: token.clone(),
                    message: format!("{token}：{error}"),
                }),
            }
        }
        let message = if failed.is_empty() {
            format!("已恢复 {} 条会话。", succeeded_tokens.len())
        } else {
            format!(
                "已恢复 {restored} 条，会有 {} 条失败：{}",
                failed.len(),
                failed
                    .iter()
                    .map(|item| item.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；"),
                restored = succeeded_tokens.len()
            )
        };
        Ok(RecycleBinBatchResponse {
            message,
            succeeded_tokens,
            failed,
        })
    })
    .await
    .map_err(|error| format!("恢复回收站任务失败：{error}"))?
}

#[tauri::command]
async fn delete_recycle_bin_entries(
    request: RecycleBinTokensRequest,
) -> Result<RecycleBinBatchResponse, String> {
    let tokens = sanitized_recycle_tokens(request.tokens)?;
    tauri::async_runtime::spawn_blocking(move || {
        let adapter = codex_pilot_data::storage::SQLiteStorageAdapter::new(
            codex_pilot_core::app_paths::codex_state_db_path(),
        );
        let mut succeeded_tokens = Vec::new();
        let mut failed = Vec::new();
        for token in tokens {
            match adapter.delete_undo_backup(&token) {
                Ok(result) if result.status == codex_pilot_data::storage::DeleteStatus::Deleted => {
                    succeeded_tokens.push(token);
                }
                Ok(result) => failed.push(RecycleBinBatchFailure {
                    token,
                    message: format!("{}：{}", result.session_id, result.message),
                }),
                Err(error) => failed.push(RecycleBinBatchFailure {
                    token: token.clone(),
                    message: format!("{token}：{error}"),
                }),
            }
        }
        let message = if failed.is_empty() {
            format!("已永久删除 {} 条回收站记录。", succeeded_tokens.len())
        } else {
            format!(
                "已永久删除 {deleted} 条，会有 {} 条失败：{}",
                failed.len(),
                failed
                    .iter()
                    .map(|item| item.message.as_str())
                    .collect::<Vec<_>>()
                    .join("；"),
                deleted = succeeded_tokens.len()
            )
        };
        Ok(RecycleBinBatchResponse {
            message,
            succeeded_tokens,
            failed,
        })
    })
    .await
    .map_err(|error| format!("永久删除回收站记录任务失败：{error}"))?
}

#[tauri::command]
async fn diagnostics_snapshot() -> Result<DiagnosticsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let status_path = codex_pilot_core::status::status_path();
        let provider_sync_check = provider_sync_diagnostic_check();
        DiagnosticsSnapshot {
            checks: vec![
                DiagnosticCheck {
                    name: "后端状态文件".to_string(),
                    status: if status_path.exists() {
                        "ok"
                    } else {
                        "missing"
                    }
                    .to_string(),
                    detail: status_path.to_string_lossy().to_string(),
                },
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
fn collect_diagnostics(state: tauri::State<'_, ManagerState>) -> Result<String, String> {
    append_diagnostic_snapshot(&state)?;
    Ok("诊断快照已写入日志。".to_string())
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(ManagerState {
            launch_state: Mutex::new(LaunchState::Idle),
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                hide_main_window_on_close(window, event);
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_version,
            backend_status,
            launch_snapshot,
            launch_codex,
            reinject_codex,
            restart_codex_and_inject,
            save_launch_preferences,
            enhancement_settings_snapshot,
            save_enhancement_settings,
            provider_snapshot,
            apply_provider,
            save_provider_profile,
            activate_provider_profile,
            delete_provider_profile,
            clear_provider,
            provider_sync_snapshot,
            sync_provider_sessions,
            recycle_bin_snapshot,
            restore_recycle_bin_entries,
            delete_recycle_bin_entries,
            diagnostics_snapshot,
            collect_diagnostics
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

fn launch_state_label(state: &LaunchState) -> String {
    match state {
        LaunchState::Idle => "空闲".to_string(),
        LaunchState::Launching => "启动中".to_string(),
        LaunchState::Running => "运行中".to_string(),
        LaunchState::Failed(message) => format!("失败：{message}"),
    }
}

fn launch_action_kind(
    ready: bool,
    manager_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
) -> String {
    if manager_running || codex_pilot_core::ports::can_connect_loopback_port(options.helper_port) {
        "running".to_string()
    } else if codex_pilot_core::ports::can_connect_loopback_port(options.debug_port) {
        "reinject".to_string()
    } else if is_codex_process_running() {
        "restart".to_string()
    } else if ready {
        "launch".to_string()
    } else {
        "unavailable".to_string()
    }
}

fn launch_action_label(
    ready: bool,
    manager_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
) -> String {
    match launch_action_kind(ready, manager_running, options).as_str() {
        "running" => "已运行".to_string(),
        "reinject" => "重新注入".to_string(),
        "restart" => "重启并注入".to_string(),
        "launch" => "启动 Codex".to_string(),
        _ => "不可启动".to_string(),
    }
}

fn launch_action_detail(
    ready: bool,
    manager_running: bool,
    options: &codex_pilot_core::launcher::LaunchOptions,
) -> String {
    match launch_action_kind(ready, manager_running, options).as_str() {
        "running" => "CodexPilot 后端已连接，无需重复启动。".to_string(),
        "reinject" => "检测到 Codex 调试端口，可以直接重新注入。".to_string(),
        "restart" => "检测到 Codex 已运行，但没有调试端口；需要确认后重启。".to_string(),
        "launch" => "未检测到运行中的 Codex，可以从 CodexPilot 启动并注入。".to_string(),
        _ => "需要检查 Codex 应用路径或启动偏好。".to_string(),
    }
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

fn request_codex_quit() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("osascript")
            .args(["-e", r#"tell application "Codex" to quit"#])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
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

fn sanitized_recycle_tokens(tokens: Vec<String>) -> Result<Vec<String>, String> {
    let mut sanitized = Vec::new();
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if token.contains('/') || token.contains('\\') || token.contains("..") {
            return Err("回收站记录标识无效。".to_string());
        }
        if !sanitized.iter().any(|item| item == token) {
            sanitized.push(token.to_string());
        }
    }
    if sanitized.is_empty() {
        return Err("请选择回收站记录。".to_string());
    }
    Ok(sanitized)
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
            "launch_state": launch_state_label(&launch_state),
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

fn append_diagnostic_event(event: &str, detail: serde_json::Value) -> Result<(), String> {
    codex_pilot_core::diagnostic_log::append(event, detail)
        .map_err(|error| format!("写入诊断日志失败：{error}"))
}

fn provider_sync_message(sync: codex_pilot_data::provider_sync::ProviderSyncResult) -> String {
    format!(
        "Provider Sync：{}，目标 {}，会话文件 {} 个，数据库行 {} 条。",
        sync.message, sync.target_provider, sync.changed_session_files, sync.sqlite_rows_updated
    )
}

fn sanitize_provider_sync_target(value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("同步目标 Provider 不能为空。".to_string());
    }
    if trimmed.len() > 80 {
        return Err("同步目标 Provider 过长。".to_string());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err("同步目标 Provider 只能包含字母、数字、下划线、中划线或点。".to_string());
    }
    Ok(trimmed.to_string())
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

fn resolve_launcher_path() -> Result<std::path::PathBuf, String> {
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

fn manager_config_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("manager-launch.json")
}

fn provider_profiles_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("provider-profiles.json")
}

fn enhancement_settings_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("enhancement-settings.json")
}

fn load_provider_profiles() -> ProviderProfilesState {
    load_provider_profiles_from_path(&provider_profiles_path()).unwrap_or_default()
}

fn load_provider_profiles_from_path(path: &Path) -> Result<ProviderProfilesState, String> {
    if !path.exists() {
        return Ok(ProviderProfilesState::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取中转配置档失败：{error}"))?;
    let state = serde_json::from_str::<ProviderProfilesState>(&contents)
        .map_err(|error| format!("解析中转配置档失败：{error}"))?;
    sanitize_provider_profiles_state(state)
}

fn save_provider_profiles_to_path(
    path: &Path,
    state: &ProviderProfilesState,
) -> Result<(), String> {
    let state = sanitize_provider_profiles_state(state.clone())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(&state)
        .map_err(|error| format!("序列化中转配置档失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入中转配置档失败：{error}"))
}

fn profile_by_id(
    state: &ProviderProfilesState,
    id: Option<&str>,
) -> Result<ProviderProfile, String> {
    let id = id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&state.active_profile_id);
    state
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .or_else(|| state.profiles.first())
        .cloned()
        .ok_or_else(|| "没有可用的中转配置档。".to_string())
}

fn sanitize_provider_profile(
    request: ProviderProfileSaveRequest,
) -> Result<ProviderProfile, String> {
    let id = request
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("profile-{}", now_nanos()));
    let name = request.name.trim().to_string();
    let base_url = request.base_url.trim().to_string();
    let bearer_token = request.bearer_token.trim().to_string();
    let mode = request.mode;
    let upstream_protocol = request.upstream_protocol;
    if name.is_empty() {
        return Err("配置档名称不能为空。".to_string());
    }
    if base_url.is_empty() {
        return Err("Base URL 不能为空。".to_string());
    }
    if bearer_token.is_empty() {
        return Err("API Key 不能为空。".to_string());
    }
    Ok(ProviderProfile {
        id,
        name,
        base_url,
        bearer_token,
        mode,
        upstream_protocol,
    })
}

fn sanitize_provider_profiles_state(
    mut state: ProviderProfilesState,
) -> Result<ProviderProfilesState, String> {
    state.profiles = state
        .profiles
        .into_iter()
        .map(|profile| ProviderProfile {
            id: profile.id.trim().to_string(),
            name: profile.name.trim().to_string(),
            base_url: profile.base_url.trim().to_string(),
            bearer_token: profile.bearer_token.trim().to_string(),
            mode: profile.mode,
            upstream_protocol: profile.upstream_protocol,
        })
        .filter(|profile| !profile.id.is_empty() && !profile.name.is_empty())
        .collect();
    if state.profiles.is_empty() {
        state = ProviderProfilesState::default();
    }
    if !state
        .profiles
        .iter()
        .any(|profile| profile.id == state.active_profile_id)
    {
        state.active_profile_id = state.profiles[0].id.clone();
    }
    Ok(state)
}

fn load_launch_preferences() -> LaunchPreferences {
    load_launch_preferences_from_path(&manager_config_path()).unwrap_or_default()
}

fn load_enhancement_settings() -> EnhancementSettings {
    load_enhancement_settings_from_path(&enhancement_settings_path()).unwrap_or_default()
}

fn load_enhancement_settings_from_path(path: &Path) -> Result<EnhancementSettings, String> {
    if !path.exists() {
        return Ok(EnhancementSettings::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取页面增强设置失败：{error}"))?;
    let settings = serde_json::from_str::<EnhancementSettings>(&contents)
        .map_err(|error| format!("解析页面增强设置失败：{error}"))?;
    Ok(sanitize_enhancement_settings(settings))
}

fn save_enhancement_settings_to_path(
    path: &Path,
    settings: &EnhancementSettings,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(&sanitize_enhancement_settings(settings.clone()))
        .map_err(|error| format!("序列化页面增强设置失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入页面增强设置失败：{error}"))
}

fn sanitize_enhancement_settings(settings: EnhancementSettings) -> EnhancementSettings {
    settings
}

fn load_launch_preferences_from_path(path: &Path) -> Result<LaunchPreferences, String> {
    if !path.exists() {
        return Ok(LaunchPreferences::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取启动偏好失败：{error}"))?;
    let prefs = serde_json::from_str::<LaunchPreferences>(&contents)
        .map_err(|error| format!("解析启动偏好失败：{error}"))?;
    sanitize_launch_preferences(prefs)
}

fn save_launch_preferences_to_path(path: &Path, prefs: &LaunchPreferences) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(prefs)
        .map_err(|error| format!("序列化启动偏好失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入启动偏好失败：{error}"))
}

fn sanitize_launch_preferences(mut prefs: LaunchPreferences) -> Result<LaunchPreferences, String> {
    prefs.app_path = prefs.app_path.trim().to_string();
    migrate_old_default_ports(&mut prefs);
    validate_port("调试端口", prefs.debug_port)?;
    validate_port("后端端口", prefs.helper_port)?;
    if prefs.debug_port == prefs.helper_port {
        return Err("调试端口和后端端口不能相同。".to_string());
    }
    if !prefs.app_path.is_empty() {
        let path = PathBuf::from(&prefs.app_path);
        if !path.exists() {
            return Err("Codex 应用路径不存在。".to_string());
        }
    }
    Ok(prefs)
}

fn migrate_old_default_ports(prefs: &mut LaunchPreferences) {
    const OLD_DEBUG_PORT: u16 = 9333;
    const OLD_HELPER_PORT: u16 = 57321;
    if prefs.debug_port == OLD_DEBUG_PORT && prefs.helper_port == OLD_HELPER_PORT {
        prefs.debug_port = codex_pilot_core::ports::DEFAULT_DEBUG_PORT;
        prefs.helper_port = codex_pilot_core::ports::DEFAULT_HELPER_PORT;
    }
}

fn validate_port(label: &str, port: u16) -> Result<(), String> {
    if port == 0 {
        Err(format!("{label}不能为 0。"))
    } else {
        Ok(())
    }
}

fn launch_options_from_preferences(
    prefs: &LaunchPreferences,
) -> codex_pilot_core::launcher::LaunchOptions {
    codex_pilot_core::launcher::LaunchOptions {
        app_dir: if prefs.app_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&prefs.app_path))
        },
        debug_port: prefs.debug_port,
        helper_port: prefs.helper_port,
    }
}

fn build_codex_command_preview(app_dir: &Path, debug_port: u16) -> Vec<String> {
    if app_dir.extension().and_then(|value| value.to_str()) == Some("app") {
        codex_pilot_core::launcher::build_macos_open_command(app_dir, debug_port)
    } else {
        codex_pilot_core::launcher::build_codex_command(app_dir, debug_port)
    }
}

fn append_launcher_args(command: &mut std::process::Command, prefs: &LaunchPreferences) {
    if !prefs.app_path.is_empty() {
        command.arg("--app-path").arg(&prefs.app_path);
    }
    command
        .arg("--debug-port")
        .arg(prefs.debug_port.to_string())
        .arg("--helper-port")
        .arg(prefs.helper_port.to_string());
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
        })
        .unwrap();
        save_launch_preferences_to_path(&path, &prefs).unwrap();

        let loaded = load_launch_preferences_from_path(&path).unwrap();
        assert_eq!(loaded.app_path, app_dir.to_string_lossy());
        assert_eq!(loaded.debug_port, 9444);
        assert_eq!(loaded.helper_port, 58444);
        assert!(loaded.auto_launch_on_open);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn launch_preferences_reject_duplicate_ports() {
        let result = sanitize_launch_preferences(LaunchPreferences {
            app_path: String::new(),
            debug_port: 9444,
            helper_port: 9444,
            auto_launch_on_open: false,
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
        };

        save_enhancement_settings_to_path(&path, &settings).unwrap();
        let loaded = load_enhancement_settings_from_path(&path).unwrap();

        assert_eq!(loaded, settings);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_profiles_round_trip_and_active_selection() {
        let root = unique_temp_dir("provider-profiles");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("provider-profiles.json");
        let state = ProviderProfilesState {
            active_profile_id: "p2".to_string(),
            profiles: vec![
                ProviderProfile {
                    id: "p1".to_string(),
                    name: "配置一".to_string(),
                    base_url: "https://one.example/v1".to_string(),
                    bearer_token: "sk-one".to_string(),
                    mode: ProviderProfileMode::HybridApi,
                },
                ProviderProfile {
                    id: "p2".to_string(),
                    name: "配置二".to_string(),
                    base_url: "https://two.example/v1".to_string(),
                    bearer_token: "sk-two".to_string(),
                    mode: ProviderProfileMode::Api,
                },
            ],
        };

        save_provider_profiles_to_path(&path, &state).unwrap();
        let loaded = load_provider_profiles_from_path(&path).unwrap();
        assert_eq!(loaded.active_profile_id, "p2");
        assert_eq!(loaded.profiles.len(), 2);
        assert_eq!(
            profile_by_id(&loaded, None).unwrap().base_url,
            "https://two.example/v1"
        );
        assert_eq!(
            profile_by_id(&loaded, Some("p1")).unwrap().bearer_token,
            "sk-one"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codex-pilot-manager-{name}-{}", now_nanos()))
    }
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
