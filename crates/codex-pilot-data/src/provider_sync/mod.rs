mod filesystem;
mod inspect;
mod models;
mod run;
mod session_changes;
mod sqlite;

pub use inspect::{inspect_provider_sync, inspect_provider_sync_with_target};
pub use models::{ProviderCount, ProviderSyncInspection, ProviderSyncResult, ProviderSyncStatus};
pub use run::{run_provider_sync, run_provider_sync_with_target};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_sync::filesystem::now_secs;
    use rusqlite::Connection;
    use serde_json::{Value, json};
    use std::fs;
    use std::path::PathBuf;

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
