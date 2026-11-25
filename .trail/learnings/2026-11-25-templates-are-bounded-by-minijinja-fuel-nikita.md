---
title: "Templates are bounded by MiniJinja fuel at render, not a load-time analysis"
author: Nikita
date: 2026-11-25
branch: minijinja
cost: { minutes: 74, tokens: 281765 }
id: l-b1dadea6
scope: repo
status: active
src: agent
touches:
  - ".aiwg/activity.log"
  - ".github/workflows/cache.yml"
  - ".github/workflows/nightly.yml"
  - docs/agents/bugs/UPSTREAM.md
  - scripts/fuzz-honggfuzz.sh
  - scripts/fuzz-setup.sh
  - ".aiwg/intake/project-intake.md"
  - ".aiwg/requirements/supplementary-spec.md"
  - ".aiwg/risks/risk-list.md"
  - AGENTS.md
---

## Decided
ADR 0049 replaced Tera and the ~2,400-line static template check with MiniJinja's fuel: every render is charged per instruction, per 64 bytes produced and per item visited, and fails when it runs out (`HtmlTemplateLimits`, default 2²²). The user accepted a slower render (MiniJinja is 1.6–1.9× Tera on tight loops; under 0.4% of a 60 FPS frame for a 600-row re-render) for bounds that hold by construction.

## Tried and ruled out
- Runtime limits patched into Tera (ADR 0044's session): `[patch]` doesn't reach apps that depend on bevy_markup.
- Keeping the static check: six undercharging bugs in its model, and it refused safe shapes.

## Watch out
- `new_environment` wraps some built-in filters so they charge their result's size up front. Don't remove the wrappers without reading the private bug folder (docs/agents/bugs/private/, gitignored) and rerunning the regression test kept there; rerun it after every MiniJinja upgrade.
- The operator pre-check (`tera_tag_over_limit`) still guards the compile: fuel doesn't cover it.
- The loader reads fuel when a template loads; changing `HtmlTemplateLimits` affects templates loaded afterwards.
- Fuel calibration: ~25 fuel per element, ~1 fuel per 4 bytes of output; the examples use at most 2,605.

In the code: src/template.rs (`new_environment`, `HtmlTemplateLimits`, `TemplateFuel`, `reference_cycle`), src/html.rs (`TemplateContext`), benches/pipeline.rs (`render-compiled`).
