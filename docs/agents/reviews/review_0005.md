# review_0005 — Code review 2026-10-15 (the ADR 0027–0029 work)

| field | value |
|---|---|
| ID | review_0005 |
| Date | 2026-10-15 |
| Scope | the 22 commits `4e908e5..e3646ae` since [review_0004](review_0004.md): scoped interaction restyles (ADR 0029), text properties, lists, entity decoding and `overflow: auto` (ADR 0027), and ADR 0028's chunks A–G (text-transform/white-space, background-image, `@font-face`, structural pseudo-classes, sibling combinators, tables, attribute selectors) — ~9 500 lines; the crate at `e3646ae` |
| Method | one session: CI's exact commands (`ci.yml` env: `-D warnings`, debug info off) in a worktree at `e3646ae`; commit messages and diffs read against the ADRs; behaviour probes (temporary tests, since removed) comparing a reused build with a fresh build, matching within one build, and timing at N and 4N |
| Focus | correctness, security, performance, and whether the tests test the code (as asked) |
| Findings | new defects `I01`… (stable ids) |
| Discovery agent | Claude Code session, `claude-opus-5-5` |

Nothing in this review is fixed yet.

## Coverage — read this first

**Probed** ([probe], output quoted): I02–I09, I11; the structural-selector,
sibling, attribute and `:not` matching; content reuse with sibling- and
position-dependent selectors; the style cache's key; the sibling and entity
scaling; `overflow: auto` after an in-place update.

**Read** ([code]): ADRs 0027–0029; the scoped-restyle path
(`collect_node`'s reuse gate, `matches_ignoring_pseudo`, the blocked set);
`pair_node`; the entity decoder; `promote_auto_scroll`; lists; tables;
`text-transform`; background images; the bug_0129–0131 records.

**Not covered:** the `@font-face` note, `line-height`/`letter-spacing`/
`text-align` beyond reading the commit, the golden scenes, and the browser-
oracle vectors beyond their failing cases. No fuzz runs were made.

Evidence levels as before: **[probe]**, **[code]**, **[traced]**.

## Summary

CI is red again: `--features fuzzing` doesn't compile (the fourth time this
class has shipped). One content-reachable panic. Two O(n²) costs, one of them
on every page with many siblings. The new selector features are wrong in
several directions:
- **Matching:** a sibling compound's pseudo-classes, attribute selectors and
  structural parts are ignored, and so are attribute tests on ancestors;
  `:not()` around a pseudo-class never matches.
- **Caching:** the per-build style cache doesn't key on what those selectors
  read, so results depend on document order.
- **Reuse:** across builds, content reuse keeps stale results for
  sibling- and position-dependent rules.

A test written to guard the sibling flip passes with the bug baked in. A
debug `println!` shipped again, and so did a debug log *file* that every app
appends to on every build (I16, found by the mutation audit). On the positive side: the scoped-restyle
design is careful and its core path held under every probe I ran, the
unsupported attribute operators are refused cleanly, and lists, tables and
`text-transform` look correct.

## CI at `e3646ae` (ci.yml's env)

| command | exit |
|---|---|
| `check --lib` | 0 |
| `check --lib --features text-input` | 0 |
| `check --lib --features fuzzing` | **101** (I01) |
| `build --all-targets` | 0 |
| `test` | 0 (367 tests) |
| `test --features text-input` | 0 (373) |
| `check --examples --features hot-reload,dev-tools,text-input` | 0 |
| `test --example hot-reload --features hot-reload` | 0 |
| `test --lib --features fuzzing` | **101** (I01) |
| `doc --no-deps` | 0 |

## New findings

**I01 · high · `--features fuzzing` doesn't compile** — regression from `6efb830` (chunk D) [probe]
- `src/fuzz.rs:196-197` call `styler.style_of(…)` and `styler.box_of(…)`
  without the `&ElementState` argument chunk D added (`error[E0061]`, ×2).
- CI's early "Library with fuzzing" step (added by `ce19a69` for exactly this)
  and the "Fuzz harness unit tests" step fail, and so does every nightly fuzz,
  cache and coverage job. This is the fourth time: G03, H01/bug_0105, and now
  again. The new features (selectors, entities) also went unfuzzed by CI.
