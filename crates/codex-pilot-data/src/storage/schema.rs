use crate::storage::quote_identifier;
use rusqlite::Connection;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaKind {
    GenericSessions,
    CodexThreads,
}

pub(crate) fn schema_kind(db: &Connection) -> anyhow::Result<Option<SchemaKind>> {
    if has_table(db, "sessions")? && has_columns(db, "sessions", &["id"])? {
        if has_table(db, "messages")? && !has_columns(db, "messages", &["session_id"])? {
            return Ok(None);
        }
        return Ok(Some(SchemaKind::GenericSessions));
    }
    if has_table(db, "threads")? && has_columns(db, "threads", &["id", "title", "rollout_path"])? {
        return Ok(Some(SchemaKind::CodexThreads));
    }
    Ok(None)
}

pub(crate) fn has_table(db: &Connection, table: &str) -> anyhow::Result<bool> {
    Ok(db
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .is_ok())
}

pub(crate) fn has_columns(db: &Connection, table: &str, columns: &[&str]) -> anyhow::Result<bool> {
    let existing: HashSet<String> = table_columns(db, table)?.into_iter().collect();
    Ok(columns.iter().all(|column| existing.contains(*column)))
}

pub(super) fn table_columns(db: &Connection, table: &str) -> anyhow::Result<Vec<String>> {
    let quoted = quote_identifier(table);
    let mut stmt = db.prepare(&format!("PRAGMA table_info({quoted})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns)
}
