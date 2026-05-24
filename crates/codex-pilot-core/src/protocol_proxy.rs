use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

pub const DEFAULT_PROTOCOL_PROXY_PORT: u16 = crate::ports::DEFAULT_HELPER_PORT;
const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";
const EXTRA_CHAT_PASSTHROUGH_FIELDS: &[&str] = &[
    "frequency_penalty",
    "logit_bias",
    "logprobs",
    "metadata",
    "n",
    "parallel_tool_calls",
    "presence_penalty",
    "response_format",
    "seed",
    "service_tier",
    "stop",
    "stream_options",
    "top_logprobs",
    "user",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamProtocol {
    #[default]
    Responses,
    ChatCompletions,
    AnthropicMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Direct,
    LocalProxy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyHttpResponse {
    pub status: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub struct UpstreamStreamResponse {
    pub status_code: u16,
    pub content_type: String,
    pub response: reqwest::Response,
}

#[derive(Debug, Clone)]
pub struct ActiveProxyTarget {
    pub base_url: String,
    pub api_key: String,
    pub protocol: UpstreamProtocol,
}

pub fn local_responses_proxy_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

pub fn route_mode_for_protocol(protocol: UpstreamProtocol) -> RouteMode {
    match protocol {
        UpstreamProtocol::Responses => RouteMode::Direct,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::AnthropicMessages => {
            RouteMode::LocalProxy
        }
    }
}

pub fn proxy_base_url_for_protocol(
    base_url: &str,
    protocol: UpstreamProtocol,
    helper_port: u16,
) -> String {
    match route_mode_for_protocol(protocol) {
        RouteMode::Direct => base_url.trim().to_string(),
        RouteMode::LocalProxy => local_responses_proxy_base_url(helper_port),
    }
}

pub fn is_responses_proxy_path(path: &str) -> bool {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    matches!(
        path,
        "/responses" | "/v1/responses" | "/responses/compact" | "/v1/responses/compact"
    )
}

pub fn is_models_proxy_path(path: &str) -> bool {
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    matches!(path, "/models" | "/v1/models")
}

pub async fn handle_responses_proxy_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    match target.protocol {
        UpstreamProtocol::Responses => passthrough_responses_request(target, body).await,
        UpstreamProtocol::ChatCompletions => chat_completions_responses_request(target, body).await,
        UpstreamProtocol::AnthropicMessages => {
            anthropic_messages_responses_request(target, body).await
        }
    }
}

pub fn responses_request_wants_stream(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub async fn handle_models_proxy_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    match target.protocol {
        UpstreamProtocol::Responses => passthrough_models_request(target).await,
        UpstreamProtocol::ChatCompletions => passthrough_chat_models_request(target).await,
        UpstreamProtocol::AnthropicMessages => passthrough_anthropic_models_request(target).await,
    }
}

async fn passthrough_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let endpoint = responses_url(&target.base_url);
    let payload: Value = serde_json::from_str(body)?;
    let mut request = reqwest::Client::new()
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&payload);
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

async fn passthrough_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = reqwest::Client::new().get(models_url(&target.base_url));
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

async fn passthrough_chat_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = reqwest::Client::new().get(models_url(&target.base_url));
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

async fn passthrough_anthropic_models_request(
    target: &ActiveProxyTarget,
) -> anyhow::Result<ProxyHttpResponse> {
    let mut request = reqwest::Client::new().get(models_url(&target.base_url));
    request = request
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-dangerous-direct-browser-access", "true");
    if !target.api_key.trim().is_empty() {
        request = request.header("x-api-key", target.api_key.trim());
    }
    let response = request.send().await?;
    proxy_http_response_from_reqwest(response).await
}

async fn chat_completions_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let is_stream = responses_request_wants_stream(body);
    let upstream = open_chat_completions_upstream(target, body).await?;
    let status_code = upstream.status_code;
    let content_type = upstream.content_type.clone();
    let upstream_body = upstream.response.bytes().await?;

    if !(200..300).contains(&status_code) {
        return Ok(ProxyHttpResponse {
            status: http_status_line(status_code),
            content_type,
            body: upstream_body.to_vec(),
        });
    }

    if is_stream || content_type.contains("text/event-stream") {
        let text = String::from_utf8_lossy(&upstream_body);
        return Ok(ProxyHttpResponse {
            status: "200 OK".to_string(),
            content_type: "text/event-stream; charset=utf-8".to_string(),
            body: chat_sse_to_responses_sse(&text).into_bytes(),
        });
    }

    let chat_json: Value = serde_json::from_slice(&upstream_body)?;
    let response_json = chat_completion_to_response(chat_json)?;
    Ok(ProxyHttpResponse {
        status: "200 OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&response_json)?,
    })
}

