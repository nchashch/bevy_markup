# 17. Keyed sub-document updates: skip unchanged sub-trees when content changes

| Field | Content |
|---|---|
| ADR | 0017 |
| Title | Keyed sub-document updates: skip unchanged sub-trees when content changes |
| Date | 2026-10-12 05:47 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp — drafted for the project owner |
| Commit | `f541ee3` Add visibility, CSS variables and transitions; hot-reload and component examples + uncommitted ADR 0017 draft and index row |
| Status | Proposed — designed, not yet implemented |
| Related | 0009 (superseded's "Open" item: per-frame cost scales with the document), 0005, 0012; AGENTS "Limits and next steps" item 6 |

## Context

0009's update model already skips most of the cost of a change: an
identical render leaves `RenderedHtml` untouched (no parse, no cascade, no
diff), and kept elements' components are written only where they differ
(`build.rs` `put`), so a settled UI costs ~0.2–0.4 ms/frame (0009's
"Nothing changing" column: 0.19 ms at 120 elements, 0.44 ms at 600).

What still scales with the document is the *changed* path. A context
change that alters the rendered HTML (the `calc` example's steppers, the
`live` example's party list) runs, per frame:

1. **Render** — Tera renders the whole template (`html.rs`
   `render_templates`);
2. **Parse** — `tl` parses the whole output;
3. **Cascade + spec** — every element is matched against the stylesheet,
   its declared style resolved (`cascade.rs` `HtmlStyles::get`), and a
   `NodeSpec` built (`build.rs` `collect_items` → `item_spec`);
4. **Diff** — `update_children` matches old and new children and writes
   the differences.

Stage 4 is already incremental (0009). Stages 1–3 are O(document) no
matter how little changed. The measured whole-path cost after 0009 is
4.06 ms/frame at 600 elements with one value changing per frame (0009's
table, method recorded there); with the skips in place the unchanged
parts of stages 1–3 are the remaining gap.

Two aggravating cases:

- **Per-frame context UIs**: the `calc` example updates its context every
  frame by design, so every frame pays stages 1–4 in full.
- **State changes**: a `:hover`/`:active` restyle also runs stages 2–4
  over the whole document (`build.rs` `Restyle` re-collects everything),
  though only the affected element's style changed — the extra `font-size`
  proof in the `transitions_animate_a_restyle` test (uncommitted, now
  committed) lands instantly while everything else is re-collected first.

## Decision (proposed)

Keep the pipeline's shape (render → parse → collect → diff) and make
stages 3–4 skip any sub-tree whose inputs are unchanged, by fingerprinting
items per *stable DOM path* and reusing the previous build's `NodeSpec`.

1. **Stable paths, not id-or-position matching.** `collect_node` threads a
   path through the walk (an `Ancestor`-like chain already exists for
   selector combinators): each segment is the fragment (own document or a
   translation) plus, per level, the element's tag and its ordinal among
   same-tag siblings. A path is stable under sibling insertions elsewhere
   in the parent and under re-renders of unrelated parts — the property
   the current unique-`id`-or-position matching only partially provides
   (0009: id-less elements can't move).
2. **Per-path fingerprint and spec cache.** A component on the `HtmlUi`
   (`build.rs`, alongside `RebuildState`) holds `path → (fingerprint,
   NodeSpec)` from the last build. During collection, an item's
   fingerprint is a hash of everything its spec depends on that is cheaper
   than building the spec:
   - the DOM node's own subtree identity: tag, attributes, element
     identity (`is` + dataset), text runs after collapsing, `<img>`
     attributes and any translation fragment;
   - the element's own `Pseudo` state *and its ancestors'* (combinator
     selectors and inherited pseudo styles reach down);
   - the style inputs: stylesheet asset id, locale bundle id, a
     `FontFamilies` change counter (the frame's `fonts_changed` signal,
     made persistent), `RemSize`, and the sheet's `var()` names' resolved
     context values (the same hash `Frame::vars_changed` already
     computes).
   A fingerprint equal to the cached one reuses the cached `NodeSpec`
   verbatim: no cascade, no spec construction, and — in stage 4 — no
   `update_spec` at all (the components already match it).
3. **Diff compares fingerprints, not specs.** `update_children` first
   matches children as today (unique id, else position); for a
   same-identity match whose *fingerprint* also matches, it writes
   nothing and doesn't recurse. The `put` write-only-if-different rule
   stays as the belt to the fingerprint's braces.
4. **Restyles fingerprint too.** A `Restyle` (state change, stylesheet
   swap) runs the same collection; the fingerprint covers pseudo state and
   style inputs, so an unaffected sub-tree is skipped — the
   `:hover`-on-a-big-document case costs only the hovered chain.
5. **Stages 1–2 stay whole-document** (see Alternatives). The skips apply
   from parse onward.

Nothing in the public API changes: no new components beyond the internal
spec-cache one, `HtmlUiBuilt`/`HtmlUiRestyled` fire as before (a restyle
that skipped everything still fired `HtmlUiRestyled` — it did run), and
kept elements keep entities and attached state exactly as 0009 promised.

## Alternatives considered

- **Dependency-tracking render instrumentation** (the other candidate in
  the dirty-flags discussion): render each template once per variable with
  sentinel values to learn which output ranges each variable feeds, then
  re-render/re-parse only those ranges. Rejected: Tera renders opaquely,
  so the sentinel diff is heuristic; control flow (`{% if %}`, `{% for %}`)
  changes *structure*, which remounts sub-trees anyway; the map leans on
  `tl` source positions surviving the render→parse round trip; and it
  only ever helps context changes, not locale, theme, or state changes.
  If keyed updates land and measurement still shows the Tera render
  dominating a per-frame UI, this can be revisited with numbers in hand.
- **Incremental parse (re-parse only changed regions)**: cutting an HTML
  string at changed byte ranges and parsing fragments cannot know it isn't
  cutting a tag in half, and a parser error inside one fragment poisons
  the whole document's structure. Rejected; full re-parse stays (tl parse
  is fast and allocation-free per 0009's measurements of the pipeline).
- **Do nothing, measure first**: the fallback if the fingerprint overhead
  (hashing every node's subtree identity per build) eats the savings. The
  design keeps a kill switch: fingerprints off → the exact 0009 path.
  Hashing should be a small fraction of cascade+spec (a hash over text and
  attributes vs. selector matching + `Node` construction), but that is
  `[INFERENCE]` until measured.
- **Per-variable value invalidation instead of sheet-wide** (fingerprint
  granularity for `var()`): each element's *own* referenced names are only
  known after the cascade we're trying to skip; computing them per element
  costs the work being skipped. First cut is conservative (a change to any
  referenced name invalidates every fingerprint); refinement is an open
  item, not part of this design.

## Consequences

- **Expected gain** `[INFERENCE]`: the changed-path cost approaches
  stage-4-only for small edits (one changed leaf skips its siblings'
  cascade and spec); `:active`/`:hover` restyles on large documents touch
  only the affected chains. The `calc` example's per-frame cost (4.06 ms
  at 600 elements, one value changing per frame) should drop to the diff
  of the changed cards plus N fingerprints; the target is the same order
  as the idle path (0.44 ms), not that number itself.
- **New risk: fingerprint staleness** — a missed input (a cascade input
  not in the fingerprint hash) shows up as a stale UI until the next
  full rebuild. Every input enumerated in Decision 2 must be traced to a
  `Frame` signal or a cascade input; the property harnesses are the net:
  `tests/stateful.rs` and `tests/properties.rs` already model-check
  context, locale, theme, `FontFamilies`, outline and pointer-op
  sequences against reference models, and a *new* property should compare
  fingerprinted builds against fresh builds the way
  `content_update_matches_a_fresh_build` does (0009's net for the same
  risk class).
- **Spec cache memory**: one `NodeSpec` tree per UI, held between builds.
  `NodeSpec` already exists transiently today; keeping it alive doubles
  the peak for the build moment, and the cache must be dropped with the
  UI (it lives in a component on the entity, so despawn takes it).
- **Path stability** becomes a correctness property, like 0009's id
  uniqueness: a duplicated-id bug class was found by a property test back
  then; the equivalent here is a path-collision property (two items with
  the same path must not alias) — added alongside the implementation.
- **Open after landing**: per-element `var()` invalidation (see
  Alternatives); text-run granularity (a span's respawn on style change
  is `update_spec`'s today, unaffected); measuring the `calc` example
  before and after with the 0009 benchmark method.
