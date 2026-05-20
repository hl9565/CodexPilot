# CodexPilot Feature Guide

This document keeps the full CodexPilot feature guide. The README stays focused on the project homepage, quick start, and core entry points. New features should be documented here first, then promoted into the README only when they are important enough for the first page.

## Launch And Injection

CodexPilot starts Codex through a local launcher and connects to the renderer through Chromium DevTools Protocol. After injection succeeds, a CodexPilot action menu appears inside Codex.

If Codex is already running through another path, the manager will suggest re-injection or restart based on the current state. Restarting asks for confirmation first so unsaved input is not closed unexpectedly.

## Session Export And Maintenance

CodexPilot can add extra actions to regular and archived sessions:

- export Markdown;
- delete sessions;
- briefly undo deletion;
- view, restore, or permanently clean deleted backups;
- batch-delete archived sessions.

Delete and restore operations read and write the local Codex session database. CodexPilot keeps recoverable backups where possible, but you should still review session contents before batch cleanup.

## Model Channel

### Hybrid Relay

Hybrid Relay is for users who have already completed the official Codex/ChatGPT login and want model requests to go through a custom compatible API.

Setup steps:

1. Log in with ChatGPT in the original Codex App.
2. Open CodexPilot Manager and go to Model Channel.
3. Create or select a relay profile.
4. Fill in Base URL and API Key, then save the profile.
5. Choose Hybrid Relay and save.
6. Launch or re-inject Codex from CodexPilot.

CodexPilot writes to `~/.codex/config.toml` in a shape similar to:

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

### Official Channel

When you choose Official Channel and save, CodexPilot will:

- remove the `CodexPilot` provider configuration;
- remove root-level `OPENAI_API_KEY`;
- switch `model_provider` back to `chatgpt`;
- keep a configuration backup before writing.

## Provider Ownership Sync

After provider changes, old sessions may be hidden or grouped incorrectly because their `model_provider` metadata differs. CodexPilot no longer rewrites historical session ownership automatically. To maintain historical data, open Dialog Maintenance, use Dialog Ownership Sync, preview the impact, then manually sync to the selected provider.

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

When using a custom compatible API, verify the provider's privacy, billing, and data handling policies yourself.

## Compatibility

CodexPilot depends on Codex App's page structure and local data format. If Codex App changes its renderer structure, session database, or configuration format, CodexPilot may need updates to its page connection scripts or sync logic.
