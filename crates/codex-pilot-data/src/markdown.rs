use crate::storage::{SchemaKind, SessionRef, has_columns, normalize_session_id, schema_kind};
use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Exported,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportResult {
    pub status: ExportStatus,
    pub session_id: String,
    pub message: String,
    pub filename: Option<String>,
    pub markdown: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MarkdownExportService {
    db_path: PathBuf,
}

#[derive(Debug)]
struct Message {
    speaker: String,
    timestamp: Option<String>,
    body: String,
}

impl MarkdownExportService {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn export(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        if !self.db_path.exists() {
            return Ok(failed(
                &session.normalized_id(),
                format!("database not found: {}", self.db_path.display()),
            ));
        }

        let db = Connection::open(&self.db_path)?;
        match schema_kind(&db)? {
            Some(SchemaKind::GenericSessions) => export_generic_session(&db, session),
            Some(SchemaKind::CodexThreads) => export_codex_thread(&db, session),
            None => Ok(failed(
                &session.normalized_id(),
                "unsupported local storage schema",
            )),
        }
    }
}

fn export_generic_session(db: &Connection, session: &SessionRef) -> anyhow::Result<ExportResult> {
    let session_id = session.normalized_id();
    let title = match fetch_optional_title(db, "sessions", "id", &session_id)? {
        Some(title) => display_title(&title),
        None => return Ok(not_found(&session_id, "session not found in local storage")),
    };
    let messages = fetch_generic_messages(db, &session_id)?;
    let markdown = render_markdown(&title, &messages);
    let filename = build_filename(&title, &session_id);
    Ok(exported(session_id, filename, markdown))
}

fn export_codex_thread(db: &Connection, session: &SessionRef) -> anyhow::Result<ExportResult> {
    let thread_id = normalize_session_id(&session.id);
    let row = db.query_row(
        "SELECT title, rollout_path FROM threads WHERE id = ?1",
        [&thread_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        },
    );
    let (title, rollout_path) = match row {
        Ok(row) => row,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Ok(not_found(&thread_id, "thread not found in local storage"));
        }
        Err(err) => return Err(err.into()),
    };
    let title = display_title(title.as_deref().unwrap_or("Untitled session"));
    let Some(rollout_path) = rollout_path.filter(|path| !path.trim().is_empty()) else {
        return Ok(failed(&thread_id, "thread has no rollout_path"));
    };
    let messages = load_rollout_messages(Path::new(&rollout_path))
        .with_context(|| format!("read rollout {}", rollout_path))?;
    let markdown = render_markdown(&title, &messages);
    let filename = build_filename(&title, &thread_id);
    Ok(exported(thread_id, filename, markdown))
}

