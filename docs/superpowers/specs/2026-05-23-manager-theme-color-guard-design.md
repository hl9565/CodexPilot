# Manager Theme Color Guard Design

## Context

`apps/codex-pilot-manager/src/styles.css` already supports light and dark mode
through semantic theme variables defined in `:root` and `:root.dark`.

The `CCSwitch 配置` import row regressed because its component styles used
hard-coded light-mode colors directly:

- `#f7f9fc`
- `#e2e7ee`
- `#1f2b3a`
- `#637183`

This bypassed the theme tokens, so dark mode activated for the page but not for
that local UI block.

The current project does not have a style lint step for the manager frontend.
Without a targeted guard, the same mistake can be reintroduced in future UI
work.

## Goal

Add a lightweight repository guard that prevents new theme-breaking hard-coded
component colors from being added to the manager stylesheet.

## Non-Goals

- Do not introduce a full Stylelint setup.
- Do not scan unrelated apps or Rust files in this iteration.
- Do not block color literals inside the theme token definitions in `:root` and
  `:root.dark`.
- Do not redesign the manager theme system.

## Decision

Add a small Node-based check script dedicated to the manager stylesheet and run
it as part of the existing manager `check` command.

This is the preferred tradeoff because:

- it is much cheaper than adding a new lint framework;
- it matches the current problem precisely;
- it can distinguish token-definition sections from component-style sections;
- it keeps the rule easy to understand during review failures.

## Rule

The guard checks only:

- `apps/codex-pilot-manager/src/styles.css`

Allowed:

- color literals inside the top-level `:root { ... }` block;
- color literals inside the top-level `:root.dark { ... }` block;
- existing `var(--token)` usage anywhere.

Rejected outside those theme-definition blocks:

- hex colors such as `#fff` or `#f7f9fc`;
- `rgb(...)` and `rgba(...)`;
- `hsl(...)` and `hsla(...)`.

When the script finds a violation, it should fail with:

- line number;
- the matched color literal;
- guidance to replace it with a semantic theme variable.

## Implementation

### New Script

Add:

- `scripts/check-manager-theme-colors.mjs`

Responsibilities:

1. read `apps/codex-pilot-manager/src/styles.css`;
2. identify the source ranges belonging to the top-level `:root` and
   `:root.dark` blocks;
3. scan all remaining lines for forbidden color literals;
4. print actionable errors and exit non-zero when violations exist.

The parser does not need to be a full CSS parser. A small brace-aware scanner is
enough because this stylesheet is plain CSS and the rule scope is narrow.

### Package Command

Update:

- `apps/codex-pilot-manager/package.json`

Change `check` from a pure TypeScript check into:

- TypeScript check
- manager theme color guard

The command should remain simple to run locally from the manager app directory.

## Error Message Shape

Example failure copy:

```text
Manager theme color guard failed:
- styles.css:1557 uses "#f7f9fc" outside :root/:root.dark. Replace it with a theme variable.
```

The exact wording may vary, but the output must make the fix obvious without
opening the script.

## Testing

Manual verification:

1. run the manager `check` command and confirm it passes on the current file;
2. temporarily add a hard-coded color to a component rule and confirm the check
   fails with the expected line number;
3. keep color literals inside `:root` and `:root.dark` and confirm they are not
   flagged.

## Acceptance Criteria

- The repository contains a dedicated manager theme color guard script.
- `apps/codex-pilot-manager/package.json` runs the guard from `check`.
- Hard-coded component color literals in manager `styles.css` fail the check.
- Theme token literals inside `:root` and `:root.dark` still pass.
- The current manager stylesheet passes the new guard after the `CCSwitch`
  import-row fix.
