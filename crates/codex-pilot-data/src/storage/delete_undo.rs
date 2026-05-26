use crate::storage::{
    BackupPayload, DeleteResult, DeleteStatus, SQLiteStorageAdapter, SchemaKind, SessionRef,
    deleted, failed, failed_with_backup, has_columns, has_table, not_found, quote_identifier,
    restore_files, restore_session_index_entries, restore_tables, rollout_file_backups,
    schema_kind, session_index_backups, sql_value_to_json, table_columns,
};
use anyhow::Context;
use rusqlite::{Connection, ToSql};
use serde_json::{Map, Value, json};
use std::fs;

impl SQLiteStorageAdapter {
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

        let file_errors = crate::storage::remove_rollout_files(&file_backups);
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
        let index_errors =
            crate::storage::remove_session_index_entries(&session_index_backups, &thread_id);
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
}

pub(super) fn sample_thread_ids(db: &Connection) -> anyhow::Result<Vec<String>> {
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