fn fetch_optional_title(
    db: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> anyhow::Result<Option<String>> {
    if has_columns(db, table, &["title"])? {
        let sql = format!("SELECT title FROM {table} WHERE {id_column} = ?1");
        let row = db.query_row(&sql, [id], |row| row.get::<_, Option<String>>(0));
        match row {
            Ok(title) => Ok(title),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    } else {
        let sql = format!("SELECT {id_column} FROM {table} WHERE {id_column} = ?1");
        let row = db.query_row(&sql, [id], |_| Ok(()));
        match row {
            Ok(()) => Ok(Some(id.to_string())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

fn fetch_generic_messages(db: &Connection, session_id: &str) -> anyhow::Result<Vec<Message>> {
    if !crate::storage::has_table(db, "messages")? {
        return Ok(Vec::new());
    }
    let has_role = has_columns(db, "messages", &["role"])?;
    let body_column = ["body", "content", "text"]
        .iter()
        .find(|column| has_columns(db, "messages", &[*column]).unwrap_or(false))
        .copied();
    let Some(body_column) = body_column else {
        return Ok(Vec::new());
    };
    let order_clause = if has_columns(db, "messages", &["created_at"])? {
        " ORDER BY created_at, id"
    } else if has_columns(db, "messages", &["id"])? {
        " ORDER BY id"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {}{}{} FROM messages WHERE session_id = ?1{}",
        if has_role { "role, " } else { "" },
        body_column,
        if has_columns(db, "messages", &["created_at"])? {
            ", created_at"
        } else {
            ""
        },
        order_clause
    );
    let mut stmt = db.prepare(&sql)?;
    let messages = stmt
        .query_map([session_id], |row| {
            let mut index = 0;
            let role = if has_role {
                let value = row.get::<_, Option<String>>(index)?.unwrap_or_default();
                index += 1;
                value
            } else {
                String::new()
            };
            let body = row.get::<_, Option<String>>(index)?.unwrap_or_default();
            index += 1;
            let timestamp = if has_columns(db, "messages", &["created_at"]).unwrap_or(false) {
                row.get::<_, Option<String>>(index)?
            } else {
                None
            };
            Ok(Message {
                speaker: display_role(&role),
                timestamp,
                body,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(messages
        .into_iter()
        .filter(|message| !message.body.trim().is_empty())
        .collect())
}

fn load_rollout_messages(path: &Path) -> anyhow::Result<Vec<Message>> {
    let mut messages = Vec::new();
    for raw in fs::read_to_string(path)?.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(raw)?;
        if event.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let payload = &event["payload"];
        if payload.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
        if !matches!(role, "user" | "assistant" | "system") {
            continue;
        }
        let body = serialize_message_content(&payload["content"]);
        if body.trim().is_empty() {
            continue;
        }
        messages.push(Message {
            speaker: display_role(role),
            timestamp: event
                .get("timestamp")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            body,
        });
    }
    Ok(messages)
}

fn serialize_message_content(content: &Value) -> String {
    let Some(items) = content.as_array() else {
        return String::new();
    };
    items
        .iter()
        .filter_map(|block| {
            let block_type = block.get("type").and_then(Value::as_str)?;
            match block_type {
                "input_text" | "output_text" | "text" => block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(normalize_newlines),
                "input_image" => Some("> Image attachment".to_string()),
                _ => None,
            }
        })
        .filter(|block| !block.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_markdown(title: &str, messages: &[Message]) -> String {
    let mut lines = vec![format!("# {title}"), String::new()];
    if messages.is_empty() {
        lines.push("_No messages found._".to_string());
        lines.push(String::new());
    }
    for message in messages {
        lines.push(format!("## {}", message.speaker));
        if let Some(timestamp) = message.timestamp.as_ref().filter(|value| !value.is_empty()) {
            lines.push(format!("_{timestamp}_"));
        }
        lines.push(String::new());
        lines.push(message.body.trim().to_string());
        lines.push(String::new());
    }
    format!("{}\n", lines.join("\n").trim_end())
}

fn exported(session_id: String, filename: String, markdown: String) -> ExportResult {
    ExportResult {
        status: ExportStatus::Exported,
        session_id,
        message: "session exported as Markdown".to_string(),
        filename: Some(filename),
        markdown: Some(markdown),
    }
}

fn not_found(session_id: &str, message: &str) -> ExportResult {
    ExportResult {
        status: ExportStatus::NotFound,
        session_id: session_id.to_string(),
        message: message.to_string(),
        filename: None,
        markdown: None,
    }
}

fn failed(session_id: &str, message: impl Into<String>) -> ExportResult {
    ExportResult {
        status: ExportStatus::Failed,
        session_id: session_id.to_string(),
        message: message.into(),
        filename: None,
        markdown: None,
    }
}

fn display_role(role: &str) -> String {
    match role {
        "user" => "User".to_string(),
        "assistant" => "Assistant".to_string(),
        "system" => "System".to_string(),
        value if !value.trim().is_empty() => display_title(value),
        _ => "Message".to_string(),
    }
}

fn display_title(value: &str) -> String {
    let normalized = normalize_newlines(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        "Untitled session".to_string()
    } else {
        normalized
    }
}

fn build_filename(title: &str, session_id: &str) -> String {
    let cleaned = replace_filename_chars(title, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut safe_title = cleaned
        .trim_matches([' ', '.'])
        .chars()
        .take(80)
        .collect::<String>();
    if safe_title.is_empty() {
        safe_title = "Untitled session".to_string();
    }
    format!(
        "{}-{}.md",
        safe_title,
        replace_filename_chars(session_id, "-").trim_matches('-')
    )
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn replace_filename_chars(value: &str, replacement: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            output.push_str(replacement);
        } else {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codex-pilot-data-{name}-{}.{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            extension
        ))
    }

    #[test]
    fn exports_generic_session_markdown() {
        let db_path = unique_temp_path("generic-export", "sqlite");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT, created_at TEXT);
            INSERT INTO sessions VALUES ('s1', 'Fixture');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'user', 'hello', '2026-01-01T00:00:00Z');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'assistant', 'hi', '2026-01-01T00:00:01Z');
            "#,
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export(&SessionRef::new("s1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("# Fixture"));
        assert!(markdown.contains("## User"));
        assert!(markdown.contains("hello"));
        assert!(markdown.contains("## Assistant"));
        assert!(markdown.contains("hi"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn exports_codex_rollout_markdown() {
        let db_path = unique_temp_path("codex-export", "sqlite");
        let rollout_path = unique_temp_path("codex-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-01-01T00:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
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

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export(&SessionRef::new("local:t1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("# Thread"));
        assert!(markdown.contains("## User"));
        assert!(markdown.contains("hello"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }
}