async fn anthropic_messages_responses_request(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<ProxyHttpResponse> {
    let is_stream = responses_request_wants_stream(body);
    let upstream = open_anthropic_messages_upstream(target, body).await?;
    let status_code = upstream.status_code;
    let content_type = upstream.content_type.clone();
    let upstream_body = upstream.response.bytes().await?;

    if !(200..300).contains(&status_code) {
        return Ok(ProxyHttpResponse {
            status: http_status_line(status_code),
            content_type,
            body: upstream_body.to_vec(),
        });
    }

    if is_stream || content_type.contains("text/event-stream") {
        let text = String::from_utf8_lossy(&upstream_body);
        return Ok(ProxyHttpResponse {
            status: "200 OK".to_string(),
            content_type: "text/event-stream; charset=utf-8".to_string(),
            body: anthropic_sse_to_responses_sse(&text).into_bytes(),
        });
    }

    let anthropic_json: Value = serde_json::from_slice(&upstream_body)?;
    let response_json = anthropic_message_to_response(anthropic_json)?;
    Ok(ProxyHttpResponse {
        status: "200 OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec(&response_json)?,
    })
}

pub async fn stream_chat_completions_as_responses(
    stream: &mut tokio::net::TcpStream,
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<()> {
    let upstream = open_chat_completions_upstream(target, body).await?;
    if !(200..300).contains(&upstream.status_code) {
        let body = upstream.response.bytes().await?.to_vec();
        write_http_response(
            stream,
            &http_status_line(upstream.status_code),
            if upstream.content_type.is_empty() {
                "application/json; charset=utf-8"
            } else {
                &upstream.content_type
            },
            &body,
        )
        .await?;
        return Ok(());
    }

    write_http_stream_headers(stream, "200 OK", "text/event-stream; charset=utf-8").await?;
    let mut converter = ChatSseToResponsesConverter::default();
    let mut response = upstream.response;
    loop {
        match response.chunk().await {
            Ok(Some(bytes)) => {
                let converted = converter.push_bytes(&bytes);
                if !converted.is_empty() {
                    stream.write_all(&converted).await?;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let mut failed = String::new();
                converter
                    .state
                    .failed_into(&mut failed, format!("Stream error: {error}"));
                if !failed.is_empty() {
                    stream.write_all(failed.as_bytes()).await?;
                }
                stream.shutdown().await?;
                return Ok(());
            }
        }
    }

    let tail = converter.finish();
    if !tail.is_empty() {
        stream.write_all(&tail).await?;
    }
    stream.shutdown().await?;
    Ok(())
}

pub async fn stream_anthropic_messages_as_responses(
    stream: &mut tokio::net::TcpStream,
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<()> {
    let upstream = open_anthropic_messages_upstream(target, body).await?;
    if !(200..300).contains(&upstream.status_code) {
        let body = upstream.response.bytes().await?.to_vec();
        write_http_response(
            stream,
            &http_status_line(upstream.status_code),
            if upstream.content_type.is_empty() {
                "application/json; charset=utf-8"
            } else {
                &upstream.content_type
            },
            &body,
        )
        .await?;
        return Ok(());
    }

    write_http_stream_headers(stream, "200 OK", "text/event-stream; charset=utf-8").await?;
    let mut converter = AnthropicSseToResponsesConverter::default();
    let mut response = upstream.response;
    loop {
        match response.chunk().await {
            Ok(Some(bytes)) => {
                let converted = converter.push_bytes(&bytes);
                if !converted.is_empty() {
                    stream.write_all(&converted).await?;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let mut failed = String::new();
                converter
                    .state
                    .failed_into(&mut failed, format!("Stream error: {error}"));
                if !failed.is_empty() {
                    stream.write_all(failed.as_bytes()).await?;
                }
                stream.shutdown().await?;
                return Ok(());
            }
        }
    }

    let tail = converter.finish();
    if !tail.is_empty() {
        stream.write_all(&tail).await?;
    }
    stream.shutdown().await?;
    Ok(())
}

async fn open_chat_completions_upstream(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<UpstreamStreamResponse> {
    let request_json: Value = serde_json::from_str(body)?;
    let chat_request = responses_to_chat_completions(request_json)?;
    let mut request = reqwest::Client::new()
        .post(chat_completions_url(&target.base_url))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&chat_request);
    if !target.api_key.trim().is_empty() {
        request = request.bearer_auth(target.api_key.trim());
    }
    let response = request.send().await?;
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    Ok(UpstreamStreamResponse {
        status_code,
        content_type,
        response,
    })
}

async fn open_anthropic_messages_upstream(
    target: &ActiveProxyTarget,
    body: &str,
) -> anyhow::Result<UpstreamStreamResponse> {
    let request_json: Value = serde_json::from_str(body)?;
    let anthropic_request = responses_to_anthropic_messages(request_json)?;
    let mut request = reqwest::Client::new()
        .post(anthropic_messages_url(&target.base_url))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-dangerous-direct-browser-access", "true")
        .json(&anthropic_request);
    if !target.api_key.trim().is_empty() {
        request = request.header("x-api-key", target.api_key.trim());
    }
    let response = request.send().await?;
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    Ok(UpstreamStreamResponse {
        status_code,
        content_type,
        response,
    })
}

async fn proxy_http_response_from_reqwest(
    response: reqwest::Response,
) -> anyhow::Result<ProxyHttpResponse> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json; charset=utf-8")
        .to_string();
    let body = response.bytes().await?.to_vec();
    Ok(ProxyHttpResponse {
        status: http_status_line(status.as_u16()),
        content_type,
        body,
    })
}

pub fn responses_to_chat_completions(body: Value) -> anyhow::Result<Value> {
    let mut result = json!({});

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = response_text(instructions);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }

    if let Some(input) = body.get("input") {
        append_responses_input(input, &mut messages);
    }
    result["messages"] = json!(messages);

    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    if let Some(value) = body.get("max_output_tokens") {
        if supports_max_completion_tokens(model) {
            result["max_completion_tokens"] = value.clone();
        } else {
            result["max_tokens"] = value.clone();
        }
    }
    if let Some(value) = body.get("max_tokens") {
        result["max_tokens"] = value.clone();
    }
    if let Some(value) = body.get("max_completion_tokens") {
        result["max_completion_tokens"] = value.clone();
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    if supports_reasoning_effort(model)
        && let Some(effort) = body.pointer("/reasoning/effort")
    {
        result["reasoning_effort"] = effort.clone();
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted = tools
            .iter()
            .filter_map(responses_tool_to_chat_tool)
            .collect::<Vec<_>>();
        if !converted.is_empty() {
            result["tools"] = json!(converted);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        result["tool_choice"] = responses_tool_choice_to_chat(tool_choice);
    }

    for key in EXTRA_CHAT_PASSTHROUGH_FIELDS {
        if let Some(value) = body.get(*key) {
            result[*key] = value.clone();
        }
    }

    Ok(result)
}

pub fn responses_to_anthropic_messages(body: Value) -> anyhow::Result<Value> {
    let mut result = json!({});

    if let Some(model) = body.get("model") {
        result["model"] = model.clone();
    }

    if let Some(instructions) = body.get("instructions") {
        let text = response_text(instructions);
        if !text.is_empty() {
            result["system"] = json!(text);
        }
    }

    let mut messages = Vec::new();
    if let Some(input) = body.get("input") {
        append_responses_input_as_anthropic(input, &mut messages);
    }
    result["messages"] = json!(messages);

    if let Some(value) = body.get("max_output_tokens") {
        result["max_tokens"] = value.clone();
    } else if let Some(value) = body.get("max_tokens") {
        result["max_tokens"] = value.clone();
    } else if let Some(value) = body.get("max_completion_tokens") {
        result["max_tokens"] = value.clone();
    } else {
        result["max_tokens"] = json!(4096);
    }

    for key in ["temperature", "top_p", "stream"] {
        if let Some(value) = body.get(key) {
            result[key] = value.clone();
        }
    }

    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let converted = tools
            .iter()
            .filter_map(responses_tool_to_anthropic_tool)
            .collect::<Vec<_>>();
        if !converted.is_empty() {
            result["tools"] = json!(converted);
        }
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        let mapped = responses_tool_choice_to_anthropic(tool_choice);
        if !mapped.is_null() {
            result["tool_choice"] = mapped;
        }
    }

    Ok(result)
}

pub fn chat_completion_to_response(body: Value) -> anyhow::Result<Value> {
    let choices = body
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("chat response missing choices"))?;
    let choice = choices
        .first()
        .ok_or_else(|| anyhow::anyhow!("chat response choices is empty"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow::anyhow!("chat response choice missing message"))?;

    let response_id = response_id_from_chat_id(body.get("id").and_then(Value::as_str));
    let mut output = Vec::new();
    if let Some(reasoning) = chat_reasoning_to_response_output_item(message, &response_id) {
        output.push(reasoning);
    }
    if let Some(message) = chat_message_to_response_output_item(message, &response_id) {
        output.push(message);
    }
    output.extend(chat_tool_calls_to_response_output_items(message));

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": body.get("created").and_then(Value::as_u64).unwrap_or(0),
        "status": response_status(choice.get("finish_reason").and_then(Value::as_str)),
        "model": body.get("model").and_then(Value::as_str).unwrap_or(""),
        "output": output,
        "usage": chat_usage_to_responses_usage(body.get("usage"))
    });

    if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

pub fn anthropic_message_to_response(body: Value) -> anyhow::Result<Value> {
    let response_id = response_id_from_chat_id(body.get("id").and_then(Value::as_str));
    let stop_reason = body.get("stop_reason").and_then(Value::as_str);
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let created_at = body
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(parse_iso8601_timestamp)
        .unwrap_or(0);

    let mut output = Vec::new();
    let mut text_content = Vec::new();
    if let Some(content) = body.get("content").and_then(Value::as_array) {
        for (index, block) in content.iter().enumerate() {
            match block.get("type").and_then(Value::as_str).unwrap_or("") {
                "thinking" => {
                    if let Some(text) = block.get("thinking").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        output.push(json!({
                            "id": format!("rs_{response_id}_{index}"),
                            "type": "reasoning",
                            "summary": [{ "type": "summary_text", "text": text }]
                        }));
                    }
                }
                "text" => {
                    if let Some(text) = block.get("text").and_then(Value::as_str)
                        && !text.is_empty()
                    {
                        text_content.push(
                            json!({ "type": "output_text", "text": text, "annotations": [] }),
                        );
                    }
                }
                "tool_use" => {
                    let call_id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    output.push(json!({
                        "id": format!("fc_{call_id}"),
                        "type": "function_call",
                        "status": "completed",
                        "call_id": call_id,
                        "name": block.get("name").and_then(Value::as_str).unwrap_or(""),
                        "arguments": json_string(block.get("input").unwrap_or(&json!({})))
                    }));
                }
                _ => {}
            }
        }
    }

    if !text_content.is_empty() {
        output.insert(
            output
                .iter()
                .take_while(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
                .count(),
            json!({
                "id": format!("{response_id}_msg"),
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": text_content
            }),
        );
    }

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "created_at": created_at,
        "status": anthropic_stop_reason_to_response_status(stop_reason),
        "model": model,
        "output": output,
        "usage": anthropic_usage_to_responses_usage(body.get("usage"))
    });

    if stop_reason == Some("max_tokens") {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }

    Ok(response)
}

pub fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/chat/completions") {
        return base.to_string();
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

pub fn anthropic_messages_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/messages") {
        return base.to_string();
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

pub fn models_url(base_url: &str) -> String {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    if base.to_ascii_lowercase().ends_with("/chat/completions") {
        base.truncate(base.len() - "/chat/completions".len());
    }
    if base.to_ascii_lowercase().ends_with("/models") {
        return base;
    }
    let origin_only = base
        .split_once("://")
        .map_or(!base.contains('/'), |(_, rest)| !rest.contains('/'));
    let mut url = if base.ends_with("/v1") || !origin_only {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    while url.contains("/v1/v1") {
        url = url.replace("/v1/v1", "/v1");
    }
    url
}

fn responses_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.to_ascii_lowercase().ends_with("/responses") {
        return base.to_string();
    }
    if base.ends_with("/v1") {
        return format!("{base}/responses");
    }
    format!("{base}/v1/responses")
}

pub fn chat_sse_to_responses_sse(input: &str) -> String {
    let mut converter = ChatSseToResponsesConverter::default();
    let mut output = converter.push_bytes(input.as_bytes());
    output.extend(converter.finish());
    String::from_utf8(output).unwrap_or_default()
}

pub fn anthropic_sse_to_responses_sse(input: &str) -> String {
    let mut converter = AnthropicSseToResponsesConverter::default();
    let mut output = converter.push_bytes(input.as_bytes());
    output.extend(converter.finish());
    String::from_utf8(output).unwrap_or_default()
}

pub struct ChatSseToResponsesConverter {
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: ChatSseState,
    failed: bool,
}

pub struct AnthropicSseToResponsesConverter {
    buffer: String,
    utf8_remainder: Vec<u8>,
    state: AnthropicSseState,
    failed: bool,
}

impl Default for ChatSseToResponsesConverter {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            utf8_remainder: Vec::new(),
            state: ChatSseState::default(),
            failed: false,
        }
    }
}

