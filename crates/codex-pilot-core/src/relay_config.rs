use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RELAY_PROVIDER: &str = "CodexPilot";
const CHANNEL_MODE_KEY: &str = "codex_pilot_channel_mode";

pub use crate::protocol_proxy::UpstreamProtocol;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAuthStatus {
    pub authenticated: bool,
    pub source: String,
    pub account_label: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProviderConfig {
    pub provider: String,
    pub active: bool,
    pub mode: String,
    pub configured: bool,
    pub authenticated: bool,
    pub account_label: Option<String>,
    pub requires_openai_auth: bool,
    pub has_bearer_token: bool,
    pub base_url: Option<String>,
    pub upstream_protocol: UpstreamProtocol,
    pub config_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayApplyResult {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialConfigSnapshot {
    pub config_toml: String,
    pub captured_at_ms: u64,
}

pub fn default_relay_provider_config() -> RelayProviderConfig {
    relay_provider_config_from_home(&codex_home_dir())
}

pub fn relay_provider_config_from_home(home: &Path) -> RelayProviderConfig {
    let auth = chatgpt_auth_status_from_home(home);
    relay_provider_config_from_path_and_auth(&home.join("config.toml"), auth)
}

pub fn relay_provider_config_from_path(config_path: &Path) -> RelayProviderConfig {
    relay_provider_config_from_path_and_auth(
        config_path,
        ChatGptAuthStatus {
            authenticated: false,
            source: String::new(),
            account_label: None,
            message: "未检查 ChatGPT 登录状态。".to_string(),
        },
    )
}

fn relay_provider_config_from_path_and_auth(
    config_path: &Path,
    auth: ChatGptAuthStatus,
) -> RelayProviderConfig {
    let contents = std::fs::read_to_string(config_path).unwrap_or_default();
    relay_provider_config_from_contents(&contents, config_path, auth)
}

pub fn default_chatgpt_auth_status() -> ChatGptAuthStatus {
    chatgpt_auth_status_from_home(&codex_home_dir())
}

pub fn chatgpt_auth_status_from_home(home: &Path) -> ChatGptAuthStatus {
    let auth_path = home.join("auth.json");
    if let Some(account_label) = auth_json_chatgpt_account_label(&auth_path) {
        return ChatGptAuthStatus {
            authenticated: true,
            source: auth_path.to_string_lossy().to_string(),
            account_label,
            message: "已检测到 ChatGPT 登录状态。".to_string(),
        };
    }

    ChatGptAuthStatus {
        authenticated: false,
        source: String::new(),
        account_label: None,
        message: "未检测到 ChatGPT 登录状态，请先在 Codex 中完成官方登录。".to_string(),
    }
}

pub fn apply_relay_provider_config(
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_with_protocol(base_url, bearer_token, UpstreamProtocol::Responses)
}

pub fn apply_relay_provider_config_to_home(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_home_with_protocol(
        home,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_relay_provider_config_with_protocol(
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_home_with_protocol(
        &codex_home_dir(),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_relay_provider_config_to_home_with_protocol(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let auth = chatgpt_auth_status_from_home(home);
    if !auth.authenticated {
        anyhow::bail!("{}", auth.message);
    }
    apply_relay_provider_config_to_path_with_protocol(
        &home.join("config.toml"),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_api_provider_config(
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_with_protocol(base_url, bearer_token, UpstreamProtocol::Responses)
}

pub fn apply_api_provider_config_with_protocol(
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_to_home_with_protocol(
        &crate::app_paths::codex_home_dir(),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_api_provider_config_to_home_with_protocol(
    home: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    write_pure_api_auth_json(home, bearer_token)?;
    apply_api_provider_config_to_path_with_protocol(
        &home.join("config.toml"),
        base_url,
        bearer_token,
        upstream_protocol,
    )
}

pub fn apply_relay_provider_config_to_path(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_relay_provider_config_to_path_with_protocol(
        config_path,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_relay_provider_config_to_path_with_protocol(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("relay Base URL cannot be empty");
    }
    let bearer_token = bearer_token.trim();
    if bearer_token.is_empty() {
        anyhow::bail!("relay bearer token cannot be empty");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let codex_base_url = crate::protocol_proxy::proxy_base_url_for_protocol(
        base_url,
        upstream_protocol,
        crate::protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
    );
    let updated =
        upsert_relay_provider_config(&existing, &codex_base_url, bearer_token, upstream_protocol);
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn apply_api_provider_config_to_path(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
) -> anyhow::Result<RelayApplyResult> {
    apply_api_provider_config_to_path_with_protocol(
        config_path,
        base_url,
        bearer_token,
        UpstreamProtocol::Responses,
    )
}

pub fn apply_api_provider_config_to_path_with_protocol(
    config_path: &Path,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> anyhow::Result<RelayApplyResult> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("API Base URL cannot be empty");
    }
    let bearer_token = bearer_token.trim();
    if bearer_token.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let codex_base_url = crate::protocol_proxy::proxy_base_url_for_protocol(
        base_url,
        upstream_protocol,
        crate::protocol_proxy::DEFAULT_PROTOCOL_PROXY_PORT,
    );
    let updated =
        upsert_api_provider_config(&existing, &codex_base_url, bearer_token, upstream_protocol);
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn clear_relay_provider_config() -> anyhow::Result<RelayApplyResult> {
    clear_relay_provider_config_from_path(&crate::app_paths::codex_config_path())
}

pub fn clear_relay_provider_config_from_home(home: &Path) -> anyhow::Result<RelayApplyResult> {
    clear_relay_provider_config_from_path(&home.join("config.toml"))
}

pub fn capture_official_config_snapshot_from_home(
    home: &Path,
) -> anyhow::Result<Option<OfficialConfigSnapshot>> {
    let config_path = home.join("config.toml");
    let current = relay_provider_config_from_home(home);
    if current.active {
        return Ok(None);
    }
    let config_toml = std::fs::read_to_string(&config_path).unwrap_or_default();
    Ok(Some(OfficialConfigSnapshot {
        config_toml,
        captured_at_ms: now_ms() as u64,
    }))
}

pub fn restore_official_config_snapshot_from_home(
    home: &Path,
    snapshot: &OfficialConfigSnapshot,
) -> anyhow::Result<RelayApplyResult> {
    restore_official_config_snapshot_to_path(&home.join("config.toml"), snapshot)
}

pub fn restore_official_config_snapshot_to_path(
    config_path: &Path,
    snapshot: &OfficialConfigSnapshot,
) -> anyhow::Result<RelayApplyResult> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    std::fs::write(config_path, &snapshot.config_toml)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    let _ = clear_api_key_auth_json(
        &config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("auth.json"),
    );

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

pub fn clear_relay_provider_config_from_path(
    config_path: &Path,
) -> anyhow::Result<RelayApplyResult> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let existing = std::fs::read_to_string(config_path).unwrap_or_default();
    let backup_path = backup_existing_config(config_path, &existing)?;
    let without_relay = remove_table(&existing, &format!("model_providers.{RELAY_PROVIDER}"));
    let without_key = remove_root_key(&without_relay, "OPENAI_API_KEY");
    let updated = remove_root_key(&without_key, "model_provider");
    std::fs::write(config_path, updated)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    let _ = clear_api_key_auth_json(
        &config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("auth.json"),
    );

    let status = relay_provider_config_from_path(config_path);
    Ok(RelayApplyResult {
        config_path: status.config_path,
        backup_path: backup_path.map(|path| path.to_string_lossy().to_string()),
        configured: status.configured,
    })
}

fn relay_provider_config_from_contents(
    contents: &str,
    config_path: &Path,
    auth: ChatGptAuthStatus,
) -> RelayProviderConfig {
    let active = root_key_string(contents, "model_provider")
        .map(|value| value == RELAY_PROVIDER)
        .unwrap_or(false);
    let provider = table_values(contents, &format!("model_providers.{RELAY_PROVIDER}"));
    let requires_openai_auth = provider
        .as_ref()
        .and_then(|values| values.get("requires_openai_auth"))
        .map(|value| value.trim() == "true")
        .unwrap_or(false);
    let has_bearer_token = provider
        .as_ref()
        .and_then(|values| values.get("experimental_bearer_token"))
        .map(|value| !unquote_toml_string(value).trim().is_empty())
        .unwrap_or(false)
        || root_key_string(contents, "OPENAI_API_KEY")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    let has_experimental_bearer_token = provider
        .as_ref()
        .and_then(|values| values.get("experimental_bearer_token"))
        .map(|value| !unquote_toml_string(value).trim().is_empty())
        .unwrap_or(false);
    let base_url = provider
        .as_ref()
        .and_then(|values| values.get("base_url"))
        .map(|value| unquote_toml_string(value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let stored_mode = provider
        .as_ref()
        .and_then(|values| values.get(CHANNEL_MODE_KEY))
        .map(|value| unquote_toml_string(value))
        .filter(|value| !value.trim().is_empty());
    let upstream_protocol = base_url
        .as_deref()
        .map(infer_upstream_protocol_from_base_url)
        .unwrap_or_default();
    let mode = if active && stored_mode.as_deref() == Some("api") {
        "api"
    } else if active && stored_mode.as_deref() == Some("hybridApi") {
        "hybridApi"
    } else if active && requires_openai_auth {
        "hybridApi"
    } else if active && base_url.is_some() && has_bearer_token {
        "api"
    } else {
        "official"
    };
    let configured = active
        && base_url.is_some()
        && ((requires_openai_auth && has_experimental_bearer_token)
            || (!requires_openai_auth && has_bearer_token));

    RelayProviderConfig {
        provider: RELAY_PROVIDER.to_string(),
        active,
        mode: mode.to_string(),
        configured,
        authenticated: auth.authenticated,
        account_label: auth.account_label,
        requires_openai_auth,
        has_bearer_token,
        base_url,
        upstream_protocol,
        config_path: config_path.to_string_lossy().to_string(),
    }
}

fn codex_home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".codex"))
        .unwrap_or_else(|| PathBuf::from(".codex"))
}

fn auth_json_chatgpt_account_label(path: &Path) -> Option<Option<String>> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return None;
    };
    let Ok(value) = serde_json::from_str::<Value>(&contents) else {
        return None;
    };
    let is_chatgpt = value
        .get("auth_mode")
        .and_then(Value::as_str)
        .map(|mode| mode.eq_ignore_ascii_case("chatgpt"))
        .unwrap_or(false);
    let tokens = value.get("tokens")?;
    if !is_chatgpt || !tokens_have_login_secret(tokens) {
        return None;
    }
    Some(account_label_from_tokens(tokens))
}

fn tokens_have_login_secret(tokens: &Value) -> bool {
    ["access_token", "id_token", "refresh_token"]
        .iter()
        .any(|key| {
            tokens
                .get(*key)
                .and_then(Value::as_str)
                .map(|token| !token.trim().is_empty())
                .unwrap_or(false)
        })
}

fn account_label_from_tokens(tokens: &Value) -> Option<String> {
    ["id_token", "access_token"].iter().find_map(|key| {
        tokens
            .get(*key)
            .and_then(Value::as_str)
            .and_then(account_label_from_jwt)
    })
}

fn account_label_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let decoded = decode_base64_url(payload)?;
    let value = serde_json::from_slice::<Value>(&decoded).ok()?;
    value
        .get("email")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("https://api.openai.com/profile")
                .and_then(|profile| profile.get("email"))
                .and_then(Value::as_str)
        })
        .or_else(|| value.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(ToString::to_string)
}

fn decode_base64_url(value: &str) -> Option<Vec<u8>> {
    let mut input = value.replace('-', "+").replace('_', "/");
    while !input.len().is_multiple_of(4) {
        input.push('=');
    }
    decode_base64(&input)
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | digit;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn backup_existing_config(config_path: &Path, existing: &str) -> anyhow::Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!("config.toml.codex-pilot-backup-{}.bak", now_ms()));
    std::fs::write(&backup_path, existing)
        .with_context(|| format!("failed to write {}", backup_path.display()))?;
    Ok(Some(backup_path))
}

fn upsert_relay_provider_config(
    contents: &str,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> String {
    let mut updated = upsert_root_keys(
        contents,
        &[(
            "model_provider",
            format!("\"{}\"", toml_escape(RELAY_PROVIDER)),
        )],
    );
    updated = remove_table(&updated, &format!("model_providers.{RELAY_PROVIDER}"));
    updated = remove_root_key(&updated, "OPENAI_API_KEY");

    let mut lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    let insert_at = first_non_provider_table_index(&lines).unwrap_or(lines.len());
    let provider_lines = vec![
        format!("[model_providers.{RELAY_PROVIDER}]"),
        format!("name = \"{}\"", toml_escape(RELAY_PROVIDER)),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = \"{}\"", toml_escape(base_url)),
        format!("{CHANNEL_MODE_KEY} = \"hybridApi\""),
        format!(
            "codex_pilot_upstream_protocol = \"{}\"",
            upstream_protocol.as_config_value()
        ),
        format!(
            "experimental_bearer_token = \"{}\"",
            toml_escape(bearer_token)
        ),
        String::new(),
    ];
    lines.splice(insert_at..insert_at, provider_lines);
    finish_lines(lines)
}

fn upsert_api_provider_config(
    contents: &str,
    base_url: &str,
    bearer_token: &str,
    upstream_protocol: UpstreamProtocol,
) -> String {
    let mut updated = upsert_root_keys(
        contents,
        &[(
            "model_provider",
            format!("\"{}\"", toml_escape(RELAY_PROVIDER)),
        )],
    );
    updated = remove_table(&updated, &format!("model_providers.{RELAY_PROVIDER}"));
    updated = remove_root_key(&updated, "OPENAI_API_KEY");

    let mut lines = updated.lines().map(ToString::to_string).collect::<Vec<_>>();
    let insert_at = first_non_provider_table_index(&lines).unwrap_or(lines.len());
    let provider_lines = vec![
        format!("[model_providers.{RELAY_PROVIDER}]"),
        format!("name = \"{}\"", toml_escape(RELAY_PROVIDER)),
        "wire_api = \"responses\"".to_string(),
        "requires_openai_auth = true".to_string(),
        format!("base_url = \"{}\"", toml_escape(base_url)),
        format!("{CHANNEL_MODE_KEY} = \"api\""),
        format!(
            "codex_pilot_upstream_protocol = \"{}\"",
            upstream_protocol.as_config_value()
        ),
        format!(
            "experimental_bearer_token = \"{}\"",
            toml_escape(bearer_token)
        ),
        String::new(),
    ];
    lines.splice(insert_at..insert_at, provider_lines);
    finish_lines(lines)
}

fn write_pure_api_auth_json(home: &Path, bearer_token: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(home)
        .with_context(|| format!("failed to create {}", home.display()))?;
    let auth_path = home.join("auth.json");
    let value = serde_json::json!({
        "OPENAI_API_KEY": bearer_token.trim()
    });
    std::fs::write(&auth_path, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("failed to write {}", auth_path.display()))?;
    Ok(())
}

fn clear_api_key_auth_json(auth_path: &Path) -> anyhow::Result<()> {
    if !auth_path.exists() {
        return Ok(());
    }
    let existing = std::fs::read_to_string(auth_path)
        .with_context(|| format!("failed to read {}", auth_path.display()))?;
    let Ok(mut value) = serde_json::from_str::<Value>(&existing) else {
        return Ok(());
    };
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    if object.remove("OPENAI_API_KEY").is_none() {
        return Ok(());
    }
    std::fs::write(auth_path, serde_json::to_vec_pretty(&value)?)
        .with_context(|| format!("failed to write {}", auth_path.display()))?;
    Ok(())
}

fn root_key_string(contents: &str, key: &str) -> Option<String> {
    root_key_value(contents, key).map(unquote_toml_string)
}

fn root_key_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            return None;
        }
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            return Some(value);
        }
    }
    None
}

fn upsert_root_keys(contents: &str, entries: &[(&str, String)]) -> String {
    let mut lines = contents
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let root_end = lines
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .unwrap_or(lines.len());

    for (key, value) in entries {
        if let Some(index) = lines[..root_end]
            .iter()
            .position(|line| root_line_key(line) == Some(*key))
        {
            lines[index] = format!("{key} = {value}");
        } else {
            lines.insert(root_end, format!("{key} = {value}"));
        }
    }

    finish_lines(lines)
}

fn remove_table(contents: &str, table: &str) -> String {
    let header = format!("[{table}]");
    let mut lines = Vec::new();
    let mut skipping = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == header {
                skipping = true;
                continue;
            }
            skipping = false;
        }
        if !skipping {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

fn remove_root_key(contents: &str, key: &str) -> String {
    let mut lines = Vec::new();
    let mut in_root = true;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_root = false;
        }
        if in_root && root_line_key(line) == Some(key) {
            continue;
        }
        lines.push(line.to_string());
    }
    lines.join("\n")
}

fn first_non_provider_table_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('[') && !trimmed.starts_with("[model_providers.")
    })
}

