use anyhow::Context;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 20 * 1024 * 1024;
const ROTATED_LOGS: usize = 5;

#[cfg(test)]
static TEST_LOG_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn test_log_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    match LOCK.get_or_init(|| std::sync::Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn log_path() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_LOG_PATH.lock().ok().and_then(|guard| guard.clone()) {
        return path;
    }
    crate::app_paths::app_state_dir().join("diagnostic.log")
}

#[cfg(test)]
pub fn set_test_log_path(path: PathBuf) {
    if let Ok(mut guard) = TEST_LOG_PATH.lock() {
        *guard = Some(path);
    }
}

#[cfg(test)]
pub fn clear_test_log_path() {
    if let Ok(mut guard) = TEST_LOG_PATH.lock() {
        *guard = None;
    }
}

pub fn append(event: &str, detail: Value) -> anyhow::Result<()> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    rotate_if_needed(&path)?;
    let line = json!({
        "ts": now_ms(),
        "event": sanitize_event(event),
        "detail": redact(detail),
    });
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn rotate_if_needed(path: &PathBuf) -> anyhow::Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }
    for index in (1..=ROTATED_LOGS).rev() {
        let source = rotated_path(path, index);
        let destination = rotated_path(path, index + 1);
        if source.exists() {
            if index == ROTATED_LOGS {
                let _ = fs::remove_file(&source);
            } else {
                fs::rename(&source, &destination).with_context(|| {
                    format!(
                        "failed to rotate {} to {}",
                        source.display(),
                        destination.display()
                    )
                })?;
            }
        }
    }
    let first = rotated_path(path, 1);
    fs::rename(path, &first)
        .with_context(|| format!("failed to rotate {} to {}", path.display(), first.display()))?;
    Ok(())
}

fn rotated_path(path: &PathBuf, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

pub fn read_tail(max_lines: usize) -> anyhow::Result<Vec<String>> {
    let path = log_path();
    if !path.exists() && !rotated_path(&path, 1).exists() {
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    for path in log_paths_newest_first(&path) {
        if !path.exists() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let remaining = max_lines.saturating_sub(lines.len());
        if remaining == 0 {
            break;
        }
        lines.extend(text.lines().rev().take(remaining).map(ToString::to_string));
    }
    lines.reverse();
    Ok(lines)
}

fn log_paths_newest_first(path: &PathBuf) -> Vec<PathBuf> {
    let mut paths = vec![path.clone()];
    paths.extend((1..=ROTATED_LOGS).map(|index| rotated_path(path, index)));
    paths
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn sanitize_event(event: &str) -> String {
    let value = event
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "event".to_string()
    } else {
        value
    }
}

fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let lower = key.to_lowercase();
                    if lower.contains("token") || lower.contains("key") || lower.contains("secret")
                    {
                        (key, Value::String("[redacted]".to_string()))
                    } else {
                        (key, redact(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(redact).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_hides_secret_like_keys() {
        let value = redact(json!({"api_key": "sk-test", "nested": {"token": "abc"}, "safe": "ok"}));
        assert_eq!(value["api_key"], "[redacted]");
        assert_eq!(value["nested"]["token"], "[redacted]");
        assert_eq!(value["safe"], "ok");
    }

    #[test]
    fn append_rotates_large_log_file() {
        let _guard = super::test_log_guard();
        let root = std::env::temp_dir().join(format!(
            "codex-pilot-diagnostic-log-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("diagnostic.log");
        set_test_log_path(path.clone());
        fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();

        append("test.rotate", json!({"safe": "ok"})).unwrap();

        assert!(path.exists());
        assert!(rotated_path(&path, 1).exists());
        assert!(fs::metadata(&path).unwrap().len() < MAX_LOG_BYTES);
        let current = fs::read_to_string(&path).unwrap();
        assert!(current.contains("test.rotate"));
        let _ = fs::remove_dir_all(root);
        clear_test_log_path();
    }

    #[test]
    fn read_tail_spans_rotated_logs() {
        let _guard = super::test_log_guard();
        let root = std::env::temp_dir().join(format!(
            "codex-pilot-diagnostic-tail-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("diagnostic.log");
        set_test_log_path(path.clone());
        fs::write(rotated_path(&path, 1), "old-1\nold-2\n").unwrap();
        fs::write(&path, "new-1\nnew-2\n").unwrap();

        assert_eq!(read_tail(3).unwrap(), vec!["old-2", "new-1", "new-2"]);

        let _ = fs::remove_dir_all(root);
        clear_test_log_path();
    }
}
