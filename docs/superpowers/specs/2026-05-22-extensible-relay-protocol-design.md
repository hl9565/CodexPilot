# Extensible Relay Protocol Design

## Context

CodexPilot currently assumes that every custom upstream already speaks Codex's
`Responses API` shape. The Provider page stores a Base URL and key, and
`relay_config.rs` always writes `wire_api = "responses"` plus the user-supplied
upstream `base_url` into `~/.codex/config.toml`.

That is enough for OpenAI-compatible Responses relays, but it blocks common
providers that expose `Chat Completions` only, such as DeepSeek-compatible
upstreams. It also leaves no clean place to add future protocol families like
`Anthropic Messages`.

CodexPlusPlus already proves a useful first step: when the upstream only
supports `Chat Completions`, Codex still talks to a local `Responses` endpoint,
and a local helper converts requests and responses on the fly. CodexPilot needs
that same capability, but its implementation should be more intentionally
extensible instead of baking protocol knowledge into one chat-specific branch.

CodexPilot also previously simplified the Provider page down to `官方通道` and
`混合中转`, hiding `纯 API`. That simplification made sense when the product
goal was narrowly "keep ChatGPT login and send traffic through a compatible
relay". The new goal is broader: support multiple upstream protocols under one
stable Codex-facing wire protocol. Under that goal, a no-login API channel is a
real product need, not accidental complexity, but it should be framed as
`无账号` rather than exposing low-level backend terminology.

## Goals

- Add an extensible protocol layer to CodexPilot so Codex can keep speaking
  `Responses API` while CodexPilot routes to different upstream protocols.
- Implement the first non-native adapter:
  `Responses API <-> Chat Completions`.
- Preserve the current direct path for upstreams that already support
  `Responses API`.
- Keep the architecture open for future `Anthropic Messages` support without
  redesigning profile storage, helper routing, or manager UI.
- Add a user-facing way to choose the upstream protocol for API-backed channels.
- Reintroduce a no-login channel in the Manager as `无账号`.

## Non-Goals

- Do not change Codex's external wire protocol away from `Responses API`.
- Do not add multimodal parity for every protocol family. First-version support
  focuses on text, streaming SSE, tool calls, reasoning, and `/v1/models`.
- Do not create a second helper service or a second local proxy port.
- Do not silently rewrite historical session provider ownership during channel
  changes or launch.

## Product Decision

The channel page should now expose three user-facing channel choices:

- `官方通道`
- `混合中转`
- `无账号`

Rationale:

- `官方通道` remains the path that uses Codex/ChatGPT's normal login and does
  not write a custom provider.
- `混合中转` keeps the existing CodexPilot value proposition: preserve official
  login while redirecting model traffic to a selected upstream.
- `无账号` is needed because protocol support is no longer limited to "official
  login plus compatible relay". Some upstreams are key-only and should be first
  class, but the UI should describe the user consequence rather than the backend
  term `pureApi`.

The backend may continue to tolerate legacy `api` values for compatibility, but
the main UI should use `无账号` and eventually normalize new saves to a single
current representation.

## Existing Design Alignment

This design intentionally updates the assumptions in
`2026-05-20-provider-channel-simplification-design.md`.

What remains consistent:

- Provider Sync is still explicit maintenance and must not run automatically on
  normal channel save or launch.
- The page remains a `模型通道` page rather than reverting to a generic advanced
  settings surface.
- The internal provider key `CodexPilot` remains hidden from the primary UI.

What changes:

- The page may now expose three channels instead of two.
- API-backed channels now need an upstream protocol selector.
- Backend compatibility for `api` is no longer only a hidden tolerance layer;
  the no-login flow becomes user-visible again under the `无账号` label.

The implementation must therefore update the older design doc or clearly
supersede its channel-count restriction.

## Architecture

CodexPilot should keep one Codex-facing protocol:

- `CodexWireProtocol = responses`

It should add a separate upstream protocol abstraction:

- `UpstreamProtocol = responses | chatCompletions | anthropicMessages`

It should also model how Codex reaches that upstream:

- `RouteMode = direct | localProxy`

The key boundary is:

- profile storage owns upstream intent;
- relay config owns what Codex should connect to right now;
- helper owns HTTP path matching and proxy transport;
- adapters own protocol semantics.

### Components

1. `relay profile settings`
   Stores the chosen channel, upstream protocol, Base URL, key, and related
   metadata.

2. `relay config writer`
   Writes `~/.codex/config.toml` and related auth state so Codex always sees a
   `Responses API` endpoint, whether direct or proxied.

3. `protocol proxy router`
   Runs inside the existing helper service and intercepts `/v1/responses`,
   `/v1/responses/compact`, and `/v1/models`.

4. `upstream protocol adapters`
   Convert Codex-facing Responses requests into upstream-specific requests and
   convert upstream responses back into Responses-compatible payloads or SSE.

5. `manager channel UI`
   Lets users choose the channel and upstream protocol without exposing raw
   helper plumbing.

## Data Model

CodexPilot-owned profile data should gain explicit upstream protocol metadata.

