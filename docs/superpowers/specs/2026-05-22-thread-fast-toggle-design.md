# Thread Fast Toggle Design

## Context

CodexPilot currently has no direct way to control Codex service tier at the
conversation level. Users who care about latency-sensitive work, especially
newly opened high-priority conversations, need a lightweight way to switch the
current conversation into `Fast` mode without changing global defaults.

The desired interaction is intentionally small and close to the conversation:

- extend the existing `Pilot` floating pill;
- place a clickable lightning icon to the left of the green status dot;
- use the lightning state to represent the current conversation's service mode.

The product goal is not to expose a complete service-tier control panel. The
goal is to let users mark the current conversation as `Fast` or `Standard`,
including before the first message of a new conversation is sent.

## Decision

Implement a current-conversation `Fast` toggle in the `Pilot` floating pill.

The first version supports only two states:

- `Standard`
- `Fast`

Scope rules:

- No global `Fast` / `Standard` mode.
- No separate multi-option thread control menu.
- No complex badge auto-placement near the composer.
- No conversation-list `Fast` indicator in the first version.

This feature must still support new conversations from the first request. If a
user enables `Fast` before the new conversation receives a stable thread id, the
first request of that conversation must already carry `serviceTier = "priority"`.

This requirement is non-negotiable for the first version. A UI that only lights
up after the first request, or a partial interceptor that misses some new
conversation entry paths, does not satisfy this design.

## User Experience

### Entry Point

The entry point lives inside the existing `Pilot` floating pill.

- Position: to the left of the green status dot.
- Form: a clickable lightning icon.
- Default visual state: gray lightning for `Standard`.
- Active visual state: yellow lightning for `Fast`.

The pill should remain compact. The lightning icon is a lightweight stateful
control, not a separate menu launcher.

### Floating Pill Structure

The current `Pilot` pill is a single clickable control that toggles the Pilot
panel. The first version must change that structure explicitly instead of
nesting one button inside another.

Required structure:

- one dedicated `Fast toggle` control rendered as the lightning icon;
- one dedicated `Pilot panel toggle` control rendered as the existing dot/label
  area;
- both controls are sibling interactive elements inside one shared pill shell.

Interaction rules:

- clicking the lightning icon toggles `Fast` / `Standard` only;
- clicking the green-dot-and-label area opens or closes the Pilot panel only;
- clicking the lightning icon must not also open the panel;
- keyboard focus must be able to land on each control separately;
- touch targets must remain usable without frequent accidental panel toggles.

### Interaction

When the current conversation is in `Standard`:

- show a gray lightning icon;
- clicking it switches the current conversation to `Fast`;
- show a toast such as `当前对话已切换为 Fast`.

When the current conversation is in `Fast`:

- show a yellow lightning icon;
- clicking it switches the current conversation to `Standard`;
- show a toast such as `当前对话已切换为 Standard`.

When the user is on a new-conversation page and no stable thread id exists yet:

- the same lightning icon remains available;
- enabling it means `the next new conversation request should start in Fast`;
- show a toast such as `下一条新对话将使用 Fast`.

### Tooltip Copy

The icon must not rely on color alone. It should expose a tooltip or equivalent
hover label:

- existing thread, standard: `当前对话使用 Standard`
- existing thread, fast: `当前对话使用 Fast`
- new-conversation draft, standard: `下一条新对话将使用 Standard`
- new-conversation draft, fast: `下一条新对话将使用 Fast`

## State Model

This feature needs two kinds of local state.

### Thread Overrides

Persist conversation-level service-mode overrides by thread id:

- `threadId -> standard`
- `threadId -> fast`

Only explicit overrides are stored. Unset conversations stay on Codex's default
behavior and should render as `Standard` in the first version's UI language.

Internally, the implementation must still distinguish:

- `no override`
- `explicit standard override`
- `explicit fast override`

The UI may still render both `no override` and `explicit standard override` as a
gray lightning icon, but the design must preserve the semantic difference
because request rewriting behavior is different.

