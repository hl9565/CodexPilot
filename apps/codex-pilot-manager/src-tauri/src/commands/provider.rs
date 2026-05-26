use super::super::*;
use codex_pilot_core::error::ManagerError;
use std::path::{Path, PathBuf};

fn provider_sync_message(sync: codex_pilot_data::provider_sync::ProviderSyncResult) -> String {
    format!(
        "Provider Sync：{}，目标 {}，会话文件 {} 个，数据库行 {} 条。",
        sync.message, sync.target_provider, sync.changed_session_files, sync.sqlite_rows_updated
    )
}

fn sanitize_provider_sync_target(value: String) -> Result<String, ManagerError> {
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

struct RecoverySnapshot {
    dir: PathBuf,
}

impl RecoverySnapshot {
    fn dir_string(&self) -> String {
        self.dir.display().to_string()
    }
}

fn capture_recovery_snapshot(operation: &str) -> Result<RecoverySnapshot, ManagerError> {
    let base = codex_pilot_core::app_paths::app_state_dir().join("recovery-snapshots");
    let files: Vec<(&str, PathBuf)> = vec![
        ("provider-profiles.json", provider_profiles_path()),
        ("config.toml", codex_pilot_core::app_paths::codex_config_path()),
        ("auth.json", codex_pilot_core::app_paths::codex_auth_path()),
    ];
    capture_recovery_snapshot_to(&base, operation, &files)
        .map(|dir| RecoverySnapshot { dir })
        .map_err(|e| ManagerError::Io(format!("生成恢复点失败：{e}")))
}

fn capture_recovery_snapshot_to(
    base_dir: &Path,
    operation: &str,
    files: &[(&str, PathBuf)],
) -> std::io::Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = base_dir.join(format!("{ts}-{operation}"));
    std::fs::create_dir_all(&dir)?;
    let mut entries = Vec::with_capacity(files.len());
    for (name, original) in files {
        let present = original.exists();
        if present {
            std::fs::copy(original, dir.join(name))?;
        }
        entries.push(serde_json::json!({
            "name": name,
            "original_path": original.display().to_string(),
            "present": present,
        }));
    }
    let manifest = serde_json::json!({
        "operation": operation,
        "captured_at_ms": ts,
        "files": entries,
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(dir)
}

#[tauri::command]
pub(crate) async fn provider_snapshot() -> ProviderSnapshot {
    tauri::async_runtime::spawn_blocking(provider_snapshot_sync)
        .await
        .expect("provider_snapshot task panicked")
}

fn provider_snapshot_sync() -> ProviderSnapshot {
    let provider = codex_pilot_core::relay_config::default_relay_provider_config();
    let profiles = load_provider_profiles();
    let active_profile = profiles
        .profiles
        .iter()
        .find(|profile| profile.id == profiles.active_profile_id)
        .or_else(|| profiles.profiles.first());
    let official_snapshot_available = profiles.official_config_snapshot.is_some();
    let backup_snapshot_available = latest_official_backup_candidate().is_some();
    let effective_route =
        infer_effective_route(&provider, active_profile, official_snapshot_available);
    let effective_profile_name = active_profile
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| "默认中转".to_string());
    let status_message = provider_status_message(
        &provider,
        active_profile,
        official_snapshot_available,
        effective_route,
    );
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
        profile: effective_profile_name,
        source: provider.config_path,
        auth_path: codex_pilot_core::app_paths::codex_auth_path()
            .to_string_lossy()
            .to_string(),
        configured: provider.configured,
        authenticated: provider.authenticated,
        account_label: provider.account_label,
        route_label: effective_route.label().to_string(),
        status_message,
        degraded: effective_route == EffectiveRoute::DegradedRelay,
        official_snapshot_available,
        backup_snapshot_available,
        profiles: profiles.profiles,
        active_profile_id: profiles.active_profile_id,
    }
}

