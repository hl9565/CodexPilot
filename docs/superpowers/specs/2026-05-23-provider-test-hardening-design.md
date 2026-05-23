# Provider Test Hardening Design

## Context

CodexPilot already has meaningful Rust unit tests around protocol conversion,
storage, relay config, and some route behavior, but its protection is uneven
relative to current risk.

Observed project state on 2026-05-23:

- `cargo test --workspace` passes with 61 Rust tests.
- Frontend automated coverage is minimal and currently centered on
  `apps/codex-pilot-manager/src/autoLaunch.test.ts`.
- High-risk files are large and combine multiple responsibilities:
  - `crates/codex-pilot-core/src/protocol_proxy.rs`
  - `crates/codex-pilot-core/src/relay_config.rs`
  - `apps/codex-pilot-manager/src-tauri/src/lib.rs`
  - `apps/codex-pilot-manager/src/main.tsx`

The most dangerous problem area is not generic UI polish but the Provider
switching and relay path:

- switching profiles can produce the wrong runtime protocol or route mode;
- `responses`, `chatCompletions`, and `anthropicMessages` conversions are easy
  to regress;
- SSE behavior can fail even when non-stream responses still work;
- saved config can appear correct while runtime behavior drifts after reload.

This design focuses on hardening those provider and protocol paths first.

## Goals

- Build a dedicated testing workstream in an isolated `worktree` and branch.
- Reduce regression risk in Provider switching, protocol routing, and SSE.
- Improve testability through targeted refactoring of overly broad modules.
- Add a layered test strategy that covers both protocol details and end-to-end
  provider decision flow.
- Allow tightly related bug fixes discovered by the new tests.
- Leave the project with clearer, more maintainable test entry points.

## Non-Goals

- Do not build a full frontend test system in this round.
- Do not pursue GUI-level end-to-end coverage for all manager behavior.
- Do not perform unrelated large-scale refactors.
- Do not optimize for test-count growth or coverage numbers alone.
- Do not change product behavior outside provider/protocol stability unless a
  directly related defect is exposed.

## Scope

This workstream covers the highest-risk provider path:

- profile save, activation, reload, and runtime resolution;
- route-mode decisions for direct vs local proxy behavior;
- request/response adaptation for supported upstream protocols;
- SSE adaptation and error propagation;
- relay config behavior required for provider switching to take effect.

This workstream may include medium-sized refactoring when it directly improves
testability or correctness in these paths.

## Worktree Strategy

The work should happen in a dedicated `worktree` on a focused branch such as
`codex/test-hardening`.

Rationale:

- test hardening will mix design, new tests, fixture support, and selective
  code movement;
- current workspace changes should not be polluted by this effort;
- review and rollback are easier when the testing campaign is isolated;
- the three phases below are strongly connected and should stay on one branch.

The same isolated worktree should carry all three phases:

1. risk assessment;
2. stop-the-bleeding test additions and related fixes;
3. testing-system consolidation.

## Risk Assessment

The first phase should produce a concrete map of current protection and current
gaps across four categories:

1. protocol conversion;
2. provider decision and route selection;
3. config persistence and reload consistency;
4. command orchestration boundaries.

The risk map should answer:

- which high-risk branches already have tests;
- which failures are only indirectly covered;
- which defects would currently escape automated detection;
- which code boundaries prevent direct, focused tests.

The risk map is a planning artifact inside the worktree and does not require a
separate public feature surface.

## Testing Architecture

The testing strategy should use four layers, with most investment concentrated
in Rust rather than manager UI automation.

### 1. Protocol Conversion Layer

Purpose:

- validate request adaptation between Codex-facing `responses` and upstream
  protocol shapes;
- validate non-stream response conversion;
- validate SSE event conversion;
- validate non-2xx and malformed payload handling.

Preferred form:

- pure or near-pure Rust tests driven by JSON fixtures and stream samples;
- minimal dependency on live networking.

### 2. Provider Decision Layer

Purpose:

- validate how active mode, active profile, helper port, and upstream protocol
  resolve into runtime route decisions;
- validate direct vs local proxy selection;
- validate chosen base URL and protocol target.

Preferred form:

- focused Rust unit tests over extracted decision helpers or domain types.

### 3. Persistence Consistency Layer

Purpose:

- validate save, activate, reload, migration, and re-read flows;
- catch state drift where persisted settings and runtime behavior disagree.

Preferred form:

- tempdir-backed tests using real config serialization and reload behavior;
- assertions on both stored state and derived runtime target.

### 4. Chain Integration Layer