Suggested shape for the persisted profile model:

```json
{
  "id": "team-relay",
  "name": "Team Relay",
  "mode": "hybridApi",
  "baseUrl": "https://relay.example.com/v1",
  "bearerToken": "sk-...",
  "upstreamProtocol": "responses"
}
```

Definitions:

- `mode`
  - `official`
  - `hybridApi`
  - `api`
- `upstreamProtocol`
  - `responses`
  - `chatCompletions`
  - `anthropicMessages`

Rules:

- `official` profiles may keep `upstreamProtocol`, but it is ignored unless the
  profile also mixes API traffic.
- `hybridApi` and `api` profiles require `baseUrl`, `bearerToken`, and
  `upstreamProtocol`.
- `anthropicMessages` is allowed in backend storage now so future work has a
  stable schema, but first-version UI does not expose it.

### Compatibility

- Existing saved profiles without `upstreamProtocol` default to `responses`.
- Existing `api` mode values remain readable.
- Newly created or updated no-login profiles may continue to serialize as `api`
  for compatibility, but the UI label is `无账号`.
- Existing config parsing should keep working for direct Responses providers.

### Uniqueness

Profile names should be unique across the whole list.

Reasoning:

- In CodexPilot, users primarily identify profiles by visible name.
- Protocol is a profile attribute, not part of its identity.
- Allowing same-name profiles with different protocols would make `混合中转`
  and `无账号` cards easy to confuse.

Save validation must therefore reject duplicate names after trimming.

## Relay Config Writing

Codex-facing config remains `wire_api = "responses"`.

### Responses Upstream

When the active profile uses `upstreamProtocol = responses`:

- `RouteMode = direct`
- `model_providers.CodexPilot.base_url` is the user-supplied upstream Base URL
- `wire_api = "responses"`
- auth handling remains consistent with the selected channel:
  - `混合中转`: `requires_openai_auth = true` plus
    `experimental_bearer_token`
  - `无账号`: `OPENAI_API_KEY` plus `env_key = "OPENAI_API_KEY"`

### Chat Completions Upstream

When the active profile uses `upstreamProtocol = chatCompletions`:

- `RouteMode = localProxy`
- `model_providers.CodexPilot.base_url` points to the local helper endpoint,
  e.g. `http://127.0.0.1:<helper_port>/v1`
- Codex still sees `wire_api = "responses"`
- the real upstream Base URL and key remain in CodexPilot-owned settings
- the helper converts:
  - Responses request -> Chat Completions request
  - Chat Completions response -> Responses response
  - Chat Completions SSE -> Responses SSE

### Anthropic Messages Upstream

When a stored profile has `upstreamProtocol = anthropicMessages`:

- `RouteMode = localProxy`
- `model_providers.CodexPilot.base_url` points to the local helper endpoint
- Codex still sees `wire_api = "responses"`
- the real upstream Base URL and key remain in CodexPilot-owned settings
- the helper converts:
  - Responses request -> Anthropic Messages request
  - Anthropic Messages response -> Responses response
  - Anthropic Messages SSE -> Responses SSE

## Helper Routing

The existing helper service should be extended, not duplicated.

The helper keeps its current management endpoints such as `/backend/status`,
diagnostics, reinjection, and provider actions. It should additionally handle:

- `POST /responses`
- `POST /v1/responses`
- `POST /responses/compact`
- `POST /v1/responses/compact`
- `GET /models`
- `GET /v1/models`
- `OPTIONS` variants where needed for consistency

Management routes and protocol-proxy routes should not live in one large
unstructured match arm. The implementation should separate:

- helper socket / raw HTTP parsing
- proxy path detection
- protocol dispatch
- adapter implementations

Suggested file boundaries:

- `helper.rs`
- `proxy_routes.rs`
- `protocol_proxy/mod.rs`
- `protocol_proxy/adapters/responses.rs`
- `protocol_proxy/adapters/chat_completions.rs`
- `protocol_proxy/adapters/anthropic_messages.rs`

Exact filenames may vary, but the semantic split should remain.

## Adapter Contract

CodexPilot should define a small internal adapter contract for any upstream
protocol implementation.

The adapter should answer these concerns:

- how to build the upstream `/models` request
- how to convert a Responses request into an upstream request body
- how to forward or map headers needed for the upstream
- how to convert a successful non-stream upstream response back into a
  Responses-compatible JSON payload
- how to convert a successful upstream stream into Responses SSE events
- how to report unsupported payload shapes or malformed upstream responses

The first two implementations:

1. `ResponsesDirectAdapter`
   - thin pass-through
   - used when upstream already supports Responses

2. `ChatCompletionsAdapter`
   - performs the real protocol translation
   - reuses the proven conversion approach from CodexPlusPlus, but inside the
     generic adapter architecture rather than as the architecture itself

3. `AnthropicMessagesAdapter`
   - performs real protocol translation for the first-version text/tool scope
   - keeps the same Codex-facing Responses contract as the other adapters

## Chat Completions Support Scope

