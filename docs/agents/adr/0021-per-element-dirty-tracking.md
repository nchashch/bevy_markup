# 21. Per-element dirty tracking: dependency maps at load, slot-local updates per frame

| Field | Content |
|---|---|
| ADR | 0021 |
| Title | Per-element dirty tracking: dependency maps at load, slot-local updates per frame |
| Date | 2026-10-12 11:24 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp — drafted for the project owner |
| Commit | `f05290d` Separate Tera statements in Markdown templates at load |
| Status | Proposed |
| Related | 0017 (its fingerprint variant was rejected on measurement — this is the successor design), 0020 (the load-time conversion this builds on), 0009 (identical-render skip, in-place updates), 0018/0019 (measurement); the "Low-Hanging / Dirty-Flag Rebuilds" items in the session record |

## Context

What still scales with the document is the *changed* path. 0009 already
skips an identical render entirely (no parse, no cascade, no diff; the
idle frame costs 194.8 µs at 600 sections, criterion) and writes kept
elements' components only where they differ (`build.rs` `put`). But a
context change that alters the rendered HTML re-runs the whole pipeline,
no matter how little changed:

1. **Render** — Tera renders the whole template (`html.rs`
   `render_templates`);
2. **Parse** — `tl` parses the whole output;
3. **Collect + spec** — every element is walked (`build.rs`
   `collect_items`), its style resolved, and a `NodeSpec` built;
4. **Diff** — `update_children` matches old and new specs and writes
   what differs.

Current criterion numbers (warm-up 0.5 s, measurement 1 s, this
machine, `f05290d`): `frame/changing/600` 2.02 ms, `frame/localized/600`
2.55 ms, `frame/markdown/600` 8.25 ms, `frame/rebuilding/600` 9.87 ms
against an idle floor of 194.8 µs. Stage instrumentation (timed spans
inside `build_html_ui`, temporary, this session) put the markdown/600
correct-DOM build at ~4.2 ms total: collect ≈ 3.4 ms, spec construction
≈ 0.42 ms, diff ≈ 0.38 ms — the collect walk dominates the library-side
cost, and much of it is fixed per-element work (the cascade style-cache
key clones `tag`/`id`/`classes` Strings per element per build; measured
earlier this session at ~3 allocations per element per build, ~22 k
allocations/frame at 7203 tags). The aggravating case is unchanged since
0017: per-frame context UIs (`calc`, `live`) pay stages 1–4 in full
every frame.

The fine-grained-reactivity frameworks (SolidJS, Leptos) solved this
class by compiling templates to real DOM nodes and updating only the
bindings whose inputs changed — no whole-document re-derivation. The
question: can bevy_markup get the same shape for its template-driven
UI, and can it afford it?

**ADR 0017 is the caution.** Its fingerprint design — per-frame hashing
of every item's subtree inputs, reusing cached `NodeSpec`s on a match —
was implemented and **rejected on measurement** (changing 600: +28%,
hashing alone +0.46 ms against −0.08 ms of reuse). Its stated reasons:
the cascade was already nearly free (per-element style cache), the diff
was already incremental, and per-frame O(subtree) hashing plus a deep
`Item` clone per hit cost more than the skip saved. Any successor must
answer those numbers, not ignore them.

## Decision (proposed)

Replace 0017's *per-frame fingerprinting* with *load-time dependency
extraction*: the mapping from context variables to template slots is
computed once when the template loads (ADR 0020 already parses the
converted template at load); per frame, only slots whose variables
changed are re-derived. The per-frame skip cost drops from "hash every
element's subtree" to "look up which slots read a changed key".

1. **Dependency extraction at load.** After the loader's conversion
   (`md::to_html_template`) and Tera compilation, walk the converted
   source and record, per tl node position in the converted output:
   - **value slots** — a text run's Tera interpolations
     (`md.rs` `tera_runs` already isolates them per node) and templated
     attributes (`style="calc(… {{ sidebar }} …)"`, `data-l10n-args`),
     each mapped to the set of context keys its expression reads;
   - **structural slots** — `{% for %}`/`{% if %}` substreams, mapped to
     the keys their collection/condition reads. A structural slot's
     body is a keyed sub-document: if its inputs changed, the existing
     `update_children` reconcile runs over just that subtree.
   Expressions' variable sets come from Tera's parsed AST. Tera 2.4
   (`src/parsing/ast.rs`) does not export it publicly (checked this
   session) — a `[patch.crates-io]` fork export, the same mechanism the
   fluent-syntax fork uses (UPSTREAM U1/U2), or a weaker self-written
   reference scanner that misses filter/function internals. A missed
   variable is a correctness bug (below), so the fork export is the
   default choice; the scanner is the fallback if the fork is refused
   upstream.
2. **Per-key change detection per frame.** Before rendering, diff the
   context's values against the previous frame's per key (`tera::Context`
   iteration; keys are few, values small). A key set with no changes
   takes 0009's identical-render path unchanged. Changed keys mark their
   slots dirty via the load-time map — a bitmap walk over the slot list,
   O(slots), no hashing of element subtrees.