impl Default for AnthropicSseToResponsesConverter {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            utf8_remainder: Vec::new(),
            state: AnthropicSseState::default(),
            failed: false,
        }
    }
}

impl ChatSseToResponsesConverter {
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);
        let mut output = String::new();
        while let Some(block) = take_sse_block(&mut self.buffer) {
            if block.trim().is_empty() {
                continue;
            }
            self.handle_block(&block, &mut output);
            if self.failed {
                break;
            }
        }
        output.into_bytes()
    }

    pub fn finish(&mut self) -> Vec<u8> {
        if !self.utf8_remainder.is_empty() {
            self.buffer
                .push_str(&String::from_utf8_lossy(&self.utf8_remainder));
            self.utf8_remainder.clear();
        }

        let mut output = String::new();
        if !self.failed {
            self.state.finalize_into(&mut output);
        }
        output.into_bytes()
    }

    fn handle_block(&mut self, block: &str, output: &mut String) {
        let mut event_name: Option<String> = None;
        let mut data_parts = Vec::new();
        for line in block.lines() {
            if let Some(event) = strip_sse_field(line, "event") {
                event_name = Some(event.trim().to_string());
            }
            if let Some(data) = strip_sse_field(line, "data") {
                data_parts.push(data.to_string());
            }
        }

        if data_parts.is_empty() {
            return;
        }
        let data = data_parts.join("\n");
        if data.trim() == "[DONE]" {
            self.state.finalize_into(output);
            return;
        }

        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
            let message = chunk
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("upstream stream error")
                .to_string();
            self.state.failed_into(output, message);
            self.failed = true;
            return;
        }
        self.state.handle_chat_chunk_into(&chunk, output);
    }
}

impl AnthropicSseToResponsesConverter {
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        append_utf8_safe(&mut self.buffer, &mut self.utf8_remainder, bytes);
        let mut output = String::new();
        while let Some(block) = take_sse_block(&mut self.buffer) {
            if block.trim().is_empty() {
                continue;
            }
            self.handle_block(&block, &mut output);
            if self.failed {
                break;
            }
        }
        output.into_bytes()
    }

    pub fn finish(&mut self) -> Vec<u8> {
        if !self.utf8_remainder.is_empty() {
            self.buffer
                .push_str(&String::from_utf8_lossy(&self.utf8_remainder));
            self.utf8_remainder.clear();
        }

        let mut output = String::new();
        if !self.failed {
            self.state.finalize_into(&mut output);
        }
        output.into_bytes()
    }

    fn handle_block(&mut self, block: &str, output: &mut String) {
        let mut event_name: Option<String> = None;
        let mut data_parts = Vec::new();
        for line in block.lines() {
            if let Some(event) = strip_sse_field(line, "event") {
                event_name = Some(event.trim().to_string());
            }
            if let Some(data) = strip_sse_field(line, "data") {
                data_parts.push(data.to_string());
            }
        }

        if data_parts.is_empty() {
            return;
        }
        let data = data_parts.join("\n");
        if data.trim() == "[DONE]" {
            self.state.finalize_into(output);
            return;
        }

        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if event_name.as_deref() == Some("error") || chunk.get("error").is_some() {
            let message = chunk
                .get("error")
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("upstream stream error")
                .to_string();
            self.state.failed_into(output, message);
            self.failed = true;
            return;
        }

        self.state
            .handle_anthropic_event_into(event_name.as_deref(), &chunk, output);
    }
}

pub fn response_id_from_chat_id(id: Option<&str>) -> String {
    id.map(|value| {
        if value.starts_with("resp_") {
            value.to_string()
        } else {
            format!("resp_{value}")
        }
    })
    .unwrap_or_else(|| "resp_codexpilot".to_string())
}

fn push_sse(output: &mut String, event: &str, data: Value) {
    output.push_str("event: ");
    output.push_str(event);
    output.push_str("\ndata: ");
    output.push_str(&serde_json::to_string(&data).unwrap_or_default());
    output.push_str("\n\n");
}

#[derive(Debug, Default)]
struct TextItemState {
    output_index: Option<u32>,
    item_id: String,
    text: String,
    added: bool,
    done: bool,
}

#[derive(Debug, Default)]
struct ReasoningItemState {
    output_index: Option<u32>,
    item_id: String,
    text: String,
    added: bool,
    done: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum InlineThinkMode {
    #[default]
    Detecting,
    Reasoning,
    Text,
}

#[derive(Debug, Default)]
struct InlineThinkState {
    mode: InlineThinkMode,
    buffer: String,
}

#[derive(Debug, Default)]
struct ToolCallState {
    output_index: Option<u32>,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    added: bool,
    done: bool,
}

#[derive(Debug, Default)]
struct AnthropicContentBlockState {
    block_type: String,
    id: String,
    name: String,
    json_input: String,
    output_index: Option<u32>,
    item_id: String,
    added: bool,
    done: bool,
}

#[derive(Debug)]
struct ChatSseState {
    response_started: bool,
    completed: bool,
    response_id: String,
    model: String,
    created_at: u64,
    next_output_index: u32,
    text: TextItemState,
    reasoning: ReasoningItemState,
    inline_think: InlineThinkState,
    tools: BTreeMap<usize, ToolCallState>,
    output_items: Vec<(u32, Value)>,
    latest_usage: Option<Value>,
    finish_reason: Option<String>,
}

#[derive(Debug)]
struct AnthropicSseState {
    response_started: bool,
    completed: bool,
    response_id: String,
    model: String,
    created_at: u64,
    next_output_index: u32,
    text: TextItemState,
    reasoning: ReasoningItemState,
    active_blocks: BTreeMap<usize, AnthropicContentBlockState>,
    output_items: Vec<(u32, Value)>,
    latest_usage: Option<Value>,
    stop_reason: Option<String>,
}

impl Default for ChatSseState {
    fn default() -> Self {
        Self {
            response_started: false,
            completed: false,
            response_id: "resp_codexpilot".to_string(),
            model: String::new(),
            created_at: 0,
            next_output_index: 0,
            text: TextItemState::default(),
            reasoning: ReasoningItemState::default(),
            inline_think: InlineThinkState::default(),
            tools: BTreeMap::new(),
            output_items: Vec::new(),
            latest_usage: None,
            finish_reason: None,
        }
    }
}

impl Default for AnthropicSseState {
    fn default() -> Self {
        Self {
            response_started: false,
            completed: false,
            response_id: "resp_codexpilot".to_string(),
            model: String::new(),
            created_at: 0,
            next_output_index: 0,
            text: TextItemState::default(),
            reasoning: ReasoningItemState::default(),
            active_blocks: BTreeMap::new(),
            output_items: Vec::new(),
            latest_usage: None,
            stop_reason: None,
        }
    }
}

impl ChatSseState {
    fn handle_chat_chunk_into(&mut self, chunk: &Value, output: &mut String) {
        if let Some(id) = chunk.get("id").and_then(Value::as_str) {
            self.response_id = response_id_from_chat_id(Some(id));
        }
        if let Some(model) = chunk.get("model").and_then(Value::as_str) {
            if !model.is_empty() {
                self.model = model.to_string();
            }
        }
        if let Some(created) = chunk.get("created").and_then(Value::as_u64) {
            self.created_at = created;
        }
        self.ensure_response_started_into(output);

        if let Some(usage) = chunk.get("usage").filter(|value| !value.is_null()) {
            self.latest_usage = Some(chat_usage_to_responses_usage(Some(usage)));
        }

        let Some(choice) = chunk
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return;
        };