fn table_values(contents: &str, table: &str) -> Option<std::collections::HashMap<String, String>> {
    let header = format!("[{table}]");
    let mut in_table = false;
    let mut values = std::collections::HashMap::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_table {
                break;
            }
            in_table = trimmed == header;
            continue;
        }
        if !in_table || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    in_table.then_some(values)
}

fn unquote_toml_string(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

fn root_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}

fn toml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn infer_upstream_protocol_from_base_url(base_url: &str) -> UpstreamProtocol {
    if base_url.starts_with("http://127.0.0.1:") || base_url.starts_with("http://localhost:") {
        return UpstreamProtocol::ChatCompletions;
    }
    UpstreamProtocol::Responses
}

impl UpstreamProtocol {
    fn as_config_value(self) -> &'static str {
        match self {
            UpstreamProtocol::Responses => "responses",
            UpstreamProtocol::ChatCompletions => "chat_completions",
            UpstreamProtocol::AnthropicMessages => "anthropic_messages",
        }
    }
}

fn finish_lines(lines: Vec<String>) -> String {
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn reads_active_relay_provider_config() {
        let contents = r#"model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://relay.example/v1"
experimental_bearer_token = "sk-test"
"#;

        let status = relay_provider_config_from_contents(
            contents,
            Path::new("/tmp/config.toml"),
            ChatGptAuthStatus {
                authenticated: true,
                source: "/tmp/auth.json".to_string(),
                account_label: Some("user@example.com".to_string()),
                message: "已检测到 ChatGPT 登录状态。".to_string(),
            },
        );

        assert!(status.active);
        assert!(status.configured);
        assert_eq!(status.mode, "hybridApi");
        assert!(status.authenticated);
        assert_eq!(status.account_label.as_deref(), Some("user@example.com"));
        assert!(status.requires_openai_auth);
        assert!(status.has_bearer_token);
        assert_eq!(status.base_url.as_deref(), Some("https://relay.example/v1"));
    }

    #[test]
    fn apply_preserves_unrelated_config_and_replaces_owned_provider() {
        let contents = r#"# keep this
model = "gpt-5"
model_provider = "openai"

[model_providers.CodexPilot]
name = "old"
base_url = "https://old.example/v1"

[model_providers.other]
name = "Other"
base_url = "https://other.example/v1"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

        let updated = upsert_relay_provider_config(
            contents,
            "https://new.example/v1",
            "sk-new",
            UpstreamProtocol::Responses,
        );

        assert!(updated.contains("# keep this"));
        assert!(updated.contains("model = \"gpt-5\""));
        assert!(updated.contains("model_provider = \"CodexPilot\""));
        assert!(!updated.contains("OPENAI_API_KEY"));
        assert!(updated.contains("[model_providers.CodexPilot]"));
        assert!(updated.contains("wire_api = \"responses\""));
        assert!(updated.contains("requires_openai_auth = true"));
        assert!(updated.contains("base_url = \"https://new.example/v1\""));
        assert!(updated.contains("experimental_bearer_token = \"sk-new\""));
        assert!(updated.contains("[model_providers.other]"));
        assert!(updated.contains("[mcp_servers.local]"));
        assert!(updated.contains("base_url = \"http://127.0.0.1:8000\""));
        assert!(!updated.contains("https://old.example/v1"));
    }

    #[test]
    fn apply_api_provider_writes_openai_api_key_provider() {
        let contents = r#"# keep this
model = "gpt-5"
model_provider = "chatgpt"
OPENAI_API_KEY = "old"

[model_providers.CodexPilot]
name = "old"
requires_openai_auth = true
base_url = "https://old.example/v1"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

        let updated = upsert_api_provider_config(
            contents,
            "https://api.example/v1",
            "sk-api",
            UpstreamProtocol::Responses,
        );

        assert!(updated.contains("# keep this"));
        assert!(updated.contains("model = \"gpt-5\""));
        assert!(updated.contains("model_provider = \"CodexPilot\""));
        assert!(updated.contains("[model_providers.CodexPilot]"));
        assert!(updated.contains("wire_api = \"responses\""));
        assert!(updated.contains("requires_openai_auth = true"));
        assert!(updated.contains("codex_pilot_channel_mode = \"api\""));
        assert!(updated.contains("base_url = \"https://api.example/v1\""));
        assert!(updated.contains("experimental_bearer_token = \"sk-api\""));
        assert!(!updated.contains("OPENAI_API_KEY = \"sk-api\""));
        assert!(!updated.contains("env_key = \"OPENAI_API_KEY\""));
        assert!(!updated.contains("https://old.example/v1"));
        assert!(updated.contains("[mcp_servers.local]"));
    }

    #[test]
    fn reads_active_api_provider_config() {
        let contents = r#"model_provider = "CodexPilot"
OPENAI_API_KEY = "sk-api"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://api.example/v1"
codex_pilot_channel_mode = "api"
experimental_bearer_token = "sk-api"
"#;

        let status = relay_provider_config_from_contents(
            contents,
            Path::new("/tmp/config.toml"),
            ChatGptAuthStatus {
                authenticated: false,
                source: String::new(),
                account_label: None,
                message: "未检测到 ChatGPT 登录状态，请先在 Codex 中完成官方登录。".to_string(),
            },
        );

        assert!(status.active);
        assert!(status.configured);
        assert_eq!(status.mode, "api");
        assert!(!status.authenticated);
        assert!(status.requires_openai_auth);
        assert!(status.has_bearer_token);
        assert_eq!(status.base_url.as_deref(), Some("https://api.example/v1"));
    }

    #[test]
    fn apply_api_provider_writes_pure_api_auth_json_and_config() {
        let root = unique_temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"token"}}"#,
        )
        .unwrap();

        let apply = apply_api_provider_config_to_home_with_protocol(
            &root,
            "https://api.example/v1",
            "sk-api",
            UpstreamProtocol::Responses,
        )
        .unwrap();

        assert!(apply.configured);
        let auth = std::fs::read_to_string(root.join("auth.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&auth).unwrap(),
            serde_json::json!({"OPENAI_API_KEY": "sk-api"})
        );
        let config = std::fs::read_to_string(root.join("config.toml")).unwrap();
        assert!(config.contains("codex_pilot_channel_mode = \"api\""));
        assert!(config.contains("requires_openai_auth = true"));
        assert!(config.contains("experimental_bearer_token = \"sk-api\""));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clear_removes_owned_provider_and_keeps_other_tables() {
        let contents = r#"OPENAI_API_KEY = "old"
model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
base_url = "https://relay.example/v1"
experimental_bearer_token = "sk-test"

[model_providers.other]
name = "Other"

[mcp_servers.local]
base_url = "http://127.0.0.1:8000"
"#;

        let without_relay = remove_table(contents, &format!("model_providers.{RELAY_PROVIDER}"));
        let without_key = remove_root_key(&without_relay, "OPENAI_API_KEY");
        let updated = remove_root_key(&without_key, "model_provider");

        assert!(!updated.contains("model_provider ="));
        assert!(!updated.contains("[model_providers.CodexPilot]"));
        assert!(!updated.contains("OPENAI_API_KEY"));
        assert!(updated.contains("[model_providers.other]"));
        assert!(updated.contains("[mcp_servers.local]"));
        assert!(updated.contains("base_url = \"http://127.0.0.1:8000\""));
    }

    #[test]
    fn apply_and_clear_create_backups_when_file_exists() {
        let root = unique_temp_dir();
        let config_path = root.join("config.toml");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&config_path, "model = \"gpt-5\"\n").unwrap();

        let apply = apply_relay_provider_config_to_path(
            &config_path,
            "https://relay.example/v1",
            "sk-test",
        )
        .unwrap();
        assert!(apply.configured);
        let apply_backup = PathBuf::from(apply.backup_path.unwrap());
        assert_eq!(
            std::fs::read_to_string(&apply_backup).unwrap(),
            "model = \"gpt-5\"\n"
        );

        let clear = clear_relay_provider_config_from_path(&config_path).unwrap();
        assert!(!clear.configured);
        assert!(PathBuf::from(clear.backup_path.unwrap()).exists());
        let final_contents = std::fs::read_to_string(&config_path).unwrap();
        assert!(!final_contents.contains("model_provider ="));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_chatgpt_auth_from_auth_json() {
        let root = unique_temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"header.eyJlbWFpbCI6InVzZXJAZXhhbXBsZS5jb20ifQ.signature"}}"#,
        )
        .unwrap();

        let auth = chatgpt_auth_status_from_home(&root);
        assert!(auth.authenticated);
        assert_eq!(auth.account_label.as_deref(), Some("user@example.com"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_to_home_requires_chatgpt_auth() {
        let root = unique_temp_dir();
        std::fs::create_dir_all(&root).unwrap();

        let error =
            apply_relay_provider_config_to_home(&root, "https://relay.example/v1", "sk-test")
                .unwrap_err();
        assert!(error.to_string().contains("未检测到 ChatGPT 登录状态"));

        std::fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "codex-pilot-relay-config-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
