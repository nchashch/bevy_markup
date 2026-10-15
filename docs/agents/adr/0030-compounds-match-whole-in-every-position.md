# 30. Compounds match whole in every position

| Field | Content |
|---|---|
| `ADR` | `0030` |
| `Title` | Compounds match whole in every position |
| `Date` | 2026-10-15 14:05 +0400 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code |
| `Commit` | `e1a5759 Improve tests` + uncommitted review_0005 fixes |
| `Status` | Accepted |
| `Related` | ADR 0021 (skeleton reuse), ADR 0028 (chunks D, E, G), ADR 0029 (scoped restyles), review_0005 I03, I05–I08, review_0003 G05 |

## Context

ADR 0028 added structural pseudo-classes (chunk D), the sibling
combinators (chunk E) and `data-*` attribute selectors (chunk G) to a
matcher built for one shape: the subject compound tested against the
element, and every other compound tested against a slim `Ancestor`
(tag, id, classes, pseudo-state). review_0005 found that the new parts
didn't survive that split:

- **I07:** a sibling compound was matched by `sibling_parts_match` (tag,
  ids, classes only), so `.a:hover + .b` and `[data-m="dark"] + p`
  over-matched. An ancestor carried no dataset, so `[data-m="dark"] p`
  matched under any ancestor. A structural part in an ancestor compound
  never matched (a documented limit).
- **I08:** `:not()` tested its argument without its pseudo-classes, so
  `:not(:hover)` never matched.
- **I06:** the style cache's key omitted the dataset and the preceding
  siblings, so the first element of a shape decided for later ones.
- **I05:** the skeleton (ADR 0021) reused an unchanged child whose
  siblings had changed, keeping stale `+`/`~`/`:nth-of-type` results.
- **I03:** each element copied its preceding siblings
  (`list[..count].to_vec()`), and `sibling_positions` recounted the list
  per child: O(n²) per container on every page.

The review's suggested fix for I07 was to refuse, at parse time, any part
a position can't honour (as the structural limit did).

## Decision

1. **One matcher for every position** (`cascade.rs`). `Compound::test`
   takes a `Subject` (tag, id, classes, dataset, pseudo-state,
   `Position`), built from the element (`Subject::element`) or from an
   `Ancestor` (`Subject::ancestor`). Ancestors and siblings are both
   `Ancestor`s, which now carry `dataset:
   Option<Rc<BTreeMap<..>>>` (shared, `None` when empty — the G05 memory
   rule holds: a chain copy clones an `Rc`) and `position: Position`
   (index, count, type index/count, `:focus-within`, `:empty`). So an
   ancestor's or a sibling's compound tests its attributes, pseudo-state
   and structural parts like the subject's. The structural-in-ancestor
   limit is gone (`li:first-child > span` matches).
2. **Three-valued matching for the reactivity record.** ADR 0029's
   `matches_ignoring_pseudo` wildcards pseudo tests as "could match".
   Inside `:not()` that inverts, so `test` returns `Tri::{No, Maybe,
   Yes}`: a wildcarded pseudo is `Maybe`, `not()` keeps `Maybe`, and AND
   takes the minimum. `:not(:hover)` matches by the real state, and
   `.row:not(:hover) .label` counts as flip-reactive. `has_pseudo` looks
   inside `:not()` too.
3. **Shared sibling lists** (`SiblingList`): one per parent, with a
   rolling hash per prefix. Each element's `previous_siblings` is a
   `Siblings` view (the list plus a count), not a copy.
   `sibling_positions` reads names once and counts once. The inline run
   walk gets the same states (`sibling_states`), so `b + i` matches
   inside a paragraph, and an `<img>` directly in a container keeps its
   real position.
4. **The cache key folds in what the sheet reads** (`SelectorReads`,
   computed at `HtmlStyles::from_sheet`): the element's dataset when any
   attribute selector exists, the preceding siblings' prefix hash when a
   sibling combinator exists, and the ancestors' dataset/position when
   attribute selectors or non-subject structural parts exist. A sheet
   without them keys exactly as before.
5. **Reuse respects siblings** (`build.rs` `pair_node`,
   `pair_trees_into`): when a sibling list changes shape (length, node
   kind, or an element's tag/attributes — `siblings_identical`) and the
   sheet is sibling-dependent (`SelectorReads::sibling_dependent`), every
   child of that list leaves the clean set (`unclean_siblings`). Each
   re-collects, and its descendants still reuse where their inherited
   style is unchanged. If an ancestor compound reads a position, the
   whole subtrees go.
6. Two parser fixes found on the way: structural pseudo-classes now add
   class-level specificity (`:not()` adds its argument's), and the
   combinator scan (`compound_end`) skips parentheses, brackets and
   quotes. lightningcss serializes `:nth-child(-n+3)` and `3n+1` as
   written, and the old scan split them at the `+`, dropping the rule.

## Alternatives considered

- **Refuse unsupported parts at parse time** (review_0005's suggestion).
  It would be safe, but it silently drops rules authors expect to work,
  and computing positions turned out to cost the same as the walk
  already did. Rejected for the full matcher.
- **Carry the dataset only when the sheet has attribute selectors.** That
  would save one `Rc` allocation per element with `data-*` attributes.
  Not done: the dataset is cloned once per element (not per chain copy),
  and the gate would couple the walk to the sheet. [INFERENCE] the cost
  is small next to `element_of`'s own allocations; not measured.
- **Record per spec whether its match read siblings, and refuse reuse
  per element** (the review's second option for I05). It would be more
  precise, but it needs match provenance threaded out of the cache.
  Rejected for the sheet-level flag plus the list-shape check, which
  costs nothing when the sheet has no sibling-dependent selectors.

## Consequences

- review_0005's reproduction tests for I03 and I05–I08 pass, and so does
  `tests/selector_model.rs`, now generating every part in every compound
  position (1 000 cases clean). An ablation (structural parts ignored in
  ancestor/sibling positions) fails the model at once.
- A sibling-dependent sheet loses content reuse for every sibling of a
  changed list element: a live list striped with `:nth-child` re-collects
  its rows (not their subtrees) when a row is inserted or retagged.
- Still open: sibling combinators away from the subject (`.a + .b p`) skip
  the rule. `:has()` and `:is()` aren't supported.