        if let Some(delta) = choice.get("delta") {
            if let Some(reasoning) = chat_delta_reasoning_text(delta) {
                self.push_reasoning_delta_into(&reasoning, output);
            }

            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                if !content.is_empty() {
                    self.push_content_delta_into(content, output);
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                self.flush_inline_think_at_boundary_into(output);
                self.finalize_reasoning_into(output);
                for tool_call in tool_calls {
                    self.push_tool_call_delta_into(tool_call, output);
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(finish_reason.to_string());
        }
    }

    fn ensure_response_started_into(&mut self, output: &mut String) {
        if self.response_started {
            return;
        }
        self.response_started = true;
        push_sse(
            output,
            "response.created",
            json!({
                "type": "response.created",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
        push_sse(
            output,
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
    }

    fn push_reasoning_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.reasoning.added {
            let output_index = self.next_output_index();
            let item_id = format!("rs_{}", self.response_id);
            self.reasoning.output_index = Some(output_index);
            self.reasoning.item_id = item_id.clone();
            self.reasoning.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "reasoning",
                        "status": "in_progress",
                        "summary": []
                    }
                }),
            );
        }
        self.reasoning.text.push_str(delta);
        push_sse(
            output,
            "response.reasoning_summary_text.delta",
            json!({
                "type": "response.reasoning_summary_text.delta",
                "item_id": self.reasoning.item_id,
                "output_index": self.reasoning.output_index.unwrap_or(0),
                "summary_index": 0,
                "delta": delta
            }),
        );
    }

    fn push_text_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.text.added {
            let output_index = self.next_output_index();
            let item_id = format!("{}_msg", self.response_id);
            self.text.output_index = Some(output_index);
            self.text.item_id = item_id.clone();
            self.text.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": []
                    }
                }),
            );
        }
        self.text.text.push_str(delta);
        push_sse(
            output,
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": self.text.item_id,
                "output_index": self.text.output_index.unwrap_or(0),
                "content_index": 0,
                "delta": delta
            }),
        );
    }

    fn push_content_delta_into(&mut self, delta: &str, output: &mut String) {
        match self.inline_think.mode {
            InlineThinkMode::Text => {
                self.finalize_reasoning_into(output);
                self.push_text_delta_into(delta, output);
            }
            InlineThinkMode::Detecting => {
                self.inline_think.buffer.push_str(delta);
                match leading_think_prefix_decision(&self.inline_think.buffer) {
                    ThinkPrefixDecision::NeedMore => {}
                    ThinkPrefixDecision::Reasoning => {
                        self.inline_think.mode = InlineThinkMode::Reasoning;
                        self.drain_complete_inline_think_into(output);
                    }
                    ThinkPrefixDecision::Text => {
                        self.inline_think.mode = InlineThinkMode::Text;
                        let text = std::mem::take(&mut self.inline_think.buffer);
                        self.finalize_reasoning_into(output);
                        self.push_text_delta_into(&text, output);
                    }
                }
            }
            InlineThinkMode::Reasoning => {
                self.inline_think.buffer.push_str(delta);
                self.drain_complete_inline_think_into(output);
            }
        }
    }

    fn drain_complete_inline_think_into(&mut self, output: &mut String) {
        let Some((reasoning, answer)) = split_leading_think_block(&self.inline_think.buffer) else {
            return;
        };
        self.inline_think.mode = InlineThinkMode::Text;
        self.inline_think.buffer.clear();
        if !reasoning.is_empty() {
            self.push_reasoning_delta_into(&reasoning, output);
            self.finalize_reasoning_into(output);
        }
        if !answer.is_empty() {
            self.push_text_delta_into(&answer, output);
        }
    }

    fn flush_inline_think_at_boundary_into(&mut self, output: &mut String) {
        match self.inline_think.mode {
            InlineThinkMode::Text => {}
            InlineThinkMode::Detecting => {
                self.inline_think.mode = InlineThinkMode::Text;
                let text = std::mem::take(&mut self.inline_think.buffer);
                if !text.is_empty() {
                    self.finalize_reasoning_into(output);
                    self.push_text_delta_into(&text, output);
                }
            }
            InlineThinkMode::Reasoning => {
                let buffered = std::mem::take(&mut self.inline_think.buffer);
                self.inline_think.mode = InlineThinkMode::Text;
                if let Some((reasoning, answer)) = split_leading_think_block(&buffered) {
                    if !reasoning.is_empty() {
                        self.push_reasoning_delta_into(&reasoning, output);
                        self.finalize_reasoning_into(output);
                    }
                    if !answer.is_empty() {
                        self.push_text_delta_into(&answer, output);
                    }
                    return;
                }
                let reasoning = strip_leading_think_open_tag(&buffered).unwrap_or(buffered);
                if !reasoning.is_empty() {
                    self.push_reasoning_delta_into(&reasoning, output);
                    self.finalize_reasoning_into(output);
                }
            }
        }
    }

    fn push_tool_call_delta_into(&mut self, tool_call: &Value, output: &mut String) {
        let chat_index = tool_call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        let id_delta = tool_call
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let function = tool_call.get("function").unwrap_or(&Value::Null);
        let name_delta = function
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string);
        let args_delta = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut call_id = String::new();
        let mut name = String::new();
        let mut output_index = None;
        let mut should_add = false;
        let item_id;

        {
            let state = self.tools.entry(chat_index).or_default();
            if let Some(id) = id_delta {
                state.call_id = id;
            }
            if let Some(next_name) = name_delta {
                state.name = next_name;
            }
            if !args_delta.is_empty() {
                state.arguments.push_str(&args_delta);
            }
            if !state.added {
                should_add = true;
                if state.call_id.is_empty() {
                    state.call_id = format!("call_{chat_index}");
                }
                if state.name.is_empty() {
                    state.name = "unknown_tool".to_string();
                }
                call_id = state.call_id.clone();
                name = state.name.clone();
                item_id = format!("fc_{}", state.call_id);
            } else {
                item_id = state.item_id.clone();
                output_index = state.output_index;
            }
        }

        if should_add {
            let assigned_index = self.next_output_index();
            {
                let state = self.tools.get_mut(&chat_index).expect("tool state exists");
                state.output_index = Some(assigned_index);
                state.added = true;
                state.item_id = item_id.clone();
            }
            output_index = Some(assigned_index);
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": assigned_index,
                    "item": {
                        "id": item_id,
                        "type": "function_call",
                        "status": "in_progress",
                        "call_id": call_id,
                        "name": name,
                        "arguments": ""
                    }
                }),
            );
        }
        if !args_delta.is_empty() {
            push_sse(
                output,
                "response.function_call_arguments.delta",
                json!({
                    "type": "response.function_call_arguments.delta",
                    "item_id": item_id,
                    "output_index": output_index.unwrap_or(0),
                    "delta": args_delta
                }),
            );
        }
    }

    fn finalize_reasoning_into(&mut self, output: &mut String) {
        if !self.reasoning.added || self.reasoning.done {
            return;
        }
        let output_index = self.reasoning.output_index.unwrap_or(0);
        let item = json!({
            "id": self.reasoning.item_id,
            "type": "reasoning",
            "summary": [{ "type": "summary_text", "text": self.reasoning.text }]
        });
        self.output_items.push((output_index, item.clone()));
        self.reasoning.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    fn finalize_text_into(&mut self, output: &mut String) {
        if !self.text.added || self.text.done {
            return;
        }
        let output_index = self.text.output_index.unwrap_or(0);
        let item = json!({
            "id": self.text.item_id,
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": self.text.text, "annotations": [] }]
        });
        self.output_items.push((output_index, item.clone()));
        self.text.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    fn finalize_tools_into(&mut self, output: &mut String) {
        let keys: Vec<usize> = self.tools.keys().copied().collect();
        for key in keys {
            let Some(state) = self.tools.get_mut(&key) else {
                continue;
            };
            if state.done {
                continue;
            }
            let output_index = state.output_index.unwrap_or(0);
            let item = json!({
                "id": state.item_id,
                "type": "function_call",
                "status": "completed",
                "call_id": state.call_id,
                "name": state.name,
                "arguments": state.arguments
            });
            state.done = true;
            self.output_items.push((output_index, item.clone()));
            push_sse(
                output,
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": item
                }),
            );
        }
    }

    fn finalize_into(&mut self, output: &mut String) {
        if self.completed {
            return;
        }
        self.ensure_response_started_into(output);
        self.flush_inline_think_at_boundary_into(output);
        self.finalize_reasoning_into(output);
        self.finalize_text_into(output);
        self.finalize_tools_into(output);
        push_sse(
            output,
            "response.completed",
            json!({
                "type": "response.completed",
                "response": self.base_response(
                    response_status(self.finish_reason.as_deref()),
                    self.completed_output_items()
                )
            }),
        );
        output.push_str("data: [DONE]\n\n");
        self.completed = true;
    }

    fn failed_into(&mut self, output: &mut String, message: String) {
        self.completed = true;
        push_sse(
            output,
            "response.failed",
            json!({
                "type": "response.failed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "failed",
                    "model": self.model,
                    "output": self.completed_output_items(),
                    "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage),
                    "error": { "message": message }
                }
            }),
        );
    }

    fn completed_output_items(&self) -> Vec<Value> {
        let mut output_items = self.output_items.clone();
        output_items.sort_by_key(|(output_index, _)| *output_index);
        output_items.into_iter().map(|(_, item)| item).collect()
    }

    fn base_response(&self, status: &str, output: Vec<Value>) -> Value {
        json!({
            "id": self.response_id,
            "object": "response",
            "created_at": self.created_at,
            "status": status,
            "model": self.model,
            "output": output,
            "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage)
        })
    }

    fn next_output_index(&mut self) -> u32 {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }
}

