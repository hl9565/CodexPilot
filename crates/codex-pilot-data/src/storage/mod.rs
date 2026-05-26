mod backup;
mod codex_threads;
mod delete_undo;
mod models;
mod recycle_bin;
mod schema;
mod sql_helpers;

use std::path::{Path, PathBuf};

use backup::{
    BackupPayload, backup_last_active_at, backup_project_cwd, backup_title, remove_rollout_files,
    remove_session_index_entries, restore_files, restore_session_index_entries, restore_tables,
    rollout_file_backups, session_index_backups,
};
pub(crate) use models::normalize_session_id;
pub use models::{DeleteResult, DeleteStatus, SessionRef};
use models::{deleted, failed, failed_with_backup, not_found};
pub use recycle_bin::RecycleBinEntry;
use schema::table_columns;
pub(crate) use schema::{SchemaKind, has_columns, has_table, schema_kind};
use sql_helpers::{
    OwnedSqlValue, decode_hex, encode_hex, json_to_sql_value, quote_identifier,
    sanitize_token_part, sql_value_to_json,
};

#[derive(Debug, Clone)]
pub struct SQLiteStorageAdapter {
    db_path: PathBuf,
    backup_dir: PathBuf,
}

impl SQLiteStorageAdapter {
    pub fn new(db_path: PathBuf) -> Self {
        let backup_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".codex-pilot-undo");
        Self {
            db_path,
            backup_dir,
        }
    }

    pub fn with_backup_dir(db_path: PathBuf, backup_dir: PathBuf) -> Self {
        Self {
            db_path,
            backup_dir,
        }
    }

    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codex-pilot-data-{name}-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn deletes_and_undoes_generic_session() {
        let db_path = unique_temp_path("delete-undo");
        let backup_dir = db_path.with_extension("undo");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, metadata BLOB);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT);
            INSERT INTO sessions VALUES ('s1', 'Fixture', x'010203');
            INSERT INTO messages (session_id, role, body) VALUES ('s1', 'user', 'hello');
            INSERT INTO messages (session_id, role, body) VALUES ('s1', 'assistant', 'hi');
            "#,
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter
            .delete_local(&SessionRef::new("local:s1", Some("Fixture".to_string())))
            .unwrap();
        assert_eq!(result.status, DeleteStatus::Deleted);
        let token = result.undo_token.clone().unwrap();
        let backups = adapter.list_undo_backups().unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].token, token);
        assert_eq!(backups[0].session_id, "s1");
        assert_eq!(backups[0].title.as_deref(), Some("Fixture"));
        assert_eq!(backups[0].schema, "generic_sessions");
        assert!(backups[0].recoverable);
        assert_eq!(backups[0].status, "可恢复");

        let db = Connection::open(&db_path).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM sessions WHERE id = 's1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        drop(db);

        let undo = adapter.undo(&token).unwrap();
        assert_eq!(undo.status, DeleteStatus::Undone);
        assert!(!adapter.backup_path(&token).unwrap().exists());
        assert!(adapter.list_undo_backups().unwrap().is_empty());
        let db = Connection::open(&db_path).unwrap();
        let title: String = db
            .query_row("SELECT title FROM sessions WHERE id = 's1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Fixture");
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }

    #[test]
    fn recycle_bin_lists_corrupt_backups_and_deletes_permanently() {
        let db_path = unique_temp_path("recycle-bin");
        let backup_dir = db_path.with_extension("undo");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            INSERT INTO sessions VALUES ('s1', 'Fixture');
            "#,
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter
            .delete_local(&SessionRef::new("s1", Some("Fixture".to_string())))
            .unwrap();
        let token = result.undo_token.clone().unwrap();
        fs::write(adapter.backup_dir().join("broken.json"), "{").unwrap();

        let backups = adapter.list_undo_backups().unwrap();
        assert_eq!(backups.len(), 2);
        assert!(backups.iter().any(|entry| {
            entry.token == token && entry.title.as_deref() == Some("Fixture") && entry.recoverable
        }));
        assert!(backups.iter().any(|entry| {
            entry.token == "broken" && !entry.recoverable && entry.status == "备份无法解析"
        }));

        let deleted = adapter.delete_undo_backup(&token).unwrap();
        assert_eq!(deleted.status, DeleteStatus::Deleted);
        assert!(!adapter.backup_path(&token).unwrap().exists());

        let missing = adapter.delete_undo_backup(&token).unwrap();
        assert_eq!(missing.status, DeleteStatus::NotFound);

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }

    #[test]
    fn undo_restores_parent_rows_before_foreign_key_children() {
        let db_path = unique_temp_path("delete-undo-fk");
        let backup_dir = db_path.with_extension("undo");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                body TEXT
            );
            INSERT INTO sessions VALUES ('s1', 'Fixture');
            INSERT INTO messages (session_id, body) VALUES ('s1', 'hello');
            "#,
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter.delete_local(&SessionRef::new("s1", Some("Fixture".to_string())));
        assert_eq!(result.unwrap().status, DeleteStatus::Deleted);
        let token = fs::read_dir(adapter.backup_dir())
            .unwrap()
            .filter_map(Result::ok)
            .find_map(|entry| {
                entry
                    .path()
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(ToString::to_string)
            })
            .unwrap();

        let db = Connection::open(&db_path).unwrap();
        db.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        drop(db);

        let undo = adapter.undo(&token).unwrap();
        assert_eq!(undo.status, DeleteStatus::Undone);
        assert!(!adapter.backup_path(&token).unwrap().exists());

        let db = Connection::open(&db_path).unwrap();
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }

    #[test]
    fn deletes_codex_thread_fixture() {
        let db_path = unique_temp_path("thread-delete");
        let backup_dir = db_path.with_extension("undo");
        let rollout_path = unique_temp_path("rollout");
        fs::write(&rollout_path, "rollout data").unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE thread_dynamic_tools (thread_id TEXT NOT NULL, tool_name TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE thread_goals (thread_id TEXT NOT NULL, goal TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE thread_spawn_edges (parent_thread_id TEXT NOT NULL, child_thread_id TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE stage1_outputs (thread_id TEXT NOT NULL, output TEXT NOT NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "CREATE TABLE agent_job_items (id TEXT PRIMARY KEY, assigned_thread_id TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        db.execute("INSERT INTO thread_dynamic_tools VALUES ('t1', 'tool')", [])
            .unwrap();
        db.execute("INSERT INTO thread_goals VALUES ('t1', 'goal')", [])
            .unwrap();
        db.execute("INSERT INTO thread_spawn_edges VALUES ('t1', 'child')", [])
            .unwrap();
        db.execute("INSERT INTO stage1_outputs VALUES ('t1', 'output')", [])
            .unwrap();
        db.execute("INSERT INTO agent_job_items VALUES ('job-1', 't1')", [])
            .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter.delete_local(&SessionRef::new("t1", None)).unwrap();
        assert_eq!(result.status, DeleteStatus::Deleted);
        let token = result.undo_token.clone().unwrap();
        let db = Connection::open(&db_path).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM threads WHERE id = 't1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM thread_dynamic_tools WHERE thread_id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let assigned: Option<String> = db
            .query_row(
                "SELECT assigned_thread_id FROM agent_job_items WHERE id = 'job-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(assigned, None);
        drop(db);
        assert!(!rollout_path.exists());

        let undo = adapter.undo(&token).unwrap();
        assert_eq!(undo.status, DeleteStatus::Undone);
        assert!(!adapter.backup_path(&token).unwrap().exists());
        assert_eq!(fs::read_to_string(&rollout_path).unwrap(), "rollout data");
        let db = Connection::open(&db_path).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM threads WHERE id = 't1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let assigned: Option<String> = db
            .query_row(
                "SELECT assigned_thread_id FROM agent_job_items WHERE id = 'job-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(assigned.as_deref(), Some("t1"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }

    #[test]
    fn deletes_and_restores_codex_session_index_entry() {
        let root = std::env::temp_dir().join(format!(
            "codex-pilot-data-session-index-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let db_path = root.join("state_5.sqlite");
        let backup_dir = root.join(".codex-pilot-undo");
        let rollout_path = root.join("rollout-t1.jsonl");
        let session_index_path = root.join("session_index.jsonl");
        fs::write(&rollout_path, "rollout data").unwrap();
        fs::write(
            &session_index_path,
            "{\"id\":\"other\",\"thread_name\":\"Other\",\"updated_at\":\"2026-05-21T00:00:00Z\"}\n{\"id\":\"t1\",\"thread_name\":\"Thread\",\"updated_at\":\"2026-05-21T01:00:00Z\"}\n",
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, rollout_path TEXT)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES (?1, ?2, ?3)",
            ("t1", "Thread", rollout_path.to_string_lossy().as_ref()),
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter.delete_local(&SessionRef::new("t1", None)).unwrap();
        assert_eq!(result.status, DeleteStatus::Deleted);
        let token = result.undo_token.clone().unwrap();
        let session_index = fs::read_to_string(&session_index_path).unwrap();
        assert!(session_index.contains("\"id\":\"other\""));
        assert!(!session_index.contains("\"id\":\"t1\""));

        fs::write(
            &session_index_path,
            format!(
                "{}{}",
                fs::read_to_string(&session_index_path).unwrap(),
                "{\"id\":\"t1\",\"thread_name\":\"Stale duplicate\",\"updated_at\":\"2026-05-21T02:00:00Z\"}\n"
            ),
        )
        .unwrap();
        let undo = adapter.undo(&token).unwrap();
        assert_eq!(undo.status, DeleteStatus::Undone);
        let session_index = fs::read_to_string(&session_index_path).unwrap();
        assert!(session_index.contains("\"id\":\"other\""));
        assert!(session_index.contains("\"thread_name\":\"Thread\""));
        assert!(!session_index.contains("Stale duplicate"));
        assert_eq!(session_index.matches("\"id\":\"t1\"").count(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recycle_bin_entry_reads_project_and_last_active_from_thread_backup() {
        let db_path = unique_temp_path("recycle-project");
        let backup_dir = db_path.with_extension("undo");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, cwd TEXT, rollout_path TEXT, updated_at_ms INTEGER, created_at_ms INTEGER);
            INSERT INTO threads VALUES ('t1', 'Thread', '/Users/huanglin/code/github/CodexPilot', '/tmp/rollout.jsonl', 1770000000000, 1760000000000);
            "#,
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        let result = adapter
            .delete_local(&SessionRef::new("t1", Some("Thread".to_string())))
            .unwrap();
        let token = result.undo_token.clone().unwrap();

        let backups = adapter.list_undo_backups().unwrap();
        let entry = backups.iter().find(|entry| entry.token == token).unwrap();
        assert_eq!(
            entry.project_cwd.as_deref(),
            Some("/Users/huanglin/code/github/CodexPilot")
        );
        assert_eq!(entry.last_active_at, Some(1_770_000_000));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }
}
