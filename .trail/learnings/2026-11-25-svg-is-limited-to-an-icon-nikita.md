---
title: SVG is limited to an icon subset instead of guarding every resvg feature
author: Nikita
date: 2026-11-25
branch: svg-allowlist
cost: { minutes: 101, tokens: 765461 }
id: l-cc526ddf
scope: repo
status: active
src: agent
touches:
  - ".aiwg/activity.log"
  - ".github/workflows/nightly.yml"
  - CHANGELOG.md
  - scripts/fuzz-ci-run.sh
  - scripts/fuzz-libfuzzer.sh
  - scripts/fuzz-triage.sh
  - src/template.rs
  - ".github/fuzz-artifacts.age.pub"
  - AGENTS.md
  - docs/agents/skills/testing.md
---

## Decided
SVGs are drawn only in an icon subset: shapes, paths, groups, defs/symbol/use, gradients and clip paths. Filters (and the CSS `filter` property), masks, patterns, markers, text, `<style>` sheets and images are refused before usvg runs, and again on the converted tree. 75% of the SVG bug classes came from those features, and about a thousand lines of guards existed only for them.

## Tried and ruled out
- Keeping every feature behind guards: each resvg release needed new bounds (regions, blurs, text layout, selector matching), and fuzzing kept finding more.
- Writing our own renderer for the subset on tiny-skia: more code and less fidelity than refusing features in front of resvg.

## Watch out
- The element check alone isn't enough: usvg turns CSS filter functions (`filter: blur()`) into real filters with no `<filter>` element. Hence the `filter` property check and the post-conversion tree check.
- Before widening the subset, re-measure against public icon sets: on 2026-11-26 all 31,833 documents of seven sets (Lucide, Tabler, MDI, game-icons, Font Awesome, Twemoji, OpenMoji) fit and drew pixel-identically. The sets are kept in ../icons.

In the code: `svg::ALLOWED_ELEMENTS`, `check_subset`, `check_tree` (src/svg.rs); ADR 0051.
