# markdown

![The reader: a framed, elliptical panel with two tabs ("Release notes" active), a rendered Markdown document — heading, emphasized paragraph, bulleted list, blockquote, struck-through last line — and the Language button](../screenshots/markdown.png)

Localized Markdown documents (ADR 0014): a reader whose pages are `.md`
templates, whose prose is Fluent values in Markdown, and whose tabs are raw
HTML inside the Markdown — real, clickable UI.

```sh
cargo run --example markdown
```

## What you see

A framed reader panel with two tabs. The **Release notes** page is a
Markdown document rendered from a Fluent value; the **Tutorial** page shows
the same pipeline with a task list, a block quote and an indented code
block. The prose of both is written in Markdown inside the `.ftl` —
switching the language switches the documents.

| Input | Action |
|---|---|
| Click a tab | Switch the page |
| Q / E, gamepad bumpers | Previous / next page (same handler a tab click sends) |
| ↑ ↓ / D-pad | Move between the tabs and the button |
| Enter / gamepad A | Press the focused element |
| Click **Language** (on either page) | Switch the language (English ↔ German) |
| Space / gamepad Y | Switch the language |

## What it shows

- **Markdown templates** — the shell (`reader.md`) is Tera + Markdown:
  `{% include %}` pulls the active page in, `{% if active_tab %}` branches
  like in an HTML template.
- **Fluent values in Markdown** — the bundle sets `markdown: true`; each
  message value is a Markdown document (heading, emphasis, lists, block
  quote, strike-through) that converts to HTML before the walk. The FTL
  authoring rules live in AGENTS.md's Gotchas (indented lists, no
  line-initial `*`, `{"{"}` for literal braces).
- **Raw HTML inside Markdown** — the tabs and the Language button are HTML
  in the `.md` page: full UI (signals, focus ring) inside a prose document.
- **CSS on generated tags** — the stylesheet targets `h2`, `em`, `strong`,
  `li`, `pre`, `blockquote p`: the Markdown output is ordinary HTML to the
  cascade.
- **The limits, honestly** — links and tables have no UI mapping and render
  as text (the tutorial says so); task-list checkboxes render as text too.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the `HtmlUi`, tab/page signals and hotkeys |
| `../assets/markdown/reader.md` | The shell: tabs + the active page include |
| `../assets/markdown/release.md`, `tutorial.md` | The pages (localized articles + an image) |
| `../assets/markdown/style.css` | The stylesheet (targets the generated tags) |
| `../assets/markdown/locales/{en-US,de}/` | Fluent bundles, `markdown: true` |

With the `dev-tools` feature, the example also serves the localhost
BRP/MCP tool surface (`bevy_mcp_harness`) for agent-driven playtesting;
off by default.
