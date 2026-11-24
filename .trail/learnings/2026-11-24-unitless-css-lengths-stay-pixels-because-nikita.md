---
title: Unitless CSS lengths stay pixels because template context numbers rely on them
author: Nikita
date: 2026-11-24
branch: css-own-parser
cost: { minutes: 58, tokens: 649783 }
id: l-ae994a2d
scope: repo
status: active
src: agent
touches:
  - ".aiwg/activity.log"
  - docs/agents/adr/0048-own-css-value-parsing-on-cssparser.md
  - docs/agents/adr/README.md
  - AGENTS.md
  - opencode.json
  - CHANGELOG.md
  - docs/agents/bugs/INDEX.md
  - docs/agents/bugs/bug_0248.md
  - src/svg.rs
  - src/svg_bounds.rs
---

## Decided
The CSS parser reads a unitless number in a length (`width: 160`) as pixels,
although CSS requires a unit. lightningcss did the same, and apps depend on
it: a numeric TemplateContext value reaches a stylesheet through `var()`
without a unit (`--w: 160`, `width: var(--w)`).

## Tried and ruled out
Following the CSS spec (only a bare `0` is unitless) broke
`template_context_drives_css_variables` and
`css_variables_resolve_from_sheet_context_and_fallback`.

## Watch out
Don't tighten this to match browsers without changing how context numbers
are substituted (for example, giving them a `px` unit).

In the code: `src/css/value.rs` `parse_lp` (the `Token::Number` arm).
