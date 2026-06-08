# Thread Fast Toggle Implementation Plan

## Goal

Implement the first version of conversation-level `Fast` mode in CodexPilot
according to
`docs/superpowers/specs/2026-05-22-thread-fast-toggle-design.md`.

This version must:

- add a lightning toggle to the `Pilot` floating pill;
- support `Fast` / `Standard` for the current conversation only;
- support new conversations from the first request via draft state;
- persist existing-thread overrides by thread id;
- avoid global mode, conversation-list markers, and composer badge placement.

## Current Codebase Anchors

- Main renderer injection entry:
  `assets/inject/renderer-inject.js`
- Pilot floating pill creation:
  `createMenu()`
- Existing thread/session change tracking:
  `installScrollRestore()`, `handleThreadMaybeChanged()`
- Existing settings gate for injected enhancements:
  `loadEnhancementSettings()` and `enhancementSettings.enabled`
- Existing renderer diagnostics:
  `reportRendererEvent(...)`

## Delivery Strategy

Implement this feature in five ordered slices:

1. renderer state model and session tracking
2. Pilot floating pill UI split and interaction
3. request-envelope interception and service-tier override
4. draft binding and diagnostics hardening
5. tests and cleanup

The order matters because UI without request interception would create false
confidence, and request interception without draft binding would fail the
“first new conversation request” requirement.

## Step 1: Add local service-tier state primitives

### Files

- `assets/inject/renderer-inject.js`

### Work

Add a self-contained service-tier state module inside the renderer script that
supports:

- thread override storage keyed by current Codex session/thread id;
- single draft storage for the next new conversation;
- explicit distinction between:
  - no override
  - explicit standard override
  - explicit fast override
- short TTL handling for draft state;
- helper functions for:
  - reading stored state
  - writing thread overrides
  - writing/clearing draft state
  - resolving the current effective UI state

### Notes

- Reuse existing session identification patterns already used by scroll restore
  and timeline rather than inventing a second unrelated thread detector.
- Keep state local to the renderer; do not add backend persistence.
- Use diagnostics for malformed storage payloads instead of hard failure.

### Exit Criteria

- Renderer code can answer:
  - what mode should current existing conversation show?
  - what mode should current new-conversation draft show?
  - does current state come from thread override, draft, or no override?

## Step 2: Split the Pilot pill into two sibling controls

### Files

- `assets/inject/renderer-inject.js`

### Work

Refactor the current single-button Pilot pill into:

- a lightning toggle control on the left
- a Pilot panel toggle control on the right

Implement:

- gray lightning for standard
- yellow lightning for fast
- tooltip copy for existing-thread and draft states
- click behavior that toggles only the lightning state
- panel open/close behavior that remains attached only to the Pilot button area

### Notes

- Do not nest one button inside another.
- Keep the pill compact and consistent with the current visual style.
- Ensure keyboard focus can reach both controls separately.
- Add toast/message updates for:
  - existing thread -> Fast
  - existing thread -> Standard
  - new draft -> Fast
  - new draft -> Standard if needed

### Exit Criteria

- Clicking lightning never opens the panel.
- Clicking the Pilot area still opens/closes the panel.
- Current visible lightning state reflects thread/draft state immediately.

## Step 3: Add supported request-envelope interception

### Files

- `assets/inject/renderer-inject.js`

### Work

Add a request-override layer that explicitly handles the supported envelope
matrix from the design:

- `send-cli-request-for-host`
- `mcp-request`
- `worker-request`
- `thread-prewarm-start`
- `start-conversation`
- `prewarm-thread-start-for-host`
- `start-thread-for-host`
- `start-turn-for-host`

Within those shapes, apply service-tier overrides for:

- `thread/start`
- `thread/resume`
- `turn/start`

Rules:

- fast thread or fast draft -> set `serviceTier = "priority"`
- explicit standard override -> remove/clear priority override
- no override -> leave the request unchanged

### Notes

- Keep the interception logic centralized so supported shapes are easy to audit.
- Emit diagnostics when a known envelope is seen but cannot be safely rewritten.
- Do not mutate unrelated request payloads.

### Exit Criteria

- Existing Fast threads force priority on supported start/resume/turn requests.
- New draft Fast forces priority on the first new-conversation path.
- Unsupported shapes remain unchanged and log diagnostics.

## Step 4: Bind draft state to resolved thread ids

### Files

- `assets/inject/renderer-inject.js`

### Work

Implement the draft binding lifecycle:

- after a draft-backed start request, mark draft as pending bind
- observe current route/session changes using the same signals already available
  in the renderer layer
- when a stable thread id appears, convert draft into a thread override
- clear the draft once binding succeeds
- retry during TTL if no thread id is immediately available
- expire and diagnose if binding never completes

### Notes

- Reuse `handleThreadMaybeChanged()` integration points where practical.
- Do not consume draft merely because the user views another existing thread.
- Protect against overwriting a newer explicit thread override accidentally.

### Exit Criteria

- New conversation first request can run in Fast.
- Once the new conversation stabilizes, later turns still resolve from thread
  override rather than draft residue.

## Step 5: Add tests and guardrails

### Files

- `assets/inject/renderer-inject.js`
- existing relevant renderer/core tests, likely including:
  - `crates/codex-pilot-core/...` script coverage if present
  - front-end or injected-script tests if available in current repo patterns

### Work

Add or extend tests for:

- lightning control renders and toggles without opening Pilot panel
- existing thread fast/standard transitions
- draft fast before first message
- envelope-by-envelope request rewrite coverage
- unsupported envelope pass-through behavior
- draft binding after thread creation
- draft expiry
- no-override vs explicit-standard semantics

### Notes

- If the repo lacks direct renderer unit coverage, add the narrowest possible
  test coverage around script content or extracted helper logic, following
  current project conventions.
- Prefer targeted tests over one giant integration test.

### Exit Criteria

- The supported envelope matrix is asserted in tests.
- The key draft-binding path is asserted in tests.
- Panel toggle and lightning toggle do not conflict.

## Suggested Implementation Order

1. state helpers
2. pill UI split
3. request interception
4. draft binding
5. tests
6. polish diagnostics/tooltips/messages

## Risks To Watch

- Codex upstream request shape drift could break one envelope while leaving
  others working, creating partial Fast behavior.
- Session/thread id detection may differ between existing conversations and new
  draft flows.
- Pilot pill DOM refactor could accidentally regress current panel toggling.
- Draft binding races may surface only when fast navigation or retries occur.

## Definition of Done

The work is done when:

- current conversation lightning toggle works inside the Pilot pill;
- new conversation first request can start in Fast;
- bound thread continues to behave as Fast afterward;
- no global mode is introduced;
- diagnostics exist for unsupported or failed rewrites;
- tests cover the documented request-envelope matrix and draft lifecycle.