Purpose:

- validate realistic flow from provider change to effective protocol behavior;
- protect against subcomponents that each pass in isolation but fail together.

Preferred form:

- compact Rust integration-style tests;
- local mock upstreams only where pure-function tests are insufficient.

## Refactoring Strategy

This round allows medium refactoring, but only when it increases testability or
stability for the scoped provider path.

### Primary Refactor Target: `protocol_proxy.rs`

`crates/codex-pilot-core/src/protocol_proxy.rs` should be the main structural
focus because it currently combines:

- route and path decisions;
- request adaptation;
- response adaptation;
- SSE adaptation;
- HTTP proxy transport behavior.

The desired direction is to split responsibilities into smaller modules or
equivalent boundaries, for example:

- route helpers;
- chat-completions adapters;
- anthropic-message adapters;
- SSE normalization helpers.

The exact file layout may vary, but the responsibility split must allow direct
tests against small units without driving every branch through one giant file.

### Secondary Refactor Target: Provider Resolution Logic

Provider activation and relay config behavior should expose a testable runtime
decision boundary that answers questions such as:

- which upstream protocol is active;
- whether route mode is direct or local proxy;
- which base URL Codex should use;
- which stored settings remain purely internal to CodexPilot.

The goal is not to rewrite all config handling, but to make runtime resolution
explicit and testable.

### Limited Refactor Target: Tauri Command Boundary

`apps/codex-pilot-manager/src-tauri/src/lib.rs` should stay mostly as a command
orchestration layer in this round.

Allowed change:

- extract provider/protocol-specific behavior into thinner service functions
  where that directly improves test coverage or correctness.

Disallowed direction:

- broad command-layer redesign unrelated to provider hardening.

### Frontend Constraint

`apps/codex-pilot-manager/src/main.tsx` is large and hard to test, but it is
not the primary battlefield for this workstream.

Frontend changes should stay minimal unless:

- a provider-switching defect is rooted in a small pure logic seam that can be
  extracted safely; or
- a very small UI-side logic test gives strong protection for provider state.

The round should not expand into full React component testing.

## Stop-The-Bleeding Deliverables

The isolated worktree should produce the following concrete protections:

1. new provider/protocol Rust tests covering:
   - route mode decisions;
   - request conversion for supported upstream protocols;
   - non-stream response conversion;
   - SSE conversion;
   - error passthrough behavior.
2. new provider-switch chain tests covering:
   - profile save;
   - active profile switch;
   - reload or reread behavior;
   - runtime target derivation;
   - resulting base URL, protocol, and route mode.
3. tightly related bug fixes revealed by those tests.
4. stable test entry points so future contributors know how to run the new
   protection without tribal knowledge.

## Execution Order

Implementation should proceed in this order:

1. create the isolated `worktree` and branch;
2. inventory current provider/protocol tests and gaps;
3. extract the smallest high-value test seams from the protocol layer;
4. add protocol conversion and SSE tests;
5. add provider-switch persistence and runtime-resolution tests;
6. fix directly exposed defects within scope;
7. normalize test entry points and documentation.

This ordering is intentional:

- protocol regressions are the highest-risk failures;
- chain tests are more valuable after core decision seams become testable;
- test infrastructure should be informed by actual stop-the-bleeding work, not
  invented in the abstract first.

## Acceptance Criteria

This effort is successful when all of the following are true:

1. critical protocol paths for supported upstreams are automatically tested for
   request, response, and SSE behavior;
2. provider switching can be validated from persisted config to runtime target;
3. direct vs local proxy routing decisions are automatically verifiable;
4. at least the known high-risk failure classes can no longer regress silently;
5. the new tests run through clear, documented entry points;
6. refactoring remains bounded to provider/protocol hardening and does not turn
   into a general cleanup campaign.

## Risks and Tradeoffs

- Refactoring too little would keep the most dangerous logic hard to test and
  force more brittle large-file tests.
- Refactoring too much would slow down stop-the-bleeding work and widen risk.
- Over-investing in frontend automation now would consume time without covering
  the deepest protocol failure surface.
- Only adding low-level unit tests would still leave provider-switch chain
  regressions exposed.

The chosen design therefore favors a dual approach:

- fast, focused protocol tests for the deepest failure surface;
- chain-level provider tests for real-world runtime consistency.

## Design Consistency

This design does not change the intended product behavior from the provider and
relay protocol specs already in `docs/superpowers/specs/`.

Instead, it adds the testing and structural work required to keep those
existing behaviors reliable as the codebase evolves.
