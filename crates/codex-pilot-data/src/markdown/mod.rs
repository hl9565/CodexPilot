mod export;
mod format;
mod models;
mod render;

use crate::storage::{SchemaKind, SessionRef, has_columns, normalize_session_id, schema_kind};
use anyhow::Context;
use format::{
    TextPart, exported_at_label, extract_image_src, normalize_newlines, parse_time_hhmm,
    replace_filename_chars, split_fenced_code, strip_image_tags, text_blocks,
};
pub use models::{ExportResult, ExportStatus};
use models::{Message, MessageBlock, exported, failed, not_found};
use render::{render_html, render_markdown};
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MarkdownExportService {
    db_path: PathBuf,
}

impl MarkdownExportService {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub fn export(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_markdown(session)
    }

    pub fn export_markdown(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Markdown)
    }

    pub fn export_html(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Html)
    }

    fn export_with(
        &self,
        session: &SessionRef,
        format: ExportFormat,
    ) -> anyhow::Result<ExportResult> {
        if !self.db_path.exists() {
            return Ok(failed(
                &session.normalized_id(),
                format!("database not found: {}", self.db_path.display()),
            ));
        }

        let db = Connection::open(&self.db_path)?;
        match schema_kind(&db)? {
            Some(SchemaKind::GenericSessions) => export_generic_session(&db, session, format),
            Some(SchemaKind::CodexThreads) => export_codex_thread(&db, session, format),
            None => Ok(failed(
                &session.normalized_id(),
                "unsupported local storage schema",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExportFormat {
    Markdown,
    Html,
}

fn export_generic_session(
    db: &Connection,
    session: &SessionRef,
    format: ExportFormat,
) -> anyhow::Result<ExportResult> {
    let session_id = session.normalized_id();
    let title = match fetch_optional_title(db, "sessions", "id", &session_id)? {
        Some(title) => display_title(&title),
        None => return Ok(not_found(&session_id, "session not found in local storage")),
    };
    let messages = fetch_generic_messages(db, &session_id)?;
    Ok(exported(session_id, &title, &messages, format))
}

fn export_codex_thread(
    db: &Connection,
    session: &SessionRef,
    format: ExportFormat,
) -> anyhow::Result<ExportResult> {
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
    Ok(exported(thread_id, &title, &messages, format))
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
                blocks: text_blocks(body),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(messages
        .into_iter()
        .filter(|message| !message.blocks.is_empty())
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
        let blocks = serialize_message_content(&payload["content"]);
        if blocks.is_empty() {
            continue;
        }
        messages.push(Message {
            speaker: display_role(role),
            timestamp: event
                .get("timestamp")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            blocks,
        });
    }
    Ok(messages)
}

fn serialize_message_content(content: &Value) -> Vec<MessageBlock> {
    let Some(items) = content.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .flat_map(|block| {
            let block_type = block.get("type").and_then(Value::as_str)?;
            match block_type {
                "input_text" | "output_text" | "text" => block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| text_blocks(text.to_string())),
                "input_image" => Some(vec![MessageBlock::Image(extract_image_src(block))]),
                _ => None,
            }
        })
        .flatten()
        .collect()
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

fn build_filename(title: &str, session_id: &str, extension: &str) -> String {
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
        "{}-{}.{}",
        safe_title,
        replace_filename_chars(session_id, "-").trim_matches('-'),
        extension
    )
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

    #[test]
    fn exports_html_with_escaped_content() {
        let db_path = unique_temp_path("html-export", "sqlite");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT, created_at TEXT);
            INSERT INTO sessions VALUES ('s1', 'Display <Thread>');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'user', '<script>alert(1)</script>', '2026-01-01T09:47:03Z');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'assistant', '```rust
fn main() {}
```', '2026-01-01T09:48:03Z');
            "#,
        )
        .unwrap();
        drop(db);

        let service = MarkdownExportService::new(db_path.clone());
        let result = service.export_html(&SessionRef::new("s1", None)).unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        assert_eq!(result.filename.as_deref(), Some("Display Thread-s1.html"));
        assert!(result.markdown.is_none());
        let html = result.html.unwrap();
        assert!(html.contains("CodexPilot Export"));
        assert!(html.contains("Display &lt;Thread&gt;"));
        assert!(html.contains(r#"<section class="message user">"#));
        assert!(html.contains(r#"<section class="message assistant">"#));
        assert!(html.contains(r#">You<span class="time">09:47</span>"#));
        assert!(html.contains(r#">AI<span class="time">09:48</span>"#));
        assert!(html.contains(r#"<pre class="code-block"><code>fn main() {}"#));
        assert!(!html.contains("```rust"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn exports_rollout_html_with_clean_image_attachment() {
        let db_path = unique_temp_path("html-image-export", "sqlite");
        let rollout_path = unique_temp_path("html-image-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"点了同步没反应\n<image>"},{"type":"input_image","image_url":"data:image/png;base64,abc"},{"type":"input_text","text":"</image>"}]}}"#,
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
        let result = service
            .export_html(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let html = result.html.unwrap();
        assert!(html.contains("点了同步没反应"));
        assert!(html.contains(r#"<span class="time">09:47</span>"#));
        assert!(html.contains(r#"<img src="data:image/png;base64,abc" alt="Image attachment">"#));
        assert!(!html.contains("&lt;image&gt;"));
        assert!(!html.contains("&lt;/image&gt;"));
        assert!(!html.contains("Image attachment</div>"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }

    #[test]
    fn exports_rollout_html_image_sources_and_placeholder() {
        let db_path = unique_temp_path("html-image-sources-export", "sqlite");
        let rollout_path = unique_temp_path("html-image-sources-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_image","image_url":" https://example.com/a.png "},{"type":"input_image","path":"/tmp/local-image.png"},{"type":"input_image"}]}}"#,
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
        let result = service
            .export_html(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let html = result.html.unwrap();
        assert!(html.contains(r#"<img src="https://example.com/a.png" alt="Image attachment">"#));
        assert!(html.contains(r#"<img src="/tmp/local-image.png" alt="Image attachment">"#));
        assert!(html.contains("<span>图片附件</span>"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }

    #[test]
    fn markdown_export_keeps_image_wrappers_and_uses_safe_image_placeholder() {
        let db_path = unique_temp_path("markdown-image-export", "sqlite");
        let rollout_path = unique_temp_path("markdown-image-rollout", "jsonl");
        fs::write(
            &rollout_path,
            r#"{"type":"response_item","timestamp":"2026-05-21T09:47:03.505Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"keep <image>inside</image>"},{"type":"input_image","image_url":"https://example.com/a).png"}]}}"#,
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
        let result = service
            .export_markdown(&SessionRef::new("local:t1", None))
            .unwrap();
        assert_eq!(result.status, ExportStatus::Exported);
        let markdown = result.markdown.unwrap();
        assert!(markdown.contains("keep <image>inside</image>"));
        assert!(markdown.contains("> Image attachment"));
        assert!(!markdown.contains("https://example.com/a).png"));

        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(rollout_path);
    }
}
