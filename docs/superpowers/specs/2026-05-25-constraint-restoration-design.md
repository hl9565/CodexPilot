# Constraint Restoration Design

## Status

Proposed. This design restores missing cross-cutting constraints first, then
uses those constraints to govern later mechanical file splitting.

## Context

Observed project state on 2026-05-25:

- `crates/codex-pilot-core/src/launcher.rs` still contains direct process
  spawning and process detection via `Command::new(...)`.
- `apps/codex-pilot-manager/src-tauri/src/lib.rs` still contains direct process
  spawning, direct process detection, and a large mixed command/orchestration
  surface.
- `apps/codex-pilot-manager/src/main.tsx` refreshes status immediately on
  `focus` and `visibilitychange`.
- `crates/codex-pilot-core/src/protocol_proxy.rs` remains a large mixed module.
- There is no dedicated `windows_integration.rs` abstraction in
  `crates/codex-pilot-core/src/`.
- There is no CI guard that blocks newly introduced subprocess calls from
  bypassing Windows-specific process creation behavior.

The current failure mode is not one isolated bug. The deeper problem is that
cross-platform and cross-cutting constraints are scattered or absent, so fixes
depend on remembering every callsite. That creates recurring regressions:

- Windows subprocess behavior is applied in some spawn paths but omitted in
  others.
- High-frequency UI refresh paths still trigger synchronous process probes.
- Large files make it easier to patch one place and miss another.

This design restores the missing railings first. It does not treat large-file
splitting as the first operation; it treats splitting as work that must happen
after the railings exist.

## Goals

- Reintroduce a centralized Windows subprocess integration layer.
- Stop window-focus refresh from repeatedly triggering synchronous process
  detection.
- Add repository contracts that future changes must follow.
- Turn the critical contracts into CI-enforced checks.
- Update affected existing design documents so the written design matches the
  intended implementation direction.
- Define a safe, mechanical follow-up plan for splitting oversized files without
  rewriting behavior.

## Non-Goals

- Do not rewrite provider, relay, or protocol behavior in this round.
- Do not perform broad architectural refactoring during the stop-the-bleeding
  implementation.
- Do not claim that every missing abstraction can be mechanically enforced in
  one step.
- Do not treat `http_client.rs` restoration as a proven root cause for the
  current Windows flash or focus-stall symptoms.
- Do not split `protocol_proxy.rs` or the Tauri manager command file in this
  round's implementation.

## Scope

This design has two layers of work.

### Layer 1: Implement Now

Implement immediately:

- `crates/codex-pilot-core/src/windows_integration.rs`
- asynchronous Codex-running state checks with a short TTL cache
- frontend debounce for focus-triggered refresh
- `docs/contracts/*`
- `scripts/check-windows-hygiene.sh`
- `scripts/lint-contracts.sh`
- CI wiring for the new checks
- updates to affected existing specs

### Layer 2: Design Now, Implement Later

Design now but do not implement in this round:

- mechanical splitting of
  `apps/codex-pilot-manager/src-tauri/src/lib.rs`
- mechanical splitting of `crates/codex-pilot-core/src/protocol_proxy.rs`
- follow-up restoration priorities for `http_client.rs` and further focused
  platform/path abstractions

## Design Principles

1. Restore centralized abstractions before attempting large file moves.
2. Enforce what can be enforced mechanically; document the rest explicitly.
3. Prefer thin compatibility helpers over thick wrappers that change command
   semantics.
4. In later splitting work, move code first and reinterpret behavior later.
5. A later AI pass should fail CI when it forgets a required platform handling
   step.

## Windows Subprocess Integration

### New Module

Add `crates/codex-pilot-core/src/windows_integration.rs`.

Its job is narrow: centralize Windows-specific subprocess creation behavior
without changing the meaning of existing commands.

The module should export a small surface such as:

- `CREATE_NO_WINDOW`
- `apply_no_window(...)`
- `spawn_hidden(...)` or an equivalently thin helper

On non-Windows platforms these helpers are no-ops.

