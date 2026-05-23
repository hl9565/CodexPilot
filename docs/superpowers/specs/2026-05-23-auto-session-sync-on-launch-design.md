# Auto Session Sync On Launch Design

## Context

CodexPilot already has an explicit `对话同步` / Provider Sync maintenance flow.
The current product rule is conservative:

- Provider Sync is manual by default.
- It should not run automatically during normal channel save.
- It should not run automatically during normal launch unless a design
  deliberately introduces an exception.

That rule exists for a good reason: sync rewrites historical session ownership,
so users should not lose meaningful provider distinctions by accident.

At the same time, users who always want CodexPilot-managed launch plus
post-launch session cleanup currently have to do the same maintenance action
manually after startup. This makes sense as a user preference, but not as a
mandatory repeated chore.

The product change here is therefore not "make Provider Sync automatic by
default." It is narrower:

- add an opt-in launch preference
- when enabled, run one controlled sync check after successful startup/injection
- only perform the actual sync when drift exists

## Goals

- Add an opt-in launch preference for automatic session sync after successful
  startup/injection.
- Keep manual Provider Sync as the default behavior.
- Avoid unnecessary sync work by checking whether drift exists first.
- Separate startup failure from sync failure in both UI messaging and
  diagnostics.

## Non-Goals

- Do not make Provider Sync automatic by default.
- Do not trigger sync merely because the Manager window opened.
- Do not add a repeated background sync scheduler.
- Do not add a per-launch confirmation dialog for the auto-sync path.
- Do not expose a separate auto-sync target selector in v1.

## Product Decision

Add a new launch preference:

- `启动后自动同步会话`

When enabled, CodexPilot should automatically check whether session ownership
drift exists after a successful launch/injection path. If no drift exists, it
stops. If drift exists, it runs one automatic sync using the current effective
provider target.

This is an explicit opt-in exception to the general "Provider Sync must be
manual" rule.

## Trigger Scope

Auto session sync should run only after a successful CodexPilot-controlled
startup/injection path, including:

- `启动 Codex`
- `重新注入`
- `重启并注入`

It should not run for:

- opening the Manager window by itself
- saving launch preferences
- refreshing snapshots

## Preference Placement

Place the new toggle inside `启动偏好`, alongside the existing launch
preferences such as app path, ports, and `打开 CodexPilot 时自动启动或注入
Codex`.

Suggested visible label:

- `启动后自动同步会话`

Suggested help text:

- `启动或注入成功后，按当前生效通道目标自动检查并同步历史会话归属。`

The copy should make it clear that this is historical-session maintenance, not
a lightweight UI refresh.

## Sync Target Rule

The automatic sync target should follow the current effective route target, not
a hard-coded `CodexPilot` default.

Reasoning:

- the user expectation is "after I start CodexPilot with my current channel,
  align history to that current route"
- hard-coding `CodexPilot` would be wrong for cases where the current effective
  ownership target differs

Implementation should therefore derive the target from the current effective
provider state / sync inspection path instead of guessing in the frontend.

## Execution Flow

After a successful launch/injection:

1. Check whether `autoSyncSessionsOnLaunch` is enabled.
2. If off, stop immediately.
3. If on, determine the current effective sync target.
4. Run `provider_sync_snapshot` / equivalent inspection for that target.
5. If both of these are zero, stop:
   - `rolloutRewriteNeeded`
   - `sqliteProviderRowsNeedingSync`
6. If drift exists, run `sync_provider_sessions` once with the same target.
7. Report the result in the UI message flow and diagnostic log.

This means the preference is best understood as:

- "auto check and sync on launch when needed"

not:

- "blindly run sync every time"

## User Messaging

Startup and sync outcomes must remain distinguishable.

### Success Cases

- launch success + no drift
  - `已启动 CodexPilot，无需同步会话。`
- launch success + sync performed
  - `已启动 CodexPilot，并完成会话同步。`

### Failure Case

If launch succeeded but sync failed:

- `已启动 CodexPilot，但自动同步会话失败。`

This must not collapse into a generic launch failure, because the user needs to
know CodexPilot itself is running even though maintenance failed.

## Error Handling

- Auto-sync failure must not roll back a successful launch/injection.
- Auto-sync failure should write a diagnostic event with:
  - launch action kind
  - resolved target provider
  - inspection summary if available
  - sync failure message
- If inspection itself fails, do not proceed to sync; report it as an
  auto-sync failure.
- If no drift exists, write a lightweight diagnostic event so "nothing happened"
  can still be explained later.

## Existing Design Alignment

This design intentionally creates a narrow exception to the older Provider Sync
rule.

Still true:

- manual Provider Sync remains available
- Provider Sync is not an always-on background behavior
- normal channel save still does not auto-sync history

What changes:

- launch can now trigger a sync automatically, but only when:
  - the user explicitly opted in
  - launch/injection actually succeeded
  - drift inspection found work to do

The older specs should therefore be read as:

- "Provider Sync is manual by default"

not as:

- "Provider Sync can never run automatically under any opted-in launch flow"

## Data Model

Extend `LaunchPreferences` with a new boolean:

- `autoSyncSessionsOnLaunch`

Default:

- `false`

Compatibility:

- existing saved preferences without this field should deserialize as `false`

## Testing

- Unit-test launch preference serialization/deserialization with
  `autoSyncSessionsOnLaunch`.
- Verify launch success with the toggle off does not attempt sync.
- Verify launch success with the toggle on checks drift.
- Verify launch success with the toggle on and zero drift does not run sync.
- Verify launch success with the toggle on and detected drift runs sync once.
- Verify sync failure does not mark launch itself as failed.
- Verify diagnostics capture both "no drift" and "sync failed" cases.

## Acceptance Criteria

- `启动偏好` shows a new toggle: `启动后自动同步会话`.
- The preference defaults to off.
- Opening the Manager alone does not trigger sync.
- Successful launch/reinject/restart-inject checks the preference.
- When the preference is on, CodexPilot checks drift after successful startup.
- Auto-sync runs only when inspection shows work is needed.
- Auto-sync uses the current effective sync target, not a hard-coded provider.
- Launch success and sync failure are reported separately.
- Manual Provider Sync remains available and unchanged.

