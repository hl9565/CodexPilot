# CodexPilot Feature Guide

This guide explains what each CodexPilot page does, which local data it reads or writes, and which operations should be previewed before writing changes. The README stays focused on the homepage and quick entry points; full feature details live here.

## Contents

- [Launch And Injection](#launch-and-injection)
- [Session Export And Maintenance](#session-export-and-maintenance)
- [Timeline](#timeline)
- [Dialog Sync](#dialog-sync)
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

## Dialog Sync

After ccSwitch or another tool changes `model_provider` in `~/.codex/config.toml`, old sessions may be hidden or grouped incorrectly because their `model_provider` metadata differs. CodexPilot no longer rewrites historical session ownership automatically. To make historical sessions visible or grouped under the current config Provider, open Dialog Maintenance, use Dialog Sync, preview the impact, then sync. For special migrations, you can still enter a manual target Provider.

If you are only switching Providers temporarily, or if the previewed impact is unclear, do not sync yet. Use this only when historical sessions are missing or grouped incorrectly and you are sure you want those records assigned to the target Provider.

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

The manager shows checks for launch, injection, dialog sync, and page connection state. It can also export diagnostic logs for troubleshooting or issue reports.

![CodexPilot diagnostics page](images/readme-diagnostics.png)

Diagnostics are mainly used to check:

- whether the Codex app path is usable;
- whether the debug port and helper port are healthy;
- whether the page has connected and injection has completed;
- whether local data required by dialog maintenance and dialog sync is accessible.

## Local Data And Security

CodexPilot reads or writes these local paths:

- `~/.codex/config.toml`: read-only current `model_provider` source for dialog sync defaults.
- `~/.codex/sessions/`: session metadata and export sources.
- `~/.codex/archived_sessions/`: archived session metadata and export sources.
- `~/.codex/state_5.sqlite`: session index, delete, restore, and dialog sync.
- `~/.codex/backups_state/provider-sync/`: dialog sync backups.
- CodexPilot's own app state directory: launch preferences, page enhancement settings, and diagnostic logs.

Use CodexPilot only on trusted devices, and avoid uploading local config, logs, screenshots, or backup directories to public repositories. Model Provider switching and API key management should be handled by ccSwitch or your own Codex configuration flow.

CodexPilot also uses a local loopback debug port and a local helper port. Chromium DevTools Protocol connections can execute page scripts, so use CodexPilot only in a trusted local environment.

Additional data locations:

- `~/.codex/.codex-pilot-undo/`: undo/recycle-bin backups created after deleting sessions.

## Compatibility

CodexPilot depends on Codex App's page structure and local data format. If Codex App changes its renderer structure, session database, or configuration format, CodexPilot may need updates to its page connection scripts or sync logic.
