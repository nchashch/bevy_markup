# 12. Template composition: one Tera set per template, Tera 2 components as the widget library

| Field | Content |
|---|---|
| ADR | 0012 |
| Title | Template composition: one Tera set per template, Tera 2 components as the widget library |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — designed and implemented in that omp session for the project owner; committed by the owner (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `d2c9b63` (2026-10-06); examples `35125b6`, `c8ab00f` |
| Status | Accepted |
| Related | 0001, 0009, 0011; `bug_0022`, `bug_0023`; prototype_19 playtest 0034 |

## Context

The template loader compiled each `.html` file into its own Tera instance
(`template.rs`, `tera.add_raw_template(&name, …)`), so `{% include %}` and
`{% extends %}` couldn't reach other files. In prototype_19 the same button
markup was hand-copied 12 times across five templates.

## Decision

`d2c9b63`, `src/template.rs`:

- **Resolve references**: `resolve_references` finds `{% include "…" %}` and
  `{% extends "…" %}` in a template, resolves each path relative to the
  template's file like a URL (`../lib/ui.html`; `/x` from the asset root;
  `embedded://` stays within its source), and rewrites it to the resolved
  path, which is the name it is registered under.
- **Load them as dependencies**: the loader reads every referenced file with
  `LoadContext::read_asset_bytes`, recursively. That registers them as
  loader dependencies (verified in `bevy_asset` `loader.rs`), so editing one
  reloads every template using it.
- **One set**: the template and its references are added to one Tera
  instance (`add_raw_templates`). Tera 2 `{% component %}`s defined in any
  of them are callable from all, so a widget library is a file of
  components, included once.

## Alternatives considered

- **`{% import %}` macros** — the original plan; Tera 2 has no `import` tag
  (a probe against tera 2.4.0 failed with "Unknown tag"). Components are its
  replacement.
- **A global template registry shared by all templates** — not needed:
  per-template sets built from explicit references keep loading and
  hot-reload dependencies exact.

## Consequences

- prototype_19 has `html/components.html` (`ui.button`,
  `ui.selector_toggle`); its menus, lobby, pause menu and VR wrist panel
  call them, with attributes identical to the hand-written markup
  (playtest 0034).
- **Tera's rule for child templates**: a template that `extends` another
  may only hold `extends`, `block` and `component` at its top level, so a
  library is included inside a block there (documented in `template.rs`).
- References must be string literals; computed template names aren't
  resolved.

## Also since 0011

- `35125b6` — `menu` (focus, modal dialog, anchored tooltips, custom
  elements) and `live` (keyed in-place updates, inline styles, `opacity`)
  examples, registered with the content lint. Running `menu` in a real
  window found bug_0022 (ADR 0008).
- `c8ab00f` — quickstart and demo fixed for the in-place update model
  (bug_0023, ADR 0009); CSS `overflow` (ADR 0007).
- Throwaway in-app drivers (key presses via `ButtonInput`, cursor moves via
  `WindowEvent`, Bevy screenshots) were used to smoke-test the examples in
  this session and deleted afterwards; a committed version would cover the
  gap ADR 0004 notes.
