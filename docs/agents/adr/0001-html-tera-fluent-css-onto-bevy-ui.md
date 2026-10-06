# 1. UI as HTML templates (Tera) + Fluent + CSS, built into native Bevy UI entities

| Field | Content |
|---|---|
| ADR | 0001 |
| Title | UI as HTML templates (Tera) + Fluent + CSS, built into native Bevy UI entities |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `59f8f32..421abbc` (2026-10-04) |
| Status | Accepted |
| Related | 0002, 0003, 0004; `bug_0006`, `bug_0007` |

## Context

The repository started as a Bevy 0.19 prototype named `p23` exploring "UI built
from assets" (`AGENTS.md` at `583891d`): ratatui panels rendered to textures
(`bevy_tui_texture`), 9-slice framed Bevy UI panels, and an experimental
HTML → Bevy UI path. Bevy UI itself is built in Rust code (`Node` bundles); a
designer or translator can't change a screen without a rebuild, and Bevy has
no templating, localization or stylesheet layer of its own. The reasoning for
going HTML-first rather than, say, a Rust builder DSL isn't recorded in the
commits [INFERENCE: the asset-driven goal — screens editable as files,
hot-reloadable, localizable — points there].

## Decision

A four-stage pipeline per `HtmlUi` entity, each stage a well-known format and
an existing crate:

1. **Render** — the `.html` asset is a **Tera** template rendered with the
   entity's context (`583891d`: `assets/html.rs`, later `src/template.rs`).
2. **Parse** — the output is parsed with **`tl`** into a DOM (`HtmlDocument`).
3. **Localize** — elements with `data-l10n-id` / `data-l10n-args` get their
   content from **Fluent** (`bevy_fluent`), following Mozilla's fluent-dom
   conventions, with inline markup in translations allowed (`583891d`; the
   demo fully translated in `421abbc`; `data-l10n-id` on containers and
   `data-l10n-name` overlays later, bug_0006/0007).
4. **Style and build** — a `.css` asset parsed with **lightningcss**
   (`f128a03`) is cascaded per element and the DOM is turned into **native
   Bevy UI**: blocks (`p`, `h1`–`h6`, `li`, `pre`) become `Text` nodes with a
   `TextSpan` per styled run, containers become `Node`s (`cb9010f` colors,
   `1205897` font family/size, `025e685` classes and containers).

Themes are just stylesheets swapped at runtime (`056d9ac`); languages are
Fluent bundles swapped at runtime (`ab7c084`, `421abbc`).

## Alternatives considered

- **Hand-built Bevy UI** (the status quo the prototype also had: `ui/panel.rs`
  at `583891d`): no asset-driven editing or localization layer.
- **ratatui panels rendered to textures** (`0ca66a4`, `583891d`): explored in
  the same prototype and dropped when the crate became a library (`c4bc132`
  deleted `src/tui/`, ~3,000 lines including `Cargo.lock` churn) [INFERENCE:
  terminal cells don't give per-element styling, layout or picking].
- **A webview / embedded browser engine**: not recorded [INFERENCE: the
  output being plain Bevy UI entities — queryable, pickable, part of the
  ECS — is the point, and rules it out].

## Consequences

- The UI is ECS data: apps query `HtmlElement`s, attach components, and pick
  with Bevy's own picking. Everything later in this ADR series builds on that.
- Only a **subset** of HTML/CSS is supported, and it's documented as such
  (`src/style.rs`, `AGENTS.md` "Limits"): no combinators, no inline boxes
  (spans have no box), a CSS-ish rather than CSS-exact layout model.
- Fidelity to the real formats became testable against their reference
  implementations — Chromium for CSS, `@fluent/dom` for Fluent (ADR 0004).
- Three parsers sit in front of user content (Tera, `tl`, lightningcss, plus
  `fluent-syntax`), which made fuzzing worthwhile (ADR 0004) and later
  surfaced upstream bugs (UPSTREAM U1, U2, U10, U11).

## Also in this span

- `b359ee9`, `2ea156e` — scrollbars for the demo's panels.
- `a95fcad`, `4384eee` — typographic features and monospace tags.
