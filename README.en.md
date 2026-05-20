# CodexPilot

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/hl9565/CodexPilot?label=release)](https://github.com/hl9565/CodexPilot/releases)
[![Release assets](https://github.com/hl9565/CodexPilot/actions/workflows/release-assets.yml/badge.svg)](https://github.com/hl9565/CodexPilot/actions/workflows/release-assets.yml)
[![Tauri](https://img.shields.io/badge/Tauri-2.x-24C8DB)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-workspace-b7410e)](Cargo.toml)

[简体中文](README.md) | [English](README.en.md)

CodexPilot is an external enhancement console for Codex App. It launches Codex locally, attaches to the running renderer through Chromium DevTools Protocol, and adds session export, session maintenance, provider relay, and diagnostics tools.

CodexPilot does not modify the Codex App installation directory.

> CodexPilot is unofficial and is not affiliated with OpenAI or Codex App.

![CodexPilot manager overview](docs/images/readme-manager-overview.png)

## What It Does

- Launch Codex from a desktop manager and inject the CodexPilot action menu.
- Export the current Codex conversation to Markdown.
- Delete sessions, briefly undo deletion, and manage deleted records from the manager recycle bin.
- Export, delete, and batch-delete archived sessions.
- Keep the official ChatGPT login state while routing model requests to a custom compatible API.
- Manage multiple relay profiles and sync local session metadata after provider changes.
- Collect diagnostics for launch, injection, page connection, route, and provider configuration issues.

## Usage

The screenshots below are manager preview images. Example data is for demonstration only, and the layout matches the real desktop manager.

The current manager UI is primarily Chinese. The English README explains the workflow for readers who do not read Chinese, but the product screenshots still show the current Chinese interface.

### 1. Open The Manager

Open the CodexPilot manager after installation. The overview page shows Codex launch status, the active model channel, recycle bin status, and diagnostics summary.

### 2. Launch Or Re-Inject Codex

Open the Launch page and check the Codex app path, debug port, and helper port. The manager will show the correct action for the current state: launch, re-inject, or restart and inject.

![CodexPilot launch page](docs/images/readme-launch.png)

If Codex was started by another method, the manager asks before restarting it so unsaved input is not closed unexpectedly.

### 3. Configure The Model Channel

Open Model Channel and choose Official Channel or Hybrid Relay. Hybrid Relay keeps the official Codex/ChatGPT login state while routing model requests to your custom compatible API.

![CodexPilot provider page](docs/images/readme-provider.png)

Hybrid Relay setup:

1. Log in with ChatGPT in the original Codex App first.
2. Create or select a relay profile in CodexPilot.
3. Fill in Base URL and API Key.
4. Save the profile and choose Hybrid Relay.
5. Launch or re-inject Codex from CodexPilot.

### 4. Maintain Local Sessions

CodexPilot adds Markdown export, delete, and undo actions inside Codex. Deleted sessions appear in the manager Recycle Bin, where you can restore recoverable records or permanently clean them.

![CodexPilot recycle bin](docs/images/readme-recycle-bin.png)

Delete and restore operations read or write the local Codex session database. CodexPilot keeps recoverable backups where possible, but you should still confirm that sessions are no longer needed before batch cleanup.

### 5. Check Diagnostics

If launch, injection, or provider configuration fails, open Diagnostics, generate a snapshot, then copy or export the logs for troubleshooting.

![CodexPilot diagnostics](docs/images/readme-diagnostics.png)

## Who It Is For

CodexPilot is for people who already use Codex App and want extra local control:

- archive important conversations as searchable Markdown files;
- maintain regular and archived sessions more comfortably;
- use a custom compatible API while keeping the official Codex/ChatGPT login state;
- troubleshoot Codex launch, page injection, or provider configuration problems.

If you only need the standard Codex App experience, you can keep using the original app directly.

## Installation

Current GitHub Releases automatically publish the Windows installer. macOS packages are built by the maintainer in a local release environment, so whether a release includes DMG assets depends on that specific version.

### Direct Install

Download the package for your platform from [GitHub Releases](https://github.com/hl9565/CodexPilot/releases):

- Windows: `CodexPilot-*-windows-x64-setup.exe`
- macOS Apple Silicon: `CodexPilot-*-macos-arm64.dmg`, when provided for that release

The macOS packaging script keeps an `x86_64-apple-darwin` target for Intel Macs, but Intel builds are not currently published as verified release assets. If you use an Intel Mac, build and verify it from source.

On macOS, open the DMG and drag `CodexPilot.app` into Applications. Current macOS packages are not signed with an Apple Developer ID and are not notarized. If macOS reports that the app is damaged or cannot verify the developer, read the note inside the DMG and use `已损坏修复.command` only if you understand why it is needed.

On Windows, run the installer; it creates desktop and Start menu shortcuts.

After installation, open the CodexPilot manager and launch Codex from there.

### Run From Source

Running from source requires Rust, Node.js, and npm:

```bash
cd apps/codex-pilot-manager
npm install
npm run dev
```

Source mode is useful for local development and temporary usage. You do not need to package a DMG first.

## Features

### Launch And Injection

CodexPilot starts Codex through a local launcher and connects to the renderer through Chromium DevTools Protocol. After injection succeeds, a CodexPilot action menu appears inside Codex.

If Codex is already running through another path, the manager will suggest re-injection or restart based on the current state. Restarting asks for confirmation first.

### Session Export And Maintenance

CodexPilot can add extra actions to regular and archived session rows:

- export Markdown;
- delete sessions;
- briefly undo deletion;
- view, restore, or permanently clean deleted backups;
- batch-delete archived sessions.

Delete and restore operations read and write the local Codex session database. CodexPilot keeps recoverable backups where possible, but you should still review session contents before batch cleanup.

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

### Provider Sync

After provider changes, old sessions may be hidden or grouped incorrectly because their `model_provider` metadata differs. CodexPilot automatically syncs local session metadata after saving Hybrid Relay and before launching Codex.

Sync scope:

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/**/rollout-*.jsonl`
- `~/.codex/state_5.sqlite`
- `~/.codex/.codex-global-state.json`

Backup location:

```text
~/.codex/backups_state/provider-sync/
```

### Official Channel

When you choose Official Channel and save, CodexPilot will:

- remove the `CodexPilot` provider configuration;
- remove root-level `OPENAI_API_KEY`;
- switch `model_provider` back to `chatgpt`;
- keep a configuration backup before writing.

### Diagnostics

The manager shows checks for launch, injection, relay, and page connection state. It can also export diagnostic logs for troubleshooting or issue reports.

## Local Data And Security

CodexPilot reads or writes these local paths:

- `~/.codex/config.toml`: relay configuration.
- `~/.codex/auth.json`: only used to detect official login state.
- `~/.codex/sessions/`: session metadata and export sources.
- `~/.codex/archived_sessions/`: archived session metadata and export sources.
- `~/.codex/state_5.sqlite`: session index, delete, restore, and provider sync.
- `~/.codex/backups_state/provider-sync/`: Provider Sync backups.
- CodexPilot's own app state directory: launch preferences, relay profiles, and diagnostic logs.

Relay profiles are saved locally. API keys are hidden in the status panel but are still stored in local configuration files. Use CodexPilot only on trusted devices, and avoid uploading local config, logs, screenshots, or backup directories to public repositories.

When using a custom compatible API, verify the provider's privacy, billing, and data handling policies yourself.

## Development

```bash
cargo test
node scripts/test-renderer-inject.mjs

cd apps/codex-pilot-manager
npm install
npm run check
```

### Manager UI Preview

When changing the manager UI, you can preview it in the browser without launching the full Tauri desktop shell:

```bash
cd apps/codex-pilot-manager
npm run preview:ui
```

Then open `http://127.0.0.1:1420`. Preview mode uses local mock data for launch, model channel, recycle bin, and diagnostics pages. The outer window uses the real app's default `1120x760` size to make layout checks closer to the desktop app.

## Pre-Release Checks

- `cargo fmt`
- `cargo test`
- `cargo check`
- `node scripts/test-renderer-inject.mjs`
- `npm run check`
- Verify Hybrid Relay save, launch, session visibility, and new-session requests with a real Codex login state.
- Check that logs, screenshots, and test data do not contain real secrets.

## Packaging And Release

The public repository keeps the Windows automatic release workflow. After a GitHub Release is published, Actions builds and uploads `CodexPilot-*-windows-x64-setup.exe` on a Windows runner.

macOS packages are built by the maintainer in a local release environment and uploaded to GitHub Releases when needed.

If you want to package it yourself, follow the official Tauri, Rust, and Node.js build process for `codex-pilot-manager`, and make sure `codex-pilot-launcher` is placed as a sidecar under `apps/codex-pilot-manager/src-tauri/binaries/`.

## Compatibility

CodexPilot depends on Codex App's page structure and local data format. If Codex App changes its renderer structure, session database, or configuration format, CodexPilot may need updates to its page connection scripts or sync logic.

## License

MIT
