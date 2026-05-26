use serde_json::{Value, json};
use crate::routes_sessions::{
    archived_thread, delete_recycle_bin_entry, delete_session, export_html, export_markdown,
    list_recycle_bin, move_thread_workspace, thread_sort_key, thread_sort_keys, undo_session,
};

#[derive(Debug, Clone)]
pub struct BridgeContext {
    pub debug_port: u16,
    pub helper_port: u16,
    pub db_path: std::path::PathBuf,
}

impl BridgeContext {
    pub fn new(debug_port: u16, helper_port: u16) -> Self {
        Self {
            debug_port,
            helper_port,
            db_path: crate::app_paths::codex_state_db_path(),
        }
    }
}

pub async fn handle_bridge_request(ctx: BridgeContext, path: &str, payload: Value) -> Value {
    let _ = crate::diagnostic_log::append(
        "route.request",
        json!({
            "path": path,
            "payload": payload
        }),
    );
    match path {
        "/backend/status" => json!({
            "status": "ok",
            "message": "CodexPilot 后端已连接",
            "version": crate::version::VERSION,
            "debug_port": ctx.debug_port,
            "helper_port": ctx.helper_port,
            "transport": "cdp-binding"
        }),
        "/backend/recover-bridge" => recover_bridge(ctx).await,
        "/session/delete" => delete_session(ctx, payload).await,
        "/session/undo" => undo_session(ctx, payload).await,
        "/session/recycle-bin/list" => list_recycle_bin(ctx).await,
        "/session/recycle-bin/restore" => undo_session(ctx, payload).await,
        "/session/recycle-bin/delete" => delete_recycle_bin_entry(ctx, payload).await,
        "/session/export-markdown" => export_markdown(ctx, payload).await,
        "/session/export-html" => export_html(ctx, payload).await,
        "/session/archived-thread" => archived_thread(ctx, payload).await,
        "/session/move-workspace" => move_thread_workspace(ctx, payload).await,
        "/session/thread-sort-key" => thread_sort_key(ctx, payload).await,
        "/session/thread-sort-keys" => thread_sort_keys(ctx, payload).await,
        "/provider/status" => json!({
            "status": "ok",
            "provider": crate::relay_config::default_relay_provider_config()
        }),
        "/provider/plugin-patch-status" => plugin_patch_status(),
        "/provider/apply" => apply_provider(payload),
        "/provider/clear" => clear_provider(),
        "/enhancement/settings" => enhancement_settings(),
        "/diagnostics/report" => report_diagnostics(payload),
        _ => json!({
            "status": "failed",
            "message": format!("unknown bridge path: {path}")
        }),
    }
}

async fn recover_bridge(ctx: BridgeContext) -> Value {
    let _ = crate::diagnostic_log::append(
        "backend.recover_bridge",
        json!({
            "debug_port": ctx.debug_port,
            "helper_port": ctx.helper_port
        }),
    );

    let debug_port = ctx.debug_port;
    let helper_port = ctx.helper_port;
    std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(anyhow::Error::from)
            .and_then(|runtime| {
                runtime.block_on(crate::launcher::inject_running_codex(
                    debug_port,
                    helper_port,
                ))
            });
        let event = if result.is_ok() {
            "backend.recover_bridge_ok"
        } else {
            "backend.recover_bridge_failed"
        };
        let _ = crate::diagnostic_log::append(
            event,
            json!({
                "debug_port": debug_port,
                "helper_port": helper_port,
                "message": result.err().map(|error| error.to_string()).unwrap_or_default()
            }),
        );
    });

    json!({
        "status": "ok",
        "message": "CodexPilot bridge 恢复已启动",
        "debug_port": ctx.debug_port,
        "helper_port": ctx.helper_port
    })
}