3. **Slot-local update, skeleton reuse.** The build keeps the previous
   frame's item/spec skeleton. Non-dirty items are reused verbatim —
   no cascade, no spec construction, and `update_children` writes
   nothing for them (it already compares and skips via `put`). Dirty
   value slots are re-collected and updated in place
   (`update_spec`'s existing path; the restyle machinery proves a
   single element can be rewritten without touching siblings). Dirty
   structural slots re-run the keyed reconcile over their subtree.
   Stages 1–2 stay whole-document: the Tera render and `tl` parse
   measured 43 µs + 46 µs at markdown/600 this session — not worth
   instrumenting (0017's Conclusion, unchanged).
4. **Phased, with a measurement gate.**
   - **A** — per-key change detection + "nothing changed → skip" fast
     path (extends the existing identical-render check). Small; no new
     semantics.
   - **B** — the dependency map + slot-local collect (1–3 above).
     Measured against 0018/0019 suites before any further work; 0017's
     kill-switch discipline applies (the map off → the exact current
     path).
   - **C** — full inversion (dirty slots drive direct entity updates,
     the spec tree becomes authoritative) is **gated**: 0017's outcome
     showed the clone-heavy reuse path loses, and clone-free reuse means
     `Rc`/ownership threading through the whole build path — a large
     redesign. C is attempted only if B's measured win makes it worth
     the redesign, with numbers in the record.
5. **Freshness contract, made testable.** A dirty-path build must equal
   a fresh build for the same inputs — the metamorphic property
   `content_update_matches_a_fresh_build` (0009) extended to
   random *key-change sequences* over random documents: after each key
   change, the built world must equal a fresh build. This is the net for
   the new bug class: a missed dependency edge renders stale UI until
   the next full rebuild (locale/stylesheet/font changes already force
   full rebuilds and are unaffected).

The public API does not change. No new components; the map and skeleton
live beside `RebuildState` on the `HtmlUi` entity. `HtmlUiBuilt` /
`HtmlUiRestyled` fire as today; kept elements keep entities and attached
state exactly as 0009 promised.

## Alternatives considered

- **0017's per-frame fingerprints** — implemented, measured, rejected
  (+28% on the changing path at 600 elements; hashing +0.46 ms, reuse
  −0.08 ms). This design differs where that one failed: the dependency
  work moves to load (one-time), so the per-frame skip is a bitmap
  lookup rather than O(subtree) hashing; and the target scale is
  different — 0017 measured 600-element plain documents (changing =
  1.93 ms, fixed costs dominant), while markdown/600 is ~4200 entities
  at 8.25 ms with collect+specs+diff ≈ 4.2 ms to skip. Whether the win
  survives the bitmap and skeleton overhead is exactly what the B-phase
  gate measures `[INFERENCE]`.
- **Static/dynamic template split** (Million.js, arXiv 2202.08409):
  partition the converted template into static chunks reused verbatim
  and dynamic holes re-derived per frame. This is the dependency map
  with all-empty variable sets — a special case of Decision 1. It is
  the fallback if the full map is rejected by measurement: less
  machinery, no Tera AST fork needed (a static/dynamic classification
  comes from the same `tera_runs` scan without expression parsing), and
  it still removes the static fraction from stages 3–4.
- **Render instrumentation / sentinel diffing** (0017's Alternative 1)
  — still rejected, same reasons: opaque Tera renders, structural
  changes remount subtrees anyway.
- **Incremental parse** (0017's Alternative 2) — still rejected:
  fragment parsing cannot know it isn't cutting a tag in half.
- **Do nothing** — the fallback if B fails its gate. 0017's lesson
  stands: the current changed path is already within ~10× of idle for
  plain documents, and the machinery is proven.

## Consequences

- **Expected gain** `[INFERENCE]`, with the fan-out honesty the design
  depends on: the savings scale with how few slots read a changed key.
  A low-fan-out change (one text field, one chip) drops the changed
  path toward idle + the dirty slots' cost; a high-fan-out change (a
  variable in every element's `style`, as the `calc` example's widths)
  saves almost nothing — every element is dirty. The markdown workload
  (mostly static prose, few dynamic values) is the best case; the calc
  example is the worst case. The B gate must report both.
- **Correctness surface**: the stale-UI bug class (0017 named it; this
  design widens it to template structure). The mitigation is the
  Decision 5 property, the existing stateful machine (extended with
  key-change ops), and full rebuilds on locale/stylesheet/font changes
  as dirty-path escape hatches.
- **The Tera AST fork** adds a vendored dependency with upstream-tracking
  obligations (UPSTREAM.md), like the fluent-syntax fork.
- **Memory**: the dependency map and the cached skeleton are held per UI
  between builds (0017's consequence, same shape). Despawn takes both.
- **Open after B**: per-element `var()` invalidation (0017's open item —
  the sheet-level `Frame::vars_changed` hash is the conservative
  version this design inherits); `data-l10n-args` changes as slots;
  includes/extends' contribution to the map (the loader merges
  references, so extraction runs on the merged set).
