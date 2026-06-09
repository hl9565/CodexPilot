# Inline Row Actions Layout Design

## Context

CodexPilot injects an inline delete action into normal Codex sidebar session
rows so users can remove a session without opening the floating Pilot panel.

The original implementation used two icon buttons placed inside the row with a
fixed absolute offset from the right side. That worked while Codex's native
trailing controls stayed narrow, but it became fragile once Codex showed more
native actions such as pinning. The Pilot buttons could then overlap or
visually crowd Codex-owned controls.

This is not a cosmetic issue only. Overlap makes ownership unclear and can
cause accidental clicks or blocked native actions.

## Goal

Keep inline session deletion available, but stop it from covering Codex's
native sidebar controls.

## Non-Goals

- Do not remove inline delete entirely.
- Do not merge Pilot actions into Codex native controls.
- Do not redesign archive-row actions in this iteration beyond staying
  compatible with the existing inline-actions switch.
- Do not change delete backend behavior.

## Product Decision

Keep direct inline delete for normal session rows, but anchor the single delete
button on the left side of the row so it no longer competes with Codex's native
trailing controls.

Behavior:

- the row still reveals Pilot actions on hover/focus;
- the visible affordance is the single direct delete button;
- there is no extra trigger and no secondary popout step.

This keeps one-step deletion while avoiding the overlap that made ownership and
click targets unclear. Normal session-row Markdown and HTML export remain
available from the floating Pilot panel.

## Placement And Direction Rules

The delete button should live on the row's left side, not on top of
Codex-native trailing controls. The reserved title mask should fade the title's
left edge while the row is hovered or focused so long titles do not collide with
the delete button.

## Interaction Model

- Hover or focus on a sidebar row reveals the Pilot delete button.
- Delete keeps its current click handling and safety behavior.

This means the interaction stays one step:

- reveal row actions
- click delete

## Existing Design Alignment

This change stays consistent with the existing enhancement specs:

- it remains governed by the `inlineActions` switch in
  `2026-05-21-page-enhancement-switches-design.md`;
- it does not alter HTML export behavior from
  `2026-05-21-html-export-design.md`;
- it does not affect Timeline, scroll restore, or the Pilot floating pill.

What changes is only the placement and collision-avoidance model for normal
session-row inline delete.

## Error Handling

- If the row cannot provide enough visual room, the buttons should still stay
  inside the row and fail soft rather than blocking Codex-owned controls.
- Any renderer errors must fail soft and not block Codex's native sidebar
  actions.

## Testing

Manual verification should cover:

- rows with Codex native trailing controls no longer show Pilot delete directly
  on top of them;
- hovering a row reveals the direct delete button without an extra trigger;
- the title fade leaves readable space for the shifted delete button;
- delete still invokes the same flow as before.

## Acceptance Criteria

- Normal session rows no longer place the direct delete button on top of Codex
  native controls.
- Delete remains directly clickable from the row.
- The delete button is visually shifted to the row's left side with matching
  title masking.
- Existing inline delete functionality remains available behind the new layout.