### New-Conversation Draft

Because a new conversation may not yet have a stable thread id when the user
clicks the lightning icon, the feature also needs one short-lived draft state:

- `draft -> standard | fast`

This draft applies only to the next newly started conversation request, then
binds to the resolved thread id once that id becomes available.

The draft state must be cleared when:

- it is successfully bound to a real thread id;
- the user explicitly toggles back to `Standard`;
- it expires after a short safety window if no new conversation starts.

Required draft rules:

- only one draft may exist at a time;
- a draft is consumed by the first qualifying new-conversation start path that
  successfully uses it;
- once a draft-backed request binds to a real thread id, that thread becomes the
  canonical owner of the state;
- if a real thread override already exists for the resolved thread id, the bound
  result must not silently discard a newer explicit thread choice;
- opening or viewing another already-existing thread must not by itself consume
  or clear the draft;
- the spec implementation should use a short TTL, for example 60 seconds, so a
  forgotten draft does not unexpectedly apply much later.

## Data Flow

### Read Current UI State

The injected renderer script should determine whether the current page maps to:

- an existing conversation with a stable thread id;
- a new-conversation draft without a stable thread id.

The floating-pill lightning state should reflect:

- the thread override if a stable thread id exists;
- otherwise the draft state for the next new conversation;
- otherwise `Standard`.

The renderer should refresh this decision on the same route and session-change
signals already used by CodexPilot for other page-bound state where practical,
rather than relying on a single one-shot read.

### Write Toggle State

When the user clicks the lightning icon:

- if the current page has a stable thread id, update that thread override;
- otherwise update the new-conversation draft state.

This remains front-end local state. No new backend settings storage is required
for the first version.

### Request Override

Before Codex requests are sent, the injected bridge layer should intercept the
conversation-start and conversation-continue request shapes that currently carry
or derive `serviceTier`.

That requirement must be implemented as an explicit compatibility matrix, not a
generic promise to intercept “the relevant requests”.

The first version should cover the current known envelope shapes used by Codex
for conversation creation and continuation, matching the practical coverage
pattern proven by CodexPlusPlus:

- host-envelope requests such as `send-cli-request-for-host`
- bridge envelope requests such as `mcp-request`
- worker envelope requests such as `worker-request`
- prewarm conversation-start envelopes such as `thread-prewarm-start`
- direct start envelopes such as `start-conversation`
- host-side prewarm start envelopes such as `prewarm-thread-start-for-host`
- host-side thread start envelopes such as `start-thread-for-host`
- host-side turn start envelopes such as `start-turn-for-host`

Within those envelopes, the implementation must cover the known service-tier
method families:

- `thread/start`
- `thread/resume`
- `turn/start`

If CodexPilot cannot confidently identify one of these supported shapes at
runtime, it should leave that message unchanged and emit a compact diagnostic
event. It must not claim that `Fast` was applied when no request rewrite
occurred.

For requests that belong to a `Fast` thread or a `Fast` new-conversation draft:

- force `serviceTier = "priority"`.

For requests that belong to a `Standard` override:

- force `serviceTier = null` or remove the priority override so the request runs
  as standard.

For requests with no override:

- leave existing behavior untouched.

### Draft Binding

When a new conversation request that used the draft state resolves into a stable
thread id:

- promote the draft mode into that thread's stored override;
- clear the draft state;
- refresh the floating-pill icon so the thread now shows as an ordinary
  conversation-level override.

This binding must not depend on the HTTP response alone. The renderer should
bind opportunistically when a stable current thread id becomes observable from
the active page/session context after the draft-backed start flow.

Required binding behavior:

- after a draft-backed start request, begin watching for the resolved thread id
  through the same session/route tracking signals already available to the
  injected UI layer;
- retry binding while the draft TTL remains valid;
- once the real thread id is observed, copy the draft mode into the thread
  override table and clear the draft;
