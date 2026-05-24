use std::path::{Path, PathBuf};

use anyhow::Context;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol_proxy::UpstreamProtocol;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcsProviderCandidate {
    pub source_id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub upstream_protocol: UpstreamProtocol,
}

pub fn default_ccs_db_path() -> PathBuf {
    home_dir().join(".cc-switch").join("cc-switch.db")
}

pub fn list_codex_providers_from_default_db() -> anyhow::Result<Vec<CcsProviderCandidate>> {
    list_codex_providers_from_db(&default_ccs_db_path())
}

pub fn list_codex_providers_from_db(path: &Path) -> anyhow::Result<Vec<CcsProviderCandidate>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open CCS database {}", path.display()))?;
    let mut stmt = conn.prepare(
        "SELECT id, name, settings_config
         FROM providers
         WHERE app_type = 'codex'
         ORDER BY COALESCE(sort_index, 999999), created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let source_id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let settings_config: String = row.get(2)?;
        Ok((source_id, name, settings_config))
    })?;

    let mut providers = Vec::new();
    for row in rows {
        let (source_id, name, settings_config) = row?;
        let Ok(config) = serde_json::from_str::<Value>(&settings_config) else {
            continue;
        };
        if let Some(provider) = import_from_ccs_value(&source_id, &name, &config) {
            providers.push(provider);
        }
    }
    Ok(providers)
}

fn import_from_ccs_value(
    source_id: &str,
    name: &str,
    config: &Value,
) -> Option<CcsProviderCandidate> {
    let base_url = extract_base_url(config)?;
    let api_key = extract_api_key(config).unwrap_or_default();
    let upstream_protocol = extract_protocol(config, &base_url);
    Some(CcsProviderCandidate {
        source_id: source_id.to_string(),
        name: name.to_string(),
        base_url,
        api_key,
        upstream_protocol,
    })
}

fn extract_base_url(config: &Value) -> Option<String> {
    string_at(config, &["base_url", "baseURL"])
        .or_else(|| {
            config
                .get("config")
                .and_then(|value| string_at(value, &["base_url", "baseURL"]))
        })
        .or_else(|| {
            config
                .get("config")
                .and_then(Value::as_str)
                .and_then(extract_toml_base_url)
        })
        .map(trim_trailing_slash)
        .filter(|value| !value.is_empty())
}

fn extract_api_key(config: &Value) -> Option<String> {
    if let Some(key) = config
        .pointer("/env/OPENAI_API_KEY")
        .and_then(Value::as_str)
    {
        return Some(key.to_string());
    }
    if let Some(key) = config
        .pointer("/auth/OPENAI_API_KEY")
        .and_then(Value::as_str)
    {
        return Some(key.to_string());
    }
    string_at(config, &["apiKey", "api_key"]).or_else(|| {
        config
            .get("config")
            .and_then(|value| string_at(value, &["apiKey", "api_key"]))
    })
}

fn extract_protocol(config: &Value, base_url: &str) -> UpstreamProtocol {
    if let Some(api_format) = string_at(config, &["api_format", "apiFormat"]) {
        if is_chat_protocol(&api_format) {
            return UpstreamProtocol::ChatCompletions;
        }
    }
    if let Some(wire_api) = config
        .get("config")
        .and_then(Value::as_str)
        .and_then(extract_toml_wire_api)
    {
        if is_chat_protocol(&wire_api) {
            return UpstreamProtocol::ChatCompletions;
        }
    }
    if base_url.to_ascii_lowercase().ends_with("/chat/completions") {
        return UpstreamProtocol::ChatCompletions;
    }
    UpstreamProtocol::Responses
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

fn trim_trailing_slash(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn is_chat_protocol(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "chat" | "chat_completions" | "chat-completions" | "openai_chat" | "openai-chat"
    )
}

fn extract_toml_base_url(text: &str) -> Option<String> {
    extract_toml_string_value(text, "base_url")
}

fn extract_toml_wire_api(text: &str) -> Option<String> {
    extract_toml_string_value(text, "wire_api")
}

fn extract_toml_string_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let rest = &rest[quote.len_utf8()..];
        let end = rest.find(quote)?;
        return Some(rest[..end].to_string());
    }
    None
}

fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use serde_json::json;

    fn create_ccs_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE providers (
                id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                name TEXT NOT NULL,
                settings_config TEXT NOT NULL,
                created_at INTEGER,
                sort_index INTEGER,
                PRIMARY KEY (id, app_type)
            )",
            [],
        )
        .unwrap();
    }

    fn insert_provider(path: &Path, id: &str, name: &str, config: Value, sort_index: i64) {
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "INSERT INTO providers (id, app_type, name, settings_config, created_at, sort_index)
             VALUES (?1, 'codex', ?2, ?3, ?4, ?5)",
            params![id, name, config.to_string(), 1000 + sort_index, sort_index],
        )
        .unwrap();
    }

    #[test]
    fn imports_direct_base_url_and_api_key_provider() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cc-switch.db");
        create_ccs_db(&db);
        insert_provider(
            &db,
            "openai",
            "OpenAI",
            json!({
                "base_url": "https://api.openai.com/v1/",
                "api_key": "sk-openai"
            }),
            0,
        );

        let providers = list_codex_providers_from_db(&db).unwrap();

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].source_id, "openai");
        assert_eq!(providers[0].name, "OpenAI");
        assert_eq!(providers[0].base_url, "https://api.openai.com/v1");
        assert_eq!(providers[0].api_key, "sk-openai");
        assert_eq!(providers[0].upstream_protocol, UpstreamProtocol::Responses);
    }

    #[test]
    fn imports_auth_and_config_object_provider_as_chat_protocol() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cc-switch.db");
        create_ccs_db(&db);
        insert_provider(
            &db,
            "chat",
            "Chat Provider",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-chat" },
                "config": { "base_url": "https://relay.example/v1/chat/completions" }
            }),
            0,
        );

        let providers = list_codex_providers_from_db(&db).unwrap();

        assert_eq!(
            providers[0].base_url,
            "https://relay.example/v1/chat/completions"
        );
        assert_eq!(providers[0].api_key, "sk-chat");
        assert_eq!(
            providers[0].upstream_protocol,
            UpstreamProtocol::ChatCompletions
        );
    }

    #[test]
    fn skips_rows_without_base_url() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("cc-switch.db");
        create_ccs_db(&db);
        insert_provider(
            &db,
            "missing",
            "Missing",
            json!({
                "auth": { "OPENAI_API_KEY": "sk-missing" }
            }),
            0,
        );

        let providers = list_codex_providers_from_db(&db).unwrap();

        assert!(providers.is_empty());
    }
}
