# Manager Night Mode Design

## Context

CodexPilot Manager currently uses a light-only interface. Most colors in
`apps/codex-pilot-manager/src/styles.css` are hard-coded hex values, so adding
night mode as isolated `.dark ...` overrides would be brittle and easy to miss.

CodexPlusPlus already has a proven lightweight theme pattern:

- keep the theme state in the React app;
- persist the selection in `localStorage`;
- toggle a root `.dark` / `.light` class;
- place a global icon button in the top bar.

CodexPilot should follow that interaction model, but keep light mode as the
default because the current manager UI and documentation are light-first.

## Decision

Add a manager-only night mode using:

- React state for `Theme = "light" | "dark"`;
- `localStorage` persistence under a CodexPilot-specific key;
- root document classes for CSS theme selection;
- a global icon-only toggle next to the existing refresh button in the page
  header;
- semantic CSS variables for shared colors.

No backend command, Tauri permission, or persisted app settings file is needed
for this iteration.

## Interaction

The toggle lives in `headerActions`, beside the refresh button. It behaves like
a global view control:

- light mode shows a moon icon and tooltip `切换到夜晚模式`;
- dark mode shows a sun icon and tooltip `切换到浅色模式`;
- clicking the button updates the current page immediately;
- the selected theme is remembered for the next manager launch.

The existing refresh and launch buttons keep their current positions and
behavior. The theme toggle must not trigger backend refreshes or progress
dialogs.

## State Flow

Frontend state:

```ts
type Theme = "light" | "dark";
```

Initial value:

1. if `localStorage["codex-pilot-theme"]` is `"dark"`, start in dark mode;
2. otherwise start in light mode.

On change:

1. update React state;
2. toggle `document.documentElement.classList` for `.dark` and `.light`;
3. write the selected value to `localStorage`.

The app should also add the current theme class to the top-level `.shell` so
component selectors can target either the document root or the app shell when
useful.

## CSS Architecture

Refactor core colors into semantic CSS variables. The initial variable values
must preserve the current light appearance closely.

Required variable groups:

- app backgrounds: root, preview stage, shell content, sidebar, panels;
- text colors: primary, secondary, muted, inverted toast text;
- borders and dividers;
- buttons: primary, secondary, danger, hover, active, disabled;
- navigation: default, hover, active;
- form controls;
- status tokens: ok, warning, danger;
- overlays, dialogs, toast, tables, code text.

Night mode should be implemented mainly by changing variable values under
`:root.dark`, not by adding one-off dark overrides for every component.
Component-specific overrides are acceptable only when a state cannot be
expressed cleanly as a shared token.

## Visual Rules

- Keep the operational, compact CodexPilot style.
- Do not introduce a new settings page section for this iteration.
- Do not change navigation labels or page layout.
- Keep contrast strong enough for buttons, status pills, tables, and form
  controls.
- Avoid a pure black interface; use dark neutral surfaces with clear panel
  boundaries.
- Keep accent usage consistent with the existing blue primary action.

## Implementation Touchpoints

Expected frontend files:

- `apps/codex-pilot-manager/src/main.tsx`
  - import `Moon` and `Sun`;
  - add `Theme` state, initial loader, persistence effect, and toggle action;
  - render the icon button next to refresh.
- `apps/codex-pilot-manager/src/styles.css`
  - introduce semantic variables for the current light palette;
  - add `:root.dark` variable values;
  - replace hard-coded shared colors with variables across manager UI.

No Rust backend files are expected to change.

## Testing

Manual UI verification:

- launch the manager preview;
- confirm the default theme is light when no stored value exists;
- click the header theme toggle and confirm the whole manager changes without
  layout shift;
- refresh the page and confirm the selected theme persists;
- check overview, launch, provider, sessions, diagnostics, progress dialog, and
  toast surfaces in night mode.

Automated or command checks:

- run the existing frontend build or project check command if available;
- use a browser screenshot pass for light and dark mode if a local preview is
  running.

## Acceptance Criteria

- A theme icon button appears next to the refresh button in the manager header.
- Light mode remains the default.
- Night mode persists across manager reloads.
- The main manager pages, panels, forms, buttons, tables, toasts, and dialogs
  are readable in night mode.
- The implementation does not add backend commands or Tauri permissions.
- CSS uses semantic theme variables as the primary mechanism rather than a large
  set of isolated `.dark` overrides.
