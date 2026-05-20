# Single App Auto Launch Design

## Status

Superseded by the current single-manager launch workflow. The packaging and
product direction from this document still applies: CodexPilot remains one
visible app and `codex-pilot-launcher` remains an internal sidecar. The
auto-launch-on-open preference described below is not part of the current
implemented design because the manager is being simplified around explicit,
user-triggered launch and save actions.

Do not implement the preference switch from this document unless the product
direction is reopened in a new spec.

## Goal

CodexPilot should remain one visible product. Users should not see a second
CodexPilot launcher app or have to choose between two similar entries. The
daily path should be: open CodexPilot, let it start and inject Codex
automatically, and only show management UI when the user needs configuration or
when startup fails.

## Non-Goals

- Do not add a second `.app`, desktop shortcut, or Start menu product entry.
- Do not rename the existing product.
- Do not replace the existing manual "launch Codex" workflow.
- Do not hide failures silently.

## User Experience

The current implemented experience keeps the manager as the explicit entry
point for launch and configuration. Users launch, reinject, restart, and save
preferences from the manager UI. Startup failures remain visible in the launch
or diagnostics area.

## Architecture

Keep the existing `codex-pilot-launcher` binary as an internal sidecar. It
continues to own provider sync, Codex process startup, helper startup, and page
injection.

The Tauri manager owns the product entry point and launch preferences. On app
startup, it loads preferences and exposes the current launch snapshot without
invoking Codex automatically.

The frontend should not duplicate launch logic. It should call a backend command
for explicit launch/reinject/restart actions, or receive a startup snapshot that
reflects current readiness.

## Startup Flow

1. Manager starts normally.
2. Backend loads launch preferences for app path and ports.
3. No automatic launch is attempted on open.
4. Manual launch handles these cases:
   - helper already running: mark as running and do not spawn another launcher.
   - debug port reachable: reinject.
   - unrelated Codex already running: surface the current "restart required"
     state instead of killing it automatically.
   - no Codex running: spawn the sidecar launcher.
5. On failure, the manager stays visible and shows the error.

## Error Handling

Launch must be conservative. It must not close or restart an existing Codex
process without explicit user confirmation.

Errors should be written to the existing diagnostic log. The launch view should
show the latest failure message in the same style as manual launch failures.

If the app path is missing or invalid, auto launch should not loop. It should
show the manager and let the user fix the path.

## Packaging

Packaging remains single-product:

- macOS DMG contains only `CodexPilot.app`.
- Windows installer keeps one product entry for CodexPilot.
- `codex-pilot-launcher` remains bundled as an internal sidecar only.

No user-facing launcher app, shortcut, or second product name is added.

## Testing

- Unit-test launch preference serialization.
- Verify opening the manager does not launch Codex automatically.
- Verify the existing manual launch button still works.
- Verify failure states keep the manager visible and write diagnostics.
- Run existing Rust tests and renderer injection tests.