### Why Thin Helpers Instead Of A Heavy Wrapper

This repository already mixes `std::process::Command` and `tokio::process::Command`.
A heavy abstraction that tries to replace every command-building pattern would
increase migration risk and could accidentally change blocking vs async behavior.

The real invariant to enforce is:

- Windows process creation behavior must be applied before spawn/status/output
  where relevant.

That means the design should standardize the Windows adaptation step, not
pretend every subprocess call can or should be rewritten into one oversized
builder abstraction.

### Required Adoption Paths

The first implementation round must route current high-risk subprocess paths
through the new integration helpers or an explicit equivalent:

- `crates/codex-pilot-core/src/launcher.rs`
  - Codex launch spawn path
  - process-running detection path
- `apps/codex-pilot-manager/src-tauri/src/lib.rs`
  - sidecar launcher spawn path
  - process-running detection path
  - other runtime subprocess calls must either use the helper or be explicitly
    allowed by the hygiene script rule model

### Allowed Exceptions

Some subprocess calls may remain direct when the Windows no-window behavior is
not relevant or when the call is outside runtime product behavior. Those
exceptions must be deliberately modeled rather than silently ignored.

Example:

- `build.rs` `println!` for Cargo directives is not a runtime logging violation.

Equivalent narrow exceptions may exist for subprocess checks, but they must be
written into the hygiene policy rather than treated as “too annoying to check”.

## Async Process State And Refresh Control

### Problem

Current manager refresh behavior can trigger direct process-running checks from
high-frequency window lifecycle events. Today that path is synchronous and can
block on subprocess creation.

### Backend Direction

The Codex-running check exposed to the manager should become asynchronous and
should read through a short in-memory cache with a TTL of about 2 seconds.

The point of the cache is not to make state eventually consistent forever. The
point is to collapse repeated focus-triggered probes into one short-lived result
during window switching bursts.

### Scope Guard

This round changes only:

- how the running-state probe is performed;
- how often repeated probes are allowed;
- how the frontend schedules refresh on focus-like events.

This round does not redefine the meaning of launch actions such as:

- launch
- reinject
- restart
- running

### Frontend Direction

`apps/codex-pilot-manager/src/main.tsx` keeps the existing “refresh when the
window becomes relevant again” behavior, but no longer fires immediately and
repeatedly on every burst of focus and visibility events.

Add a debounce of about 500 ms around the focus/visibility-triggered refresh.

This is intentionally a rate-control change, not a UX redesign.

## Contract Documents

Add `docs/contracts/` with focused, enforceable guidance.

### `subprocess.md`

Must define:

- runtime subprocess calls must go through the subprocess integration rule
  surface;
- raw `Command::new(...)` is not automatically forbidden, but any new runtime
  subprocess path must apply the Windows integration helper or match an explicit
  allowed pattern;
- the hygiene script is the authoritative enforcement entry point.

### `paths.md`

Must define:

- new path resolution logic should prefer existing path abstractions such as
  `app_paths`;
- new platform-specific path assembly should not be scattered through unrelated
  modules.

### `ipc.md`

Must define:

- new Tauri commands default to async;
- a synchronous command in a user-triggerable or frequently triggered path
  requires explicit justification.

### `logging.md`

Must define:

- runtime diagnostics should go through `diagnostic_log`;
- new runtime `println!` calls are forbidden;
- build-script Cargo directive printing is not a violation.

### `windows.md`

Must define:

- new functionality that touches subprocesses, paths, permissions, or visible
  window behavior requires Windows verification evidence;
- CI can enforce builds and tests, but screenshot evidence remains a process
  requirement, not a fully automatable lint rule.

## Lint And Verification Scripts

### `scripts/check-windows-hygiene.sh`

This is the highest-priority guardrail.

Its job is to fail when runtime subprocess code bypasses the Windows integration
rule.

The initial version should:

- scan runtime Rust sources for direct `Command::new(...)` patterns;
- ignore or specially-handle approved non-runtime cases;
- fail when new runtime subprocess callsites do not match the expected guarded
  patterns.

