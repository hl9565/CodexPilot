# Session ZIP Export And Import Design

## Context

CodexPilot already supports:

- single-session Markdown export;
- single-session HTML export;
- recycle-bin recovery for deleted sessions;
- provider sync for historical ownership normalization.

What it does not yet offer is a whole-library backup and restore flow for the
user's current local Codex session store.

The requested capability is:

- export all current session storage into one zip file;
- import that zip later to restore data;
- when importing, force the user to explicitly choose `合并` or `覆盖`.

Because this touches both session files and `state_5.sqlite`, the design must
avoid pretending that file replacement and database replacement are lightweight
operations.

## Goals

- Add one-click whole-library ZIP export from the Manager `对话维护` page.
- Include the current local session file roots and `state_5.sqlite` in the
  backup scope.
- Add ZIP import to the Manager with an explicit user choice between:
  - `合并导入`
  - `覆盖恢复`
- Keep import confirmation inside the page UI rather than relying on
  `window.confirm`.
- Protect users from silent destructive restore behavior.

## Non-Goals

- Do not merge SQLite contents record-by-record in v1.
- Do not add background auto-backup scheduling.
- Do not sync these ZIP backups across devices.
- Do not treat ZIP import as a provider-sync replacement.
- Do not expose raw filesystem internals unless they help the user understand
  what will be restored.

## Backup Scope

The exported ZIP includes:

- `sessions/` from `~/.codex/sessions/`
- `archived_sessions/` from `~/.codex/archived_sessions/`
- `state_5.sqlite` from `~/.codex/state_5.sqlite`
- `manifest.json`

If any of the three storage targets do not exist, export still succeeds, but
`manifest.json` must record that absence explicitly.

## ZIP Structure

Suggested ZIP layout:

```text
codex-sessions-backup-2026-05-24-153000.zip
├── manifest.json
├── sessions/...
├── archived_sessions/...
└── state_5.sqlite
```

`manifest.json` should include at least:

- `version`
- `exportedAt`
- `product`
- `includes.sessions`
- `includes.archivedSessions`
- `includes.stateSqlite`
- `counts.sessionFiles`
- `counts.archivedSessionFiles`

Example:

```json
{
  "version": 1,
  "product": "CodexPilot",
  "exportedAt": "2026-05-24T15:30:00+08:00",
  "includes": {
    "sessions": true,
    "archivedSessions": true,
    "stateSqlite": true
  },
  "counts": {
    "sessionFiles": 128,
    "archivedSessionFiles": 14
  }
}
```

## Manager UI

Add a `备份与恢复` section inside the `对话维护` page.

### Visible Actions

- `导出 ZIP`
- `导入 ZIP`

### Export Flow

Clicking `导出 ZIP` should:

1. create the ZIP in one backend action;
2. return the saved filename/path;
3. show success messaging in-page.

### Import Flow

Clicking `导入 ZIP` should:

1. let the user choose a ZIP file;
2. parse and validate it;
3. show an in-page import summary block;
4. require the user to explicitly choose one mode:
   - `合并导入`
   - `覆盖恢复`
5. only then show the final execute action.

The page should not default silently to either mode.

## Import Modes

### 合并导入

Purpose:

- restore session files from the ZIP without replacing the current local SQLite
  database.

Rules:

- restore `sessions/` files from the ZIP into the local `~/.codex/sessions/`
  tree;
- restore `archived_sessions/` files from the ZIP into the local
  `~/.codex/archived_sessions/` tree;
- same-path files from the ZIP may overwrite same-path local files;
- local files not mentioned by the ZIP remain untouched;
- if the ZIP contains `state_5.sqlite`, the UI must clearly say that merge mode
  does **not** import the SQLite file.

Reasoning:

- replacing `state_5.sqlite` is not a true merge;
- v1 should not claim record-level DB merging without dedicated storage-layer
  reconciliation logic.

### 覆盖恢复

Purpose:

- restore the ZIP as a full local state replacement for the covered targets.

Rules:

Before restore:

1. create a safety backup ZIP from the current local state;
2. expose the backup path in the result message.

Then restore:

- replace `sessions/` with the ZIP's `sessions/` when present;
- replace `archived_sessions/` with the ZIP's `archived_sessions/` when present;
- replace `state_5.sqlite` with the ZIP's SQLite file when present.

If the ZIP omits one of these targets, the summary block must show that clearly
before execution so the user understands which parts will actually be replaced.

## Confirmation Design

The import confirmation must be page-visible and stateful.

It should show:

- selected ZIP filename
- export time from `manifest.json`
- whether the ZIP contains:
  - `sessions/`
  - `archived_sessions/`
  - `state_5.sqlite`
- merge/overwrite consequence text
- final action button

For `覆盖恢复`, add stronger warning copy:

- it will replace current local session directories;
- it may replace current `state_5.sqlite`;
- a local safety backup will be created first.

This must not rely on `window.confirm` as the only destructive checkpoint.

## Validation And Safety

### Import Validation

Import must fail early when:

- the selected file is not a ZIP;
- `manifest.json` is missing;
- `manifest.json` cannot be parsed;
- the ZIP contains none of:
  - `sessions/`
  - `archived_sessions/`
  - `state_5.sqlite`

### Path Safety

ZIP extraction must reject unsafe paths, including:

- `../`
- absolute paths
- symlink escape behavior if the library exposes such entries directly

The restore path must stay inside the intended Codex home targets.

## Failure Handling

### Export Failure

- no local data should be modified;
- return a concise error message.

### Merge Import Failure

- partial file restore is acceptable in v1;
- report success/failure counts;
- do not falsely report full success;
- keep the existing SQLite untouched.

### Overwrite Restore Failure

- if restore fails after the safety backup is created, return failure;
- include the safety backup path in the message so the user can manually revert;
- do not claim the current local store is fully restored unless the whole
  overwrite flow completed successfully.

## Architecture

### Backend Layers

Add a session backup/restore service responsible for:

- collecting export inputs;
- generating `manifest.json`;
- writing ZIP archives;
- inspecting import ZIP summary;
- executing merge restore;
- executing overwrite restore with a pre-restore local backup ZIP.

### Manager / Tauri

Add commands for:

- export ZIP
- inspect ZIP import summary
- execute import in selected mode

### Frontend

Add a `备份与恢复` panel inside `对话维护` with:

- export button
- import button
- import summary / mode-selection state
- final execute action

## Existing Design Alignment

This design complements, rather than replaces:

- single-session export features;
- recycle-bin restore;
- provider sync.

It is intentionally broader in scope than HTML/Markdown export because it
targets full local recovery rather than presentation or sharing.

## Testing

- backend unit test: export succeeds when one or more targets are missing and
  `manifest.json` marks them correctly
- backend unit test: import summary parsing reads included targets and counts
- backend unit test: unsafe ZIP paths are rejected
- backend unit test: merge import restores files but does not replace SQLite
- backend unit test: overwrite restore creates a safety backup before replacing
  targets
- frontend build check: `npm run vite:build`
- Manager flow verification: import requires explicit `合并` or `覆盖` choice

## Acceptance Criteria

- `对话维护` page shows a `备份与恢复` section.
- Users can export a whole-library ZIP that includes session directories,
  `state_5.sqlite`, and `manifest.json`.
- Import requires explicit user selection between `合并导入` and `覆盖恢复`.
- Merge mode restores session files but does not replace `state_5.sqlite`.
- Overwrite mode creates a local safety backup before replacing local state.
- Import validation rejects malformed or unsafe ZIP contents.