impl AnthropicSseState {
    fn handle_anthropic_event_into(
        &mut self,
        event_name: Option<&str>,
        chunk: &Value,
        output: &mut String,
    ) {
        match event_name.unwrap_or_default() {
            "message_start" => {
                if let Some(message) = chunk.get("message") {
                    if let Some(id) = message.get("id").and_then(Value::as_str) {
                        self.response_id = response_id_from_chat_id(Some(id));
                    }
                    if let Some(model) = message.get("model").and_then(Value::as_str) {
                        self.model = model.to_string();
                    }
                    if let Some(created_at) = message.get("created_at").and_then(Value::as_str) {
                        self.created_at = parse_iso8601_timestamp(created_at).unwrap_or(0);
                    }
                    self.latest_usage =
                        Some(anthropic_usage_to_responses_usage(message.get("usage")));
                }
                self.ensure_response_started_into(output);
            }
            "content_block_start" => {
                self.ensure_response_started_into(output);
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = chunk.get("content_block").unwrap_or(&Value::Null);
                let block_type = block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let block_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let block_name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let needs_tool_add = {
                    let state = self.active_blocks.entry(index).or_default();
                    state.block_type = block_type.clone();
                    state.id = block_id.clone();
                    state.name = block_name.clone();
                    block_type == "tool_use" && !state.added
                };
                if needs_tool_add {
                    let output_index = self.next_output_index();
                    let call_id = if block_id.is_empty() {
                        format!("call_{index}")
                    } else {
                        block_id.clone()
                    };
                    let item_id = format!("fc_{call_id}");
                    if let Some(state) = self.active_blocks.get_mut(&index) {
                        state.output_index = Some(output_index);
                        state.added = true;
                        state.item_id = item_id.clone();
                    }
                    push_sse(
                        output,
                        "response.output_item.added",
                        json!({
                            "type": "response.output_item.added",
                            "output_index": output_index,
                            "item": {
                                "id": item_id,
                                "type": "function_call",
                                "status": "in_progress",
                                "call_id": call_id,
                                "name": if block_name.is_empty() { "unknown_tool" } else { &block_name },
                                "arguments": ""
                            }
                        }),
                    );
                }
            }
            "content_block_delta" => {
                self.ensure_response_started_into(output);
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let delta = chunk.get("delta").unwrap_or(&Value::Null);
                if let Some(state) = self.active_blocks.get_mut(&index) {
                    match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                        "thinking_delta" => {
                            let text = delta.get("thinking").and_then(Value::as_str).unwrap_or("");
                            if !text.is_empty() {
                                self.push_reasoning_delta_into(text, output);
                            }
                        }
                        "text_delta" => {
                            let text = delta.get("text").and_then(Value::as_str).unwrap_or("");
                            if !text.is_empty() {
                                self.finalize_reasoning_into(output);
                                self.push_text_delta_into(text, output);
                            }
                        }
                        "input_json_delta" => {
                            let partial = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            if !partial.is_empty() {
                                state.json_input.push_str(partial);
                                push_sse(
                                    output,
                                    "response.function_call_arguments.delta",
                                    json!({
                                        "type": "response.function_call_arguments.delta",
                                        "item_id": state.item_id,
                                        "output_index": state.output_index.unwrap_or(0),
                                        "delta": partial
                                    }),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                let index = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(state) = self.active_blocks.get_mut(&index) {
                    match state.block_type.as_str() {
                        "thinking" => self.finalize_reasoning_into(output),
                        "text" => {}
                        "tool_use" => {
                            let output_index = state.output_index.unwrap_or(0);
                            let call_id = if state.id.is_empty() {
                                format!("call_{index}")
                            } else {
                                state.id.clone()
                            };
                            let item = json!({
                                "id": state.item_id,
                                "type": "function_call",
                                "status": "completed",
                                "call_id": call_id,
                                "name": state.name,
                                "arguments": state.json_input
                            });
                            self.output_items.push((output_index, item.clone()));
                            state.done = true;
                            push_sse(
                                output,
                                "response.output_item.done",
                                json!({
                                    "type": "response.output_item.done",
                                    "output_index": output_index,
                                    "item": item
                                }),
                            );
                        }
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(delta) = chunk.get("delta") {
                    self.stop_reason = delta
                        .get("stop_reason")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                }
                if let Some(usage) = chunk.get("usage") {
                    self.latest_usage = Some(anthropic_usage_to_responses_usage(Some(usage)));
                }
            }
            "message_stop" => self.finalize_into(output),
            _ => {}
        }
    }

    fn ensure_response_started_into(&mut self, output: &mut String) {
        if self.response_started {
            return;
        }
        self.response_started = true;
        push_sse(
            output,
            "response.created",
            json!({
                "type": "response.created",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
        push_sse(
            output,
            "response.in_progress",
            json!({
                "type": "response.in_progress",
                "response": self.base_response("in_progress", Vec::new())
            }),
        );
    }

    fn push_reasoning_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.reasoning.added {
            let output_index = self.next_output_index();
            let item_id = format!("rs_{}", self.response_id);
            self.reasoning.output_index = Some(output_index);
            self.reasoning.item_id = item_id.clone();
            self.reasoning.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "reasoning",
                        "status": "in_progress",
                        "summary": []
                    }
                }),
            );
        }
        self.reasoning.text.push_str(delta);
        push_sse(
            output,
            "response.reasoning_summary_text.delta",
            json!({
                "type": "response.reasoning_summary_text.delta",
                "item_id": self.reasoning.item_id,
                "output_index": self.reasoning.output_index.unwrap_or(0),
                "summary_index": 0,
                "delta": delta
            }),
        );
    }

    fn push_text_delta_into(&mut self, delta: &str, output: &mut String) {
        if !self.text.added {
            let output_index = self.next_output_index();
            let item_id = format!("{}_msg", self.response_id);
            self.text.output_index = Some(output_index);
            self.text.item_id = item_id.clone();
            self.text.added = true;
            push_sse(
                output,
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": []
                    }
                }),
            );
        }
        self.text.text.push_str(delta);
        push_sse(
            output,
            "response.output_text.delta",
            json!({
                "type": "response.output_text.delta",
                "item_id": self.text.item_id,
                "output_index": self.text.output_index.unwrap_or(0),
                "content_index": 0,
                "delta": delta
            }),
        );
    }

    fn finalize_reasoning_into(&mut self, output: &mut String) {
        if !self.reasoning.added || self.reasoning.done {
            return;
        }
        let output_index = self.reasoning.output_index.unwrap_or(0);
        let item = json!({
            "id": self.reasoning.item_id,
            "type": "reasoning",
            "summary": [{ "type": "summary_text", "text": self.reasoning.text }]
        });
        self.output_items.push((output_index, item.clone()));
        self.reasoning.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    fn finalize_text_into(&mut self, output: &mut String) {
        if !self.text.added || self.text.done {
            return;
        }
        let output_index = self.text.output_index.unwrap_or(0);
        let item = json!({
            "id": self.text.item_id,
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": self.text.text, "annotations": [] }]
        });
        self.output_items.push((output_index, item.clone()));
        self.text.done = true;
        push_sse(
            output,
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": output_index,
                "item": item
            }),
        );
    }

    fn finalize_into(&mut self, output: &mut String) {
        if self.completed {
            return;
        }
        self.ensure_response_started_into(output);
        self.finalize_reasoning_into(output);
        self.finalize_text_into(output);
        push_sse(
            output,
            "response.completed",
            json!({
                "type": "response.completed",
                "response": self.base_response(
                    anthropic_stop_reason_to_response_status(self.stop_reason.as_deref()),
                    self.completed_output_items()
                )
            }),
        );
        output.push_str("data: [DONE]\n\n");
        self.completed = true;
    }