- **Fix:** pass `&ElementState::default()` (or a varied one) in the harness.
  Make running CI's whole command list part of "done" for any change that
  touches `Styler`/`cascade` signatures.

**I02 · high · `:nth-child` arithmetic overflows: a stylesheet panics the app** [probe]
- `AnB::matches` (`cascade.rs:966-975`) computes `index - self.b` and
  `remainder.rem_euclid(a)` in `i32`. `:nth-child(n-2147483647)` →
  `attempt to subtract with overflow` at `cascade.rs:972` on a compute-pool
  thread (debug builds: the app aborts). `:nth-child(-n-2147483647)` likewise.
  In release builds it wraps, giving a silent wrong match.
- Reachable from any stylesheet, so high by the project's rule.
- **Fix:** compute in `i64` (index ≤ the child count; `a`, `b` are `i32`), or
  checked arithmetic treating overflow as "no match".

**I03 · high (DoS) · Every element copies its preceding siblings; positions are recounted per child** — `008502c`, `6efb830` [probe]
- `collect_node` (`build.rs:4880-4883`) builds `previous_siblings` as
  `Rc::new(list[..count].to_vec())` per element: a copy of every preceding
  sibling's `Ancestor` (tag, id, classes). That's O(n²) time and memory per
  container (the G05 class), on every page, whether or not the sheet uses a
  sibling combinator.
- `sibling_positions` (`build.rs` `fn sibling_positions`) recomputes
  `element_total` by filtering all of a container's handles inside its
  per-child loop, allocating a `String` per check: O(n²) again.
- **Probe** (release, one container of `<p class="x">` children, `settle()`
  wall time):

  | children | `.hot ~ p` | no sibling rule |
  |---|---|---|
  | 1 000 | 66 ms | 62 ms |
  | 4 000 | 868 ms | 855 ms |

  4× the input costs ~13× the time, with or without a sibling rule. A hostile
  page with tens of thousands of siblings becomes a memory and time DoS, and
  any long list or table pays it per build.
- **Fix:** share one `Rc<[Ancestor]>` per container and give each element an
  end index (a slice view, no copy). Count the elements once.

**I04 · medium (DoS) · The entity decoder is quadratic on `&#` without `;`** — `3b0187f` [probe]
- `terminated_digits` (`template.rs`) does `rest.split(';').next()` before
  checking the digit bound, so each `&#` scans to the next `;`, or to the end
  of the text.
- **Probe** (release): `"&#1".repeat(10_000)` 5.4 ms, `repeat(40_000)` 80.7 ms
  (15× for 4×). The named path is linear (0.16 → 0.63 ms). The decoder runs on
  every text node and attribute of every render, every frame for a churning
  template.
- **Fix:** scan at most `max + 1` bytes for the terminator.

