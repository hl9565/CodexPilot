use codex_pilot_core::error::ManagerError;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSyncSnapshot {
    pub(crate) target_provider: String,
    pub(crate) current_provider: String,
    pub(crate) available_providers: Vec<String>,
    pub(crate) rollout_files: usize,
    pub(crate) rollout_rewrite_needed: usize,
    pub(crate) sqlite_rows: usize,
    pub(crate) sqlite_provider_rows_needing_sync: usize,
    pub(crate) sqlite_total_updates_needed: usize,
    pub(crate) rollout_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
    pub(crate) sqlite_providers: Vec<codex_pilot_data::provider_sync::ProviderCount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderSyncRequest {
    pub(crate) target_provider: Option<String>,
}

pub(crate) fn provider_sync_message(
    sync: codex_pilot_data::provider_sync::ProviderSyncResult,
) -> String {
    format!(
        "Provider Sync：{}，目标 {}，会话文件 {} 个，数据库行 {} 条。",
        sync.message, sync.target_provider, sync.changed_session_files, sync.sqlite_rows_updated
    )
}

pub(crate) fn sanitize_provider_sync_target(value: String) -> Result<String, ManagerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ManagerError::InvalidInput(
            "同步目标 Provider 不能为空。".to_string(),
        ));
    }
    if trimmed.len() > 80 {
        return Err(ManagerError::InvalidInput(
            "同步目标 Provider 过长。".to_string(),
        ));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(ManagerError::InvalidInput(
            "同步目标 Provider 只能包含字母、数字、下划线、中划线或点。".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn requested_target(request: Option<ProviderSyncRequest>) -> Result<Option<String>, ManagerError> {
    request
        .and_then(|item| item.target_provider)
        .map(|target| {
            if target.trim().is_empty() {
                Ok(None)
            } else {
                sanitize_provider_sync_target(target).map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

#[tauri::command]
pub(crate) async fn provider_sync_snapshot(
    request: Option<ProviderSyncRequest>,
) -> Result<ProviderSyncSnapshot, ManagerError> {
    tauri::async_runtime::spawn_blocking(move || {
        let current_inspection = codex_pilot_data::provider_sync::inspect_provider_sync(None)
            .map_err(|error| ManagerError::Internal(format!("检查当前 Provider 失败：{error}")))?;
        let current_provider = current_inspection.target_provider.clone();
        let requested_target = requested_target(request)?;
        let inspection = match requested_target.as_deref() {
            Some(target) => codex_pilot_data::provider_sync::inspect_provider_sync_with_target(
                None,
                Some(target),
            )
            .map_err(|error| ManagerError::Internal(format!("检查历史会话同步失败：{error}")))?,
            None => current_inspection,
        };
        let mut available = vec![current_provider.clone()];
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
    .map_err(|error| ManagerError::Internal(format!("检查历史会话同步任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn sync_provider_sessions(
    request: Option<ProviderSyncRequest>,
) -> Result<String, ManagerError> {
    tauri::async_runtime::spawn_blocking(move || {
        let target_provider = requested_target(request)?;
        let result = match target_provider.as_deref() {
            Some(target) => {
                codex_pilot_data::provider_sync::run_provider_sync_with_target(None, Some(target))
            }
            None => codex_pilot_data::provider_sync::run_provider_sync(None),
        };
        Ok(provider_sync_message(result))
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("同步历史会话任务失败：{error}")))?
}
