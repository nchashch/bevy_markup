# 29. Restyle reuse: dependency-tracked re-cascade instead of full restyles

| Field | Content |
|---|---|
| ADR | 0029 |
| Title | Restyle reuse: dependency-tracked re-cascade instead of full restyles |
| Date | 2026-10-15 06:01 +0400 |
| Author | GLM 5.3 Flash (Z.ai), via omp |
| Commit | `e2081e8 Release 0.7.0` (uncommitted at writing) |
| Status | Accepted (project owner, 2026-10-15: "8 ms → 0.7 ms is worth it, even if it adds complexity") |
| Related | ADR 0021 (the skeleton this extends), ADR 0028 (Tier-4 animation item interacts with this), bug_0102/0103's guards (the reuse tests this must keep passing) |

## Context

ADR 0021's skeleton skips the cascade, the spec construction and the diff
visit for subtrees whose *content* is unchanged — but only for content
changes. Any style-side signal sets `RebuildState::style_pending`
(`rebuild.rs:99`, set in `:133`), and the build gate disables the skeleton
outright (`build.rs:1077-1082`):

```rust
let skeleton_eligible = SKELETON
    && matches!(decision, Decision::Build(_))
    && outline.is_none()
    && removed_outlines...
    && rendered.is_changed()
    && !style_pending;
```

Every style-side change therefore re-cascades and re-specs the *whole UI*.
Measured on the testbench (release, hotpath, 2000 rows / 6000 elements):
`collect_items` averages 7.7–8.3 ms per style-only build — the cascade
match (`HtmlStyles::get_with` ~340 ns) plus `Styler::style_of` (~650 ns)
plus spec construction, per element, for elements whose computed style
usually comes out *identical*.

The triggers make this worse than it sounds. `Changed<PseudoState>` feeds
`style_pending` (`build.rs:856`, `:902`), so a single hover, press or
focus flip on a large page costs a full-page re-cascade — the commonest
interaction in a UI, priced at O(page). The same holds for a `var()` that
only some declarations reference, and for a stylesheet *image* arriving
late (a restyle that changes nothing but a frame's picture).

The skeleton is disabled because the cached specs' styles would be stale —
a content-order problem, not a fundamental one: on a style-only build the
*document* is unchanged, so the pairing itself is trivial.

## Decision

Let reuse survive style-only changes, with **conservative dependency
tracking** deciding which elements re-cascade:

1. **Pair on style-pending.** Drop `!style_pending` from the gate. On a
   pure style build the fresh document equals the previous one, so every
   subtree pairs; a style signal that arrives in the same frame as a
   content change already pairs against the old document today and keeps
   doing so.
2. **Dependency records per paired element**, built when its spec is
   constructed (they are pure functions of the match): the `var()` names
   its computed style consumed (the sheet's tracked-name plumbing from
   G30 already collects these), and the pseudo-classes its matched rules
   reference (own and ancestors').
3. **Scoped invalidation.** A style-side change re-cascades only the
   elements whose records reference it:
   - a `PseudoState` change of X → X and X's subtree (no `:has`, so
     nothing else can be affected; ancestor pseudo matching only makes
     descendants of a hovered element match — never ancestors of it);
   - a `var(--a)` change → elements consuming `--a`, plus subtrees below
     any element whose *inherited* declarations (color, font-*) consume
     it;
   - a stylesheet swap/reload or `FontFamilies` change → everything
     (unchanged behaviour; these are rare and structural).
   Under-approximation is the one failure mode that matters, so the
   records are built conservatively: an unresolvable/failed match
   defaults the element to "always re-cascade".
4. **Style equality short-circuits the diff.** An element that *does*
   re-cascade computes its style, and only constructs a fresh spec if it
   differs from the cached one (the computed `Style` gets a cheap
   `PartialEq`); equal styles re-emit the cached `Arc<NodeSpec>` and the
   `put`/`set_if_neq` diff path touches nothing — change ticks survive,
   as content reuse already promises.
5. **Restyle events stay per the current contract:** a restyle that
   spawns nothing reports `HtmlUiRestyled` even when most elements'
   styles were unchanged (the event is per-UI, not per-element —
   unchanged from today).

### Measured expectation (correcting the earlier claim)

The motivating quote ("8 ms → 0.7 ms") is right for **scoped** restyles
and wrong for whole-page recolors — recorded here so the follow-up work
isn't judged against the wrong number:

| change | today (`collect_items`, 2000 rows) | after |
|---|---|---|
| hover/active/focus flip anywhere | ~8 ms (full re-cascade) | **O(hovered subtree)** — µs–sub-ms for typical widgets |
| `var()` referenced by some elements | ~8 ms | O(affected subtrees) |
| whole-page recolor (testbench restyle-storm) | ~8 ms | **~8 ms** (every element genuinely restyles; the cascade cannot be skipped) |
| stylesheet swap/reload, fonts | full | full (unchanged) |

Whole-page recolors are also the case where Bevy's own re-layout dominates
the frame (ADR 0028's context), so the honest headline win is: *hovering a
large UI stops costing the page*.

## Alternatives considered

- **Cache the cascade by (sheet revision, element, pseudo) key** — memoize
  whole cascade results across builds: sound for the sheet's own
  revisions, but pseudo-state and inherited values make the key fragile;
  the per-element match cache already does this within a build. The
  dependency records do the same job across builds with the *right*
  granularity.
- **Do nothing; wait for upstream incremental taffy layout**
  (bevy#22909): rejected as a substitute — upstream fixes *layout*
  invalidation, not bevy_markup's own O(n) re-cascade; the two compound
  and are independent.
- **Reuse with full re-diff** (pair, re-cascade all, but let
  `set_if_neq` no-op unchanged elements): keeps the 8 ms cascade — the
  diff was never the expensive part.

## Consequences

- **Win:** the commonest style-side changes (interaction pseudo-states,
  scoped `var()`s) drop from O(page) to O(affected) — the restyle-storm
  scenario's *moving highlight* and every hover in the demo/demo-scale
  UIs stop paying the page; the all-dirty and full-recolor cases are
  unchanged.
- **Complexity (accepted):** dependency records live beside the skeleton
  (`Skeleton` gains a per-element map; memory O(elements × consumed
  vars), bounded by the same `MAX_DOM_DEPTH` walks); two invalidation
  paths (scope table + equality short-circuit) must agree; the
  under-approximation guard ("failed match ⇒ always re-cascade") is
  load-bearing and needs an explicit unit test.
- **Guards that must keep passing:** `restyle_matches_a_fresh_build` and
  `content_update_matches_a_fresh_build` (properties) are exactly the
  soundness nets for this change; the stateful machine already sweeps
  theme swaps, font swaps and pointer state over one long-lived UI, and
  gains a scoped-`var()` op. The existing reuse guards
  (`wait_swallows_two_changes_then_reuses_currently`, the R04/R05 gates)
  pin the content path this touches.
- **Migration:** none — internal only; `style_pending`'s public effect
  (a pending style change survives a `Wait` and applies on resolution)
  is unchanged.
- **Follow-up:** ADR 0028's Tier 4 (animations) should land *after*
  this: an animation writing styles per frame is cheap only under
  dependency-tracked restyles.
