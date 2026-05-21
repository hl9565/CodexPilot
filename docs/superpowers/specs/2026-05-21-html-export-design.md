# HTML Export Design

## Context

CodexPilot currently offers Markdown export from the injected Pilot panel. The
existing action is useful for backup, editing, and reuse, but it is not ideal for
showing a conversation to someone who expects a readable document they can open
directly in a browser.

## Goal

Add a presentation-oriented HTML export while keeping the floating Pilot panel
minimal. The export entry should be visually split into two equal actions:
Markdown on the left and HTML on the right.

## User Experience

The Pilot panel keeps its current title, version, and backend status message. The
single export row is replaced by a compact segmented export control:

- left segment: `导出 MD`;
- right segment: `导出 HTML`;
- both segments have equal weight inside one subtle container.

The control should borrow only the structure of a segmented switch: outer tray,
inner split, and clear left/right affordance. It should not use saturated colors,
heavy shadows, or a game-like treatment. The visual language remains aligned with
the current CodexPilot panel: white surface, light gray border, restrained blue
accent.

Clicking either segment exports the current detected session. Markdown keeps the
existing behavior. HTML downloads a `.html` file and the panel message reports
the exported filename.

The sidebar row hover actions remain unchanged in this iteration. Adding another
row icon would increase crowding and is not needed for the primary showcase
workflow.

## HTML Output

The HTML export is a self-contained document with inline CSS and no external
network dependencies. It is meant for opening, sending, or presenting directly.

The page includes:

- conversation title;
- export metadata such as generation time and message count;
- ordered message sections with role, optional timestamp, and body;
- readable typography, clear separators, and responsive layout;
- styled code blocks through the existing text content, without adding a
  Markdown parser dependency in this change.

The renderer must escape HTML-sensitive characters from the conversation content
before inserting them into the document.

## Architecture

Reuse the existing session loading path in `MarkdownExportService`. Extend the
export result with optional `html` content and add a service method for HTML
export. The core bridge gets a new `/session/export-html` route.

The injected renderer adds:

- `exportHtml(session)` on `window.__CODEX_PILOT__`;
- a `downloadHtml` helper;
- a shared export click path that handles Markdown and HTML status messages.

## Error Handling

HTML export uses the same detection, not-found, and failed states as Markdown
export. If the backend route fails or no session is detected, the panel message
shows a concise failure reason and the Codex page remains unaffected.

## Testing

Automated coverage should include:

- Rust data tests for HTML export filename/content and escaping;
- bridge route coverage through existing renderer fixture calls;
- renderer injection tests that the Pilot panel shows both export segments and
  clicking HTML calls `/session/export-html`.

Manual verification should check the panel visual balance and that a downloaded
HTML file opens as a readable standalone document.
