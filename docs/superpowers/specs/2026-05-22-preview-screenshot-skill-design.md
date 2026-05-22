# Preview Screenshot Skill Design

## Context

CodexPilot already has a stable local preview surface for the manager UI, and
the project increasingly depends on real screenshots for README sections,
feature docs, release notes, and community promotion posts.

The current screenshot workflow works, but it is too easy for the agent to make
the wrong local choice in the moment:

- use the user's personal browser instead of the in-app browser
- mix personal tabs, translation bars, or extensions into the capture
- improvise output naming each time
- treat screenshot work as ad hoc browser automation instead of a repeatable
  project workflow

The user expectation is narrower and clearer than a general screenshot system:
when they say "帮我截图", the agent should default to a clean in-app browser
workflow and produce usable project screenshots without needing extra steering.

## Goal

Add a project-local skill that standardizes how CodexPilot screenshot requests
are handled.

The skill should make one default behavior reliable:

1. prefer the Codex in-app browser
2. avoid the user's personal browser unless explicitly requested
3. open or verify the local preview target
4. navigate to the requested page state
5. capture clean screenshots
6. save them with readable names to the requested location

This is a workflow skill, not a generic screenshot framework.

## Non-Goals

This design does not introduce:

- a fully scripted bulk screenshot exporter
- a new application feature inside CodexPilot itself
- automatic image post-processing or beautification
- support for every possible browser target outside normal CodexPilot preview
  flows

If a future need emerges for batch documentation exports, that can build on top
of this skill later.

## Decision

Create a new project skill at:

```text
.agents/skills/capture-preview/SKILL.md
```

This skill will encode a strong default operating procedure for screenshot
requests in this repository.

It should be intentionally opinionated. The main value is not abstract
reusability; the main value is making the agent stop improvising.

## Default Workflow

When a user asks for screenshots in this project, the skill should instruct the
agent to follow this order:

1. determine whether the request targets the local preview or another project
   web surface
2. prefer the in-app browser for capture
3. only use the user's personal browser if the user explicitly asks for it or
   the task depends on their logged-in browser state
4. verify that the preview service is already running; if not, start or restart
   it
5. open the target URL in the in-app browser
6. verify that the visible page matches the requested page or state
7. capture one or more screenshots
8. save them to the requested destination, or use a documented default
9. report back the saved file paths

The skill should also push the agent toward reusing existing preview services
instead of restarting them unnecessarily.

## Browser Policy

The skill should define a clear browser preference policy:

- first choice: Codex in-app browser
- second choice: other browser automation only when the in-app browser cannot
  satisfy the requirement
- personal Chrome or another user browser only with explicit user instruction or
  a concrete logged-in-state need

This rule exists because project screenshots should be clean by default. The
agent should not capture user tabs, browser extensions, translation UI, or
other personal context unless the user deliberately wants that environment.

## Preview Service Policy

The skill should document the current CodexPilot preview target:

```text
apps/codex-pilot-manager
npm run preview:ui
http://127.0.0.1:1420/
```

It should treat this as the default manager preview workflow unless the user
asks for a different local target.

The skill should instruct the agent to:

- check whether the expected preview is already available
- avoid duplicate servers when one is already serving the correct page
- restart the preview only when the user asks for a fresh run or when the page
  is stale/broken

## Screenshot Quality Rules

The skill should define lightweight but strict capture rules:

- capture the intended page state, not a transitional state
- avoid visible browser chrome or unrelated UI noise when the task is about the
  page itself
- avoid translation banners, account menus, extension popups, and unrelated
  overlays
- prefer semantically complete page captures over random cropped fragments,
  unless the user explicitly requests a cropped detail
- if multiple related pages are requested, capture them as individually named
  files instead of overwriting or using timestamp-only names

The point is not pixel-perfect art direction. The point is operationally clean,
usable screenshots.

## Output Rules

The skill should tell the agent to honor user-specified destinations first.

Default destinations:

- if the user names a filesystem location, use it
- if the user says `Downloads`, save there
- if the user asks to upload to the repo, place files under a meaningful
  repository image directory rather than leaving them in a personal folder

Default naming should be semantic, for example:

- `codexpilot-overview.png`
- `codexpilot-provider.png`
- `codexpilot-sessions.png`
- `codexpilot-diagnostics.png`

Timestamp suffixes are acceptable when the user explicitly wants distinct
versions or when name collision avoidance is necessary.

## Project-Specific Defaults

For the current manager preview, the skill should document the common page
targets:

- `总览`
- `模型通道`
- `对话维护`
- `诊断`

When the user asks broadly for "这几个页面截一下", the skill should bias
toward these known top-level views before inventing deeper navigation.

## Error Handling

The skill should define expected recovery behavior:

- if the preview service is unavailable, start it and retry
- if writing to the requested location fails, save to a permitted temporary
  location and then move the files to the requested destination when possible
- if the page does not match the expected state, verify the route/view before
  taking a screenshot
- if the task genuinely requires the user's browser session, explain why the
  in-app browser is insufficient before switching away from it

The agent should not silently downgrade from in-app browser capture to personal
browser capture.

## Scope of Implementation

The implementation for this design should stay small:

1. add the new project skill file
2. encode the default screenshot workflow and project defaults in that skill
3. do not add batch screenshot scripts yet
4. do not modify the application code unless a missing preview affordance blocks
   normal capture

## Acceptance Criteria

- a project-local screenshot skill exists under `.agents/skills/capture-preview`
- the skill explicitly prefers the in-app browser for screenshot requests
- the skill documents the current manager preview command and URL
- the skill tells the agent not to use the user's personal browser by default
- the skill defines destination and naming defaults
- the skill gives enough project-specific guidance that a future "帮我截图"
  request can be handled without re-deriving the workflow from scratch
