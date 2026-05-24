# Provider Auto Relay With Official Fallback

## Context

The current `模型通道` page still asks the user to choose an explicit channel
(`官方通道` / `混合中转` / `传统中转`) before a profile can take effect.

That no longer matches the intended product model:

- using a profile should immediately switch CodexPilot to the target upstream;
- whether CodexPilot applies that profile as hybrid relay or API relay should
  be inferred from the current official login state;
- a profile may optionally prefer official direct mode when a valid official
  login exists.

This design replaces manual channel selection with profile-driven application
behavior while preserving the existing relay adapters and config-writing paths.

## Goals

- Remove manual channel selection from the Manager UI.
- Make profile activation immediately apply the selected profile.
- Default profile application to:
  - `混合中转` when official login is present;
  - `API 中转` when official login is absent.
- Add a per-profile advanced option: when official login exists, prefer
  `官方直连`.
- Persist an `official config snapshot` so official direct mode can restore the
  user's original Codex config instead of approximating it.
- Degrade safely when a profile prefers official direct mode but no official
  snapshot is available or no official login is present.

## Non-Goals

- Do not remove the existing low-level relay config writers.
- Do not redesign helper protocol routing or upstream adapters.
- Do not migrate historical provider sync behavior.
- Do not guarantee perfect restoration of files outside the captured Codex
  config snapshot.

## Product Behavior

### Page Structure

The `模型通道` page should contain:

- `当前状态`
- `配置档`

The old `选择通道` card group is removed.

### Current Status

The status panel should show:

- official login detected / not detected
- current effective route
- active profile name
- official snapshot available / missing
- account label when detectable

Effective route labels:

- `官方直连`
- `自动中转（登录态）`
- `自动中转（API）`
- `已退化为自动中转`

### Profile Editing

Each profile stores:

- name
- Base URL
- API Key
- upstream protocol
- authenticated behavior

Authenticated behavior values:

- `relay`
- `officialDirect`

User-facing copy for the advanced option:

- `登录态存在时改走官方原版`

### Apply Rules

When a profile becomes active:

1. If official login is present and the profile behavior is `officialDirect`:
   - restore the saved official config snapshot when available;
   - remove any API-only `OPENAI_API_KEY` from `~/.codex/auth.json`;
   - show route `官方直连`.
2. If official login is present and the profile behavior is `relay`:
   - apply the profile as `混合中转`.
3. If official login is absent:
   - apply the profile as API relay, regardless of authenticated behavior.

### Degraded Fallbacks

If a profile prefers `officialDirect` but the environment cannot satisfy it:

- no official snapshot:
  - keep the profile enabled;
  - degrade to automatic relay;
  - show a warning that the original official snapshot was not captured.
- no official login:
  - keep the profile enabled;
  - degrade to API relay;
  - show a warning that official login was not detected.

The degrade result is still considered a successful profile activation.

## Data Model

`provider-profiles.json` gains:

```json
{
  "activeProfileId": "team-relay",
  "officialConfigSnapshot": {
    "configToml": "...",
    "capturedAtMs": 1770000000000
  },
  "profiles": [
    {
      "id": "team-relay",
      "name": "Team Relay",
      "baseUrl": "https://relay.example.com/v1",
      "bearerToken": "sk-...",
      "upstreamProtocol": "responses",
      "authenticatedBehavior": "relay"
    }
  ]
}
```

Compatibility rules:

- missing `authenticatedBehavior` defaults to `relay`;
- missing `officialConfigSnapshot` means snapshot unavailable;
- legacy `mode` fields remain readable but no longer drive the UI flow.

## Official Snapshot Rules

### Capture

CodexPilot should capture the current `~/.codex/config.toml` as the official
snapshot only when:

- no official snapshot is already stored; and
- Codex is not currently routed through `CodexPilot`.

This avoids overwriting the snapshot with CodexPilot-managed relay config.

If `config.toml` does not exist yet, the snapshot may store an empty string.

### Restore

Restoring official direct mode should:

1. back up the current `config.toml` the same way other relay writes do;
2. write the snapshot contents back to `~/.codex/config.toml`;
3. remove API-only `OPENAI_API_KEY` from `~/.codex/auth.json`;
4. leave ChatGPT login tokens intact.

## Backend Changes

### Manager/Tauri

- `provider_snapshot` returns:
  - effective route
  - status message
  - official snapshot available
- `save_provider_profile`:
  - persists the updated profile;
  - if the saved profile is active, immediately reapplies it.
- `activate_provider_profile`:
  - changes active profile;
  - immediately applies it.

### Core Relay Config

Add a restore helper for official snapshot contents instead of reusing
`clear_relay_provider_config`.

Reason:

- `clear_relay_provider_config` only removes CodexPilot-owned keys;
- official direct mode now means restoring a captured baseline config, not just
  "remove the current relay table".

## UI Messaging

Status copy should explain why a fallback happened:

- `未找到官方原版快照，已退化为自动中转。`
- `未检测到官方登录，当前已按 API 中转应用。`

Normal success messages:

- `已按登录态应用自动中转。`
- `已恢复官方原版配置。`

## Testing

Add or update tests for:

- provider profile state round-trip with `officialConfigSnapshot` and
  `authenticatedBehavior`
- snapshot capture only when current config is not CodexPilot-managed
- official snapshot restore clears API-only auth key but preserves official
  login shape
- activation fallback when snapshot is missing
- UI mock snapshot shape for the new status fields
