# 14. Markdown as a source syntax via pulldown-cmark

| Field | Content |
|---|---|
| ADR | 0014 |
| Title | Markdown as a source syntax via pulldown-cmark |
| Date | 2026-10-10 11:56 +0400 |
| Author | GLM-5.3-Flash (Z.ai), via omp, at the project owner's request |
| Commit | `55f011f` Fix broken fuzzing CI + uncommitted `dev-tools` harness (bevy_mcp_harness) and the `inline` example |
| Status | Accepted |
| Related | 0001 (UI as HTML templates), 0004 (verification strategy), 0007 (layout through CSS); the `<img>`/inline features of 0.5.1 |

## Context

bevy_markup's pipeline is HTML-only: a template (`.html`, Tera) renders to an
HTML string, `tl` parses it into a DOM, and the DOM builds Bevy UI
(`src/template.rs` loader, `src/html.rs` render). Fluent translations are
stored as strings and parsed as markup fragments by `walk_translation`
(`src/build.rs`). Every visible string — titles, journal entries, tutorial
text, item descriptions, patch notes — therefore has to be authored as HTML.

The project owner wants prose in Markdown: documents that read well as source
(headings, emphasis, lists) without hand-writing `<h2>`/`<em>` tags, for
text-heavy UI, while HTML stays the tool for interactive layouts. Since
Markdown compiles to HTML, both Tera templates and Fluent message values can
take Markdown as their source syntax and feed the existing pipeline — no new
DOM model, no new layout mapping.

The converter is a new third-party dependency. The crate's dependency
discipline (0002: only what the pipeline needs) makes the choice worth a
record.

## Decision

Four decisions, one per concern:

1. **Markdown → HTML via pulldown-cmark 0.13 (MIT).** A single conversion
   boundary — `src/md.rs::to_html(source) -> String` — that all call sites
   share, enabled with `Options::ENABLE_TABLES | ENABLE_STRIKETHROUGH |
   ENABLE_TASKLISTS | ENABLE_FOOTNOTES`. The event-stream → `push_html`
   shape keeps the conversion stateless and allocation-free enough to run
   per message value and per re-render.
2. **`.md`/`.markdown` templates.** A second template loader
   (`src/template.rs`, next to the `.html`/`.htm` one, same
   `resolve_references`/one-Tera-set logic, `extensions()` returning
   `["md", "markdown"]`). `HtmlTemplate::render` converts the rendered
   source through `md::to_html` before `HtmlDocument::parse`, keyed off the
   template's own extension — so a Markdown template may `{% include %}`
   HTML components and vice versa.
3. **Force Tera autoescaping for `.md`/`.markdown`.** Tera autoescapes by
   extension and Markdown's is not in its default list; without this,
   `{{ value }}` in a `.md` template silently injects raw text into the
   HTML (a behavior *and* injection difference from every `.html`
   template). The loader sets the extension list explicitly.
4. **Markdown in Fluent values.** Each message value is converted through
   the same `md::to_html` at localize time (`src/l10n.rs`, where
   `LocalizedText` stores translation strings). FTL's escaping rules stand
   unchanged (`{"{"}` for braces, `&lt;` for literal `<`); Markdown's own
   escaping (`\*`) covers its punctuation.

Raw HTML passes through everywhere (CommonMark inline HTML → the DOM as
usual): a `.md` file may embed real UI — `data-on-click` buttons,
containers — exactly like an HTML template. This matches the crate's
existing permissive-markup policy (translations keep markup by design; see
the `fluent_permissive_markup` vector).

Documented limits follow the HTML mapping unchanged: links (`<a>`) render
as plain text until anchors gain a behavior, Markdown tables and task-list
checkboxes fall outside the container model, and `ol` numbering remains the
known `li` limit.

## Alternatives considered

- **comrak 0.56** (BSD-2-Clause, the cmark-gfm lineage): a much broader
  extension set (underline, superscript, description lists, math,
  wikilinks, front matter, heading anchors, shortcodes). Lost on weight and
  fit: ~8 crates of dependencies (typed-arena, phf, finl_unicode, …)
  against pulldown's ~3; MSRV 1.89 against pulldown's 1.71; an arena AST
  allocated per conversion, which runs per *message value* on every locale
  switch and template re-render; and its safety-first defaults (escaped raw
  HTML) would have to be turned off to match this crate's permissive policy.
  Pulldown's event stream also leaves future room to intercept specific
  tags before rendering (rewriting `<a>`, say) without an AST rework.
  *Escape hatch:* the choice is contained in `md::to_html`; comrak remains
  a drop-in replacement if a concrete extension need appears.
- **A custom Markdown converter:** rejected — CommonMark compliance is a
  large, subtle spec; writing one repeats the fluent-syntax lesson
  (UPSTREAM U1/U2, `bug_0005`, `bug_0015`: hand-rolled text parsing is the
  crate's main bug source) at worse odds.
- **No Markdown support** (status quo): the trigger was real authoring
  friction — the owner wants prose documents without hand-written tags.
  Rejected on that need, not on effort.

## Consequences

- **New dependency:** `pulldown-cmark` 0.13 (+ its ~3 transitive crates) in
  `Cargo.toml`, called from library code for the first time as a
  *converter* (not an asset parser). Affects `cargo check --lib` and the
  fuzzing-feature gate.
- **Implementation pending** (this record pins the design):
  `src/md.rs`; the second loader and the render hook in `src/template.rs`;
  the localize-time conversion in `src/l10n.rs`; the forced autoescape
  list. Nothing in `build.rs`/`cascade.rs`/layout changes.
- **Fuzz surface:** Markdown bytes reach the DOM through a new parser. The
  `html` fuzz target should feed Markdown (and `.md`-rendered documents)
  alongside raw HTML — consistent with 0004's "attack every generated
  input" stance. Rustdoc's continuous fuzzing of pulldown-cmark makes this
  mostly a safety net rather than a discovery surface, but the harness
  contract (no panic) still applies.
- **Tests to add:** loader-extension discovery, `.md` render vectors (Tera +
  Markdown + CSS, dump-asserted), Markdown-in-FTL vectors, a `content_lint`
  example page in both locales, and the autoescape regression (a `{{ }}`
  value containing markup must not inject raw HTML in `.md`).
- **Docs:** README compatibility table unaffected (Markdown is a library
  feature, not a Bevy coupling); `style.rs`'s CSS-subset docs and AGENTS.md
  gain the Markdown section when the feature lands.