fn enhancement_settings() -> Value {
    let path = crate::app_paths::app_state_dir().join("enhancement-settings.json");
    let parsed = std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .unwrap_or_else(|| json!({}));
    json!({
        "status": "ok",
        "result": {
            "enabled": parsed.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            "timeline": parsed.get("timeline").and_then(Value::as_bool).unwrap_or(true),
            "inlineActions": parsed.get("inlineActions").and_then(Value::as_bool).unwrap_or(true),
            "scrollRestore": parsed.get("scrollRestore").and_then(Value::as_bool).unwrap_or(true)
        }
    })
}

fn plugin_patch_status() -> Value {
    let provider = crate::relay_config::default_relay_provider_config();
    json!({
        "status": "ok",
        "result": {
            "mode": provider.mode,
            "authenticated": provider.authenticated,
            "configured": provider.configured,
            "pluginPatchEnabled": provider.active && provider.mode == "api"
        }
    })
}


fn apply_provider(payload: Value) -> Value {
    let base_url = payload
        .get("base_url")
        .or_else(|| payload.get("baseUrl"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let bearer_token = payload
        .get("bearer_token")
        .or_else(|| payload.get("bearerToken"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    crate::relay_config::apply_relay_provider_config(base_url, bearer_token)
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|error| failed(error.to_string()))
}

fn clear_provider() -> Value {
    crate::relay_config::clear_relay_provider_config()
        .map(|result| json!({"status": "ok", "result": result}))
        .unwrap_or_else(|error| failed(error.to_string()))
}

fn report_diagnostics(payload: Value) -> Value {
    let event = payload
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("renderer.event");
    let detail = payload.get("detail").cloned().unwrap_or(json!({}));
    crate::diagnostic_log::append(&format!("renderer.{event}"), detail)
        .map(|_| json!({"status": "ok"}))
        .unwrap_or_else(|error| failed(error.to_string()))
}

pub(crate) fn session_ref_from_payload(
    payload: &Value,
) -> Option<codex_pilot_data::storage::SessionRef> {
    let id = payload
        .get("id")
        .or_else(|| payload.get("session_id"))
        .or_else(|| payload.get("sessionId"))
        .and_then(Value::as_str)?
        .trim();
    if id.is_empty() {
        return None;
    }
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Some(codex_pilot_data::storage::SessionRef::new(
        id.to_string(),
        title,
    ))
}

pub(crate) fn failed(message: impl Into<String>) -> Value {
    json!({
        "status": "failed",
        "message": message.into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn backend_status_reports_bridge_transport() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9688,
                helper_port: 58888,
                db_path: std::path::PathBuf::from("state_5.sqlite"),
            },
            "/backend/status",
            json!({}),
        )
        .await;

        assert_eq!(result["status"], "ok");
        assert_eq!(result["debug_port"], 9688);
        assert_eq!(result["helper_port"], 58888);
        assert_eq!(result["transport"], "cdp-binding");
    }

    #[tokio::test]
    async fn unknown_path_uses_failed_shape() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9688,
                helper_port: 58888,
                db_path: std::path::PathBuf::from("state_5.sqlite"),
            },
            "/missing",
            json!({}),
        )
        .await;

        assert_eq!(result["status"], "failed");
        assert!(result["message"].as_str().unwrap().contains("/missing"));
    }

    #[tokio::test]
    async fn recover_bridge_schedules_reinjection() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9,
                helper_port: 58888,
                db_path: std::path::PathBuf::from("state_5.sqlite"),
            },
            "/backend/recover-bridge",
            json!({}),
        )
        .await;

        assert_eq!(result["status"], "ok");
        assert!(result["message"].as_str().unwrap().contains("恢复已启动"));
    }

    #[tokio::test]
    async fn diagnostics_report_uses_ok_shape() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let root = std::env::temp_dir().join(format!(
            "codex-pilot-route-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        crate::diagnostic_log::set_test_log_path(root.join("diagnostic.log"));

        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9688,
                helper_port: 58888,
                db_path: std::path::PathBuf::from("state_5.sqlite"),
            },
            "/diagnostics/report",
            json!({
                "event": "test",
                "detail": {
                    "message": "hello",
                    "api_key": "sk-test"
                }
            }),
        )
        .await;

        assert_eq!(result["status"], "ok");
        let text = std::fs::read_to_string(root.join("diagnostic.log")).unwrap();
        assert!(text.contains("renderer.test"));
        assert!(text.contains("[redacted]"));
        assert!(!text.contains("sk-test"));
        let _ = std::fs::remove_dir_all(root);
        crate::diagnostic_log::clear_test_log_path();
    }

    #[tokio::test]
    async fn recycle_bin_list_uses_ok_shape() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let db_path = std::env::temp_dir().join(format!(
            "codex-pilot-core-recycle-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9688,
                helper_port: 58888,
                db_path: db_path.clone(),
            },
            "/session/recycle-bin/list",
            json!({}),
        )
        .await;

        assert_eq!(result["status"], "ok");
        assert!(result["result"].as_array().is_some());
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn recycle_bin_entries_use_camel_case_fields() {
        let _guard = crate::diagnostic_log::test_log_guard();
        let db_path = std::env::temp_dir().join(format!(
            "codex-pilot-core-recycle-camel-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backup_dir = db_path.parent().unwrap().join(".codex-pilot-undo");
        std::fs::create_dir_all(&backup_dir).unwrap();
        let backup_path = backup_dir.join("s1-1.json");
        std::fs::write(
            &backup_path,
            serde_json::to_vec(&json!({
                "session_id": "s1",
                "db_path": db_path,
                "schema": "codex_threads",
                "tables": {
                    "threads": [{
                        "id": "s1",
                        "title": "Fixture",
                        "cwd": "/Users/huanglin/code/github/CodexPilot",
                        "updated_at_ms": 1770000000000u64,
                        "rollout_path": "/tmp/rollout.jsonl"
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let result = handle_bridge_request(
            BridgeContext {
                debug_port: 9688,
                helper_port: 58888,
                db_path: db_path.clone(),
            },
            "/session/recycle-bin/list",
            json!({}),
        )
        .await;

        let entry = &result["result"][0];
        assert_eq!(entry["sessionId"], "s1");
        assert_eq!(
            entry["projectCwd"],
            "/Users/huanglin/code/github/CodexPilot"
        );
        assert_eq!(entry["lastActiveAt"], 1770000000u64);
        assert!(entry.get("session_id").is_none());
        assert!(entry.get("deletedAt").is_some());
        assert!(entry.get("last_active_at").is_none());
        assert!(entry.get("deleted_at").is_none());
        assert!(entry.get("backupPath").is_some());
        assert!(entry.get("backup_path").is_none());

        let _ = std::fs::remove_file(backup_path);
        let _ = std::fs::remove_dir_all(backup_dir);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn plugin_patch_status_enables_for_api_mode() {
        let root = std::env::temp_dir().join(format!(
            "codex-pilot-plugin-patch-status-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.toml"),
            r#"model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://api.example/v1"
codex_pilot_channel_mode = "api"
experimental_bearer_token = "sk-api"
"#,
        )
        .unwrap();
        std::fs::write(root.join("auth.json"), r#"{"OPENAI_API_KEY":"sk-api"}"#).unwrap();

        let provider =
            crate::relay_config::relay_provider_config_from_path(&root.join("config.toml"));
        assert_eq!(provider.mode, "api");
        assert!(provider.configured);

        let payload = json!({
            "status": "ok",
            "result": {
                "mode": provider.mode,
                "authenticated": provider.authenticated,
                "configured": provider.configured,
                "pluginPatchEnabled": provider.active && provider.mode == "api"
            }
        });
        assert_eq!(payload["result"]["mode"], "api");
        assert_eq!(payload["result"]["pluginPatchEnabled"], true);

        let _ = std::fs::remove_dir_all(root);
    }
}