This script should be opinionated enough to stop silent bypasses, but not so
clever that nobody can maintain it.

### `scripts/lint-contracts.sh`

This script should aggregate the mechanically enforceable repository contracts,
including:

- Windows subprocess hygiene
- runtime `println!` bans
- other small grep-based checks that directly support the written contracts

The script should not pretend every design rule is grep-enforceable. Only rules
with a clear, low-noise mechanical check should be added.

## CI Strategy

Add a dedicated Windows verification workflow for regular validation, rather
than relying on release-only packaging jobs.

Recommended checks:

- `cargo build --workspace`
- `cargo test --workspace`
- `scripts/lint-contracts.sh`

Why a dedicated workflow:

- the repository already builds Windows release assets, so Windows support is
  already a first-class concern;
- release-time discovery is too late for subprocess-platform regressions;
- the new contract scripts should block pull requests, not just releases.

## Existing Spec Alignment

This work affects the assumptions behind
`docs/superpowers/specs/2026-05-20-single-app-auto-launch-design.md`.

That spec should be updated so it no longer implicitly assumes that
focus-triggered refresh is cheap or that launch-state probes remain synchronous.

The aligned design should still preserve:

- the one-visible-app model;
- the current launch action semantics;
- the conservative restart policy.

It should additionally state that refresh and state-probe implementation must
avoid repeated synchronous subprocess checks on high-frequency window events.

## Later Mechanical Splitting Plan

This section is a design commitment for later work, not part of this round's
implementation.

### Tauri Manager Command File

Later split `apps/codex-pilot-manager/src-tauri/src/lib.rs` by command domain,
for example into:

- `commands/launch.rs`
- `commands/provider.rs`
- `commands/sessions.rs`
- `commands/diagnostics.rs`

Rules for that later work:

- move command functions and directly related helpers only;
- do not rewrite function bodies during the move;
- fix imports, module plumbing, and invocation wiring only;
- run `cargo check` after each moved group;
- commit after each verified group move.

### `protocol_proxy.rs`

Later split `crates/codex-pilot-core/src/protocol_proxy.rs` by responsibility,
starting with a boundary between:

- route / target decision logic
- protocol adaptation / transport logic

The exact filenames may differ, but the responsibility boundary must become
explicit before any deeper cleanup.

Rules for that later work:

- move code first;
- do not reinterpret protocol behavior while splitting;
- keep each step small enough to verify mechanically.

### Next Restoration Priorities After This Round

After the stop-the-bleeding work lands, restoration priority should be:

1. `windows_integration.rs`
2. `http_client.rs`
3. further path-focused abstractions
4. mechanical splitting of oversized files

`http_client.rs` is listed as a follow-up restoration target because HTTP client
setup is currently scattered. That is enough to justify consolidation work, but
this design does not overclaim that it already proves the current Windows flash
or focus-stall symptoms.

## Testing

The immediate implementation round should verify:

- Windows subprocess hygiene script catches unsafe runtime subprocess patterns
- contract lint script aggregates the expected checks
- frontend tests still cover auto-launch decision logic
- Rust workspace still builds and tests successfully
- existing manager launch behavior remains semantically unchanged except for the
  new probe scheduling and caching behavior

Manual verification should include:

- switching window focus does not burst-refresh the manager immediately
- Windows launch/reinject/process-detection paths no longer open visible console
  windows

## Acceptance Criteria

This design is satisfied for the immediate round when all of the following are
true:

- a dedicated Windows subprocess integration module exists;
- the known high-risk runtime subprocess paths adopt the integration rule;
- manager process-running detection no longer depends on repeated synchronous
  focus-triggered subprocess probes;
- frontend focus-triggered refresh is debounced;
- `docs/contracts/*` exists with the scoped contract set;
- repository scripts enforce the highest-value contract checks;
- CI runs the new contract checks on Windows-oriented verification paths;
- affected existing design docs are updated to match the new implementation
  direction;
- the later mechanical splitting strategy is documented without being
  prematurely implemented.
