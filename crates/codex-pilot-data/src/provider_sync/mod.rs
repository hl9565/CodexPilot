mod filesystem;
mod inspect;
mod models;
mod run;
mod session_changes;
mod sqlite;

pub use models::{ProviderCount, ProviderSyncInspection, ProviderSyncResult, ProviderSyncStatus};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use filesystem::{
    apply_global_state_update, apply_session_changes, count_global_state_updates, create_backup,
    dirs_home, log_provider_sync_event, normalize_target_provider, now_secs, prune_backups,
    read_current_provider, restore_session_changes,
};
use models::{provider_counts, result};
use session_changes::{collect_session_changes, rollout_provider_from_first_line};
use sqlite::{
    apply_sqlite_update, count_sqlite_provider_rows_needing_sync, count_sqlite_rows,
    count_sqlite_updates, sqlite_provider_counts, sqlite_provider_drift_details,
};

pub fn inspect_provider_sync(codex_home: Option<&Path>) -> anyhow::Result<ProviderSyncInspection> {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    inspect_provider_sync_with_target(Some(&home), None)
}

pub fn inspect_provider_sync_with_target(
    codex_home: Option<&Path>,
    target_provider: Option<&str>,
) -> anyhow::Result<ProviderSyncInspection> {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    let target_provider = normalize_target_provider(
        target_provider
            .map(ToString::to_string)
            .unwrap_or_else(|| read_current_provider(&home.join("config.toml"))),
    );
    let changes = collect_session_changes(&home, &target_provider)?;
    let thread_ids_with_user_events = changes
        .iter()
        .filter(|change| change.has_user_event)
        .filter_map(|change| change.thread_id.clone())
        .collect::<HashSet<_>>();
    let cwd_by_thread_id = changes
        .iter()
        .filter_map(|change| Some((change.thread_id.clone()?, change.cwd.clone()?)))
        .collect::<HashMap<_, _>>();
    let sqlite_path = home.join("state_5.sqlite");
    let sqlite_total_updates_needed = count_sqlite_updates(
        &sqlite_path,
        &target_provider,
        &thread_ids_with_user_events,
        &cwd_by_thread_id,
    )?;

    let sqlite_provider_rows_needing_sync =
        count_sqlite_provider_rows_needing_sync(&sqlite_path, &target_provider)?;

    Ok(ProviderSyncInspection {
        target_provider,
        rollout_files: changes.len(),
        rollout_rewrite_needed: changes
            .iter()
            .filter(|change| change.rewrite_needed)
            .count(),
        sqlite_rows: count_sqlite_rows(&sqlite_path)?,
        sqlite_provider_rows_needing_sync,
        sqlite_total_updates_needed,
        rollout_providers: provider_counts(
            changes
                .iter()
                .filter_map(|change| rollout_provider_from_first_line(&change.original_first_line)),
        ),
        sqlite_providers: sqlite_provider_counts(&sqlite_path)?,
    })
}

pub fn run_provider_sync(codex_home: Option<&Path>) -> ProviderSyncResult {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    let target_provider = read_current_provider(&home.join("config.toml"));
    run_provider_sync_with_target(Some(&home), Some(&target_provider))
}