    fn failed_into(&mut self, output: &mut String, message: String) {
        self.completed = true;
        push_sse(
            output,
            "response.failed",
            json!({
                "type": "response.failed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created_at": self.created_at,
                    "status": "failed",
                    "model": self.model,
                    "output": self.completed_output_items(),
                    "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage),
                    "error": { "message": message }
                }
            }),
        );
    }

    fn completed_output_items(&self) -> Vec<Value> {
        let mut output_items = self.output_items.clone();
        output_items.sort_by_key(|(output_index, _)| *output_index);
        output_items.into_iter().map(|(_, item)| item).collect()
    }

    fn base_response(&self, status: &str, output: Vec<Value>) -> Value {
        json!({
            "id": self.response_id,
            "object": "response",
            "created_at": self.created_at,
            "status": status,
            "model": self.model,
            "output": output,
            "usage": self.latest_usage.clone().unwrap_or_else(default_responses_usage)
        })
    }

    fn next_output_index(&mut self) -> u32 {
        let index = self.next_output_index;
        self.next_output_index += 1;
        index
    }
}

fn take_sse_block(buffer: &mut String) -> Option<String> {
    let lf = buffer.find("\n\n").map(|index| (index, 2));
    let crlf = buffer.find("\r\n\r\n").map(|index| (index, 4));
    let (index, delimiter_len) = match (lf, crlf) {
        (Some(left), Some(right)) => {
            if left.0 <= right.0 {
                left
            } else {
                right
            }
        }
        (Some(value), None) | (None, Some(value)) => value,
        (None, None) => return None,
    };
    let block = buffer[..index].to_string();
    buffer.drain(..index + delimiter_len);
    Some(block)
}

fn append_utf8_safe(buffer: &mut String, remainder: &mut Vec<u8>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let mut combined = Vec::new();
    if !remainder.is_empty() {
        combined.extend_from_slice(remainder);
        remainder.clear();
    }
    combined.extend_from_slice(bytes);

    match std::str::from_utf8(&combined) {
        Ok(text) => buffer.push_str(text),
        Err(error) => {
            let valid = error.valid_up_to();
            if valid > 0 {
                buffer.push_str(std::str::from_utf8(&combined[..valid]).unwrap_or_default());
            }
            if error.error_len().is_none() {
                remainder.extend_from_slice(&combined[valid..]);
            } else {
                buffer.push_str(&String::from_utf8_lossy(&combined[valid..]));
            }
        }
    }
}

fn strip_sse_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(field)?.strip_prefix(':')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

fn chat_delta_reasoning_text(delta: &Value) -> Option<String> {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = delta.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    let reasoning = delta.get("reasoning")?;
    for key in ["content", "text", "summary"] {
        if let Some(text) = reasoning.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

enum ThinkPrefixDecision {
    NeedMore,
    Reasoning,
    Text,
}

fn leading_think_prefix_decision(buffer: &str) -> ThinkPrefixDecision {
    let trimmed = buffer.trim_start();
    if trimmed.is_empty() {
        return ThinkPrefixDecision::NeedMore;
    }
    if trimmed.starts_with(THINK_OPEN_TAG) {
        return ThinkPrefixDecision::Reasoning;
    }
    if THINK_OPEN_TAG.starts_with(trimmed) {
        return ThinkPrefixDecision::NeedMore;
    }
    ThinkPrefixDecision::Text
}

fn append_responses_input(input: &Value, messages: &mut Vec<Value>) {
    match input {
        Value::String(text) => messages.push(json!({ "role": "user", "content": text })),
        Value::Array(items) => {
            let mut pending_tool_calls = Vec::new();
            let mut pending_reasoning = None;
            for item in items {
                append_responses_item(
                    item,
                    messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                );
            }
            flush_tool_calls(messages, &mut pending_tool_calls, &mut pending_reasoning);
            flush_pending_reasoning(messages, &mut pending_reasoning);
        }
        Value::Object(_) => {
            let mut pending_tool_calls = Vec::new();
            let mut pending_reasoning = None;
            append_responses_item(
                input,
                messages,
                &mut pending_tool_calls,
                &mut pending_reasoning,
            );
            flush_tool_calls(messages, &mut pending_tool_calls, &mut pending_reasoning);
            flush_pending_reasoning(messages, &mut pending_reasoning);
        }
        _ => {}
    }
}

fn append_responses_input_as_anthropic(input: &Value, messages: &mut Vec<Value>) {
    match input {
        Value::String(text) => messages.push(json!({
            "role": "user",
            "content": [{ "type": "text", "text": text }]
        })),
        Value::Array(items) => {
            let mut pending_assistant_tool_calls = Vec::new();
            for item in items {
                append_responses_item_as_anthropic(
                    item,
                    messages,
                    &mut pending_assistant_tool_calls,
                );
            }
            flush_anthropic_tool_calls(messages, &mut pending_assistant_tool_calls);
        }
        Value::Object(_) => {
            let mut pending_assistant_tool_calls = Vec::new();
            append_responses_item_as_anthropic(input, messages, &mut pending_assistant_tool_calls);
            flush_anthropic_tool_calls(messages, &mut pending_assistant_tool_calls);
        }
        _ => {}
    }
}

fn append_responses_item(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => pending_tool_calls.push(json!({
            "id": item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            "type": "function",
            "function": {
                "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
                "arguments": json_string(item.get("arguments").unwrap_or(&json!({})))
            }
        })),
        Some("function_call_output") => {
            flush_tool_calls(messages, pending_tool_calls, pending_reasoning);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                "content": response_text(item.get("output").unwrap_or(&Value::Null))
            }));
        }
        Some("reasoning") => {
            if let Some(text) = responses_reasoning_text(item) {
                if !text.is_empty() {
                    *pending_reasoning = Some(text);
                }
            }
        }
        _ => {
            flush_tool_calls(messages, pending_tool_calls, pending_reasoning);
            if item.get("role").is_some() || item.get("content").is_some() {
                let role = responses_role_to_chat_role(item.get("role").and_then(Value::as_str));
                let mut message = json!({
                    "role": role,
                    "content": responses_content_to_chat_content(
                        item.get("content").unwrap_or(&Value::Null)
                    )
                });
                if role == "assistant" {
                    if let Some(reasoning) = pending_reasoning.take() {
                        message["reasoning_content"] = json!(reasoning);
                    }
                }
                messages.push(message);
            }
        }
    }
}

fn append_responses_item_as_anthropic(
    item: &Value,
    messages: &mut Vec<Value>,
    pending_assistant_tool_calls: &mut Vec<Value>,
) {
    match item.get("type").and_then(Value::as_str) {
        Some("function_call") => pending_assistant_tool_calls.push(json!({
            "type": "tool_use",
            "id": item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            "name": item.get("name").and_then(Value::as_str).unwrap_or(""),
            "input": parse_json_or_string(item.get("arguments").unwrap_or(&json!({})))
        })),
        Some("function_call_output") => {
            flush_anthropic_tool_calls(messages, pending_assistant_tool_calls);
            messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": item.get("call_id").and_then(Value::as_str).unwrap_or(""),
                    "content": response_text(item.get("output").unwrap_or(&Value::Null))
                }]
            }));
        }
        Some("reasoning") => {
            let text = responses_reasoning_text(item).unwrap_or_default();
            if !text.is_empty() {
                messages.push(json!({
                    "role": "assistant",
                    "content": [{ "type": "thinking", "thinking": text }]
                }));
            }
        }
        _ => {
            flush_anthropic_tool_calls(messages, pending_assistant_tool_calls);
            if item.get("role").is_some() || item.get("content").is_some() {
                let role =
                    responses_role_to_anthropic_role(item.get("role").and_then(Value::as_str));
                let content = responses_content_to_anthropic_content(
                    item.get("content").unwrap_or(&Value::Null),
                );
                if !content.is_empty() {
                    messages.push(json!({
                        "role": role,
                        "content": content
                    }));
                }
            }
        }
    }
}

fn responses_role_to_chat_role(role: Option<&str>) -> &'static str {
    match role {
        Some("developer") | Some("system") => "system",
        Some("assistant") => "assistant",
        Some("tool") => "tool",
        Some("user") | None => "user",
        Some(_) => "user",
    }
}

fn responses_role_to_anthropic_role(role: Option<&str>) -> &'static str {
    match role {
        Some("assistant") => "assistant",
        Some("tool") => "user",
        Some("developer") | Some("system") | Some("user") | None => "user",
        Some(_) => "user",
    }
}

