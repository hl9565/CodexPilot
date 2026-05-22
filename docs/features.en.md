# CodexPilot Feature Guide

This guide explains what each CodexPilot page does, which local data it reads or writes, and which operations should be previewed before writing changes. The README stays focused on the homepage and quick entry points; full feature details live here.

## Contents

- [Launch And Injection](#launch-and-injection)
- [Session Export And Maintenance](#session-export-and-maintenance)
- [Timeline](#timeline)
- [Model Channel](#model-channel)
- [Provider Ownership Sync](#provider-ownership-sync)
- [Diagnostics](#diagnostics)
- [Local Data And Security](#local-data-and-security)
- [Compatibility](#compatibility)

## Launch And Injection

CodexPilot starts Codex through a local launcher and connects to the renderer through Chromium DevTools Protocol. After injection succeeds, a CodexPilot action menu appears inside Codex.

If Codex is already running through another path, the manager will suggest re-injection or restart based on the current state. Restarting asks for confirmation first so unsaved input is not closed unexpectedly.

![CodexPilot launch page](images/readme-launch.png)

## Session Export And Maintenance

CodexPilot can add extra actions to regular and archived sessions:

- export Markdown;
- delete sessions;
- briefly undo deletion;
- view, restore, or permanently clean deleted backups;
- batch-delete archived sessions.

Delete and restore operations read and write the local Codex session database. CodexPilot keeps recoverable backups where possible, but you should still review session contents before batch cleanup.

![CodexPilot dialog maintenance page](images/readme-dialog-maintenance.png)

## Timeline

In the current Codex conversation, when CodexPilot detects at least two user prompts, it shows a lightweight Timeline near the right edge of the page. Each marker represents one user prompt. Hover to preview the prompt text, or click a marker to scroll that prompt into the center of the viewport.

Timeline only reads the current page content. It does not write session files, state databases, or configuration files. If the current page is not a conversation, the session cannot be detected, or there are not enough user prompts, Timeline hides itself automatically.

## Model Channel

### Hybrid Relay

Hybrid Relay is for users who have already completed the official Codex/ChatGPT login and want model requests to go through a custom compatible API. The point is not just to “switch API endpoints”: CodexPilot keeps the official login path available, so you can keep using mobile ChatGPT to control or continue desktop Codex while desktop Codex sends model requests through your configured Provider.

![CodexPilot model channel page](images/readme-provider.png)

Base URL and API Key come from a third-party or self-hosted API endpoint. In the first protocol-expansion release, CodexPilot supports upstreams that already speak Responses API and upstreams that only speak Chat Completions. The official login state keeps Codex/ChatGPT login compatibility and the cross-device control experience; once Hybrid Relay is enabled, model requests are sent to the custom Provider you configured, and that Provider's privacy, billing, and data handling policies apply.

Setup steps:

1. Log in with ChatGPT in the original Codex App.
2. Open CodexPilot Manager and go to Model Channel.
3. Create or select a relay profile.
4. Fill in Base URL, API Key, and the upstream protocol, then save the profile.
5. Choose Hybrid Relay and save.
6. Launch or re-inject Codex from CodexPilot.

You normally do not need to edit the config file manually. CodexPilot writes to `~/.codex/config.toml` in a shape similar to:

```toml
model_provider = "CodexPilot"

[model_providers.CodexPilot]
name = "CodexPilot"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://example.com/v1"
experimental_bearer_token = "sk-..."
```

If CodexPilot does not detect a ChatGPT login state in `~/.codex/auth.json`, it refuses to save Hybrid Relay configuration.

### No-Account Channel

No-Account Channel is for users who want to use a configured API provider without relying on Codex/ChatGPT login state. It uses the same profile model as Hybrid Relay, including upstream protocol selection, but it does not require official login and does not preserve the official cross-device control path.

In the current release, the same upstream protocol choices apply:

- Responses API
- Chat Completions

When the selected upstream only supports Chat Completions, CodexPilot writes a local Responses endpoint into Codex config and performs protocol conversion through its local helper service.

### Official Channel

When you choose Official Channel and save, CodexPilot will:

- remove the `CodexPilot` provider configuration;
- remove root-level `OPENAI_API_KEY`;
- remove root-level `model_provider` so Codex uses its default official channel;
- keep a configuration backup before writing.

If you manually added a root-level `OPENAI_API_KEY` in `~/.codex/config.toml`, switching back to Official Channel will remove it too. CodexPilot keeps a backup before writing.

## Provider Ownership Sync

After provider changes, old sessions may be hidden or grouped incorrectly because their `model_provider` metadata differs. CodexPilot no longer rewrites historical session ownership automatically. To make historical sessions visible or grouped under a selected provider, open Dialog Maintenance, use Dialog Ownership Sync, preview the impact, then manually sync to the selected provider.

If you are only switching model channels temporarily, or if the previewed impact is unclear, do not sync yet. Use this only when historical sessions are missing or grouped incorrectly and you are sure you want those records assigned to the target Provider.

Sync scope:

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/**/rollout-*.jsonl`
- `~/.codex/state_5.sqlite`
- `~/.codex/.codex-global-state.json`

Backup location:

```text
~/.codex/backups_state/provider-sync/
```

## Diagnostics

The manager shows checks for launch, injection, relay, and page connection state. It can also export diagnostic logs for troubleshooting or issue reports.

![CodexPilot diagnostics page](images/readme-diagnostics.png)

Diagnostics are mainly used to check:

- whether the Codex app path is usable;
- whether the debug port and helper port are healthy;
- whether the page has connected and injection has completed;
- whether the current model channel configuration is complete;
- whether local data required by dialog maintenance and Provider sync is accessible.

## Local Data And Security

CodexPilot reads or writes these local paths:

- `~/.codex/config.toml`: relay configuration.
- `~/.codex/auth.json`: only used to detect official login state.
- `~/.codex/sessions/`: session metadata and export sources.
- `~/.codex/archived_sessions/`: archived session metadata and export sources.
- `~/.codex/state_5.sqlite`: session index, delete, restore, and provider sync.
- `~/.codex/backups_state/provider-sync/`: Provider Sync backups.
- CodexPilot's own app state directory: launch preferences, relay profiles, and diagnostic logs.

Relay profiles are saved locally. API keys are hidden in status panels, but they are still stored in local configuration files. Use CodexPilot only on trusted devices, and avoid uploading local config, logs, screenshots, or backup directories to public repositories.

When using a custom API, verify the provider's privacy, billing, and data handling policies yourself.

CodexPilot also uses a local loopback debug port and a local helper port. Chromium DevTools Protocol connections can execute page scripts, so use CodexPilot only in a trusted local environment.

Additional data locations:

- `~/.codex/config.toml.codex-pilot-backup-*.bak`: config backups kept before model-channel writes; they may contain old API keys.
- `~/.codex/.codex-pilot-undo/`: undo/recycle-bin backups created after deleting sessions.
- `provider-profiles.json` under the CodexPilot app state directory: channel profiles containing Base URL, API Key, and upstream protocol metadata. On macOS/Linux this is usually under `~/.config/CodexPilot/`; on Windows it is usually under `%APPDATA%\CodexPilot\`.

## Compatibility

CodexPilot depends on Codex App's page structure and local data format. If Codex App changes its renderer structure, session database, or configuration format, CodexPilot may need updates to its page connection scripts or sync logic.