- if binding does not complete before TTL expiry, clear the draft and emit
  diagnostics instead of leaving an indefinite hidden state behind.

## UI Placement and Visual Rules

The lightning icon belongs to the `Pilot` pill rather than the composer area.

Reasons:

- the `Pilot` pill is a CodexPilot-owned surface and is more stable than
  upstream Codex composer DOM;
- conversation-level control fits the `Pilot` affordance;
- this avoids the fragile DOM heuristics required to inject a composer-adjacent
  badge into changing upstream layouts.

Visual rules:

- keep the existing pill size as unchanged as possible;
- lightning icon should read as an inline state chip, not a heavy primary CTA;
- yellow active state should be noticeable but not overpower the `Pilot` label;
- disabled or unavailable state should look intentionally muted rather than
  broken.

Because the first version deliberately avoids composer-adjacent badges and
conversation-list markers, the floating-pill UI must compensate with stronger
local feedback:

- tooltip copy must always reflect the currently effective conversation or draft
  state;
- toggling to `Fast` on a new conversation draft must show an explicit toast
  that the next new conversation starts in `Fast`;
- when route/session changes cause the effective state to change, the icon state
  should refresh immediately so the user does not have to reopen the panel.

## Error Handling

If the renderer cannot confidently determine the current conversation context:

- if a new-conversation draft action is still safe, allow draft toggling;
- otherwise disable the icon and expose a tooltip such as `当前页面暂不支持切换服务模式`.

If request interception fails or encounters an unsupported message shape:

- keep the UI responsive;
- log a diagnostic event with compact technical detail;
- do not silently fake a successful `Fast` request override if the request was
  not actually modified.

If draft binding fails after the first request:

- prefer preserving the request override for that launch over prematurely
  clearing state;
- emit diagnostics so the mismatch can be explained later.

Diagnostics must not include message bodies or secrets.

## Dependency on Enhancements

This feature lives inside the injected `Pilot` floating pill and therefore
depends on the same page-enhancement injection being active.

Required behavior:

- if page enhancements are disabled, the lightning control is unavailable
  because the Pilot pill itself is unavailable;
- the feature must not introduce a parallel backend-only control path that
  suggests thread-level `Fast` is available without injected UI/request
  interception.

## Non-Goals

- No global service-tier switch.
- No per-thread multi-state menu beyond `Fast` and `Standard`.
- No conversation-list lightning marker in the first version.
- No complex composer badge insertion or composer footer heuristics.
- No backend-managed cross-device sync for this state.

## Testing

Add tests that cover:

- existing conversation toggles from `Standard` to `Fast` and back;
- new-conversation draft toggles before the first message;
- first new-conversation request receives `serviceTier = "priority"` when draft
  is `Fast`;
- draft binds to the resolved thread id after conversation creation;
- existing `Fast` thread continues to apply priority on each supported request
  family and envelope shape;
- unsupported or unknown request shapes remain unchanged;
- tooltip/state rendering matches stored mode;
- floating-pill lightning click does not also toggle the Pilot panel;
- panel toggle still works independently of the lightning control;
- draft expiry and multi-step bind behavior follow the documented TTL and
  ownership rules;
- diagnostics fire on read or override failures.

## Acceptance Criteria

- The `Pilot` floating pill shows a clickable lightning icon to the left of the
  green status dot.
- Gray lightning represents `Standard`; yellow lightning represents `Fast`.
- Clicking the icon toggles only the current conversation, not all
  conversations.
- For existing conversations, the selected mode persists by thread id.
- For new conversations, enabling `Fast` before the first message causes the
  first request to use `serviceTier = "priority"`.
- After the new conversation receives a stable thread id, the draft mode binds
  to that thread and continues to apply.
- The implementation documents and tests the supported request-envelope matrix
  instead of relying on an unspecified “relevant requests” catch-all.
- No global service-tier mode is introduced.
- No conversation-list `Fast` indicator is added in this version.
- No composer-adjacent injected badge is required for this version.