#[tauri::command]
pub(crate) async fn ccs_provider_snapshot() -> CcsProviderSnapshot {
    tauri::async_runtime::spawn_blocking(|| ccs_provider_snapshot_for_state(&load_provider_profiles()))
        .await
        .expect("ccs_provider_snapshot task panicked")
}

#[tauri::command]
pub(crate) async fn import_official_snapshot_from_backup(
) -> Result<OfficialSnapshotImportResult, ManagerError> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut state = load_provider_profiles();
        let backup = latest_official_backup_candidate()
            .ok_or_else(|| ManagerError::NotFound("未找到可导入的官方原版备份。".to_string()))?;
        let config_toml = std::fs::read_to_string(&backup.path)
            .map_err(|error| ManagerError::Io(format!("读取官方原版备份失败：{error}")))?;
        state.official_config_snapshot = Some(OfficialConfigSnapshot {
            config_toml,
            captured_at_ms: backup.modified_at_ms,
        });
        save_provider_profiles_to_path(&provider_profiles_path(), &state)
            .map_err(ManagerError::Io)?;
        Ok(OfficialSnapshotImportResult {
            message: format!("已从备份导入官方原版快照：{}。", backup.path.display()),
            provider: provider_snapshot_sync(),
        })
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("导入官方原版备份任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn prepare_official_snapshot_after_clearing_relay(
) -> Result<OfficialSnapshotPrepareResult, ManagerError> {
    tauri::async_runtime::spawn_blocking(|| {
        codex_pilot_core::relay_config::clear_relay_provider_config()
            .map_err(|error| ManagerError::Internal(format!("停止 CodexPilot 中转失败：{error}")))?;

        let mut state = load_provider_profiles();
        let snapshot = codex_pilot_core::relay_config::capture_official_config_snapshot_from_home(
            &codex_pilot_core::app_paths::codex_home_dir(),
        )
        .map_err(|error| ManagerError::Internal(format!("准备官方原版恢复点失败：{error}")))?;

        let snapshot = snapshot.ok_or_else(|| {
            ManagerError::InvalidInput(
                "当前仍不是可保存的官方状态，暂时无法准备官方原版恢复点。".to_string(),
            )
        })?;

        state.official_config_snapshot = Some(OfficialConfigSnapshot {
            config_toml: snapshot.config_toml,
            captured_at_ms: snapshot.captured_at_ms,
        });
        save_provider_profiles_to_path(&provider_profiles_path(), &state)
            .map_err(ManagerError::Io)?;
        Ok(OfficialSnapshotPrepareResult {
            message: "已停止当前中转，并准备好官方原版恢复点。".to_string(),
            provider: provider_snapshot_sync(),
        })
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("准备官方原版恢复点任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn import_ccs_provider_profiles() -> Result<CcsImportResult, ManagerError> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut state = load_provider_profiles();
        let candidates = codex_pilot_core::ccs_import::list_codex_providers_from_default_db()
            .map_err(|error| ManagerError::Internal(format!("读取 CCSwitch 配置失败：{error}")))?;

        let mut imported_count = 0usize;
        let mut skipped_count = 0usize;
        let mut renamed_count = 0usize;
        let mut next_profiles = state.profiles.clone();

        for candidate in candidates {
            let mode = ProviderProfileMode::Api;
            if next_profiles
                .iter()
                .any(|profile| profiles_equivalent(profile, &candidate, mode))
            {
                skipped_count += 1;
                continue;
            }

            let unique_name = unique_imported_profile_name(&next_profiles, &candidate.name);
            if !unique_name.eq(candidate.name.trim()) {
                renamed_count += 1;
            }
            next_profiles.push(ProviderProfile {
                id: unique_profile_id(&next_profiles),
                name: unique_name,
                base_url: candidate.base_url.trim().to_string(),
                bearer_token: candidate.api_key.trim().to_string(),
                mode,
                upstream_protocol: candidate.upstream_protocol,
                authenticated_behavior: default_authenticated_behavior(),
            });
            imported_count += 1;
        }

        if imported_count > 0 {
            state.profiles = next_profiles;
            save_provider_profiles_to_path(&provider_profiles_path(), &state)
                .map_err(ManagerError::Io)?;
        }

        let provider = provider_snapshot_sync();
        let ccs = ccs_provider_snapshot_for_state(&load_provider_profiles());
        let message = if imported_count == 0 {
            "没有新的 CCSwitch 配置需要导入。".to_string()
        } else {
            format!(
                "已导入 {imported_count} 个 CCSwitch 配置，跳过 {skipped_count} 个，重命名 {renamed_count} 个。"
            )
        };

        Ok(CcsImportResult {
            imported_count,
            skipped_count,
            renamed_count,
            provider,
            ccs,
            message,
        })
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("导入 CCSwitch 配置任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn apply_provider(request: ProviderApplyRequest) -> Result<String, ManagerError> {
    let profiles = load_provider_profiles();
    let profile = profile_by_id(&profiles, request.profile_id.as_deref())
        .map_err(ManagerError::NotFound)?;
    let snapshot = profiles.official_config_snapshot.clone();
    let requested_mode = request.mode;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(mode) = requested_mode {
            let result = match mode {
                ProviderProfileMode::HybridApi => {
                    codex_pilot_core::relay_config::apply_relay_provider_config_with_protocol(
                        &profile.base_url,
                        &profile.bearer_token,
                        profile.upstream_protocol,
                    )
                    .map_err(|error| ManagerError::Internal(format!("应用混合中转失败：{error}")))?
                }
                ProviderProfileMode::Api => {
                    codex_pilot_core::relay_config::apply_api_provider_config_with_protocol(
                        &profile.base_url,
                        &profile.bearer_token,
                        profile.upstream_protocol,
                    )
                    .map_err(|error| ManagerError::Internal(format!("应用传统中转失败：{error}")))?
                }
            };
            return Ok(result
                .backup_path
                .map(|path| format!("{} 已应用，备份：{path}。", mode.label()))
                .unwrap_or_else(|| format!("{} 已应用。", mode.label())));
        }
        apply_profile_now(&profile, snapshot.as_ref())
            .map(|result| result.message)
            .map_err(ManagerError::Internal)
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("应用运行模式任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn save_provider_profile(
    request: ProviderProfileSaveRequest,
) -> Result<ProviderProfileSaveResponse, ManagerError> {
    tauri::async_runtime::spawn_blocking(move || {
        let recovery = capture_recovery_snapshot("save_provider_profile")?;
        let wrap = |err: ManagerError| ManagerError::WithRecoveryPoint {
            message: err.to_string(),
            recovery_dir: recovery.dir_string(),
        };
        let mut state = load_provider_profiles();
        let activate = request.activate;
        let profile =
            sanitize_provider_profile(request).map_err(|e| wrap(ManagerError::InvalidInput(e)))?;
        let normalized_name = profile.name.trim();
        if state.profiles.iter().any(|item| {
            item.id != profile.id && item.name.trim().eq_ignore_ascii_case(normalized_name)
        }) {
            return Err(wrap(ManagerError::Conflict("配置档名称不能重复。".to_string())));
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
        capture_official_snapshot_if_missing(&mut state)
            .map_err(|e| wrap(ManagerError::Internal(e)))?;
        save_provider_profiles_to_path(&provider_profiles_path(), &state)
            .map_err(|e| wrap(ManagerError::Io(e)))?;
        let message = if state.active_profile_id == id {
            apply_active_profile(&state).map_err(|e| wrap(ManagerError::Internal(e)))?
        } else {
            "中转配置档已保存。".to_string()
        };
        Ok(ProviderProfileSaveResponse { id, message })
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("保存中转配置档任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn activate_provider_profile(
    request: ProviderProfileIdRequest,
) -> Result<String, ManagerError> {
    tauri::async_runtime::spawn_blocking(move || {
        let recovery = capture_recovery_snapshot("activate_provider_profile")?;
        let wrap = |err: ManagerError| ManagerError::WithRecoveryPoint {
            message: err.to_string(),
            recovery_dir: recovery.dir_string(),
        };
        let mut state = load_provider_profiles();
        if !state.profiles.iter().any(|profile| profile.id == request.id) {
            return Err(wrap(ManagerError::NotFound(
                "中转配置档不存在。".to_string(),
            )));
        }
        state.active_profile_id = request.id;
        capture_official_snapshot_if_missing(&mut state)
            .map_err(|e| wrap(ManagerError::Internal(e)))?;
        save_provider_profiles_to_path(&provider_profiles_path(), &state)
            .map_err(|e| wrap(ManagerError::Io(e)))?;
        apply_active_profile(&state).map_err(|e| wrap(ManagerError::Internal(e)))
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("启用中转配置档任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn delete_provider_profile(
    request: ProviderProfileIdRequest,
) -> Result<String, ManagerError> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = load_provider_profiles();
        if state.profiles.len() <= 1 {
            return Err(ManagerError::InvalidInput(
                "至少保留一个中转配置档。".to_string(),
            ));
        }
        let before = state.profiles.len();
        state.profiles.retain(|profile| profile.id != request.id);
        if state.profiles.len() == before {
            return Err(ManagerError::NotFound("中转配置档不存在。".to_string()));
        }
        if state.active_profile_id == request.id {
            state.active_profile_id = state
                .profiles
                .first()
                .map(|profile| profile.id.clone())
                .unwrap_or_else(|| "default".to_string());
        }
        save_provider_profiles_to_path(&provider_profiles_path(), &state)
            .map_err(ManagerError::Io)?;
        Ok("中转配置档已删除。".to_string())
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("删除中转配置档任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn clear_provider() -> Result<String, ManagerError> {
    tauri::async_runtime::spawn_blocking(|| {
        let result = codex_pilot_core::relay_config::clear_relay_provider_config()
            .map_err(|error| ManagerError::Internal(format!("清除中转失败：{error}")))?;
        Ok(result
            .backup_path
            .map(|path| format!("中转已清除，备份：{path}"))
            .unwrap_or_else(|| "中转已清除。".to_string()))
    })
    .await
    .map_err(|error| ManagerError::Internal(format!("清除中转任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn provider_sync_snapshot(
    request: Option<ProviderSyncRequest>,
) -> Result<ProviderSyncSnapshot, ManagerError> {
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
        .map_err(|error| ManagerError::Internal(format!("检查历史会话同步失败：{error}")))?;
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
    .map_err(|error| ManagerError::Internal(format!("检查历史会话同步任务失败：{error}")))?
}

#[tauri::command]
pub(crate) async fn sync_provider_sessions(
    request: ProviderSyncRequest,
) -> Result<String, ManagerError> {
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
    .map_err(|error| ManagerError::Internal(format!("同步历史会话任务失败：{error}")))?
}

#[cfg(test)]
mod recovery_snapshot_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn captures_and_writes_manifest() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("rs");
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let a = src.join("a.json");
        fs::write(&a, b"AAA").unwrap();
        let b = src.join("b.toml");
        fs::write(&b, b"BBB").unwrap();
        let m = src.join("m.json");
        let files = vec![("a.json", a), ("b.toml", b), ("m.json", m)];
        let dir = capture_recovery_snapshot_to(&base, "op", &files).unwrap();
        assert!(dir.starts_with(&base));
        assert_eq!(fs::read(dir.join("a.json")).unwrap(), b"AAA");
        assert_eq!(fs::read(dir.join("b.toml")).unwrap(), b"BBB");
        assert!(!dir.join("m.json").exists());
        let mf: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(mf["operation"], "op");
        let arr = mf["files"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["present"], true);
        assert_eq!(arr[2]["present"], false);
    }
}