First-version `Chat Completions` support must include:

- non-stream requests and responses
- streaming SSE
- tool calls / function calls
- reasoning extraction from explicit reasoning fields
- reasoning extraction from inline `<think>...</think>` content when needed
- `/v1/models`

First-version support explicitly does not promise:

- full multimodal parity for images, audio, files, or vendor-specific content
  blocks beyond the simple mappings already needed for text-based flows
- vendor-specific extensions that cannot be represented safely in Codex-facing
  Responses payloads

If a payload contains unsupported content, the adapter should either preserve
what it safely can or fail explicitly. It must not silently emit structurally
wrong Responses events.

## Manager UI

### Channel Choices

The `模型通道` page should show:

- `官方通道`
- `混合中转`
- `无账号`

Descriptions:

- `官方通道`: `使用 Codex/ChatGPT 官方登录，不写入自定义模型供应商。`
- `混合中转`: `保留 Codex/ChatGPT 登录，把 Codex 请求转换后转到当前上游 API 配置。`
- `无账号`: `不依赖 Codex/ChatGPT 登录，直接使用当前 API 配置。`

### Profile Editing

API-backed channels (`混合中转` and `无账号`) show:

- profile cards
- `配置名称`
- `Base URL`
- `API Key`
- `上游协议`

`上游协议` options in first-version UI:

- `Responses API`
- `Chat Completions`

`Anthropic Messages` is not shown yet, even though backend support exists.

### Validation

- name required
- Base URL required for API-backed channels
- API Key required for API-backed channels
- profile names unique after trimming
### Save Behavior

#### 官方通道

Saving `官方通道`:

1. clears the CodexPilot provider config
2. keeps profile drafts in CodexPilot-owned settings
3. refreshes the snapshot

#### 混合中转

Saving `混合中转`:

1. validates profile
2. saves it with `mode = hybridApi`
3. applies the selected profile
4. writes either direct Responses config or local-proxy config depending on
   `upstreamProtocol`
5. refreshes the snapshot

#### 无账号

Saving `无账号`:

1. validates profile
2. saves it with `mode = api`
3. applies the selected profile
4. writes either direct Responses config or local-proxy config depending on
   `upstreamProtocol`
5. refreshes the snapshot

## User Messaging

For `Chat Completions` profiles, the UI should show a small hint that Codex
still talks to a local Responses endpoint and CodexPilot performs protocol
conversion through the helper.

The hint should mention the local helper concept, but it should not hardcode a
specific future-unstable port number into user-facing copy unless the product
already treats that port as fixed.

## Diagnostics and Error Handling

Diagnostics should record:

- active profile id
- active profile mode
- upstream protocol
- route mode
- requested endpoint
- upstream status code
- stream vs non-stream
- conversion failure category

Diagnostics must not record:

- API keys
- auth tokens
- full request bodies
- full response bodies
- conversation content

Error behavior:

- malformed upstream response: return `502 Bad Gateway`
- upstream non-2xx: preserve status code and body when safe
- stream conversion failure: emit a Responses failure event and close cleanly

No-login and hybrid apply operations must be conservative:

- if apply fails, keep the edited profile in CodexPilot settings
- do not partially report success
- do not leave Codex config rewritten to an unusable unsupported-protocol state

## Testing

Automated coverage should include:

- profile serialization with `upstreamProtocol`
- backward compatibility defaulting missing protocol to `responses`
- duplicate profile name rejection
- relay config writer direct Responses path
- relay config writer local-proxy path for Chat Completions
- helper route detection for `/responses`, `/responses/compact`, and `/models`
- helper active-profile loading for `chatCompletions` and `anthropicMessages`
- Responses direct adapter pass-through
- Chat Completions request conversion
- Chat Completions response conversion
- Chat Completions SSE conversion
- Anthropic Messages request conversion
- Anthropic Messages response conversion
- Anthropic Messages SSE conversion
- `/v1/models` proxying
- manager save flow for `官方通道`
- manager save flow for `混合中转`
- manager save flow for `无账号`

Manual verification should check:

- switching among all three channels updates visible fields correctly
- `Chat Completions` profiles actually work when launched through CodexPilot
- `无账号` does not require ChatGPT login
- `混合中转` still requires official login
- Provider Sync remains manual and unchanged

## Acceptance Criteria

- CodexPilot stores upstream protocol as first-class profile metadata.
- CodexPilot can use a direct Responses upstream without local conversion.
- CodexPilot can use a Chat Completions upstream through the existing helper
  port with Responses-compatible behavior.
- The helper exposes proxy routes for Responses and models on the same local
  service as existing management routes.
- Manager UI exposes `官方通道`, `混合中转`, and `无账号`.
- Manager UI exposes `Responses API` and `Chat Completions` as the selectable
  upstream protocols for API-backed channels.
- Duplicate profile names are rejected.
- `Anthropic Messages` is supported in backend relay/config/helper paths while
  remaining hidden in the first-version UI.
- Existing provider sync behavior remains manual and unchanged.
- Code, tests, and user-facing docs are updated together.
