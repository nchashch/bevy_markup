# 31. Windowed build: spec-level viewport culling with height-preserving spacers

| Field | Content |
|---|---|
| ADR | 0031 |
| Title | Windowed build: spec-level viewport culling with height-preserving spacers |
| Date | 2026-10-16 05:05 +0400 |
| Author | GLM 5.3 Flash (Z.ai), via omp |
| Commit | `c71a6cd Update testbench example` (tree also carries an uncommitted, machine-local `[patch.crates-io]` for a vendored bevy/taffy — this ADR exists so that patch is not needed) |
| Status | Proposed |
| Related | ADR 0021 (the skeleton this hangs the height model on), ADR 0029 (same dependency-tracking philosophy, restyles), ADR 0028 (the roadmap's perf items), the `testbench` example |

## Context

Frame time on long documents scales with node count at every stage of the
pipeline: the ECS→taffy tree sync, taffy's layout pass, text measurement,
render extraction — all O(document size), all per frame, for a document
whose visible part is one screen. The project owner's profiling
observation (the `testbench` example exists to produce exactly these
numbers) is that taffy layout is the dominant term. The design goal: a
10,000-block document that scrolls should cost about the same per frame
as a one-screen document.

Two constraints shape any solution:

1. **Taffy has no holes.** Layout is top-down constraint propagation; a
   subtree either participates (and contributes size) or it doesn't
   (and contributes zero). `Display::None` — the only "skip" primitive
   bevy/taffy expose — collapses the subtree's height. An off-screen
   block below the fold still determines the document height (the
   scroll range), so naive culling breaks the scrollbar, the scroll
   position and every ancestor's size.
2. **Block heights are only known after layout.** Deciding what is in
   the viewport requires document-Y positions, which require heights,
   which require layout — the chicken-and-egg at the heart of every
   virtualization design.

What the crate already has that most don't: ADR 0021's skeleton pairs
the fresh DOM against the previous build by content and identifies
unchanged subtrees, and ADR 0029's re-cascade tracks exactly which
subtrees an inherited-style change can reach. A height cache keyed on
the same identity is therefore mostly a new *use* of existing
machinery, not a new parallel structure.

## Decision

Cull at the **spec boundary** in `build.rs`, with **spacers that
preserve the layout contribution**. The windowing is pure crate-side:
no bevy_ui or taffy changes, no fork to carry.

### 1. Cull where the DOM walk builds specs, not on entities

The build walk (DOM → `Item` tree → `NodeSpec`) already visits every
element every rebuild. It learns a document-Y cursor: the running
offset of the block being considered, accumulated from the height
model. A subtree whose full Y-range lies outside the window
`[scroll_offset − margin, scroll_offset + viewport + margin]` emits a
**spacer spec** instead of descending — a leaf spec carrying the
subtree's cached height and no children.

This skips *everything* downstream for the culled part: entity spawn,
the cascade, run construction, text measurement, render extraction,
picking registration. Culling at the entity level (spawn, then mark,
then exclude) pays the spawn, the despawn and the `HtmlUiBuilt` churn
on every window move — and a window moves on every scroll frame.

### 2. The height model hangs off the skeleton

`BlockHeights` — a cache keyed by the skeleton's subtree identity
(ADR 0021's pairing key), storing the last measured content height:

- **Hit** (unchanged subtree + unchanged width + no height-affecting
  style signal): use the cached height; the subtree may be culled.
- **Miss** (content, style, width, font or locale change): the subtree
  materializes normally; its measured height updates the model.

Invalidation rides the signals the skeleton already computes. Width
changes (viewport resize) invalidate everything — this is fundamentally
a **vertical** optimization; horizontal culling is out of scope until
the cross-axis can be resolved independently.

### 3. What drives the window

The window's offset = the root's scroll position (`ScrollPosition` on
the scrolling root) each frame; the margin is a multiple of the
viewport height. The window also opens around **focus**: focus
navigation to an off-window element materializes it first (the
`Focusable` walk consults the model's document-Y map, not the entity
tree — culled elements don't exist to be found by
`HtmlElements`). `scroll_focused_into_view` keeps working because
focus pulls its target into the window before navigation.

### 4. The feedback loop gets an explicit policy

A corrected height shifts content below it, which can change the
window, which materializes different blocks — the virtual-scroll
thrash. Two rules prevent it:

- **Total-height reservation:** the model's sum never shrinks without
  a proven change; corrections only ever *split* reserved space
  downward (a browser's scroll anchoring in miniature).
- **Correction is sticky:** a block's height only updates when that
  block materializes; culled blocks keep their last measured height
  until a signal invalidates them.

### 5. Scope guards

- Opt-in per UI: a component on the `HtmlUi` root (the
  `HtmlModal`/`HtmlNoFocus` precedent), not a global resource —
  windowing changes *when entities exist*, which some UIs (an app
  nesting into slots on `HtmlUiBuilt`, bug_0017's contract) must not
  have sprung on them.
- Subtrees containing `position: fixed` or `absolute` descendants are
  never culled (the demo's fixed ribbon would vanish).
- V1 does not cull inside grid containers (row height depends on the
  tallest cell; per-block heights aren't independent) — a whole grid
  culled or materialized, never half.
- Off in outline mode, in headless test harnesses (`TestUi::settle`
  must see the whole tree), and for `HtmlDebugOutline`.

## Alternatives considered

- **Spawn + marker component; skip marked nodes in subsequent layouts**
  (the first sketch). Rejected: taffy has no way to skip a node that
  stays in the tree — removal is total and height-collapsing (§
  Context 1); and even with spacers, entity-level culling pays
  spawn/despawn/`HtmlUiBuilt` churn per scroll frame. The spec boundary
  dominates it on every axis.
- **Patch bevy_ui/taffy** (lazy layout inside the engine; skip marked
  nodes when bevy syncs the taffy tree). Rejected: a fork of one or
  both engines to build and carry forever — the project just removed a
  vendored-bevy patch experiment for exactly that reason — and the win
  is *smaller* than spec-level culling, which also skips text
  measurement and render extraction; a taffy-level skip cannot.
- **`Display::None` windowing, no spacers** — kept as the measurement
  spike, not the design: breaks the scroll extent, but an afternoon of
  work bounds the actual win before the full machinery is built.
- **App-side virtualization** (the crate exposes the height model; the
  app materializes windows itself). Rejected: the crate's contract is
  "HTML renders as it would in a browser"; the app should not learn
  the DOM's shape to scroll it.
- **Do nothing; rely on taffy's measure caches.** Insufficient: bevy
  still syncs and walks the whole tree each frame, and text
  measurement for below-fold content repeats after every content
  change.

## Consequences

- **Gained** `[INFERENCE, pending the spike's measurements]`: per-frame
  cost roughly flat in document length for scrolling pages; the win
  compounds across taffy, text and render because culling happens
  before all of them.
- **Cost:** a new cache with real invalidation semantics; scroll-extent
  correctness now depends on spacer heights being exact (an estimate
  shows up as a jumping scrollbar); the interaction surface (focus,
  tooltips, `HtmlElements::iter` — which must keep returning
  document-order elements *that exist*) needs its contracts re-checked
  against partial materialization.
- **Contract change:** with windowing enabled, apps must not assume
  every DOM element has an entity. `HtmlElements::iter`/`by_*` return
  what's materialized; signal handlers and custom elements
  (`define_html_element`) run only on materialized spawns. This is why
  the feature is opt-in per UI.
- **Open questions:** the exact anchoring policy under fast content
  churn; whether `absolute`-descendant exclusion is granular enough
  for the demo's ribbon; whether the height cache should be persisted
  across `HtmlUi` rebuilds (it must survive restyles, should survive
  locale swaps, cannot survive template changes); measurement spike
  results before implementation begins.
