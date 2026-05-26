use crate::storage::{
    SQLiteStorageAdapter, SchemaKind, SessionRef, has_columns, quote_identifier, schema_kind,
    sql_value_to_json, table_columns,
};
use rusqlite::Connection;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 单次 `thread_sort_keys` 请求允许查询的去重 id 上限。
/// 超过这个数量的 id 会被截断，调用方需要分批请求；返回体里 `truncated` 字段会显式标记。
pub(crate) const MAX_SORT_KEY_BATCH: usize = 200;

impl SQLiteStorageAdapter {
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

pub(super) fn add_timestamp_payload(payload: &mut Map<String, Value>, row: &Map<String, Value>) {
    for column in ["updated_at", "updated_at_ms", "created_at_ms"] {
        payload.insert(
            column.to_string(),
            row.get(column).cloned().unwrap_or(Value::Null),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SQLiteStorageAdapter;
    use rusqlite::Connection;
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
