use crate::storage::{SchemaKind, SessionRef, has_columns, normalize_session_id, schema_kind};
use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub html: Option<String>,
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
        self.export_markdown(session)
    }

    pub fn export_markdown(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Markdown)
    }

    pub fn export_html(&self, session: &SessionRef) -> anyhow::Result<ExportResult> {
        self.export_with(session, ExportFormat::Html)
    }

    fn export_with(&self, session: &SessionRef, format: ExportFormat) -> anyhow::Result<ExportResult> {
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

fn render_html(title: &str, messages: &[Message]) -> String {
    let exported_at = exported_at_label();
    let mut sections = String::new();
    if messages.is_empty() {
        sections.push_str(
            r#"<section class="empty">No messages found.</section>"#,
        );
    }
    for message in messages {
        let timestamp = message
            .timestamp
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(|value| format!(r#"<span class="time">{}</span>"#, escape_html(value)))
            .unwrap_or_default();
        sections.push_str(&format!(
            r#"<section class="message"><aside class="speaker">{speaker}{timestamp}</aside><div class="body">{body}</div></section>"#,
            speaker = escape_html(&message.speaker),
            timestamp = timestamp,
            body = render_html_body(&message.body)
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <style>
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      background: #f6f8fb;
      color: #1f2937;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      line-height: 1.65;
    }}
    .page {{
      background: #fff;
      border: 1px solid #dde5ee;
      border-radius: 12px;
      box-shadow: 0 18px 48px rgba(15, 23, 42, 0.10);
      margin: 32px auto;
      max-width: 980px;
      overflow: hidden;
    }}
    header {{
      border-bottom: 1px solid #e5ebf2;
      padding: 28px 34px 22px;
    }}
    .brand {{
      color: #526174;
      font-size: 12px;
      font-weight: 800;
      margin-bottom: 10px;
    }}
    h1 {{
      font-size: 28px;
      line-height: 1.25;
      margin: 0 0 10px;
    }}
    .meta {{
      color: #66758a;
      display: flex;
      flex-wrap: wrap;
      font-size: 13px;
      gap: 12px;
    }}
    main {{ padding: 10px 34px 34px; }}
    .message {{
      border-bottom: 1px solid #e8edf4;
      display: grid;
      gap: 18px;
      grid-template-columns: 116px minmax(0, 1fr);
      padding: 20px 0;
    }}
    .message:last-child {{ border-bottom: 0; }}
    .speaker {{
      color: #405068;
      font-size: 12px;
      font-weight: 800;
      padding-top: 3px;
    }}
    .time {{
      color: #8995a5;
      display: block;
      font-size: 11px;
      font-weight: 600;
      margin-top: 5px;
      overflow-wrap: anywhere;
    }}
    .body {{
      color: #1f2937;
      font-size: 14px;
      min-width: 0;
      overflow-wrap: anywhere;
      white-space: pre-wrap;
    }}
    .empty {{
      color: #66758a;
      padding: 24px 0 0;
    }}
    @media (max-width: 720px) {{
      .page {{ border-left: 0; border-right: 0; border-radius: 0; margin: 0; }}
      header, main {{ padding-left: 20px; padding-right: 20px; }}
      .message {{ grid-template-columns: 1fr; gap: 6px; }}
    }}
  </style>
</head>
<body>
  <article class="page">
    <header>
      <div class="brand">CodexPilot Export</div>
      <h1>{title}</h1>
      <div class="meta">
        <span>Exported {exported_at}</span>
        <span>{message_count} messages</span>
      </div>
    </header>
    <main>
      {sections}
    </main>
  </article>
</body>
</html>
"#,
        title = escape_html(title),
        exported_at = escape_html(&exported_at),
        message_count = messages.len(),
        sections = sections
    )
}

fn exported_at_label() -> String {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "unknown time".to_string();
    };
    format_unix_utc(duration.as_secs())
}

fn format_unix_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era
        - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}

fn render_html_body(body: &str) -> String {
    escape_html(body.trim())
}

fn escape_html(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}

fn exported(
    session_id: String,
    title: &str,
    messages: &[Message],
    format: ExportFormat,
) -> ExportResult {
    match format {
        ExportFormat::Markdown => ExportResult {
            status: ExportStatus::Exported,
            session_id: session_id.clone(),
            message: "session exported as Markdown".to_string(),
            filename: Some(build_filename(title, &session_id, "md")),
            markdown: Some(render_markdown(title, messages)),
            html: None,
        },
        ExportFormat::Html => ExportResult {
            status: ExportStatus::Exported,
            session_id: session_id.clone(),
            message: "session exported as HTML".to_string(),
            filename: Some(build_filename(title, &session_id, "html")),
            markdown: None,
            html: Some(render_html(title, messages)),
        },
    }
}

fn not_found(session_id: &str, message: &str) -> ExportResult {
    ExportResult {
        status: ExportStatus::NotFound,
        session_id: session_id.to_string(),
        message: message.to_string(),
        filename: None,
        markdown: None,
        html: None,
    }
}

fn failed(session_id: &str, message: impl Into<String>) -> ExportResult {
    ExportResult {
        status: ExportStatus::Failed,
        session_id: session_id.to_string(),
        message: message.into(),
        filename: None,
        markdown: None,
        html: None,
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

    #[test]
    fn exports_html_with_escaped_content() {
        let db_path = unique_temp_path("html-export", "sqlite");
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, body TEXT, created_at TEXT);
            INSERT INTO sessions VALUES ('s1', 'Display <Thread>');
            INSERT INTO messages (session_id, role, body, created_at) VALUES ('s1', 'user', '<script>alert(1)</script>', '2026-01-01T00:00:00Z');
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
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn formats_unix_time_as_readable_utc() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_utc(1_767_225_600), "2026-01-01 00:00 UTC");
    }
}