pub fn run_provider_sync_with_target(
    codex_home: Option<&Path>,
    target_provider: Option<&str>,
) -> ProviderSyncResult {
    let home = codex_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| dirs_home().join(".codex"));
    let target_provider = normalize_target_provider(
        target_provider
            .map(ToString::to_string)
            .unwrap_or_else(|| read_current_provider(&home.join("config.toml"))),
    );
    if !home.exists() {
        return result(
            ProviderSyncStatus::Skipped,
            format!("Codex home 不存在：{}", home.display()),
            &target_provider,
            None,
            0,
            0,
        );
    }
    let lock_dir = home.join("tmp/provider-sync.lock");
    if acquire_lock(&lock_dir).is_err() {
        return result(
            ProviderSyncStatus::Skipped,
            format!("Provider Sync 正在运行：{}", lock_dir.display()),
            &target_provider,
            None,
            0,
            0,
        );
    }

    let sync_result = (|| -> anyhow::Result<ProviderSyncResult> {
        let changes = collect_session_changes(&home, &target_provider)?;
        let rewrite_changes = changes
            .iter()
            .filter(|change| change.rewrite_needed)
            .cloned()
            .collect::<Vec<_>>();
        let thread_ids_with_user_events = changes
            .iter()
            .filter(|change| change.has_user_event)
            .filter_map(|change| change.thread_id.clone())
            .collect::<HashSet<_>>();
        let cwd_by_thread_id = changes
            .iter()
            .filter_map(|change| Some((change.thread_id.clone()?, change.cwd.clone()?)))
            .collect::<HashMap<_, _>>();
        let sqlite_path = home.join("state_5.sqlite");
        let sqlite_update_count = count_sqlite_updates(
            &sqlite_path,
            &target_provider,
            &thread_ids_with_user_events,
            &cwd_by_thread_id,
        )?;
        log_provider_sync_event(
            &home,
            "provider_sync.before",
            json!({
                "target_provider": target_provider,
                "rollout_files": changes.len(),
                "rollout_rewrite_needed": rewrite_changes.len(),
                "sqlite_rows": count_sqlite_rows(&sqlite_path).unwrap_or_default(),
                "sqlite_provider_rows_needing_sync": count_sqlite_provider_rows_needing_sync(&sqlite_path, &target_provider).unwrap_or_default(),
                "sqlite_total_updates_needed": sqlite_update_count,
                "rollout_providers": provider_counts(changes.iter().filter_map(|change| rollout_provider_from_first_line(&change.original_first_line))),
                "sqlite_providers": sqlite_provider_counts(&sqlite_path).unwrap_or_default(),
                "drift_details": sqlite_provider_drift_details(&sqlite_path, &target_provider).unwrap_or_default()
            }),
        );
        let global_state_update_count =
            count_global_state_updates(&home.join(".codex-global-state.json"))?;
        if rewrite_changes.is_empty() && sqlite_update_count == 0 && global_state_update_count == 0
        {
            return Ok(result(
                ProviderSyncStatus::Synced,
                "Provider Sync 已是最新",
                &target_provider,
                None,
                0,
                0,
            ));
        }

        let backup_dir = create_backup(&home, &target_provider, &rewrite_changes)?;
        apply_session_changes(&rewrite_changes)?;
        let apply_result = (|| -> anyhow::Result<usize> {
            let sqlite_rows_updated = apply_sqlite_update(
                &sqlite_path,
                &target_provider,
                &thread_ids_with_user_events,
                &cwd_by_thread_id,
            )?;
            let remaining_after_commit =
                count_sqlite_provider_rows_needing_sync(&sqlite_path, &target_provider)?;
            log_provider_sync_event(
                &home,
                "provider_sync.after_commit",
                json!({
                    "target_provider": target_provider,
                    "sqlite_provider_rows_updated": sqlite_rows_updated,
                    "sqlite_provider_rows_remaining": remaining_after_commit,
                    "sqlite_providers": sqlite_provider_counts(&sqlite_path).unwrap_or_default(),
                    "drift_details": sqlite_provider_drift_details(&sqlite_path, &target_provider).unwrap_or_default()
                }),
            );
            apply_global_state_update(&home.join(".codex-global-state.json"))?;
            prune_backups(&home)?;
            Ok(sqlite_rows_updated)
        })();
        let sqlite_rows_updated = match apply_result {
            Ok(count) => count,
            Err(error) => {
                let _ = restore_session_changes(&rewrite_changes);
                return Err(error);
            }
        };
        schedule_provider_sync_delayed_recheck(home.clone(), target_provider.clone());
        Ok(result(
            ProviderSyncStatus::Synced,
            "Provider Sync 完成",
            &target_provider,
            Some(backup_dir),
            rewrite_changes.len(),
            sqlite_rows_updated,
        ))
    })();
    let _ = release_lock(&lock_dir);
    sync_result.unwrap_or_else(|error| {
        result(
            ProviderSyncStatus::Skipped,
            format!("Provider Sync 跳过：{error}"),
            &target_provider,
            None,
            0,
            0,
        )
    })
}

fn acquire_lock(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    fs::create_dir(path)?;
    fs::write(
        path.join("owner.json"),
        json!({"pid": std::process::id(), "startedAt": now_secs()}).to_string(),
    )
}

