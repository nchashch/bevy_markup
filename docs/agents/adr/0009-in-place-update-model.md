# 9. Update in place: skip identical renders, reconcile by key, write only what changed

| Field | Content |
|---|---|
| ADR | 0009 |
| Title | Update in place: skip identical renders, reconcile by key, write only what changed |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — designed and implemented in that omp session for the project owner, who set the order of work; committed by the owner (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `a871c13`, `d033fe1`, `cfdb4b3`, `c8ab00f`, `d2c9b63` (2026-10-05 – 06) |
| Status | Accepted (supersedes the content-rebuild part of 0005) |
| Related | 0002, 0005, 0007; `bug_0016`, `bug_0017`, `bug_0023`; prototype_19 playtests 0026, 0027, 0030, 0037 |

## Context

Every content change despawned and respawned an `HtmlUi`'s whole subtree
(0005). On a game UI this cost:

- **Re-attached state**: anything the app attached (materials, icon images,
  markers) had to be re-attached on every `HtmlUiBuilt`; prototype_19 had
  five such observers.
- **App-side diffing**: apps kept copies of last-written values to avoid
  rebuilds (prototype_19's data frame, nameplates, TUI clock).
- **Rust-side per-frame visuals**: nameplate fade and health width were
  per-frame `Node`/colour writes, with fade bases re-recorded after every
  build and restyle.
- **Lost state**: hover and focus were lost, or restored only by `id`.

Item #3 of the original roadmap (`AGENTS.md` at `30570ae`: "Incremental
rebuilds: keep entities for unchanged elements (key by DOM path)") had been
open since the start.

## Decision

1. **Skip identical renders** (`a871c13`): `render_templates` compares the
   new render with `HtmlDocument::source` and leaves `RenderedHtml`
   untouched when equal, so writing a context every frame is cheap.
2. **Custom elements** (`d033fe1`): `<div is="name" data-…>` runs the app's
   `define_html_element(name, system)` once per spawned element, before
   `HtmlUiBuilt`. App components are declared in the template, not
   re-attached.
3. **Keyed reconciliation** (`cfdb4b3`, `build.rs` `update_children`):
   - matching: an element whose `id` is unique among both the old and the
     new siblings is matched by it, the rest by position;
   - a match with the same identity (tag, `id`, `is` and dataset) is updated
     in place, the rest are spawned or despawned, and siblings are reordered;
   - nested UIs are processed deepest first.

   Restyles use the same path. Inline `style="…"` attributes (cascaded as in
   CSS) and `opacity` (multiplied down the subtree) arrived with it, so
   per-frame visuals became template values.
4. **Write only what changed** (`d2c9b63`, `build.rs` `put`): a kept
   element's components are written only where they differ (`set_if_neq`),
   so layout and text systems skip unchanged elements.

## Alternatives considered

- **CSS custom properties (`var(--x)`) for per-frame values** — planned
  first; dropped because a templated `style` attribute does the same with
  no new cascade feature once updates are in place.
- **Key by DOM path** (the roadmap wording) — positional matching gives the
  same result for id-less elements; `id` keys handle insertions.
- **Key duplicate `id`s by first occurrence** — tried; the second copy was
  respawned on every update, and its fresh `PseudoState` triggered the next
  restyle, an endless loop. Caught by the property test
  `content_update_matches_a_fresh_build`; only ids unique on both sides key
  now.
- **Keep full rebuilds and make apps re-attach** — the previous model; its
  costs are listed in Context.

## Consequences

- **Measured** (throwaway release benchmark: one `HtmlUi`, 1200 frames, one
  templated value changing per frame; times in ms/frame):

  | Elements | Before `put` | After | Nothing changing |
  |---|---|---|---|
  | 120 | 2.09 | 0.97 | 0.19 |
  | 600 | 9.85 | 4.06 | 0.44 |

  What remains is O(document): any change re-renders, re-parses and
  re-styles the whole template.
- **`HtmlUiBuilt` handlers now run again for kept elements.** Code that
  added an observer per build stacked observers: quickstart added 1, then 2,
  then 3 coins per click, and the demo duplicated its content documents
  (bug_0023, fixed in `c8ab00f`). Its docs now require idempotent handlers
  and point to `data-on-*` and `is=` instead.
- prototype_19 lost all its `HtmlUiBuilt`/`HtmlUiRestyled` observers, its
  app-side diffing and its fade bookkeeping (playtests 0026, 0027, 0030).
- Nested UIs survive an ancestor's update while their slot is kept; the
  bug_0016 skip-under-a-rebuilding-ancestor rule is gone.
- Open: id-less elements don't move (an insertion before them replaces those
  after it); per-frame cost still scales with the document, see the table.
