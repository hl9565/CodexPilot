use anyhow::Context;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
static TEST_LOG_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

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

pub fn append(event: &str, detail: Value) -> anyhow::Result<()> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
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

pub fn read_tail(max_lines: usize) -> anyhow::Result<Vec<String>> {
    let path = log_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut lines = text
        .lines()
        .rev()
        .take(max_lines)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    lines.reverse();
    Ok(lines)
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
}