fn release_lock(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn schedule_provider_sync_delayed_recheck(home: PathBuf, target_provider: String) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let sqlite_path = home.join("state_5.sqlite");
        log_provider_sync_event(
            &home,
            "provider_sync.after_delay",
            json!({
                "target_provider": target_provider,
                "sqlite_provider_rows_remaining": count_sqlite_provider_rows_needing_sync(&sqlite_path, &target_provider).unwrap_or_default(),
                "sqlite_providers": sqlite_provider_counts(&sqlite_path).unwrap_or_default(),
                "drift_details": sqlite_provider_drift_details(&sqlite_path, &target_provider).unwrap_or_default()
            }),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::Value;

    #[test]
    fn provider_sync_updates_rollout_sqlite_and_global_state() {
        let home = unique_temp_dir("provider-sync");
        fs::create_dir_all(home.join("sessions/2026")).unwrap();
        fs::write(
            home.join("config.toml"),
            "model_provider = \"CodexPilot\"\n",
        )
        .unwrap();
        let rollout = home.join("sessions/2026/rollout-thread-1.jsonl");
        fs::write(
            &rollout,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"thread-1\",\"model_provider\":\"openai\",\"cwd\":\"\\\\\\\\?\\\\C:\\\\workspace\"}}\n{\"type\":\"user_message\",\"payload\":{\"text\":\"hello\"}}\n",
        )
        .unwrap();
        fs::write(
            home.join(".codex-global-state.json"),
            json!({
                "electron-saved-workspace-roots": ["\\\\?\\C:\\workspace", "C:/workspace"],
                "project-order": ["\\\\?\\C:\\workspace"],
                "active-workspace-roots": "\\\\?\\C:\\workspace",
                "electron-workspace-root-labels": {"\\\\?\\C:\\workspace": "Workspace"}
            })
            .to_string(),
        )
        .unwrap();
        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT, has_user_event INTEGER, cwd TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES ('thread-1', 'openai', 0, 'C:/old')",
            [],
        )
        .unwrap();
        drop(db);

        let result = run_provider_sync(Some(&home));
        assert_eq!(result.status, ProviderSyncStatus::Synced);
        assert_eq!(result.target_provider, "CodexPilot");
        assert_eq!(result.changed_session_files, 1);
        assert_eq!(result.sqlite_rows_updated, 1);
        assert!(result.backup_dir.as_ref().unwrap().exists());

        let first_line = fs::read_to_string(&rollout)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        let value = serde_json::from_str::<Value>(&first_line).unwrap();
        assert_eq!(value["payload"]["model_provider"], "CodexPilot");

        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        let row = db
            .query_row(
                "SELECT model_provider, has_user_event, cwd FROM threads WHERE id = 'thread-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            ("CodexPilot".to_string(), 1, "C:/workspace".to_string())
        );

        let global_state = serde_json::from_str::<Value>(
            &fs::read_to_string(home.join(".codex-global-state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            global_state["electron-saved-workspace-roots"],
            json!(["C:/workspace"])
        );
        assert_eq!(
            global_state["active-workspace-roots"],
            json!("C:/workspace")
        );

        let manifest = serde_json::from_str::<Value>(
            &fs::read_to_string(
                result
                    .backup_dir
                    .as_ref()
                    .unwrap()
                    .join("session-meta-backup.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let manifest_items = manifest.as_array().unwrap();
        assert_eq!(manifest_items.len(), 1);
        assert_eq!(
            manifest_items[0]["path"],
            rollout.to_string_lossy().to_string()
        );
        assert!(manifest_items[0].get("originalFirstLine").is_some());
        assert!(manifest_items[0].get("separator").is_none());
        let manifest_text = serde_json::to_string(&manifest).unwrap();
        assert!(!manifest_text.contains("user_message"));
        assert!(!manifest_text.contains("hello"));

        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn provider_sync_skips_when_lock_exists() {
        let home = unique_temp_dir("provider-sync-lock");
        fs::create_dir_all(home.join("tmp/provider-sync.lock")).unwrap();
        fs::write(
            home.join("config.toml"),
            "model_provider = \"CodexPilot\"\n",
        )
        .unwrap();

        let result = run_provider_sync(Some(&home));
        assert_eq!(result.status, ProviderSyncStatus::Skipped);
        assert!(result.message.contains("正在运行"));

        let _ = fs::remove_dir_all(home);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", now_secs()))
    }
}
