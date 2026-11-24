---
title: Unitless CSS lengths stay pixels because template context numbers rely on them
author: Nikita
date: 2026-11-24
branch: css-own-parser
cost: { minutes: 87, tokens: 673769 }
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
To claim a small speedup or "no regression" between two builds of bevy_markup, compile
both with functions and branch targets aligned and bevy_markup in one codegen unit, and
include a no-op build (two functions swapped) to measure the remaining noise.

## Tried and ruled out
Following the CSS spec (only a bare `0` is unitless) broke
`template_context_drives_css_variables` and
`css_variables_resolve_from_sheet_context_and_fallback`.
Plain release builds: a no-op reordering moved CSS workloads by up to 4%, and the default
16 codegen units shifted inlining in unchanged modules (a 3% "regression" on code that
didn't change). Neither setting alone was enough.

## Watch out
Don't tighten this to match browsers without changing how context numbers
are substituted (for example, giving them a `px` unit).

In the code: `src/css/value.rs` `parse_lp` (the `Token::Number` arm).
Apps build with the default units, so a few percent either way per build is normal and not
caused by the change. perf needs `-e cpu-clock:u` here, has no call graphs (the kernel
setting blocks them), and must run outside the command sandbox.

In the code: RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
plus `[profile.release.package.bevy_markup] codegen-units = 1` in the benchmark crate; best of
5 rounds per binary, interleaved.
