# Single App Auto Launch Design

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

The manager keeps its current entry point and navigation. A new launch
preference controls automatic startup:

```text
启动 CodexPilot 时自动启动 Codex
```

When this preference is off, CodexPilot behaves as it does today.

When it is on, opening CodexPilot runs the same checks as the current launch
button. If startup and injection succeed, the manager should get out of the
way. The default behavior should be to hide or minimize the manager window
after successful launch, while keeping the helper and launcher lifecycle intact.

If startup fails, the manager window must remain visible and show the failure in
the launch or diagnostics area. Users should have an obvious place to inspect
the reason and retry.

## Architecture

Keep the existing `codex-pilot-launcher` binary as an internal sidecar. It
continues to own provider sync, Codex process startup, helper startup, and page
injection.

The Tauri manager owns the product entry point and the user-visible preference.
On app startup, it loads launch preferences and decides whether to invoke the
same launch path used by the existing button.

The frontend should not duplicate launch logic. It should call a backend command
or receive a startup snapshot that reflects the auto-launch attempt.

## Preference Model

Extend launch preferences with a boolean field:

```text
auto_launch_on_open
```

Default value is `false` so existing users keep the current behavior after
upgrade.

The preference should be saved beside the existing launch settings. It should be
visible in the launch view, near the existing app path and port controls.

## Startup Flow

1. Manager starts normally.
2. Backend loads launch preferences.
3. If `auto_launch_on_open` is false, no launch is attempted.
4. If true, backend/frontend triggers the existing launch command once per app
   startup.
5. Existing launch command handles these cases:
   - helper already running: mark as running and do not spawn another launcher.
   - debug port reachable: reinject.
   - unrelated Codex already running: surface the current "restart required"
     state instead of killing it automatically.
   - no Codex running: spawn the sidecar launcher.
6. On success, the manager hides or minimizes itself.
7. On failure, the manager stays visible and shows the error.

## Error Handling

Auto launch must be conservative. It must not close or restart an existing
Codex process without explicit user confirmation.

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

- Unit-test launch preference serialization with the new field and default.
- Verify auto launch does not run when the preference is false.
- Verify auto launch attempts exactly once when the preference is true.
- Verify the existing manual launch button still works.
- Verify failure states keep the manager visible and write diagnostics.
- Run existing Rust tests and renderer injection tests.

