mod backup;
mod codex_threads;
mod delete_undo;
mod models;
mod recycle_bin;
mod schema;
mod sql_helpers;

use anyhow::{Context, anyhow};
use rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef};
use rusqlite::{Connection, ToSql};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) use models::normalize_session_id;
pub use models::{DeleteResult, DeleteStatus, SessionRef};
use models::{deleted, failed, failed_with_backup, not_found};
use schema::table_columns;
pub(crate) use schema::{SchemaKind, has_columns, has_table, schema_kind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecycleBinEntry {
    pub token: String,
    pub session_id: String,
    pub title: Option<String>,
    pub project_cwd: Option<String>,
    pub schema: String,
    pub db_path: PathBuf,
    pub backup_path: PathBuf,
    pub deleted_at: Option<u64>,
    pub last_active_at: Option<u64>,
    pub recoverable: bool,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct SQLiteStorageAdapter {
    db_path: PathBuf,
    backup_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct OwnedSqlValue(SqlValue);

impl ToSql for OwnedSqlValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(self.0.clone()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupPayload {
    session_id: String,
    db_path: PathBuf,
    schema: String,
    tables: Map<String, Value>,
}

/// 单次 `thread_sort_keys` 请求允许查询的去重 id 上限。
/// 超过这个数量的 id 会被截断，调用方需要分批请求；返回体里 `truncated` 字段会显式标记。
pub(crate) const MAX_SORT_KEY_BATCH: usize = 200;

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

    pub fn delete_local(&self, session: &SessionRef) -> anyhow::Result<DeleteResult> {
        if !self.db_path.exists() {
            return Ok(failed(
                &session.normalized_id(),
                format!("database not found: {}", self.db_path.display()),
            ));
        }

        let mut db = Connection::open(&self.db_path)?;
        match schema_kind(&db)? {
            Some(SchemaKind::GenericSessions) => self.delete_generic_session(&mut db, session),
            Some(SchemaKind::CodexThreads) => self.delete_codex_thread(&mut db, session),
            None => Ok(failed(
                &session.normalized_id(),
                "unsupported local storage schema",
            )),
        }
    }

    pub fn inspect_delete_local(&self, session: &SessionRef) -> anyhow::Result<Value> {
        if !self.db_path.exists() {
            return Ok(json!({
                "db_path": self.db_path,
                "db_exists": false,
                "requested_id": session.id,
                "normalized_id": session.normalized_id(),
                "title": session.title,
            }));
        }

        let db = Connection::open(&self.db_path)?;
        let schema = schema_kind(&db)?;
        let normalized_id = session.normalized_id();
        let schema_name = match schema {
            Some(SchemaKind::GenericSessions) => "generic_sessions",
            Some(SchemaKind::CodexThreads) => "codex_threads",
            None => "unknown",
        };

        let thread_exists = if schema == Some(SchemaKind::CodexThreads) {
            select_rows(&db, "threads", "id = ?1", &[&normalized_id])?.len()
        } else {
            0
        };
        let session_exists = if schema == Some(SchemaKind::GenericSessions) {
            select_rows(&db, "sessions", "id = ?1", &[&normalized_id])?.len()
        } else {
            0
        };

        let sample_ids = if schema == Some(SchemaKind::CodexThreads) {
            sample_thread_ids(&db)?
        } else {
            Vec::new()
        };

        Ok(json!({
            "db_path": self.db_path,
            "db_exists": true,
            "schema": schema_name,
            "requested_id": session.id,
            "normalized_id": normalized_id,
            "title": session.title,
            "thread_exists_count": thread_exists,
            "session_exists_count": session_exists,
            "sample_thread_ids": sample_ids,
        }))
    }

    pub fn undo(&self, token: &str) -> anyhow::Result<DeleteResult> {
        let backup_path = self.backup_path(token)?;
        let raw = fs::read_to_string(&backup_path)
            .with_context(|| format!("read undo backup {}", backup_path.display()))?;
        let payload: BackupPayload = serde_json::from_str(&raw)
            .with_context(|| format!("parse undo backup {}", backup_path.display()))?;

        if payload.db_path != self.db_path {
            return Ok(failed_with_backup(
                &payload.session_id,
                "undo token belongs to a different database",
                Some(backup_path),
                None,
            ));
        }

        let mut db = Connection::open(&self.db_path)?;
        restore_tables(&mut db, &payload.tables)?;
        restore_files(&payload.tables)?;
        restore_session_index_entries(&payload.tables)?;
        fs::remove_file(&backup_path)
            .with_context(|| format!("delete restored undo backup {}", backup_path.display()))?;
        Ok(DeleteResult {
            status: DeleteStatus::Undone,
            session_id: payload.session_id,
            message: "已撤销删除".to_string(),
            undo_token: Some(token.to_string()),
            backup_path: Some(backup_path),
        })
    }

    pub fn list_undo_backups(&self) -> anyhow::Result<Vec<RecycleBinEntry>> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.backup_dir)
            .with_context(|| format!("read undo backup dir {}", self.backup_dir.display()))?
        {
            let item = item?;
            let path = item.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            entries.push(self.recycle_entry_from_path(&path));
        }
        entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
        Ok(entries)
    }

    pub fn delete_undo_backup(&self, token: &str) -> anyhow::Result<DeleteResult> {
        let backup_path = self.backup_path(token)?;
        let session_id = token_session_id(token);
        if !backup_path.exists() {
            return Ok(not_found(&session_id, "回收站记录不存在"));
        }
        fs::remove_file(&backup_path)
            .with_context(|| format!("delete undo backup {}", backup_path.display()))?;
        Ok(DeleteResult {
            status: DeleteStatus::Deleted,
            session_id,
            message: "已永久删除回收站记录".to_string(),
            undo_token: Some(token.to_string()),
            backup_path: Some(backup_path),
        })
    }

    pub fn find_archived_thread_by_title(&self, title: &str) -> anyhow::Result<Option<SessionRef>> {
        if !self.db_path.exists() {
            return Ok(None);
        }
        let db = Connection::open(&self.db_path)?;
        if schema_kind(&db)? != Some(SchemaKind::CodexThreads)
            || !has_columns(&db, "threads", &["archived"])?
        {
            return Ok(None);
        }

        let mut stmt = db.prepare(
            "SELECT id, title FROM threads
             WHERE archived = 1 AND (title = ?1 OR title LIKE ?2 OR ?1 LIKE '%' || title || '%')
             ORDER BY archived_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query((title, format!("%{title}%")))?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let id: String = row.get(0)?;
        let row_title: Option<String> = row.get(1)?;
        Ok(Some(SessionRef::new(
            id,
            row_title.or_else(|| Some(title.to_string())),
        )))
    }

    pub fn move_codex_thread_workspace(
        &self,
        session: &SessionRef,
        target_cwd: &str,
    ) -> anyhow::Result<Value> {
        let target = target_cwd.trim();
        let thread_id = session.normalized_id();
        if target.is_empty() {
            return Ok(json!({
                "status": "failed",
                "session_id": thread_id,
                "message": "目标项目路径为空"
            }));
        }
        if !self.db_path.exists() {
            return Ok(json!({
                "status": "failed",
                "session_id": thread_id,
                "message": format!("database not found: {}", self.db_path.display())
            }));
        }

        let db = Connection::open(&self.db_path)?;
        if schema_kind(&db)? != Some(SchemaKind::CodexThreads)
            || !has_columns(&db, "threads", &["cwd", "rollout_path"])?
        {
            return Ok(json!({
                "status": "failed",
                "session_id": thread_id,
                "message": "unsupported local storage schema"
            }));
        }

        let timestamp_columns = codex_thread_timestamp_columns(&db)?;
        let mut columns = vec![
            "id".to_string(),
            "title".to_string(),
            "cwd".to_string(),
            "rollout_path".to_string(),
        ];
        columns.extend(timestamp_columns);
        let sql = format!(
            "SELECT {} FROM threads WHERE id = ?1",
            columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let row = match db.query_row(&sql, [&thread_id], |row| {
            let mut data = Map::new();
            for (index, column) in columns.iter().enumerate() {
                data.insert(column.clone(), sql_value_to_json(row.get_ref(index)?));
            }
            Ok(data)
        }) {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Ok(json!({
                    "status": "failed",
                    "session_id": thread_id,
                    "message": "thread not found in local storage"
                }));
            }
            Err(error) => return Err(error.into()),
        };

        let previous_cwd = row
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let rollout_path = row
            .get("rollout_path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        db.execute(
            "UPDATE threads SET cwd = ?1 WHERE id = ?2",
            (target, thread_id.as_str()),
        )?;
        let (rollout_updated, rollout_error) =
            update_rollout_session_meta_cwd(&rollout_path, &thread_id, target);
        let mut payload = json!({
            "status": "moved",
            "session_id": thread_id,
            "message": "已移动对话",
            "previous_cwd": previous_cwd,
            "target_cwd": target,
            "rollout_updated": rollout_updated,
            "rollout_error": rollout_error
        });
        if let Some(object) = payload.as_object_mut() {
            add_timestamp_payload(object, &row);
        }
        Ok(payload)
    }

    pub fn codex_thread_sort_key(&self, session: &SessionRef) -> anyhow::Result<Value> {
        let thread_id = session.normalized_id();
        if !self.db_path.exists() {
            return Ok(json!({
                "status": "failed",
                "session_id": thread_id,
                "message": format!("database not found: {}", self.db_path.display())
            }));
        }
        let db = Connection::open(&self.db_path)?;
        if schema_kind(&db)? != Some(SchemaKind::CodexThreads) {
            return Ok(json!({
                "status": "failed",
                "session_id": thread_id,
                "message": "unsupported local storage schema"
            }));
        }
        match fetch_thread_timestamp_payload(&db, &thread_id)? {
            Some(mut payload) => {
                payload.insert("source".to_string(), json!("sqlite"));
                payload.insert("status".to_string(), json!("ok"));
                payload.insert("session_id".to_string(), json!(thread_id));
                Ok(Value::Object(payload))
            }
            None => {
                let sessions_dir = sessions_dir_for(&self.db_path);
                match rollout_fallback_timestamp_payload(&sessions_dir, &thread_id) {
                    Some(mut payload) => {
                        payload.insert("status".to_string(), json!("ok"));
                        payload.insert("session_id".to_string(), json!(thread_id));
                        payload.insert("source".to_string(), json!("rollout_fallback"));
                        Ok(Value::Object(payload))
                    }
                    None => Ok(json!({
                        "status": "failed",
                        "session_id": thread_id,
                        "message": "thread not found in local storage"
                    })),
                }
            }
        }
    }

    pub fn codex_thread_sort_keys(&self, sessions: &[SessionRef]) -> anyhow::Result<Value> {
        let unique_ids: Vec<String> = sessions
            .iter()
            .map(SessionRef::normalized_id)
            .filter(|id| !id.is_empty())
            .fold(Vec::<String>::new(), |mut acc, id| {
                if !acc.contains(&id) {
                    acc.push(id);
                }
                acc
            });
        let requested = unique_ids.len();
        let truncated = requested > MAX_SORT_KEY_BATCH;
        let thread_ids: Vec<String> = unique_ids.into_iter().take(MAX_SORT_KEY_BATCH).collect();

        if !self.db_path.exists() {
            return Ok(json!({
                "status": "failed",
                "message": format!("database not found: {}", self.db_path.display()),
                "sort_keys": [],
                "requested": requested,
                "returned": 0,
                "truncated": truncated,
                "max_batch": MAX_SORT_KEY_BATCH,
            }));
        }
        if thread_ids.is_empty() {
            return Ok(json!({
                "status": "ok",
                "sort_keys": [],
                "requested": requested,
                "returned": 0,
                "truncated": truncated,
                "max_batch": MAX_SORT_KEY_BATCH,
            }));
        }

        let db = Connection::open(&self.db_path)?;
        if schema_kind(&db)? != Some(SchemaKind::CodexThreads) {
            return Ok(json!({
                "status": "failed",
                "message": "unsupported local storage schema",
                "sort_keys": [],
                "requested": requested,
                "returned": 0,
                "truncated": truncated,
                "max_batch": MAX_SORT_KEY_BATCH,
            }));
        }
        let sessions_dir = sessions_dir_for(&self.db_path);
        let fallback_map = collect_rollout_timestamps(&sessions_dir);
        let mut sort_keys = Vec::new();
        for thread_id in thread_ids {
            if let Some(mut payload) = fetch_thread_timestamp_payload(&db, &thread_id)? {
                payload.insert("session_id".to_string(), json!(thread_id));
                payload.insert("source".to_string(), json!("sqlite"));
                sort_keys.push(Value::Object(payload));
            } else if let Some((created_at_ms, updated_at_ms)) =
                fallback_map.get(&thread_id).copied()
            {
                let mut payload = Map::new();
                payload.insert("session_id".to_string(), json!(thread_id));
                payload.insert("updated_at".to_string(), Value::Null);
                payload.insert("updated_at_ms".to_string(), json!(updated_at_ms));
                payload.insert("created_at_ms".to_string(), json!(created_at_ms));
                payload.insert("source".to_string(), json!("rollout_fallback"));
                sort_keys.push(Value::Object(payload));
            }
        }
        let returned = sort_keys.len();
        Ok(json!({
            "status": "ok",
            "sort_keys": sort_keys,
            "requested": requested,
            "returned": returned,
            "truncated": truncated,
            "max_batch": MAX_SORT_KEY_BATCH,
        }))
    }

    fn delete_generic_session(
        &self,
        db: &mut Connection,
        session: &SessionRef,
    ) -> anyhow::Result<DeleteResult> {
        let session_id = session.normalized_id();
        let sessions = select_rows(db, "sessions", "id = ?1", &[&session_id])?;
        if sessions.is_empty() {
            return Ok(not_found(&session_id, "session not found in local storage"));
        }

        let mut tables = Map::new();
        tables.insert("sessions".to_string(), Value::Array(sessions));
        if has_table(db, "messages")? {
            let messages = select_rows(db, "messages", "session_id = ?1", &[&session_id])?;
            tables.insert("messages".to_string(), Value::Array(messages));
        }

        let token = self.write_backup(&session_id, "generic_sessions", tables.clone())?;
        let backup_path = self.backup_path(&token)?;

        let tx = db.transaction()?;
        if has_table(&tx, "messages")? {
            tx.execute("DELETE FROM messages WHERE session_id = ?1", [&session_id])?;
        }
        tx.execute("DELETE FROM sessions WHERE id = ?1", [&session_id])?;
        tx.commit()?;

        Ok(deleted(&session_id, token, backup_path))
    }

    fn delete_codex_thread(
        &self,
        db: &mut Connection,
        session: &SessionRef,
    ) -> anyhow::Result<DeleteResult> {
        let thread_id = session.normalized_id();
        let threads = select_rows(db, "threads", "id = ?1", &[&thread_id])?;
        if threads.is_empty() {
            return Ok(not_found(&thread_id, "thread not found in local storage"));
        }

        let file_backups = rollout_file_backups(&threads);
        let mut tables = Map::new();
        tables.insert("threads".to_string(), Value::Array(threads));
        backup_related_rows(
            db,
            &mut tables,
            "thread_dynamic_tools",
            "thread_id = ?1",
            &[&thread_id],
        )?;
        backup_related_rows(
            db,
            &mut tables,
            "thread_goals",
            "thread_id = ?1",
            &[&thread_id],
        )?;
        backup_related_rows(
            db,
            &mut tables,
            "thread_spawn_edges",
            "parent_thread_id = ?1 OR child_thread_id = ?1",
            &[&thread_id],
        )?;
        backup_related_rows(
            db,
            &mut tables,
            "stage1_outputs",
            "thread_id = ?1",
            &[&thread_id],
        )?;
        backup_related_rows(
            db,
            &mut tables,
            "agent_job_items",
            "assigned_thread_id = ?1",
            &[&thread_id],
        )?;
        if !file_backups.is_empty() {
            tables.insert("__files".to_string(), Value::Array(file_backups.clone()));
        }
        let session_index_backups = session_index_backups(&self.db_path, &thread_id);
        if !session_index_backups.is_empty() {
            tables.insert(
                "__session_index".to_string(),
                Value::Array(session_index_backups.clone()),
            );
        }

        let token = self.write_backup(&thread_id, "codex_threads", tables)?;
        let backup_path = self.backup_path(&token)?;

        let tx = db.transaction()?;
        delete_related_rows(&tx, "thread_dynamic_tools", "thread_id = ?1", &[&thread_id])?;
        delete_related_rows(&tx, "thread_goals", "thread_id = ?1", &[&thread_id])?;
        delete_related_rows(
            &tx,
            "thread_spawn_edges",
            "parent_thread_id = ?1 OR child_thread_id = ?1",
            &[&thread_id],
        )?;
        delete_related_rows(&tx, "stage1_outputs", "thread_id = ?1", &[&thread_id])?;
        if has_table(&tx, "agent_job_items")?
            && has_columns(&tx, "agent_job_items", &["assigned_thread_id"])?
        {
            tx.execute(
                "UPDATE agent_job_items SET assigned_thread_id = NULL WHERE assigned_thread_id = ?1",
                [&thread_id],
            )?;
        }
        tx.execute("DELETE FROM threads WHERE id = ?1", [&thread_id])?;
        tx.commit()?;

        let file_errors = remove_rollout_files(&file_backups);
        if !file_errors.is_empty() {
            return Ok(failed_with_backup(
                &thread_id,
                format!(
                    "本地数据库已删除，但 rollout 文件删除失败：{}",
                    file_errors.join("; ")
                ),
                Some(backup_path),
                Some(token),
            ));
        }
        let index_errors = remove_session_index_entries(&session_index_backups, &thread_id);
        if !index_errors.is_empty() {
            return Ok(failed_with_backup(
                &thread_id,
                format!(
                    "本地数据库已删除，但 Codex 会话索引更新失败：{}",
                    index_errors.join("; ")
                ),
                Some(backup_path),
                Some(token),
            ));
        }

        Ok(deleted(&thread_id, token, backup_path))
    }

    fn write_backup(
        &self,
        session_id: &str,
        schema: &str,
        tables: Map<String, Value>,
    ) -> anyhow::Result<String> {
        fs::create_dir_all(&self.backup_dir)?;
        let token = format!(
            "{}-{}",
            sanitize_token_part(session_id),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let payload = BackupPayload {
            session_id: session_id.to_string(),
            db_path: self.db_path.clone(),
            schema: schema.to_string(),
            tables,
        };
        let backup_path = self.backup_path(&token)?;
        fs::write(&backup_path, serde_json::to_vec_pretty(&payload)?)?;
        Ok(token)
    }

    fn backup_path(&self, token: &str) -> anyhow::Result<PathBuf> {
        if token.is_empty() || token.contains('/') || token.contains('\\') || token.contains("..") {
            return Err(anyhow!("invalid undo token"));
        }
        Ok(self.backup_dir.join(format!("{token}.json")))
    }

    fn recycle_entry_from_path(&self, path: &Path) -> RecycleBinEntry {
        let token = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let deleted_at = path
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        let raw = fs::read_to_string(path);
        let payload = raw
            .as_deref()
            .ok()
            .and_then(|text| serde_json::from_str::<BackupPayload>(text).ok());
        match payload {
            Some(payload) => {
                let db_matches = payload.db_path == self.db_path;
                RecycleBinEntry {
                    token,
                    session_id: payload.session_id,
                    title: backup_title(&payload.tables),
                    project_cwd: backup_project_cwd(&payload.tables),
                    schema: payload.schema,
                    db_path: payload.db_path,
                    backup_path: path.to_path_buf(),
                    deleted_at,
                    last_active_at: backup_last_active_at(&payload.tables),
                    recoverable: db_matches,
                    status: if db_matches {
                        "可恢复".to_string()
                    } else {
                        "数据库不匹配".to_string()
                    },
                }
            }
            None => RecycleBinEntry {
                session_id: token_session_id(&token),
                token,
                title: None,
                project_cwd: None,
                schema: "unknown".to_string(),
                db_path: self.db_path.clone(),
                backup_path: path.to_path_buf(),
                deleted_at,
                last_active_at: None,
                recoverable: false,
                status: "备份无法解析".to_string(),
            },
        }
    }
}

fn sample_thread_ids(db: &Connection) -> anyhow::Result<Vec<String>> {
    if !has_table(db, "threads")? {
        return Ok(Vec::new());
    }
    let mut stmt = db.prepare("SELECT id FROM threads ORDER BY updated_at_ms DESC, updated_at DESC, created_at_ms DESC LIMIT 8")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

fn select_rows(
    db: &Connection,
    table: &str,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> anyhow::Result<Vec<Value>> {
    let columns = table_columns(db, table)?;
    let sql = format!(
        "SELECT {} FROM {} WHERE {where_clause}",
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        quote_identifier(table)
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt
        .query_map(params, |row| {
            let mut object = Map::new();
            for (index, column) in columns.iter().enumerate() {
                object.insert(column.clone(), sql_value_to_json(row.get_ref(index)?));
            }
            Ok(Value::Object(object))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn backup_related_rows(
    db: &Connection,
    tables: &mut Map<String, Value>,
    table: &str,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> anyhow::Result<()> {
    if has_table(db, table)? {
        let rows = select_rows(db, table, where_clause, params)?;
        if !rows.is_empty() {
            tables.insert(table.to_string(), Value::Array(rows));
        }
    }
    Ok(())
}

fn delete_related_rows(
    db: &Connection,
    table: &str,
    where_clause: &str,
    params: &[&dyn ToSql],
) -> anyhow::Result<()> {
    if has_table(db, table)? {
        let sql = format!(
            "DELETE FROM {} WHERE {where_clause}",
            quote_identifier(table)
        );
        db.execute(&sql, params)?;
    }
    Ok(())
}

fn restore_tables(db: &mut Connection, tables: &Map<String, Value>) -> anyhow::Result<()> {
    validate_restore_tables(db, tables)?;
    let tx = db.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    for table in restore_table_order(tables) {
        let Some(rows) = tables.get(&table) else {
            continue;
        };
        if table.starts_with("__") {
            continue;
        }
        let Some(rows) = rows.as_array() else {
            continue;
        };
        for row in rows {
            let Some(row) = row.as_object() else {
                continue;
            };
            insert_row(&tx, &table, row)?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn restore_table_order(tables: &Map<String, Value>) -> Vec<String> {
    let preferred = [
        "sessions",
        "threads",
        "messages",
        "thread_dynamic_tools",
        "thread_goals",
        "thread_spawn_edges",
        "stage1_outputs",
        "agent_job_items",
    ];
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for table in preferred {
        if tables.contains_key(table) && seen.insert(table.to_string()) {
            ordered.push(table.to_string());
        }
    }
    let mut rest = tables
        .keys()
        .filter(|table| !table.starts_with("__"))
        .filter(|table| seen.insert((*table).clone()))
        .cloned()
        .collect::<Vec<_>>();
    rest.sort();
    ordered.extend(rest);
    ordered
}

fn validate_restore_tables(db: &Connection, tables: &Map<String, Value>) -> anyhow::Result<()> {
    for (table, rows) in tables {
        if table.starts_with("__") {
            continue;
        }
        if !has_table(db, table)? {
            return Err(anyhow!("cannot restore missing table {table}"));
        }
        let existing: HashSet<String> = table_columns(db, table)?.into_iter().collect();
        let Some(rows) = rows.as_array() else {
            continue;
        };
        for row in rows {
            let Some(row) = row.as_object() else {
                continue;
            };
            for column in row.keys() {
                if !existing.contains(column) {
                    return Err(anyhow!("cannot restore missing column {table}.{column}"));
                }
            }
        }
    }
    Ok(())
}

fn insert_row(db: &Connection, table: &str, row: &Map<String, Value>) -> anyhow::Result<()> {
    if row.is_empty() {
        return Ok(());
    }
    if table == "agent_job_items" && update_existing_agent_job_item(db, row)? {
        return Ok(());
    }
    let columns = row.keys().cloned().collect::<Vec<_>>();
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>();
    let values = columns
        .iter()
        .map(|column| OwnedSqlValue(json_to_sql_value(&row[column])))
        .collect::<Vec<_>>();
    let params = values
        .iter()
        .map(|value| value as &dyn ToSql)
        .collect::<Vec<_>>();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_identifier(table),
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        placeholders.join(", ")
    );
    db.execute(&sql, params.as_slice())?;
    Ok(())
}

fn update_existing_agent_job_item(
    db: &Connection,
    row: &Map<String, Value>,
) -> anyhow::Result<bool> {
    let Some(id) = row.get("id") else {
        return Ok(false);
    };
    if !row.contains_key("assigned_thread_id") || !has_table(db, "agent_job_items")? {
        return Ok(false);
    }
    let id_value = OwnedSqlValue(json_to_sql_value(id));
    let current = db.query_row(
        "SELECT assigned_thread_id FROM agent_job_items WHERE id = ?1 LIMIT 1",
        [&id_value as &dyn ToSql],
        |row| row.get::<_, Option<String>>(0),
    );
    let current = match current {
        Ok(value) => value,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if current.is_some() {
        return Err(anyhow!("restore conflict: agent job item already assigned"));
    }
    let assigned = OwnedSqlValue(json_to_sql_value(&row["assigned_thread_id"]));
    db.execute(
        "UPDATE agent_job_items SET assigned_thread_id = ?1 WHERE id = ?2 AND assigned_thread_id IS NULL",
        [&assigned as &dyn ToSql, &id_value as &dyn ToSql],
    )?;
    Ok(true)
}

fn rollout_file_backups(thread_rows: &[Value]) -> Vec<Value> {
    thread_rows
        .iter()
        .filter_map(|row| row.get("rollout_path").and_then(Value::as_str))
        .filter(|path| !path.trim().is_empty())
        .filter_map(|path| {
            let bytes = fs::read(path).ok()?;
            Some(json!({
                "path": path,
                "content_hex": encode_hex(&bytes),
            }))
        })
        .collect()
}

fn remove_rollout_files(files: &[Value]) -> Vec<String> {
    let mut errors = Vec::new();
    for file in files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        if let Err(error) = fs::remove_file(path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("{path}: {error}"));
            }
        }
    }
    errors
}

fn restore_files(tables: &Map<String, Value>) -> anyhow::Result<()> {
    let Some(files) = tables.get("__files").and_then(Value::as_array) else {
        return Ok(());
    };
    for file in files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        let Some(content) = file.get("content_hex").and_then(Value::as_str) else {
            continue;
        };
        let path = Path::new(path);
        if path.exists() {
            return Err(anyhow!(
                "restore conflict: file already exists: {}",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, decode_hex(content)?)?;
    }
    Ok(())
}

fn session_index_backups(db_path: &Path, thread_id: &str) -> Vec<Value> {
    let Some(index_path) = session_index_path(db_path) else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(&index_path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| session_index_line_id(line).as_deref() == Some(thread_id))
        .map(|line| {
            json!({
                "path": index_path,
                "line": line,
            })
        })
        .collect()
}

fn remove_session_index_entries(entries: &[Value], thread_id: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let paths = entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    for path in paths {
        let path = Path::new(path);
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let mut changed = false;
        let kept = text
            .lines()
            .filter(|line| {
                let should_remove = session_index_line_id(line).as_deref() == Some(thread_id);
                changed |= should_remove;
                !should_remove
            })
            .collect::<Vec<_>>();
        if changed {
            let next = if kept.is_empty() {
                String::new()
            } else {
                format!("{}\n", kept.join("\n"))
            };
            if let Err(error) = fs::write(path, next) {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    errors
}

fn restore_session_index_entries(tables: &Map<String, Value>) -> anyhow::Result<()> {
    let Some(entries) = tables.get("__session_index").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut by_path = HashSet::new();
    for path in entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
    {
        by_path.insert(path.to_string());
    }
    for path in by_path {
        let path = PathBuf::from(path);
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let restore_lines = entries
            .iter()
            .filter(|entry| {
                entry.get("path").and_then(Value::as_str) == Some(path.to_string_lossy().as_ref())
            })
            .filter_map(|entry| entry.get("line").and_then(Value::as_str))
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();
        if restore_lines.is_empty() {
            continue;
        }
        let restore_ids = restore_lines
            .iter()
            .filter_map(|line| session_index_line_id(line))
            .collect::<HashSet<_>>();
        let mut lines = existing
            .lines()
            .filter(|line| {
                session_index_line_id(line)
                    .map(|id| !restore_ids.contains(&id))
                    .unwrap_or(true)
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        lines.extend(restore_lines.into_iter().map(ToString::to_string));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &path,
            if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n", lines.join("\n"))
            },
        )?;
    }
    Ok(())
}

fn session_index_path(db_path: &Path) -> Option<PathBuf> {
    if db_path.file_name().and_then(|name| name.to_str()) != Some("state_5.sqlite") {
        return None;
    }
    Some(db_path.parent()?.join("session_index.jsonl"))
}

fn session_index_line_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn update_rollout_session_meta_cwd(
    rollout_path: &str,
    thread_id: &str,
    target_cwd: &str,
) -> (bool, String) {
    if rollout_path.trim().is_empty() || !Path::new(rollout_path).is_file() {
        return (false, String::new());
    }
    let result = (|| -> anyhow::Result<bool> {
        let text = fs::read_to_string(rollout_path)?;
        let mut changed = false;
        let mut output = String::new();
        for line in text.split_inclusive('\n') {
            let (body, end) = line
                .strip_suffix('\n')
                .map_or((line, ""), |body| (body, "\n"));
            let mut raw = line.to_string();
            if let Ok(mut item) = serde_json::from_str::<Value>(body) {
                if item.get("type") == Some(&json!("session_meta"))
                    && item["payload"]["id"] == thread_id
                    && item["payload"]["cwd"] != target_cwd
                    && let Some(payload) = item.get_mut("payload").and_then(Value::as_object_mut)
                {
                    payload.insert("cwd".to_string(), json!(target_cwd));
                    raw = serde_json::to_string(&item)? + end;
                    changed = true;
                }
            }
            output.push_str(&raw);
        }
        if changed {
            fs::write(rollout_path, output)?;
        }
        Ok(changed)
    })();
    match result {
        Ok(changed) => (changed, String::new()),
        Err(error) => (false, error.to_string()),
    }
}

fn codex_thread_timestamp_columns(db: &Connection) -> anyhow::Result<Vec<String>> {
    let existing = table_columns(db, "threads")?
        .into_iter()
        .collect::<HashSet<_>>();
    Ok(["updated_at", "updated_at_ms", "created_at_ms"]
        .iter()
        .filter(|column| existing.contains(**column))
        .map(|column| column.to_string())
        .collect())
}

fn fetch_thread_timestamp_payload(
    db: &Connection,
    thread_id: &str,
) -> anyhow::Result<Option<Map<String, Value>>> {
    let timestamp_columns = codex_thread_timestamp_columns(db)?;
    let mut columns = vec!["id".to_string()];
    columns.extend(timestamp_columns);
    let sql = format!(
        "SELECT {} FROM threads WHERE id = ?1",
        columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let row = db.query_row(&sql, [thread_id], |row| {
        let mut selected = Map::new();
        for (index, column) in columns.iter().enumerate() {
            selected.insert(column.clone(), sql_value_to_json(row.get_ref(index)?));
        }
        Ok(selected)
    });
    match row {
        Ok(row) => {
            let mut payload = Map::new();
            add_timestamp_payload(&mut payload, &row);
            Ok(Some(payload))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn sessions_dir_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("sessions")
}

fn collect_rollout_timestamps(
    sessions_dir: &Path,
) -> std::collections::HashMap<String, (i64, i64)> {
    let mut map = std::collections::HashMap::new();
    if !sessions_dir.exists() {
        return map;
    }
    walk_rollout_dir(sessions_dir, &mut map);
    map
}

fn walk_rollout_dir(dir: &Path, out: &mut std::collections::HashMap<String, (i64, i64)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rollout_dir(&path, out);
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some((thread_id, created_at_ms)) = parse_rollout_filename(name) else {
            continue;
        };
        let updated_at_ms = path
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(created_at_ms);
        out.insert(thread_id, (created_at_ms, updated_at_ms));
    }
}

fn days_from_civil_utc(y: i32, m: u32, d: u32) -> i64 {
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = y - era * 400;
    let mm = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mm as i64 + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn parse_rollout_filename(name: &str) -> Option<(String, i64)> {
    let stem = name.strip_suffix(".jsonl")?;
    let body = stem.strip_prefix("rollout-")?;
    if body.len() < 20 {
        return None;
    }
    let (ts_part, rest) = body.split_at(19);
    let thread_id = rest.strip_prefix('-')?.to_string();
    let bytes = ts_part.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b'-'
        || bytes[16] != b'-'
    {
        return None;
    }
    let year: i32 = ts_part[0..4].parse().ok()?;
    let month: u32 = ts_part[5..7].parse().ok()?;
    let day: u32 = ts_part[8..10].parse().ok()?;
    let hour: u32 = ts_part[11..13].parse().ok()?;
    let minute: u32 = ts_part[14..16].parse().ok()?;
    let second: u32 = ts_part[17..19].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil_utc(year, month, day);
    let ms = days * 86_400_000
        + hour as i64 * 3_600_000
        + minute as i64 * 60_000
        + second as i64 * 1_000;
    Some((thread_id, ms))
}

fn rollout_fallback_timestamp_payload(
    sessions_dir: &Path,
    thread_id: &str,
) -> Option<Map<String, Value>> {
    let map = collect_rollout_timestamps(sessions_dir);
    let (created_at_ms, updated_at_ms) = map.get(thread_id).copied()?;
    let mut payload = Map::new();
    payload.insert("updated_at".to_string(), Value::Null);
    payload.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    payload.insert("created_at_ms".to_string(), json!(created_at_ms));
    Some(payload)
}

fn add_timestamp_payload(payload: &mut Map<String, Value>, row: &Map<String, Value>) {
    for column in ["updated_at", "updated_at_ms", "created_at_ms"] {
        payload.insert(
            column.to_string(),
            row.get(column).cloned().unwrap_or(Value::Null),
        );
    }
}

fn backup_title(tables: &Map<String, Value>) -> Option<String> {
    ["threads", "sessions"].iter().find_map(|table| {
        tables
            .get(*table)
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .and_then(Value::as_object)
            .and_then(|row| row.get("title"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(ToString::to_string)
    })
}

fn backup_project_cwd(tables: &Map<String, Value>) -> Option<String> {
    backup_first_row(tables, "threads")
        .and_then(|row| row.get("cwd"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(ToString::to_string)
}

fn backup_last_active_at(tables: &Map<String, Value>) -> Option<u64> {
    let row = backup_first_row(tables, "threads")?;
    ["updated_at_ms", "updated_at", "created_at_ms"]
        .iter()
        .find_map(|column| timestamp_seconds(row.get(*column)?))
}

fn backup_first_row<'a>(
    tables: &'a Map<String, Value>,
    table: &str,
) -> Option<&'a Map<String, Value>> {
    tables
        .get(table)
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(Value::as_object)
}

fn timestamp_seconds(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64().map(|value| {
            if value > 10_000_000_000 {
                value / 1000
            } else {
                value
            }
        }),
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else if let Ok(number) = trimmed.parse::<u64>() {
                Some(if number > 10_000_000_000 {
                    number / 1000
                } else {
                    number
                })
            } else {
                parse_rfc3339_seconds(trimmed)
            }
        }
        _ => None,
    }
}

fn parse_rfc3339_seconds(value: &str) -> Option<u64> {
    let date_time = value.strip_suffix('Z').unwrap_or(value);
    let (date, time) = date_time.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i64>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let time = time.split(['+', '-']).next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u64>().ok()?;
    let minute = time_parts.next()?.parse::<u64>().ok()?;
    let second = time_parts
        .next()
        .and_then(|part| part.split('.').next())
        .unwrap_or("0")
        .parse::<u64>()
        .ok()?;
    let days = days_from_civil(year, month, day)?;
    u64::try_from(days)
        .ok()?
        .checked_mul(86_400)?
        .checked_add(hour.checked_mul(3_600)?)?
        .checked_add(minute.checked_mul(60)?)?
        .checked_add(second)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn token_session_id(token: &str) -> String {
    token
        .rsplit_once('-')
        .map(|(session_id, _)| session_id)
        .unwrap_or(token)
        .to_string()
}

fn sql_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => json!(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => json!({
            "$blob": encode_hex(value),
        }),
    }
}

fn json_to_sql_value(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(value) => SqlValue::Integer(i64::from(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                SqlValue::Integer(value)
            } else if let Some(value) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
                SqlValue::Integer(value)
            } else {
                SqlValue::Real(value.as_f64().unwrap_or_default())
            }
        }
        Value::String(value) => SqlValue::Text(value.clone()),
        Value::Array(_) | Value::Object(_) => {
            if let Some(blob) = value.get("$blob").and_then(Value::as_str) {
                SqlValue::Blob(decode_hex(blob).unwrap_or_default())
            } else {
                SqlValue::Text(value.to_string())
            }
        }
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sanitize_token_part(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').chars().take(48).collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(value: &str) -> anyhow::Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err(anyhow!("invalid hex blob"));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let raw = value.as_bytes();
    for pair in raw.chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_digit(byte: u8) -> anyhow::Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(anyhow!("invalid hex blob")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

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
    fn finds_archived_thread_moves_workspace_and_reads_sort_keys() {
        let db_path = unique_temp_path("thread-ops");
        let backup_dir = db_path.with_extension("undo");
        let rollout_path = unique_temp_path("thread-ops-rollout");
        fs::write(
            &rollout_path,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"t1\",\"cwd\":\"/old/project\",\"title\":\"Thread One\"}}\n{\"type\":\"session_meta\",\"payload\":{\"id\":\"t2\",\"cwd\":\"/other/project\"}}\n",
        )
        .unwrap();
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                title TEXT,
                rollout_path TEXT,
                cwd TEXT,
                archived INTEGER,
                archived_at INTEGER,
                updated_at INTEGER,
                updated_at_ms INTEGER,
                created_at_ms INTEGER
            )",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES ('t1', 'Thread One', ?1, '/old/project', 1, 123, 10, 10000, 1)",
            [rollout_path.to_string_lossy().as_ref()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO threads VALUES ('t2', 'Thread Two', ?1, '/other/project', 0, NULL, 20, 20000, 2)",
            [rollout_path.to_string_lossy().as_ref()],
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);
        assert_eq!(
            adapter
                .find_archived_thread_by_title("Thread One 2026年5月19日，11:00")
                .unwrap(),
            Some(SessionRef::new("t1", Some("Thread One".to_string())))
        );

        let moved = adapter
            .move_codex_thread_workspace(
                &SessionRef::new("local:t1", Some("Thread One".to_string())),
                "/new/project",
            )
            .unwrap();
        assert_eq!(moved["status"], "moved");
        assert_eq!(moved["previous_cwd"], "/old/project");
        assert_eq!(moved["target_cwd"], "/new/project");
        assert_eq!(moved["rollout_updated"], true);
        assert_eq!(moved["updated_at_ms"], 10000);
        let rollout = fs::read_to_string(&rollout_path).unwrap();
        assert!(rollout.contains("\"cwd\":\"/new/project\""));
        assert!(rollout.contains("\"id\":\"t2\",\"cwd\":\"/other/project\""));

        assert_eq!(
            adapter
                .codex_thread_sort_key(&SessionRef::new("t1", None))
                .unwrap(),
            json!({
                "source": "sqlite",
                "status": "ok",
                "session_id": "t1",
                "updated_at": 10,
                "updated_at_ms": 10000,
                "created_at_ms": 1
            })
        );
        assert_eq!(
            adapter
                .codex_thread_sort_keys(&[
                    SessionRef::new("t2", None),
                    SessionRef::new("local:t1", None),
                    SessionRef::new("t2", None),
                ])
                .unwrap(),
            json!({
                "status": "ok",
                "sort_keys": [
                    {"session_id": "t2", "updated_at": 20, "updated_at_ms": 20000, "created_at_ms": 2, "source": "sqlite"},
                    {"session_id": "t1", "updated_at": 10, "updated_at_ms": 10000, "created_at_ms": 1, "source": "sqlite"}
                ],
                "requested": 2,
                "returned": 2,
                "truncated": false,
                "max_batch": MAX_SORT_KEY_BATCH,
            })
        );

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
    }

    #[test]
    fn sort_keys_truncates_at_max_batch_with_explicit_marker() {
        let db_path = unique_temp_path("sort-keys-truncate");
        let backup_dir = db_path.with_extension("undo");
        let db = Connection::open(&db_path).unwrap();
        db.execute(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                title TEXT,
                rollout_path TEXT,
                updated_at_ms INTEGER,
                created_at_ms INTEGER
            )",
            [],
        )
        .unwrap();
        drop(db);

        let adapter = SQLiteStorageAdapter::with_backup_dir(db_path.clone(), backup_dir);

        let over_limit = MAX_SORT_KEY_BATCH + 5;
        let sessions: Vec<SessionRef> = (0..over_limit)
            .map(|i| SessionRef::new(&format!("t{i}"), None))
            .collect();
        let result = adapter.codex_thread_sort_keys(&sessions).unwrap();

        assert_eq!(result["status"], "ok");
        assert_eq!(result["requested"], over_limit);
        assert_eq!(result["truncated"], true);
        assert_eq!(result["max_batch"], MAX_SORT_KEY_BATCH);
        let returned = result["returned"].as_u64().unwrap() as usize;
        assert!(returned <= MAX_SORT_KEY_BATCH);
        assert_eq!(returned, result["sort_keys"].as_array().unwrap().len());

        let under_limit: Vec<SessionRef> = (0..3)
            .map(|i| SessionRef::new(&format!("t{i}"), None))
            .collect();
        let small = adapter.codex_thread_sort_keys(&under_limit).unwrap();
        assert_eq!(small["requested"], 3);
        assert_eq!(small["truncated"], false);
        assert_eq!(small["max_batch"], MAX_SORT_KEY_BATCH);

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_dir_all(adapter.backup_dir());
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

    #[test]
    fn parse_rollout_filename_extracts_id_and_timestamp() {
        let (id, ms) = parse_rollout_filename(
            "rollout-2026-03-20T10-56-02-019d092b-dd6b-7f32-8954-5968e66b372a.jsonl",
        )
        .expect("should parse");
        assert_eq!(id, "019d092b-dd6b-7f32-8954-5968e66b372a");
        assert_eq!(ms, 1_774_004_162_000);
    }

    #[test]
    fn parse_rollout_filename_rejects_garbage() {
        assert!(parse_rollout_filename("garbage.jsonl").is_none());
        assert!(parse_rollout_filename("rollout-not-a-date.jsonl").is_none());
        assert!(parse_rollout_filename("rollout-2026-03-20T10-56-02.jsonl").is_none());
    }

    #[test]
    fn rollout_fallback_finds_existing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let nested = tmp.path().join("2026").join("03").join("20");
        fs::create_dir_all(&nested).unwrap();
        let fname = "rollout-2026-03-20T10-56-02-019d092b-dd6b-7f32-8954-5968e66b372a.jsonl";
        fs::write(nested.join(fname), b"{}").unwrap();

        let payload =
            rollout_fallback_timestamp_payload(tmp.path(), "019d092b-dd6b-7f32-8954-5968e66b372a")
                .expect("should find");
        assert_eq!(payload["created_at_ms"], 1_774_004_162_000_i64);
        assert!(payload["updated_at_ms"].is_i64());
    }

    #[test]
    fn rollout_fallback_returns_none_for_missing_id() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(rollout_fallback_timestamp_payload(tmp.path(), "no-such-id").is_none());
    }
}