fn flush_tool_calls(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    pending_reasoning: &mut Option<String>,
) {
    if pending_tool_calls.is_empty() {
        return;
    }
    let mut message = json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": std::mem::take(pending_tool_calls)
    });
    if let Some(reasoning) = pending_reasoning.take() {
        message["reasoning_content"] = json!(reasoning);
    }
    messages.push(message);
}

fn flush_anthropic_tool_calls(
    messages: &mut Vec<Value>,
    pending_assistant_tool_calls: &mut Vec<Value>,
) {
    if pending_assistant_tool_calls.is_empty() {
        return;
    }
    messages.push(json!({
        "role": "assistant",
        "content": std::mem::take(pending_assistant_tool_calls)
    }));
}

fn flush_pending_reasoning(messages: &mut Vec<Value>, pending_reasoning: &mut Option<String>) {
    let Some(reasoning) = pending_reasoning.take() else {
        return;
    };
    messages.push(json!({
        "role": "assistant",
        "content": Value::Null,
        "reasoning_content": reasoning
    }));
}

fn responses_reasoning_text(item: &Value) -> Option<String> {
    if let Some(text) = item.get("reasoning_content").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(text) = item.get("content").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(summary) = item.get("summary").and_then(Value::as_array) {
        let text = summary
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .or_else(|| part.get("content"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn responses_content_to_chat_content(content: &Value) -> Value {
    if content.is_null() || content.is_string() {
        return content.clone();
    }

    let Some(parts) = content.as_array() else {
        return content.clone();
    };
    let mut text = Vec::new();
    let mut rich_parts = Vec::new();
    let mut has_non_text = false;

    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "input_text" | "output_text" | "text" => {
                if let Some(value) = part.get("text").and_then(Value::as_str) {
                    if !value.is_empty() {
                        text.push(value.to_string());
                        rich_parts.push(json!({ "type": "text", "text": value }));
                    }
                }
            }
            "refusal" => {
                if let Some(value) = part.get("refusal").and_then(Value::as_str) {
                    if !value.is_empty() {
                        text.push(value.to_string());
                        rich_parts.push(json!({ "type": "text", "text": value }));
                    }
                }
            }
            "input_image" => {
                has_non_text = true;
                if let Some(image_url) = part.get("image_url") {
                    let image_url = if image_url.is_object() {
                        image_url.clone()
                    } else {
                        json!({ "url": image_url.as_str().unwrap_or_default() })
                    };
                    rich_parts.push(json!({ "type": "image_url", "image_url": image_url }));
                }
            }
            _ => {}
        }
    }

    if has_non_text {
        Value::Array(rich_parts)
    } else {
        Value::String(text.join("\n"))
    }
}

fn responses_content_to_anthropic_content(content: &Value) -> Vec<Value> {
    if content.is_null() {
        return Vec::new();
    }
    if let Some(text) = content.as_str() {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![json!({ "type": "text", "text": text })];
    }

    let Some(parts) = content.as_array() else {
        let text = response_text(content);
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({ "type": "text", "text": text })]
        };
    };

    let mut out = Vec::new();
    for part in parts {
        match part.get("type").and_then(Value::as_str).unwrap_or("") {
            "input_text" | "output_text" | "text" => {
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push(json!({ "type": "text", "text": text }));
                }
            }
            "refusal" => {
                if let Some(text) = part.get("refusal").and_then(Value::as_str)
                    && !text.is_empty()
                {
                    out.push(json!({ "type": "text", "text": text }));
                }
            }
            _ => {}
        }
    }
    out
}

fn responses_tool_to_chat_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    if tool.get("function").is_some() {
        return Some(tool.clone());
    }
    Some(json!({
        "type": "function",
        "function": {
            "name": tool.get("name").and_then(Value::as_str).unwrap_or(""),
            "description": tool.get("description").cloned().unwrap_or(Value::Null),
            "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({}))
        }
    }))
}

fn responses_tool_to_anthropic_tool(tool: &Value) -> Option<Value> {
    if tool.get("type").and_then(Value::as_str) != Some("function") {
        return None;
    }
    let function = tool.get("function").unwrap_or(tool);
    Some(json!({
        "name": function.get("name").and_then(Value::as_str).unwrap_or(""),
        "description": function.get("description").cloned().unwrap_or(Value::Null),
        "input_schema": function.get("parameters").cloned().unwrap_or_else(|| json!({}))
    }))
}

fn responses_tool_choice_to_chat(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            json!({
                "type": "function",
                "function": {
                    "name": object.get("name").and_then(Value::as_str).unwrap_or("")
                }
            })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("required") => {
            json!("required")
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("auto") => {
            json!("auto")
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("none") => {
            json!("none")
        }
        other => other.clone(),
    }
}

fn responses_tool_choice_to_anthropic(tool_choice: &Value) -> Value {
    match tool_choice {
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("function") => {
            json!({
                "type": "tool",
                "name": object.get("name").and_then(Value::as_str).unwrap_or("")
            })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("required") => {
            json!({ "type": "any" })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("auto") => {
            json!({ "type": "auto" })
        }
        Value::Object(object) if object.get("type").and_then(Value::as_str) == Some("none") => {
            json!({ "type": "none" })
        }
        Value::String(text) => match text.as_str() {
            "required" => json!({ "type": "any" }),
            "auto" => json!({ "type": "auto" }),
            "none" => json!({ "type": "none" }),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn chat_reasoning_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let reasoning = chat_reasoning_text(message)?;
    if reasoning.is_empty() {
        return None;
    }
    Some(json!({
        "id": format!("rs_{response_id}"),
        "type": "reasoning",
        "summary": [{ "type": "summary_text", "text": reasoning }]
    }))
}

fn chat_reasoning_text(message: &Value) -> Option<String> {
    for key in ["reasoning_content", "reasoning"] {
        if let Some(text) = message.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    if let Some(reasoning) = message.get("reasoning") {
        for key in ["content", "text", "summary"] {
            if let Some(text) = reasoning.get(key).and_then(Value::as_str) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }

    if let Some(content) = message.get("content").and_then(Value::as_str) {
        if let Some((reasoning, _answer)) = split_leading_think_block(content) {
            if !reasoning.is_empty() {
                return Some(reasoning);
            }
        }
    }

    None
}

fn chat_message_to_response_output_item(message: &Value, response_id: &str) -> Option<Value> {
    let mut content = Vec::new();
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        let text = split_leading_think_block(text)
            .map(|(_reasoning, answer)| answer)
            .unwrap_or_else(|| text.to_string());
        if !text.is_empty() {
            content.push(json!({ "type": "output_text", "text": text, "annotations": [] }));
        }
    } else if let Some(parts) = message.get("content").and_then(Value::as_array) {
        for part in parts {
            match part.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" | "output_text" => {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            content.push(
                                json!({ "type": "output_text", "text": text, "annotations": [] }),
                            );
                        }
                    }
                }
                "refusal" => {
                    if let Some(refusal) = part.get("refusal").and_then(Value::as_str) {
                        if !refusal.is_empty() {
                            content.push(json!({ "type": "refusal", "refusal": refusal }));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        if !refusal.is_empty() {
            content.push(json!({ "type": "refusal", "refusal": refusal }));
        }
    }

    if content.is_empty() {
        return None;
    }

    Some(json!({
        "id": format!("{response_id}_msg"),
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": content
    }))
}

fn chat_tool_calls_to_response_output_items(message: &Value) -> Vec<Value> {
    let mut output = Vec::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, tool_call) in tool_calls.iter().enumerate() {
            output.push(chat_tool_call_to_response_item(tool_call, index));
        }
    }
    output
}

fn chat_tool_call_to_response_item(tool_call: &Value, index: usize) -> Value {
    let call_id = tool_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("call_{index}"));
    let function = tool_call.get("function").unwrap_or(&Value::Null);
    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = json_string(function.get("arguments").unwrap_or(&json!({})));
    json!({
        "id": format!("fc_{call_id}"),
        "type": "function_call",
        "status": "completed",
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}

fn split_leading_think_block(text: &str) -> Option<(String, String)> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    if !after_ws.starts_with(THINK_OPEN_TAG) {
        return None;
    }
    let body_start = leading_ws_len + THINK_OPEN_TAG.len();
    let close_relative = text[body_start..].find(THINK_CLOSE_TAG)?;
    let close_start = body_start + close_relative;
    let answer_start = close_start + THINK_CLOSE_TAG.len();
    Some((
        text[body_start..close_start].trim().to_string(),
        strip_think_answer_separator(&text[answer_start..]).to_string(),
    ))
}

fn strip_leading_think_open_tag(text: &str) -> Option<String> {
    let leading_ws_len = text.len() - text.trim_start().len();
    let after_ws = &text[leading_ws_len..];
    after_ws
        .strip_prefix(THINK_OPEN_TAG)
        .map(|value| value.trim().to_string())
}

fn strip_think_answer_separator(text: &str) -> &str {
    text.trim_start_matches(['\r', '\n', '\t', ' '])
}

fn supports_max_completion_tokens(model: &str) -> bool {
    model.starts_with("gpt-5") || model.contains("reasoner")
}

fn supports_reasoning_effort(model: &str) -> bool {
    supports_max_completion_tokens(model)
}

fn default_responses_usage() -> Value {
    json!({ "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 })
}

fn chat_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return default_responses_usage();
    };
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": usage
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens + output_tokens)
    })
}

fn anthropic_usage_to_responses_usage(usage: Option<&Value>) -> Value {
    let Some(usage) = usage.filter(|value| value.is_object() && !value.is_null()) else {
        return default_responses_usage();
    };
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": input_tokens + output_tokens
    })
}

