# Provider Channel Simplification Design

## Context

The current Provider page exposes too much implementation detail. It presents three run modes, a separate login status action row, a profile list, and a separate editor panel. This makes the page feel like a technical control surface rather than a simple way to choose how Codex sends model requests.

The most confusing parts are:

- `纯 API` broadens the product into a generic API launcher and weakens the main CodexPilot use case.
- `混合 API` is short but unclear; it should be framed as a channel choice.
- Login detection, clearing, syncing, applying, saving, and profile editing all appear as peer actions.
- The profile list and editor duplicate the same object: a selected profile appears on the left, but editing happens on the right.

The desired page should make the primary decision obvious: use the official Codex channel, or use Codex/ChatGPT login with a custom API relay.

## Decision

Rename the page concept from `运行模式` to `模型通道`.

The Provider page has two channel choices:

- `官方通道`
- `混合中转`

Remove `纯 API` from the UI. Existing `api` profile values may still be tolerated by backend compatibility code, but the UI must not create new pure API profiles or expose pure API as a selectable mode.

## Channel Behavior

### 官方通道

When `官方通道` is selected:

- Hide profile cards, Base URL, and API Key fields.
- Show a small confirmation panel for the official channel.
- The panel explains that Codex/ChatGPT official login is used and CodexPilot custom model provider configuration is not written.
- The only main action is `保存`.

This is not a disabled version of the custom API form. The form is hidden because custom API configuration is not part of the official channel.

### 混合中转

When `混合中转` is selected:

- Show editable profile cards.
- Keep the currently applied profile visually selected.
- Editing happens directly inside the selected profile card.
- The only main action is `保存`.
- `保存` saves the profile and applies it as the active mixed relay channel.

The visible profile card fields are:

- `配置名称`
- `Base URL`
- `API Key`

The card keeps a light delete action in the top-right corner. The add-profile affordance is a compact `+` control below the cards.

## Status Area

Keep a compact status strip at the top of the page:

- Official login state and account label.
- Current channel.
- Current profile.
- Configured state if space allows.

Do not show the internal provider key such as `CodexPilot` in the status strip. It is implementation detail and does not help users choose a channel.

The overview page's provider/channel summary must use the same terminology and information hierarchy as the Provider page summary. If the Provider page hides or renames a status field, update the overview summary at the same time.

Remove the large action row from the status area:

- No `检测登录` button.
- No `切回官方登录模式` button.
- No primary `同步历史会话` button.
- No `写入混合 API` button.

Refresh already exists at the page header. Session sync is a maintenance action and should not compete with the main channel flow.

## Provider Sync Maintenance

Provider Sync must be explicit. It must not silently rewrite all historical
sessions during normal channel save or Codex launch, because users may keep
history from multiple providers and expect those provider names to remain
meaningful.

Expose Provider Sync as a secondary maintenance tool, either in the diagnostics
area or in a visually separated maintenance section. The default target provider
is `CodexPilot`, but users can choose another target before running sync.

Target provider choices:

- `CodexPilot` as the default.
- The current `model_provider` from `~/.codex/config.toml`.
- Provider names discovered from rollout metadata and SQLite thread rows.
- A manual custom provider name.

Before running sync, the UI should show an inspection summary:

- Target provider.
- Rollout files that would be rewritten.
- SQLite rows that would be updated.
- Current rollout and SQLite provider distribution.

Running sync requires an explicit user action. The backend continues to create
Provider Sync backups under `~/.codex/backups_state/provider-sync/`.
Rollout metadata backups must stay compact and must not copy conversation
content: `session-meta-backup.json` stores each changed rollout path and its
original first `session_meta` line only. It must not store the rest of the
rollout file.

## Copy

Use these visible labels:

- Page/nav label: `模型通道`
- Official channel title: `官方通道`
- Mixed relay title: `混合中转`
- Mixed relay description: `保留 Codex/ChatGPT 登录，把模型请求转到当前 API 配置。`
- Official channel description: `使用 Codex/ChatGPT 官方登录，不写入自定义模型供应商。`
- Save button: `保存`

Do not use these labels in the new UI:

- `纯 API`
- `混合 API`
- `运行模式`
- `写入混合 API`

Backend messages may continue to use older terminology until the backend compatibility layer is cleaned up, but the main UI should use the new labels.

## Data Flow

The page continues to use existing Tauri commands:

- `provider_snapshot`
- `save_provider_profile`
- `activate_provider_profile`
- `delete_provider_profile`
- `apply_provider`
- `clear_provider`

Provider Sync maintenance uses separate commands:

- `provider_sync_snapshot`
- `sync_provider_sessions`

For `混合中转`, `保存` should:

1. Validate name, Base URL, and API Key.
2. Save the selected profile as `hybridApi`.
3. Use the saved profile id returned by `save_provider_profile`.
4. Apply the saved profile as `hybridApi`, including newly created profiles.
5. Refresh the snapshot.

Deleting a mixed relay profile should ask for confirmation using the visible
profile name before calling `delete_provider_profile`. The backend still keeps
the at-least-one-profile rule.

For `官方通道`, `保存` should:

1. Clear the CodexPilot provider configuration.
2. Refresh the snapshot.

If saving a mixed relay profile succeeds but applying fails, show the backend error and keep the edited values visible.

## Non-Goals

- Do not change the underlying `CodexPilot` provider name.
- Do not remove backend compatibility for existing `api` profiles in this change.
- Do not add a new provider storage format.
- Do not move session sync into this page as a primary action.

## Acceptance Criteria

- The Provider nav/page label reads `模型通道`.
- The UI only presents `官方通道` and `混合中转`.
- `纯 API` is not visible anywhere on the Provider page.
- Official channel selection hides custom API profile fields.
- Mixed relay selection shows editable profile cards.
- The selected profile can be edited in place.
- The main action area has only one primary `保存` button.
- Status action buttons are removed.
- Overview and Provider page summaries do not show `当前供应商` or the internal `CodexPilot` provider key.
- The account label is visible in full and can wrap; it is not truncated with ellipsis.
- Existing provider snapshot/apply/save/delete commands are reused.
- Saving provider channel settings does not automatically rewrite historical session providers.
- Codex launch does not automatically rewrite historical session providers.
- Provider Sync can be run manually with `CodexPilot` selected by default.
- Provider Sync target can be changed before running sync.
- Provider Sync shows the expected rewrite/update counts before the user runs it.
