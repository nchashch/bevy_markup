---
title: "Font sizes are bounded by a count of distinct sizes, not only a maximum size"
author: Nikita
date: 2026-11-25
branch: main
cost: { minutes: 70, tokens: 1275371 }
id: l-3ba4586d
scope: repo
status: active
src: agent
touches:
  - AGENTS.md
  - CHANGELOG.md
  - docs/agents/bugs/INDEX.md
  - docs/agents/bugs/UPSTREAM.md
  - docs/agents/bugs/bug_0249.md
  - src/cascade.rs
  - src/css/grid.rs
  - src/css/value.rs
  - tests/html_ui.rs
  - ".aiwg/activity.log"
---

## Decided
Bevy 0.20 keeps a font atlas per exact font size and never frees it (at least 1 MiB each, a large glyph gets an atlas as big as itself). A maximum font size alone doesn't bound memory: 2,000 elements at 10, 10.001, 10.002 … px took 2 GB. So `HtmlFontLimits` has two knobs — `max_font_size` (default 2048 px; tall glyphs need a 2× atlas, which must stay within the 8192 px GPU texture limit at scale factor 2) and `max_font_sizes` (default 128, app-wide). Past the count, a new size reuses the nearest size in use by ratio: those atlases exist anyway, so reuse is free. Counted sizes are never released, matching Bevy's atlases.

## Tried and ruled out
- Quantizing every size to a grid: changes exact sizes on ordinary pages (and every dump), and still allows thousands of distinct sizes.
- A per-document budget: atlases are global, and a skeleton-reused subtree skips the cascade, so a per-build count would undercount.

## Watch out
- The budget is applied in `Styler::style_of`, so text, `em` and letter spacing agree. Changing `HtmlFontLimits` at runtime doesn't restyle anything.
- Glyph count per size is still unbounded (one atlas per large glyph); that needs atlas eviction in Bevy (UPSTREAM U55).

In the code: `src/fonts.rs` (`HtmlFontLimits`, `FontSizeBudget::admit`), `src/build.rs` (`build_html_ui` takes the budget into a `RefCell` for the build).
