# Inline Row Actions Popout Design

## Context

CodexPilot injects inline session actions into the Codex sidebar so users can
export or delete a session without opening the floating Pilot panel.

The original implementation used two always-ready icon buttons placed inside the
row with a fixed absolute offset from the right side. That worked while Codex's
native trailing controls stayed narrow, but it became fragile once Codex showed
more native actions such as pinning. The fixed-offset Pilot buttons could then
overlap or visually crowd Codex-owned controls.

This is not a cosmetic issue only. Overlap makes ownership unclear and can
cause accidental clicks or blocked native actions.

## Goal

Keep inline session export/delete available, but stop them from covering Codex's
native sidebar controls.

## Non-Goals

- Do not remove inline actions entirely.
- Do not merge Pilot actions into Codex native controls.
- Do not redesign archive-row actions in this iteration beyond staying
  compatible with the existing inline-actions switch.
- Do not change export/delete backend behavior.

## Product Decision

Replace the always-visible dual overlay buttons in normal session rows with a
small Pilot action trigger that opens a compact popout action panel.

Behavior:

- the row still reveals Pilot affordance on hover/focus;
- the default visible affordance is a single lightweight trigger;
- clicking the trigger opens a compact panel containing:
  - export
  - delete

This reduces persistent overlap pressure while preserving one-step access to the
same actions.

## Placement And Direction Rules

The popout panel should prefer expanding into the row's trailing blank space on
the right side.

Direction policy:

- default: open to the right of the trigger;
- fallback: if the panel would overflow the viewport or available edge space,
  flip and open to the left.

The trigger itself remains near the row's trailing edge, but it should reserve
less width than the previous two-button cluster so Codex titles do not need such
an aggressive text mask.

## Interaction Model

- Hover or focus on a sidebar row reveals the trigger.
- Clicking the trigger toggles the Pilot action panel.
- Opening one row's panel closes any other open row panel.
- Leaving the row or moving focus away closes the panel.
- Export and delete keep their current click handling and safety behavior.

This means the interaction becomes:

- reveal affordance
- open panel
- choose action

instead of:

- immediately show both action icons inside the row at all times on hover

## Existing Design Alignment

This change stays consistent with the existing enhancement specs:

- it remains governed by the `inlineActions` switch in
  `2026-05-21-page-enhancement-switches-design.md`;
- it does not alter HTML export behavior from
  `2026-05-21-html-export-design.md`;
- it does not affect Timeline, scroll restore, or the Pilot floating pill.

What changes is only the presentation and collision-avoidance model for normal
session-row inline actions.

## Error Handling

- If the panel cannot be laid out to the right, it flips left.
- If layout measurement still fails, the trigger remains present and the panel
  should keep a safe default side instead of breaking the row.
- Any renderer errors must fail soft and not block Codex's native sidebar
  actions.

## Testing

Manual verification should cover:

- rows with Codex native trailing controls no longer show Pilot delete/export
  directly on top of them;
- clicking the trigger opens the Pilot panel on the right when space exists;
- the panel flips left when right-side space is insufficient;
- only one row panel stays open at a time;
- export and delete still invoke the same flows as before.

## Acceptance Criteria

- Normal session rows no longer use a fixed two-button overlay that can cover
  Codex native controls.
- A compact trigger opens the Pilot inline action panel.
- The panel prefers right-side expansion and can flip left when needed.
- Existing inline export/delete functionality remains available behind the new
  popout interaction.