**I05 · medium · Content reuse keeps stale styles for sibling- and position-dependent rules** — `6efb830`, `008502c` (with ADR 0021's pairing) [probe]
- `pair_node` pairs children by position whenever the child count is
  unchanged, and an unchanged child is reused. Whether `+`, `~`,
  `:nth-of-type` or `:last-of-type` match it depends on its siblings, which
  the pairing doesn't consider.
- **Probe** (state A rendered, then changed to B; the reused UI compared with a
  skeleton-less fresh build of B):

  | rule | change | reused vs fresh |
  |---|---|---|
  | `.hot + p` | the preceding div's class `hot` → `cold` | **different**: `p#t bg=#ff0000` kept |
  | `.hot ~ p` | same | **different** |
  | `p:nth-of-type(1)` | the preceding sibling `div` → `p` | **different** |
  | `p:last-of-type` | the following sibling `div` → `p` | **different** |
  | `.sel p` (descendant) | ancestor class change | same (handled) |
  | `[data-mode="dark"] p` | ancestor attribute change | same (both wrong, I07) |
  | `ol` markers | sibling `p` → `li` | same (handled) |

- **Fix:** a sheet using a sibling combinator or a structural pseudo-class
  (it's known at load) must re-collect a changed element's following siblings
  (the scoped-restyle blocked set already has the shape). Or record per spec
  whether its match read siblings or positions, and refuse reuse when any
  sibling of it changed.

**I06 · medium · The style cache's key omits the preceding siblings and the element's `data-*`** — `008502c`, `b69b95e` [probe]
- `HtmlStyles::get` (`cascade.rs:1957-1990`) keys on tag, id, classes,
  pseudo-state, the ancestors' tag/id/classes/pseudo, and the position. The
  sibling combinators read `state.previous_siblings`, and attribute selectors
  read `element.dataset`; neither is in the key.
- **Probe:** two id-less `p`s at the same position in two containers. Only the
  first follows `.hot`, yet both get `.hot + p`'s background; in reversed
  order neither does. With `[data-x="2"]`: in the order `data-x="1"`,
  `data-x="2"` neither element is styled, and in the reverse order both are.
  Alone, the attribute matches correctly.
- Results depend on document order. The commits' tests don't reach this:
  their elements already differ in id, class or position.
- **Fix:** fold the dataset and the preceding siblings' identity (or a hash of
  them, computed once per container) into the key, but only when the sheet has
  rules that read them, so the cache stays effective.

**I07 · medium · Sibling and ancestor compounds ignore everything but tag, id and class** — `008502c`, `b69b95e` [probe]/[code]
- `sibling_parts_match` (`cascade.rs:2102`) compares only tag, ids and
  classes. A sibling compound's pseudo-classes, attribute selectors,
  structural parts and `:not` are ignored, so the rule *over*-matches.
- **Probe:**
  - `.a:hover + .b` colors `.b` before any hover.
  - `[data-m="dark"] + p` matches after a `data-m="light"` sibling.
  - `[data-m="dark"] p` matches under `data-m="light"` and under an ancestor
    with no `data-m`; `Ancestor` carries no dataset.
- That contradicts `008502c`'s "a class-only sibling (`.hot + p`) doesn't [mark
  the element reactive]; a selector testing the SIBLING's pseudo state
  (`.hot:hover + p`) marks the element flip-reactive". The match itself never
  tests the sibling's state.
- A structural pseudo-class in an ancestor position "parses but never
  matches" (documented). An attribute test there silently matches
  *everything*, which is the more dangerous direction, and isn't documented.
- **Fix:** give `Ancestor` the dataset (only for sheets with attribute
  selectors), and match sibling and ancestor compounds with the full compound
  test (pseudo against the sibling's or ancestor's state). Refuse the rule at
  parse time for any part that can't be honoured in that position, as the
  structural limit does.

**I08 · medium · `:not()` around a pseudo-class never matches** — `6efb830` [probe]
- `StructuralPseudo::Not` negates the argument compound's match with its
  pseudo-class test dropped. So `:not(:hover)` and `:not(:focus)` negate a
  compound that matches everything, and never match.
- **Probe:** `.a:not(:hover)` doesn't style an unhovered `.a`, and
  `p:not(:focus)` doesn't style an unfocused `p`. Both are common CSS.
- **Hazard for the fix:** ADR 0029's reactivity check wildcards pseudo tests
  as "match". Inside a `:not` that inverts the answer, so a correct `:not`
  must count as flip-reactive. Otherwise `.row:not(:hover) .label` and
  `.a:not(:hover) + .b` would reuse stale specs on a flip.
- **Fix:** evaluate the argument's pseudo-classes against the element's state;
  treat any `:not` containing a pseudo-class as reactive.

**I09 · medium · `overflow: auto` stops scrolling after any in-place update** — `ae421ec` [probe]
- `promote_auto_scroll` runs only on `Changed<ComputedNode>`. An in-place
  restyle or update rewrites the element's `Node` from its spec (axis
  `Hidden`, the build's clip). If the layout's sizes don't change,
  `ComputedNode` doesn't change, and the promotion never runs again.
- **Probe** (layout harness, a 40 px `overflow-y: auto` panel holding six
  paragraphs): `overflow.y` is `Scroll` after the first build and `Hidden`
  after a class change that restyles it in place.
- **Impact:** a `:hover` rule on such a panel takes the same path, so a hovered
  panel stops scrolling for good.
- Also the G15 class: `&mut node.overflow.x` through `Mut<Node>` marks the
  node changed even when the axis is already right.
- **Fix:** run on `Changed<Node>` too (or `Or<(Changed<ComputedNode>,
  Changed<AutoScroll>, Changed<Node>)>`), and compare before taking `&mut`.
  Better still, let the spec carry the last promoted mode so a restyle doesn't
  reset it.

**I10 · medium · A debug `println!` shipped in library code** — `6efb830` [code]
- `build.rs:1145`: `println!("DECIDEDBG decision=… own=… default=… …")` in
  `build_html_ui`. It runs for every UI on every frame where the default
  stylesheet isn't `Ready`, which is every frame for an app without a default
  sheet. That's stdout spam in every such app.
- `e3646ae` says "a stray STATEDBG eprintln left in build.rs removed"; this one
  is from the same series. Same class as H05 (review_0004).
- **Fix:** remove it, and add the CI grep H05 suggested
  (`println!|eprintln!|dbg!(` in `src/` outside `fuzz.rs` and tests).

**I11 · low · `+n+3` is rejected** [probe] — `AnB::parse` reads the `+` of
`+n+3` as part of `a`, which fails to parse, so the rule is skipped. CSS
allows it. (`2n + 1` with spaces works: lightningcss normalizes it.)

**I16 · high · Every build appends a line to `/tmp/bevy_markup_buildlog.txt`** — `6efb830` [code]/[probe]
- `build.rs:1194`: `build_html_ui` opens `/tmp/bevy_markup_buildlog.txt`
  (`create(true).append(true)`) and writes `entity=… source=… decision=…`
  for every UI on every build. It's debug scaffolding from the same series
  as I10; review_0005 missed it at first, and the mutation audit (below)
  turned it up.
- Effects in every app: an unbounded file (23 MB, 200 808 lines on this
  machine after one session of test runs), a file open and write per UI per
  build (a restyle storm writes one per frame), and a fixed path in a
  world-writable directory. Another local user can pre-create the path as a
  symlink, and the app then appends to whatever it points at. On platforms
  without `/tmp` the open just fails silently.
- **Fix:** remove it. `tests/source_lint.rs` now fails on file writes in
  library code (`OpenOptions`, `File::create`, `fs::write`, `create_dir`, a
  `"/tmp` literal) outside the test modules.

### Tests (do they test the code?)

**I12 · medium · `sibling_flip_restyles_the_following_sibling` passes with I07 baked in** [code]/[probe]
- `tests/html_ui.rs`: the test takes `before = dump()` but never asserts the
  unhovered state. Because `.hot:hover + .after` already matches before the
  hover (I07), `after` is green before *and* after. "The unhover restored the
  start" then compares green with green.
- The commit cites this test as the sibling flip's guard. It can't fail on the
  bug it should catch.
- **Fix:** assert the start state (`#ffffff`) before the flip. Generalize: every
  flip or restyle test should assert the "off" state too.

**I13 · medium · The new selector tests can't reach the cache, reuse or position bugs** [code]
- The structural, sibling and attribute tests and vectors use elements that
  already differ in id, class or position, build once, and check one element
  per shape.
- None compares two elements that differ *only* in what the new selectors read
  (I06), none puts an attribute test in an ancestor or sibling compound (I07),
  and none changes a sibling under content reuse (I05).
- The project's own twin pattern (`spawn_two` + `assert_same_after`) is exactly
  the tool for I05. A quickcheck/proptest model over random small documents ×
  random selector lists (as `cascade_winner_matches_precedence_model` does for
  precedence) would catch I06, I07 and I08 together.

**I14 · low · bug_0129's crate half is guarded only by a flaky test** [code]
- `4b02251` found the real cause in the crate (a full-page style change
  latched in a `Wait` was replayed scoped). Its `build.rs` consumer
  (`pending_full_style` ORed into `full_style`) has no deterministic test.
  `rebuild::tests` cover the latch, and "the flaking test is the
  pipeline-level regression coverage" — a test that failed one run in three.
- **Fix:** a twin test, a stylesheet swap to a broken sheet that resolves
  during a `Wait`, compared with a fresh build.

### Records

**I15 · low · bug_0129, bug_0130/0131 and `4b02251`'s message are inaccurate** [code]
- bug_0129's header says "Severity: low (test flake only)" and "Component:
  tests/…", while its own fix commit says the root cause was the crate's: a
  user-visible stale style after a swap to a broken sheet. It should be
  medium, component `src/build.rs`/`src/rebuild.rs`.
- INDEX lists bug_0130 and bug_0131 as "fixed (uncommitted)"; both were
  committed in `e3646ae`.
- `4b02251` is titled as the bug_0129 fix, but it also carries most of chunk
  D's code (`ElementState`, `AnB`, `StructuralPseudo`, the `:nth-*` parsing).
  `6efb830` ("chunk D") came after, so the history attributes that code to the
  wrong commit.

## Refuted or unconfirmed

- **List markers under content reuse** (predicted from the code: changing a
  sibling to an `li` with the count unchanged): [probe] the reused `li`
  renumbers correctly. Not filed.
- **Unsupported attribute operators over-matching** (`^=`, `*=`, `~=`, the `i`
  flag, non-`data-*` attributes): [probe] each skips its rule. Fine.
- **`2n + 1` with spaces:** works (lightningcss normalizes the selector text).
- **Background-image remote URLs:** go through the shared `image_urls` path,
  which refuses `http`/`https` (`remote_background_images_are_refused`).

## Checked and fine

- The scoped-restyle core: flipped elements and their ancestors re-collect,
  inherited-style comparisons gate the descendants, and pseudo-reactive
  descendants re-cascade (`323bd4e`'s fix). The f9b9123 test matrix compares
  against fresh builds. Within the shapes it supports (no I07/I08 selectors),
  I found no stale result.
- The entity decoder's semantics: code-point validation (NUL, surrogates, past
  U+10FFFF → U+FFFD), the C1 table, one-level decoding, longest-match named
  references, byte slicing on ASCII boundaries.
- Lists (disc/circle/square/decimal/none; no `start` to overflow), tables (no
  spans), `text-transform` (whitespace-neutral, Unicode-aware).

## Triage

1. **CI, the panic and the debug leftovers:** I01, I02, I10, I16 (each a few
   lines).
2. **DoS:** I03 (every page with long sibling lists), then I04.
3. **Selector correctness:** I07 and I08 (matching), I06 (cache), I05 (reuse).
   Fix in that order: the reuse fix (I05) should track what the corrected
   matching reads. Add the model-based property from I13 first, so the fixes
   are checked against a reference.
4. **I09** (`overflow: auto`).
5. **Tests and records:** I12, I13, I14, I15, I11.

## Reproduction tests

Each finding has a test that fails at `e3646ae` for the reason the finding
names, and must pass once it is fixed. Each was run and its failure message
checked against the finding. I14's guard passes, as it should for a fixed
bug, and fails 3/3 with the fix's `build.rs` half removed.

| Finding | Test | Fails at `e3646ae` with |
|---|---|---|
| I01 | CI's `cargo check --lib --features fuzzing` (a feature that doesn't compile can't be a cargo test) | `error[E0061]` ×2 in `src/fuzz.rs` |
| I02 | `cascade::tests::an_b_extreme_offsets_match_without_overflow`; `review_0005::nth_child_extreme_offsets_neither_panic_nor_mismatch` (tests/html_ui.rs) | `attempt to subtract with overflow` (`cascade.rs:972`) |
| I03 | `review_0005::long_sibling_lists_build_linearly` | 800 → 108 ms, 3 200 → 1.23 s (11.3×; bound 8×) |
| I04 | `template::entity_decoding_stays_linear_without_terminators` | numeric: 10k 13 ms, 40k 143 ms (10.7×) |
| I05 | `review_0005::reuse_rechecks_{adjacent_sibling_rules,general_sibling_rules,nth_of_type,last_of_type}` | reused `p#t bg=#ff0000` vs fresh `p#t` |
| I06 | `review_0005::cache_keys_on_preceding_siblings`, `cache_keys_on_the_dataset`; `selector_model` | order-dependent backgrounds |
| I07 | `review_0005::sibling_compounds_test_their_pseudo_state`, `sibling_compounds_test_their_attributes`, `ancestor_compounds_test_their_attributes`; `sibling_flip_restyles_the_following_sibling` (with I12's assertion); `selector_model` | the rule applies where the left compound doesn't match |
| I08 | `review_0005::not_hover_matches_while_not_hovered`, `not_focus_matches_while_not_focused`, `not_hover_sibling_rules_follow_the_flip` (also needs I07 and the reactivity hazard); `selector_model` | `:not(:hover)`/`:not(:focus)` never match |
| I09 | `review_0005::auto_scroll_survives_an_in_place_update`, `auto_scroll_survives_a_hover_restyle` | `overflow.y` `Hidden`, expected `Scroll` |
| I10 | `tests/source_lint.rs` `library_code_has_no_debug_prints` | `src/build.rs:1145: println!(` |
| I16 | `tests/source_lint.rs` `library_code_writes_no_files` | `src/build.rs:1194` (`OpenOptions`), `:1197` (`"/tmp/bevy_markup_buildlog.txt"`) |
| I11 | `cascade::tests::an_b_accepts_a_leading_plus` | `AnB::parse("+n+3")` is `None` |
| I12 | `sibling_flip_restyles_the_following_sibling` now asserts the unhovered start | the start is already green |
| I13 | `tests/selector_model.rs` `selectors_match_the_reference_model` (proptest, 96 random documents × rules vs a reference matcher, within the documented limits) | I06/I07/I08 cases, e.g. `p:not(:hover)`, `[data-m] > p:first-child` |
| I14 | `review_0005::failed_sheet_swap_during_a_wait_updates_unstyled` (guard; passes) | — (fails 3/3 with `full_style_latch` removed) |
| I15 | none (records) | — |

## Mutation audit (do the existing tests test the code?)

`cargo mutants --in-diff` over the 22 commits' `src/` changes, with the
tests CI's mutation job runs plus `signals` (unit, `html_ui`, `stateful`,
`signals`), in a clean worktree at `e3646ae` (statically linked:
`bevy/dynamic_linking` doesn't resolve offline).

| | caught | missed | timeout | unviable |
|---|---|---|---|---|
| at `e3646ae` (522 mutants) | 394 | **80** | 13 | 35 |
| the 93 missed + timeouts, rerun with the gap tests | +53 | **27** | 13 | — |

Timeouts are the scanners turned infinite (`decode_entities`,
`json_from_tera`): caught. One of the 27 (`build.rs:663`, `rem`
letter-spacing) was killed afterwards by a by-hand check.

**Gap tests** (all pass at `e3646ae`; `mutation_gaps` in
`tests/html_ui.rs`, plus two unit tests), each killing mutants that changed
behaviour with no test failing:

| Code | Test |
|---|---|
| `sibling_positions`' per-type counts; the `:nth-of-type`/`:first-of-type`/`:last-of-type` parse arms and match | `of_type_pseudo_classes_count_per_tag` |
| `:empty`'s comment rule | `empty_counts_text_but_not_comments` |
| a sibling selector's leading compounds (`.x h1 + p`), in `matches` and `matches_ignoring_pseudo` | `sibling_selectors_test_the_compounds_before_the_pair` |
| `sibling_parts_match`'s tag test; spaceless `h1+p`/`h1~p` | `sibling_combinators_parse_without_spaces` |
| attribute names with `-`/`_` | `attribute_names_keep_dashes_and_underscores` |
| sibling combinators inside a translation's fragment | `sibling_combinators_match_inside_translations` |
| `letter-spacing` `em`/`rem`, `line-height` `rem` | `em_letter_spacing_and_rem_line_height_resolve` |
| `unset` for `text-align`/`line-height`/`letter-spacing`/`text-transform` | `unset_resets_the_text_properties` |
| `text-transform` with an unsupported extra keyword | `text_transform_with_full_width_is_dropped` |
| `repeat_tiled`, `background_clip` (the vector only used the default box and `no-repeat`) | `background_repeat_axes_and_clip_boxes` |
| the `background` shorthand | `background_shorthand_sets_color_image_and_resets_repeat` |
| background URLs as sheet dependencies (the vector shares `frame.png` with a border-image) | `cascade::tests::background_image_urls_are_sheet_dependencies` |
| `json_from_tera`'s `is_ascii` guard (without it: a panic slicing `\é`) | `template::attribute_json_with_a_backslash_before_non_ascii_is_an_error` |
| the boxed block's and inline runs' `pseudo_reactive` | `boxed_block_under_a_hovered_ancestor_restyles`, `inline_run_under_a_hovered_ancestor_restyles` |
| `full_style`'s own-sheet, default-sheet and `var()` terms | `own_stylesheet_swap_restyles_unchanged_content`, `default_stylesheet_swap_restyles_unchanged_content`, `sheet_var_change_restyles_unchanged_content` |
| reuse happening at all (`document_ready`'s `!`, `full_style`'s `restyle &&`), the useful/strike rule, `pairing_health` | `a_mostly_unchanged_page_reuses_and_stays_useful` (1 of 4 changed → `(true, 0)`), `a_mostly_changed_page_strikes_the_pairing` (2 of 3 → `(false, 1)`) |

No test asserted that the skeleton ever reuses anything: with reuse switched
off entirely (`build.rs:1293`, `delete !`), every test passed. The fresh-twin
tests compare results, and a build that reuses nothing gets the same result.

**Still missed (26), and why:**

- **Redundant or conservative** (no observable change):
  - `build.rs:1343:35` and `:57`: `full_style`'s default-sheet term.
    `RebuildState::decide` sets `pending_full_style` in the same frame for
    every full-page signal, and the consumer ORs the latch in, so the term is
    a duplicate. Checked by hand: `default_stylesheet_swap_to_a_loaded_sheet_restyles`
    passes under both mutants.
  - `build.rs:4801` (`&&` → `||`): more re-collection.
  - `rebuild.rs:149`: `full_style` implies `style`.
  - `cascade.rs:1432`, `:1455`: `matches_ignoring_pseudo` only gates
    re-collection (and `len - 2` equals `len / 2` for 3–4 compounds).
- **Equivalent:**
  - `build.rs:1618` (`>` → `>=`): differs only for an empty page.
  - `build.rs:1654`: equal sources parse alike.
  - `cascade.rs:973` (`/` → `*`): same sign.
  - `cascade.rs:1322:63/:75`: lightningcss re-serializes selectors with
    spaces around combinators, so the spaceless split never runs.
  - `cascade.rs:1619:25`, `:1646:62`: the parse fails later on the same
    inputs.
- **Masked:** `build.rs:2553` and `:2722`, the container's and block's
  `style` replaced by `NodeSpec::new`'s placeholder. The
  inherited-style comparison they feed (`children_inherited_changed`) didn't
  decide any case I could build. Checked by hand:
  `hover_back_to_the_default_color_recascades_children` passes under the
  mutant, because the flipped subtree re-cascades by another path. Either
  the comparison is redundant (simplify it) or it needs a shape I didn't
  find.
- **Dead code:** `cascade.rs:907–915`, 6 mutants. `ElementState`'s hand
  `PartialEq` is never called (derive it or drop it).
- **Covered once fixed:** `build.rs:3412` (the I09 tests); `build.rs:1144`
  (I10's `println!` goes).
- **Open gap:** `build.rs:5446`, the reactivity of an `<img>` run (`.row:hover
  img { width }`). It needs a loaded image in the harness.
- **Harness gap:** `build.rs:2473–2486`, the `text-input` `item_shell` arm.
  The mutation run (and `mutants.yml`) builds without `--features
  text-input`, so those lines are never compiled under test. Add a
  text-input pass to the mutation job.