fn response_status(finish_reason: Option<&str>) -> &'static str {
    match finish_reason {
        Some("length") => "incomplete",
        _ => "completed",
    }
}

fn anthropic_stop_reason_to_response_status(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("max_tokens") => "incomplete",
        _ => "completed",
    }
}

fn response_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(response_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .map(response_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn json_string(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        text.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
    }
}

fn parse_json_or_string(value: &Value) -> Value {
    if let Some(text) = value.as_str() {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text))
    } else {
        value.clone()
    }
}

fn parse_iso8601_timestamp(value: &str) -> Option<u64> {
    let text = value.trim();
    let date_time = text.strip_suffix('Z').unwrap_or(text);
    let (date, time) = date_time.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let time = time.split('.').next().unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    unix_timestamp_utc(year, month, day, hour, minute, second)
}

fn unix_timestamp_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<u64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let month_i = month as i64;
    let day_i = day as i64;
    let year_i = year as i64;
    let adjusted_year = year_i - ((14 - month_i) / 12);
    let adjusted_month = month_i + 12 * ((14 - month_i) / 12) - 3;
    let julian_day =
        day_i + ((153 * adjusted_month + 2) / 5) + 365 * adjusted_year + adjusted_year / 4
            - adjusted_year / 100
            + adjusted_year / 400
            - 719_469;
    if julian_day < 0 {
        return None;
    }
    Some(
        (julian_day as u64) * 86_400 + (hour as u64) * 3_600 + (minute as u64) * 60 + second as u64,
    )
}

fn http_status_line(status: u16) -> String {
    match status {
        200 => "200 OK".to_string(),
        204 => "204 No Content".to_string(),
        400 => "400 Bad Request".to_string(),
        401 => "401 Unauthorized".to_string(),
        403 => "403 Forbidden".to_string(),
        404 => "404 Not Found".to_string(),
        429 => "429 Too Many Requests".to_string(),
        500 => "500 Internal Server Error".to_string(),
        502 => "502 Bad Gateway".to_string(),
        503 => "503 Service Unavailable".to_string(),
        _ => format!("{status} Upstream"),
    }
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body).await?;
    Ok(())
}

async fn write_http_stream_headers(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    content_type: &str,
) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_and_anthropic_use_local_proxy_route_mode() {
        assert_eq!(
            route_mode_for_protocol(UpstreamProtocol::ChatCompletions),
            RouteMode::LocalProxy
        );
        assert_eq!(
            route_mode_for_protocol(UpstreamProtocol::AnthropicMessages),
            RouteMode::LocalProxy
        );
        assert_eq!(
            route_mode_for_protocol(UpstreamProtocol::Responses),
            RouteMode::Direct
        );
    }

    #[test]
    fn proxy_base_url_uses_helper_for_proxy_protocols() {
        assert_eq!(
            proxy_base_url_for_protocol(
                "https://api.example.test/v1",
                UpstreamProtocol::ChatCompletions,
                58888
            ),
            "http://127.0.0.1:58888/v1"
        );
        assert_eq!(
            proxy_base_url_for_protocol(
                "https://api.example.test/v1",
                UpstreamProtocol::Responses,
                58888
            ),
            "https://api.example.test/v1"
        );
    }

    #[test]
    fn proxy_path_matchers_cover_v1_routes() {
        assert!(is_responses_proxy_path("/v1/responses"));
        assert!(is_responses_proxy_path("/responses/compact"));
        assert!(is_models_proxy_path("/v1/models?limit=10"));
        assert!(!is_models_proxy_path("/v1/responses"));
    }

    #[test]
    fn responses_request_converts_to_chat_completions() {
        let converted = responses_to_chat_completions(json!({
            "model": "gpt-5-mini",
            "instructions": "You are helpful.",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "hello" }
                    ]
                }
            ],
            "max_output_tokens": 512,
            "temperature": 0.2,
            "stream": true
        }))
        .unwrap();

        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][1]["content"], "hello");
        assert_eq!(converted["stream"], true);
    }

    #[test]
    fn chat_completion_response_converts_to_responses_response() {
        let converted = chat_completion_to_response(json!({
            "id": "chatcmpl_123",
            "created": 1710000000,
            "model": "gpt-5-mini",
            "choices": [
                {
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "hi there"
                    }
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }))
        .unwrap();

        assert_eq!(converted["object"], "response");
        assert_eq!(converted["output"][0]["type"], "message");
        assert_eq!(converted["usage"]["input_tokens"], 10);
    }

    #[test]
    fn chat_sse_converts_to_responses_sse_events() {
        let converted = chat_sse_to_responses_sse(
            r#"data: {"id":"chatcmpl_1","created":1710000000,"model":"gpt-5-mini","choices":[{"delta":{"content":"hel"},"finish_reason":null}]}

data: {"id":"chatcmpl_1","created":1710000000,"model":"gpt-5-mini","choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}

data: [DONE]

"#,
        );

        assert!(converted.contains("event: response.created"));
        assert!(converted.contains("event: response.output_text.delta"));
        assert!(converted.contains("data: [DONE]"));
    }

    #[test]
    fn responses_request_converts_to_anthropic_messages() {
        let converted = responses_to_anthropic_messages(json!({
            "model": "claude-sonnet-4-20250514",
            "instructions": "Be careful.",
            "input": [
                {
                    "role": "user",
                    "content": [
                        { "type": "input_text", "text": "hello" }
                    ]
                }
            ],
            "tools": [{
                "type": "function",
                "name": "lookup_weather",
                "description": "check weather",
                "parameters": { "type": "object" }
            }],
            "tool_choice": { "type": "auto" },
            "max_output_tokens": 1024,
            "stream": true
        }))
        .unwrap();

        assert_eq!(converted["system"], "Be careful.");
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(converted["messages"][0]["content"][0]["text"], "hello");
        assert_eq!(converted["tools"][0]["name"], "lookup_weather");
        assert_eq!(converted["tool_choice"]["type"], "auto");
        assert_eq!(converted["max_tokens"], 1024);
        assert_eq!(converted["stream"], true);
    }

    #[test]
    fn anthropic_message_response_converts_to_responses_response() {
        let converted = anthropic_message_to_response(json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "created_at": "2026-05-22T12:00:00Z",
            "stop_reason": "end_turn",
            "content": [
                { "type": "thinking", "thinking": "first reason" },
                { "type": "text", "text": "hi there" },
                { "type": "tool_use", "id": "toolu_1", "name": "lookup_weather", "input": { "city": "Shanghai" } }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        }))
        .unwrap();

        assert_eq!(converted["object"], "response");
        assert_eq!(converted["output"][0]["type"], "reasoning");
        assert_eq!(converted["output"][1]["type"], "message");
        assert_eq!(converted["output"][2]["type"], "function_call");
        assert_eq!(converted["usage"]["input_tokens"], 10);
        assert_eq!(converted["usage"]["output_tokens"], 5);
    }

    #[test]
    fn anthropic_sse_converts_to_responses_sse_events() {
        let converted = anthropic_sse_to_responses_sse(
            r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","model":"claude-sonnet-4-20250514","role":"assistant","created_at":"2026-05-22T12:00:00Z","usage":{"input_tokens":3,"output_tokens":0}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"reason"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"hello"}}

event: content_block_stop
data: {"type":"content_block_stop","index":1}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}

event: message_stop
data: {"type":"message_stop"}

"#,
        );

        assert!(converted.contains("event: response.created"));
        assert!(converted.contains("event: response.reasoning_summary_text.delta"));
        assert!(converted.contains("event: response.output_text.delta"));
        assert!(converted.contains("data: [DONE]"));
    }
}
