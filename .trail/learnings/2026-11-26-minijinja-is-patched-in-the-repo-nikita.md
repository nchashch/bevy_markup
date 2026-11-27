---
title: "MiniJinja is patched in the repo, but bevy_markup keeps its own template guards"
author: Nikita
date: 2026-11-26
branch: main
cost: { minutes: 135, tokens: 1034767 }
id: l-cf94f2b2
scope: repo
status: active
src: agent
touches:
  - ".aiwg/activity.log"
  - ".github/workflows/ci.yml"
  - CHANGELOG.md
  - scripts/fuzz-ci-run.sh
  - scripts/fuzz-libfuzzer.sh
  - scripts/fuzz-seal.sh
  - scripts/fuzz-test-fuzz.sh
  - scripts/fuzz-triage.sh
  - src/template.rs
  - AGENTS.md
---

## Decided
- MiniJinja 3.1.0 is patched with `[patch.crates-io]` to the fork branch `bevy_markup/3.1.0` (3.1.0 plus the fixes for U56, U57, U59, U60, U62), in the root manifest and all four fuzz workspaces (ADR 0052). The maintainer asked for public issues instead of private disclosure (2026-11-26).
- The local guards stay anyway: `[patch]` never reaches crates that depend on bevy_markup, so apps get unpatched 3.1.0 from crates.io. Keep `checked_format`, the fueled filter wrappers, the tag operator limit and the formatter's item count (`check_printable`) until a MiniJinja release has the fixes.
Counting an arc's cubics from the exact endpoint of the previous segment undercounts after an arc whose radius is past ~1.5·10⁶: svgtypes starts the next segment where kurbo's last cubic ended, which can be 10¹⁵ units away. The model keeps a `drift` bound and, while it is non-zero, charges each arc a whole turn at the largest radius its widened chord allows; absolute coordinates reset it. No icon in the 31,833 survey changed its bound, so the pessimism costs nothing in practice.

## Tried and ruled out
- Streaming the escape alone: printing a huge list keeps iterating every item after the first failed write (std's `DebugList` skips formatting but the loop goes on). The formatter must count items first.
- Charging a lazily repeated list by its length when it's created: containers sharing a container still multiply items for free (`[x, x, x]`). The walk has to be charged where it happens.
- A load-time check on the parsed template: it can catch constant folding but not `~`, `in` or `==` on variables at render time.
Simulating kurbo's floating-point endpoint exactly: it would mean generating the cubics, which is the work being bounded.

## Watch out
- MiniJinja folds constant expressions at compile time with no fuel: a template can stall or exhaust memory while it loads, before any render limit applies.
- With the patch active, MiniJinja's own fuel check fires before bevy_markup's formatter. Test bevy_markup's guards directly (`check_printable` on values), or they go untested in this repository.
A huge-radius arc's endpoint is only trusted up to `EXACT_SWEEP_SUBDIVISIONS`; anything that moves the current point must keep `drift` in step (absolute coordinates reset it, a close restores the subpath start's).

In the code: src/svg_bounds.rs `path_segments`, `arc_curves`, `EXACT_SWEEP_SUBDIVISIONS`; bug_0254.
