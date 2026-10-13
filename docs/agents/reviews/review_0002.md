# review_0002 — Code review 2026-10-13 (follow-up)

| field | value |
|---|---|
| ID | review_0002 |
| Date | 2026-10-13 |
| Scope | whole crate at `a07a12a` (after the six `fix/*` merges) |
| Method | six parallel slices, re-running the R findings' scenarios; revert-tested each fix's guard |
| Findings | verdicts on `R01`–`R80`; new defects `F01`–`F43` (stable ids) |
| Discovery agent | omp session, `anthropic/claude-opus-5-5:high` |

A re-review at `a07a12a` (main after the six `fix/*` merges, the integration
docs `0089dae` and the regression tests `a07a12a`). It does two things:

1. It checks every finding of the [first review](review_0001.md) (`R01`–`R80`,
   at `b224619`) against HEAD: is it really fixed, and does the cited
   regression test fail when the fix is reverted?
2. It records new defects (`F01`…), including regressions introduced by the
   fix commits.

The IDs (`F01`…) are stable references. Nothing in this review has been
fixed yet. Bug files (`docs/agents/bugs/`) are filed when the fixes land, as
before.

## How the review was run

Six parallel slices (build/reuse, cascade, Markdown/Fluent/loaders,
signals/focus/anchors, examples/scripts/CI, records/tests), plus the
integrator's own checks. Each slice read the original finding, its bug file,
the fix diff (`git log -p b224619..HEAD`) and the current code, and re-ran the
original scenario where it could. To test whether a regression test guards a
fix, the fix was reverted in a scratch copy and the test re-run. Evidence
levels are the same as in the first review:

- **[probe]**: run in a scratch copy of the crate (since deleted). Each copy
  had its own `CARGO_TARGET_DIR`; the first review's shared-`target/`
  contamination did not recur. The quoted output is the observed output.
- **[code]**: read the exact lines at HEAD.
- **[traced]**: followed the code path by reading; not executed.

The integrator reproduced F01, F02 and F03 directly and read the code for
F04 (R01), F07 (R55), F17 (R63) and F21. When two slices disagreed (R01: "fixed" [code]
vs "not fixed" [probe]), the probe wins, confirmed here by reading
`tests/properties.rs:756`.

Discovery agent: model `anthropic/claude-opus-5-5:high`. `cargo clippy
--all-targets --features text-input` shows the same hygiene lints as before
plus two `unused variable` warnings, which are F01.

## Corrections to the fix session's own claims

The fix session reported things it had not actually verified. They are
recorded here so nobody relies on them:

- It reported "`cargo test --features text-input`: green". That was false:
  the run printed 5 `ok` suites out of 13 and exited 1, because the `html_ui`
  binary aborts with a stack overflow (F02). A count was misread as a pass.
- It reported "warning-free" for `cargo build --all-targets`. That holds only
  without `-D warnings`; CI's `RUSTFLAGS` turns two warnings into errors (F01).
- R55 was "refined" post-merge in `97e16bc` by renaming the unused `extent`
  to `_extent`. That silenced the compiler's only hint that the clamp ignores
  the overlay's size (F07).
- The scripted conflict splice in `tests/html_ui.rs` (`97e16bc`) cut the first
  line of `transitive_var_change_restyles`'s doc comment (see section I).

## Scoreboard

| verdict | count | IDs |
|---|---|---|
| fixed (cited test fails on revert, or no test needed) | 44 | the rest |
| fixed, but untested or the test doesn't guard it | 13 | R04 R07 R47 R50 R56 R58 R59 R61 R62 R64 R65 R67 R68 |
| partial | 16 | R12 R21 R22 R26 R27 R35 R38 R39 R42 R43 R44 R48 R54 R66 R69 R80 |
| not fixed | 4 | R01 R34 R55 R79 |
| regressed (the fix made it worse) | 3 | R19 R63 R72 |

So 23 of 80 findings are still open in whole or in part, and the fix commits
introduced 10 new regressions (marked *regression from …* below; three of
them are the regressed R19/R63/R72). CI is red
at HEAD (F01, F02).

## Verification of R01–R80

### A–C. Safety net, skeleton reuse, build paths

| ID | verdict | evidence | note |
|---|---|---|---|
| R01 | **not fixed** | [probe]+[code] | `skeleton_reuse_matches_a_fresh_build` keeps the skeleton on the reused side now, but every op re-inserts `HtmlStylesheet` (properties.rs:756). Each op therefore counts as a style change and turns reuse off: 334 logged builds, all with `style_pending=true`, **0 reuse passes**. The sheet-swap-with-content op is the `_` arm of `0..5u8`, so it is never generated. Ops are capped at 11, below the gate's retry, and there is no hover op. The property passes with the R02 fix reverted and with the R03 fix reverted. A strengthened variant (insert the sheet only when it changes, `0..6u8`, 1..40 ops) gets 66 reuse passes, passes at HEAD and fails with R02 reverted. That is F04. |
| R02 | fixed | [probe] | build.rs:3583-3586. `reused_walkthrough_siblings_keep_their_content` fails on revert. |
| R03 | fixed | [probe] | `!style_pending` gate. `stylesheet_swap_with_a_content_change_restyles_reused_subtrees` and `change_while_a_sheet_loads_restyles_when_it_fails` fail on revert. |
| R04 | fixed, untested | [probe] | A hand-built repro (9 all-dirty rows, then `b`) shows `"B: z"` vs fresh `"B: y"` on revert and is correct at HEAD. The cited `gate_skipped_builds_stay_sound` **passes with the fix reverted**, because its restyle step re-syncs the skeleton first. |
| R05 | fixed | [probe] | The outline variant is guarded (`render_change_during_outline_frames_stays_in_step` fails on revert). The original Wait variant (`a,b` → unloaded sheet + `b,a` → `a,c`) is correct at HEAD but has no test. |
| R06 | fixed | [probe] | `refresh_cached_handles`. `hover_applies_inside_reused_subtrees` fails on revert. Edge case: F29. |
| R07 | fixed, untested | [probe] | The gate now scores a dynamic `<p>` plus a static container as useful (`reused=1 total=2`). The only cited coverage is the property, which never reuses (R01). |
| R08 | fixed | [code] | The real `type` attribute is read (build.rs:3630-3637). Case sensitivity: F28. |
| R09 | fixed | [code] | Value, typed-text survival and restyle color are tested (`kept_input_follows_value_and_styles`). A changed `name` (claimed in CHANGELOG) and `EmSize` are not tested. |
| R10 | fixed (retraction) | [code] | The docs no longer claim `maxlength`/`placeholder`. The retraction's record (ADR 0022) rests on a false premise: F19. |
| R11 | fixed | [code] | `text_input_keeps_its_grid_placement`. |
| R12 | **partial** | [probe] | Bare nested `<div>`s are capped (20 000 levels settle in 38 ms). Inline nesting is still unbounded (F05). With `text-input` the cap itself is too deep (F02). The outline renderer is unbounded too (F37). |
| R13 | fixed | [code] | `root_calc_applies_and_stops`. The new camera choice is inverted: F10. |
| R14 | fixed | [code] | `restyle_that_respawns_spans_reports_a_build`. |
| R15 | fixed | [code] | `item_spec` borrows `children` (build.rs:1947-1950). |
| R16 | fixed | [code] | `nested_set` HashSet (build.rs:2434-2442). |
| R17 | fixed | [code] | No restyle is queued; Bevy re-resolves `Val::Rem` (bevy_ui 0.20 layout/mod.rs:89-100). `rem_size_sync_does_not_restyle`. |
| R18 | fixed | [code] | `layout_fields` (common/mod.rs:767-845) prints display, sizes, flex, grid, `vis=`, `focus`, `on=`. `data-with` payloads are still not printed. |

### D. Cascade

| ID | verdict | evidence | note |
|---|---|---|---|
| R19 | **regressed** | [probe] | The exponential shapes are gone: the self-referencing case takes 0.11 ms, the 30-level doubling chain 7.9 ms (dropped by the 64 KiB budget). But `e992bf1` removed the `depth > 32` guard, and a plain reference chain now overflows the stack: F06. |
| R20 | fixed | [probe] | `pseudo_only_compounds_match_any_element` fails on revert. |
| R21 | **partial** | [probe] | The lower rule no longer shows through. But `initial` on an inherited property *inherits* (build.rs:540 `unwrap_or(inherited)`), keywords don't clear an earlier `calc()`, and keywords over-reset: F15. |
| R22 | **partial** | [probe] | The transitive case is fixed. Inline `style="color: var(--brand)"` never restyles: `settle()` reports "no rebuild" and the text stays `#ff0000`. Unknown-property refs are skipped by `var_names` (F14). |
| R23 | fixed | [code]+[probe] | `restore_from_undoes_fixed_and_implied_tracks`. |
| R24 | fixed | [probe] | `negative_named_lines_count_from_the_end`. |
| R25 | fixed | [probe] | `repeat_names_register`. A merged boundary registers its name twice: F39. |
| R26 | **partial** | [probe] | `--x: url()` in the sheet now loads. Inline `style="border-image: url(…)"` and context-`var()` URLs still draw nothing (`Stylesheet::image` only knows the sheet's URLs). The fix also lets one bad custom-property URL fail the sheet: F11. |
| R27 | **partial** | [probe] | The cited sites are fixed (`unsupported_values_dont_erase_earlier_values`). Six more `= helper(value)` sites still erase the earlier value: `visibility` (:2002), `outline-offset` (:2382), `pointer-events` (:2395), `border-radius` (:2346ff), `outline-width` (:2371), `grid-column` start (:2272). |
| R28 | fixed | [probe] | `later_grid_declarations_reset_names`. Keyword-side over-reset: F15. |
| R29 | fixed | [code] | Computed at load (style.rs:286), read per frame (build.rs:428). The fuzz stub's empty `var_names` means `var_names` itself is never fuzzed (F41). |
| R30 | fixed | [probe] | A differential run of 200 000 random cases agrees with the old matcher; the memo key `(index, offset)` is exact. `descendant_combinator_matches_at_any_depth` fails on revert, so bug_0054's "none dedicated" is stale. Worst case is still k·n². |
| R31 | fixed | [probe] | `var_resolves_in_unknown_properties`. Context changes to those vars aren't tracked: F14. |

### E. Markdown / Fluent / templates / loaders

| ID | verdict | evidence | note |
|---|---|---|---|
| R32 | fixed | [probe] | Leading/trailing placeables and multi-line patterns match `md::to_html`. The proptest's `trail` flag is unused, so the trailing case is never generated (F01). |
| R33 | fixed | [probe] | `O'Brien & Co` is escaped once. `string_args_escape_once_through_the_cache`. |
| R34 | **not fixed** | [probe] | `` a `{{ x }}` span `` renders `a VALUE span` end to end. Fenced `let s = "{%"; // %}` fails the load. The fix only stopped `separate_statements` mangling code; code text still reaches Tera. `tera_templates_survive_the_conversion` asserts the *converted* string (`<code>{{ x }}</code>`), which Tera then executes, so the test pins the bug. F08. |
| R35 | **partial** | [probe] | Every original scenario renders right (`src`, `id`, heading, list item, table cell). The own-line test reads the wrong line (F33), fences break separation (F12), and comments hang the loader (F03). |
| R36 | fixed | [code] | template.rs:407-411. `markdown_conversion_is_per_file`. |
| R37 | fixed | [probe] | Term/message references go per-value; output is equal. |
| R38 | **partial** | [probe] | Only a marker directly after `](` is detected. `[docs](https://x.y/{ $p })`, `<{ $url }>` and `[docs]( { $url } )` still cache and lose the link; the per-value path makes `<a>`. |
| R39 | **partial** | [probe] | The miss → per-value fallback works. Duplicate ids keep the first *cacheable* pattern, not the first. `m = {$n -> *[other] **{$n}**}` then `m = { $n }` caches the second and shows `**5**`. A resource with any Junk is skipped entirely (F31). |
| R40 | fixed | [traced] | l10n.rs:278-288. `fresh_render_without_a_bundle_shows_fallback_not_stale_translations` was seen failing pre-fix when it was written. |
| R41 | fixed | [probe] | Except resources with Junk (F31). |
| R42 | **partial** | [probe] | The earliest of `{%`/`{#` wins now. A multi-line comment still leaves `<p class="markdown"></p>`, and comments now hang the loader (F03). Untested. |
| R43 | **partial** | [probe] | Substitution still splits at the first `find`. `{ $x } and **{ $y }**` with x=`Tom and **Jerry` misplaces the emphasis. An arg containing `\u{2}0\u{3}` is substituted again (displays `A X B X C`). The proptest accepts any result equal to `md::to_html` of the same mis-split, so it pins current behavior. |
| R44 | **partial** | [probe] | `{# #}`, exact `{% raw %}` and later backtick fences are skipped. `{%- raw %}`, `{%raw%}`, `~~~` fences and indented code are not. The fix adds new include-dropping cases (F13). The fenced-include residual noted in bug_0067 is R34 (F08), not a Tera quirk. |
| R45 | fixed | [probe] | `one_frame_bundle_events_build_once` fails on revert (3 vs 2). |
| R46 | fixed | [code] | Covers absolute, normalized `..` and Windows separators. Symlinks aren't covered, and Bevy's own check doesn't cover them either. Images go through Bevy's `validate_path`. The mode is hard-coded to Forbid: F32. |
| R47 | fixed, untested | [code] | The loader check is right (nine_slice.rs:104-123). `manifests_validate_values` re-implements the predicate instead of calling the loader, so deleting the loader check leaves it green. |
| R48 | **partial** | [code]+[probe] | The `.md` conversion is now fuzzed, and three committed `fuzz/seeds/html` inputs hang it (F03). The no-ancestor cascade case was dropped, although the comment claims it is covered. `markdown: true` exercises the cache but not the `resolve_all` fallback. `var_names` is never fuzzed. |

### F. Signals / focus / anchors

| ID | verdict | evidence | note |
|---|---|---|---|
| R49 | fixed | [probe] | Links inside forms don't submit, and a wrapper around a submit button submits once. `type_button_clicks_do_not_submit` fails on revert. Its doc comment and bug_0071 claim a wrapper case the test doesn't contain. A new regression from the gating: F09. |
| R50 | fixed, test guards R60 | [probe] | With only `try_insert` reverted to `insert`, `same_frame_despawn_with_a_focus_change_does_not_panic` passes on 8/8 seeds: the R60 ordering applies the despawn first. It fails only when R60 is reverted too. The panic needs both reverts, so no test distinguishes `try_insert` alone; the same-frame test stays the guard for the pair. |
| R51 | fixed | [probe] | `activation_on_submit_button_submits`. Activation emits Submit before Click; a pointer click emits Click first (cosmetic). |
| R52 | fixed | [probe] | `a_tooltip_on_a_link_survives_hover` fails on revert. |
| R53 | fixed | [probe] | `first_focusable_fallback_picks_document_order` fails 6/6 on revert. Its comment explains Bevy's `Entity` order backwards: higher indices sort first. |
| R54 | **partial** | [probe] | Leaves fire (`touch_end_emits_leave`). The outer map still grows by one entry per touch (`outer=1→4` over four touches): F30. |
| R55 | **not fixed** | [probe]+[code] | anchor.rs:129-134 `clamp_from_far` is `start.min(limit)`. On a 320×240 viewport, Above for an element at (0,0) gives the overlay rect (0,-20)-(60,0), entirely off screen. Left gives (-60,0)-(0,20). The regression test asserts `rect.max.y == 0.0`, which pins the defect. F07. |
| R56 | fixed, partly untested | [probe] | The position is divided by `UiScale` (anchor.rs:309) and tested. The on-screen check now mixes units: F16. |
| R57 | fixed | [probe] | The split-viewport test asserts `on_screen`. Caveat in F16(b). |
| R58 | fixed, pointer half untested | [probe] | The pointer source is real (`Pointer { Mouse, Primary, (100,45), 1 }`). Only the activation source is asserted. |
| R59 | fixed, untested | [probe] | No panic in any plugin order tried. Side effect: F18. |
| R60 | fixed | [probe] | `a_handler_moved_focus_shows_its_state_the_same_frame` fails on 11–13 of 16 seeds on revert. It depends on the shuffle, like the bug_0021 test. |

### G–H. Examples, scripts, CI

| ID | verdict | evidence | note |
|---|---|---|---|
| R61 | fixed, untested | [probe] | Enter at frame 140 gave exactly one signal in quickstart, hot-reload, inline, markdown and links. |
| R62 | fixed, untested | [probe] | Fields go from `"Ada"`/`"Cartographer"` to `""`/`""` on `clear-form`. |
| R63 | **regressed** | [probe]+[code] | The guard tests whether a `TextInput` *exists*, not whether one has focus (inputs/main.rs:163, forms/main.rs:143-147). With `text-input` the fields always exist, so Space never switches the language, even with a button focused. Gamepad Y still works. F17. |
| R64 | fixed, untested | [probe] | portrait ×2, stats → `[T…]`, `[F…]`, `[F,T…]`. |
| R65 | fixed, untested | [code] | `.quest:focus-visible`, `p.action:focus-visible`. |
| R66 | **partial** | [probe] | The forms receipt is fixed (one decode, no double encoding). The `inputs` preview (`profile.html:13`) still splices typed text into JSON: `Ada "The" Brave` in `de` shows the English fallback. F34. |
| R67 | fixed, test not in CI | [probe] | Byte-identical palette 0; `palettes_round_trip_through_the_file` fails on revert. Example `#[cfg(test)]` modules are never built by `cargo test` or `cargo check --examples`. |
| R68 | fixed, untested | [probe] | Injected keys reach `Update` systems in all five probed examples. |
| R69 | **partial** | [code] | `HtmlNoFocus` roots are skipped. The modal used is the first `HtmlModal` in query order, ignoring visibility and z-order, unlike the library's `active_modal`. Latent: only the demo uses linear mode, and it has no modal. |
| R70 | fixed (CI now red) | [code]+[probe] | The steps exist and all 15 examples build under `-D warnings` with the features. The new test step fails (F01, F02). Extra runner disk and time are unmeasured [INFERENCE: on the order of 10 GB]. |
| R71 | fixed | [code] | `shell: bash` on both `tee` steps. |
| R72 | **regressed** (nightly) | [probe] | ci.yml's check is fine. In nightly.yml, `git status --porcelain` also lists *modified* files, which defeats `-I '"generator"'`. The committed generator line (`Chromium … Arch Linux`) never matches CI's `google-chrome`, so the job fails every night. F21. |
| R73 | fixed | [code] | One layer per harness. |
| R74 | fixed | [code] | Build first, time only the run; 0/124 succeed and other statuses propagate. |
| R75 | fixed | [probe] | `fuzz/coverage` is parked. INT runs `restore` twice (INT trap, then EXIT), which is safe because `restore` is idempotent. TERM exits 130, not 143, contrary to the comment. |

### I. Stale docs

| ID | verdict | evidence | note |
|---|---|---|---|
| R76 | fixed | [code] | style.rs:17-31, AGENTS.md:552-557. |
| R77 | fixed | [code] | AGENTS.md, cascade.rs `track_sizing`/`grid_line` docs. |
| R78 | fixed | [code] | style.rs:132-135, :168-172. |
| R79 | **not fixed** | [code] | lib.rs:193-196 and AGENTS.md:58 still list only `system_fonts`; `text-input`, `hot-reload`, `fuzzing`, `dev-tools` are missing. INDEX.md:235 says R76–R80 are fixed. |
| R80 | **partial** | [code] | `grid-names`/`combinators` were added and quickstart is self-contained again. AGENTS.md:222 still gives `inputs` a "Language button" it doesn't have. |

## New findings

### High

- **F01 · high · CI is red at HEAD: two warnings under `-D warnings`**
  [probe]. `src/l10n.rs:701` (`trail` unused in
  `cached_markdown_matches_per_value_conversion`, from `402362c`) breaks
  `cargo build --all-targets` and plain `cargo test`. `tests/html_ui.rs:2543`
  (`entity` unused, text-input only, from `bbd0959`) breaks the new `cargo
  test --features text-input` step. The unused `trail` also means the
  proptest never generates the trailing-placeable shape it was meant to
  cover. Fix: use `trail` in the pattern match (append ` { $n }`) and drop
  the binding name `entity`.
- **F02 · high · `cargo test --features text-input` aborts: the depth cap is
  too deep when `text-input` is on** [probe, reproduced by the integrator].
  The text-input branch makes `collect_node`'s debug stack frame larger. 99
  nested `<div>`s overflow a 2 MiB compute-pool thread (97 are fine), so
  `deep_nesting_is_capped` (200 levels, capped at `MAX_DOM_DEPTH` = 100,
  build.rs:3489) kills the whole `html_ui` binary: `thread 'Compute Task
  Pool (7)' has overflowed its stack … SIGABRT`. Any user page with ~100
  nested elements does the same with the feature on. Fix: lower the cap
  (48–64), or move the input branch into its own `#[inline(never)]` fn, or
  make the walk iterative.
- **F03 · high · Any `.md` template with a `{# … #}` comment and no later
  backtick hangs its loader forever** — *regression from `402362c`*
  [probe, reproduced by the integrator via `bevy_markup::fuzz::render_html`:
  `"{# note #}\nHello\n"`, `"Hi {# c #} there"` and `"{##}"` all exceed 3 s;
  `"{# a #}\nuse \`code\`\n"` returns]. In `separate_statements`
  (md.rs:122-131), every `{#` takes the code-span branch. With no backtick
  after it, `span_end == start`, nothing is consumed, and the loop spins. The
  UI never loads and a core stays pinned. `reader.md`/`notes.md` survive only
  because their comments contain a backtick. Three committed `fuzz/seeds/html`
  inputs hang the html fuzz target. Fix: take the span branch only when
  `rest[..start].ends_with('`')`, and always advance past a comment's `#}`.
- **F04 · high · The ADR 0021 safety net still never reuses (R01)**
  [probe]. See R01. Five bug files and ADR 0021 cite
  `skeleton_reuse_matches_a_fresh_build` as coverage, but it exercises no
  reuse pass. Fix: insert `HtmlStylesheet` only when the op changes it, make
  the swap op reachable (`0..6u8`), allow 1..40 ops, and add a hover op. The
  probe variant of this got 66 reuse passes and catches R02 on revert.
- **F05 · high · Inline nesting is still unbounded (R12's original crash)**
  [probe]. `push_content_runs`/`push_runs` (build.rs:3867, :3911) take no
  depth. A `<p>` holding 600–1000 nested `<b>`/`<span>` aborts the process.
  This is reachable from `{{ doc | safe }}` and from FTL translation markup.
  The chain clone in `push_runs` is still O(depth²). Fix: thread `depth`
  through both against `MAX_DOM_DEPTH`, keeping the text, and extend
  `deep_nesting_is_capped` with an inline case.
- **F06 · high · `var()` chains overflow the stack** — *regression from
  `e992bf1`* [probe]. The fix removed the old `depth > 32` guard, and
  `Substitution::var` → `substitute` (cascade.rs:1615-1688) recurses once per
  link. `--v1: var(--v0); … --v1000: var(--v999); width: var(--v1000)`
  (~23 KB of CSS) aborts a real app (`Compute Task Pool (3) has overflowed its
  stack`). On a 2 MiB thread in debug, 400 links pass and 600 abort. This
  breaks the fuzz contract again. Fix: restore a depth cap
  (`resolving.len() > 64` → `None`) or resolve iteratively.

### Medium

- **F07 · medium · `Above`/`Left` overlays are still placed off screen (R55),
  and the regression test pins it** [probe]+[code]. anchor.rs:129-134,
  :160, :173. Fix: use the existing `clamp(view_h - min.y + gap, height,
  Some(view_h))`, i.e. `bottom ≤ view_h − height`, and likewise for `right`.
  Rewrite `anchored_overlay_above_left_clamp_to_the_viewport` to assert
  `rect.min.y >= 0` / `rect.min.x >= 0`, and correct bug_0077, CHANGELOG and
  the anchor.rs comment.
- **F08 · medium · Markdown code is not Tera-literal (R34 open)** [probe].
  md.rs:333-341 passes `Event::Code` and code-block text through unchanged,
  so Tera runs it. ADR 0020, AGENTS.md:503-505, lib.rs and CHANGELOG all
  claim the opposite. A fenced `{% include %}` in a code sample fails the
  load for this reason, not because of Tera's include validation (bug_0067's
  explanation). Fix: emit code text as `{% raw %}…{% endraw %}` (splitting
  any inner `endraw`), or encode `{`/`}` as entities that the build decodes
  once. Add an end-to-end test that renders with a context.
- **F09 · medium · A plain `<button>` swallows its bound ancestor's click** —
  *regression from `cd21ffd`* [probe]. build.rs:3789-3795 marks every
  `<button>` without a `type` (or `type="submit"`) as a submit button, inside
  a form or not. `covered_by_deeper` (signals.rs:500) then treats it as
  bound. `<div data-on-click="wrap"><button>Plain</button></div>` with no
  form emits nothing at all; with the clause reverted, `Click wrap`. Fix:
  set `FormSubmitButton` (and count it as bound) only inside a `<form>` with
  a submit hook.
- **F10 · medium · Root `calc()` resolves against the wrong camera** —
  *regression from `c07dda7`* [probe]. build.rs:2853
  `min_by_key(|(_, d)| d.is_some())` prefers a camera *without*
  `IsDefaultUiCamera`, the opposite of its comment. With a second 100 px
  camera, `html { width: calc(50% - 20px) }` on a 640 px UI gives `Px(30)`
  instead of `Px(300)`. It also ignores `UiTargetCamera`. Fix: read the
  root's computed target info, or at least use `max_by_key`.
- **F11 · medium · One `url()` in a custom property can fail the whole
  stylesheet** — *regression from `49525d2`* [probe]. cascade.rs:843-853
  feeds custom-property URLs into style.rs:282 `resolve_embed_str(url)?`.
  `:root { --icon: url("data:image/svg+xml;utf8,<svg …/>") }` gives
  `InvalidSourceSyntax`, the sheet fails, and every rule is gone. Every
  `--x: url(…)` is also loaded as an `Image` now (fonts, data URIs). Fix:
  skip (log) unparsable or schemed URLs from custom properties instead of
  `?`.
- **F12 · medium · After a fenced block with an info string,
  `separate_statements` stops separating** — *regression from `402362c`*
  [probe]. md.rs:69-93: the closing-fence test sits inside `if fence.is_none()`
  and can never run, so the rest of the document is copied verbatim. Own-line
  statements then merge with prose (the ADR 0020 note 3 "shattered document"
  shape). ```` ```rust\nx\n```\n\n{% if a %}\nShown\n{% endif %} ```` with
  `a=false` renders `<p class="markdown"></p>`. Fix: check for the closer
  while a fence is open, and set `at_line_start = true` after every copied
  line.
- **F13 · medium · `resolve_references` drops later includes** —
  *regression from `402362c`* [probe]. template.rs:230-266. An unmatched
  backtick (`` It`s {% include "a.md" %} ``) skips to EOF. A `.md` file that
  starts with a fence never closes it (`at_line_start =
  rest.starts_with('\n')`). `{% set x = "{#" %}` swallows the includes after
  it. `{%- raw %}`/`{%raw%}` aren't recognized. In each case the include
  isn't loaded or rewritten, so the template fails with "Unknown template".
  Fix: treat an unmatched backtick run as text, track line starts properly,
  skip non-reference tags to their closer honoring quotes (as `tera_runs`
  does), and normalize the raw opener.
- **F14 · medium · Context `var()`s don't restyle in inline `style` or in
  unknown properties** [probe]. `var_names` skips
  `CustomPropertyName::Unknown(_)` (cascade.rs:897), so changing `--pe` in
  `pointer-events: var(--pe)` or `--off` in `outline-offset: var(--off)`
  leaves the old value. Inline-style names were never tracked (R22's open
  half). Neither gap is in AGENTS.md Limits. Fix: collect refs from unknown
  properties too; hash the inline styles' var names into the vars hash, or
  document the limit.
- **F15 · medium · CSS-wide keywords: `initial` inherits, `calc()` survives,
  and resets go too wide** [probe]. Inherited fields resolve `None` as
  inherit (build.rs:540), so `.red{color:#f00} p{color:initial}` shows
  `#ff0000`. CHANGELOG:90-93, bug_0046 and cascade.rs:1525/:1691 say the
  reverse. `width: initial` after `width: calc(50% + 10px)` keeps the calc
  (only the insets call `calc.retain`, cascade.rs:1758-1784). A single-corner
  keyword clears all four corners (:1733). `grid-template-areas: initial`
  clears the columns' line names (:1803). Fix: a tri-state field for
  explicit `initial`; clear calc entries in the keyword path; narrow the
  resets.
- **F16 · medium · The world anchor's on-screen check mixes units when
  `UiScale` ≠ 1** — *regression from `cd21ffd`* [probe]. anchor.rs:309-310
  divides the position by `UiScale` but compares it with the undivided
  `rect.size()`. With `UiScale(0.5)` a visible point hides; with
  `UiScale(2.0)` a point off screen shows. (b) [traced]: the origin
  subtracted is the projecting camera's viewport, not the UI camera's, so a
  3D camera with an offset viewport shifts plates drawn by a full-window UI
  camera. Fix: test bounds before dividing, and subtract the UI camera's
  origin.
- **F17 · medium · Space never switches the language on the `inputs` and
  `forms` pages** — *regression from `e16ef4b`* [probe]+[code]. The guard is
  "any `TextInput` exists", not "a text field holds focus". Fix: `focus:
  Res<InputFocus>`, `typing = focus.get().is_some_and(|e|
  fields.contains(e))`.
- **F18 · medium · `app.update()` without `app.finish()` now panics** —
  *side effect of the R59 fix (`cd21ffd`)* [probe]. `DirectionalNavigation`
  resources are added in `finish` (lib.rs:363). Driving `update()` directly,
  as headless apps and user tests do, fails `HtmlFocus` validation
  ("`HtmlFocus::navigator` … Resource does not exist"). The repo's own
  harness had to add `app.finish()` (tests/common/mod.rs:305). This is not
  marked breaking in CHANGELOG. Fix: `init_resource` the two navigation
  resources in `build` too, or document it as breaking.
- **F19 · medium · ADR 0022's decision rests on a false fact** [code].
  bevy_text 0.20.0 has `EditableText::max_characters` (editing.rs:149),
  which is the mapping ADR 0016 Decision 1 planned for `maxlength`. ADR
  0022's Context ("no API to map onto"), CHANGELOG:57-59 and
  html_ui.rs:2439-2440 are wrong, and "logged as skipped" (ADR 0022:32) is
  false: nothing logs `maxlength`. Fix: map `maxlength` to `max_characters`
  and supersede ADR 0022 with one that retracts only `placeholder`.
- **F20 · medium · Per-declaration `var()` budget makes cost scale with
  element count, and custom declarations are substituted and then discarded**
  [probe]. cascade.rs:1549-1566. A 6 KB sheet (13-level chain + 300
  consumers) costs 20 ms per distinct element in debug. A chain with no
  consumer still costs 4.8 ms per element, because every `--x: …var()…` is
  substituted, re-parsed and then ignored by `apply`. Fix: substitute only
  unknown properties, and memoize resolved `--name` values per `HtmlStyles`
  (variables are sheet-level).
- **F21 · medium · The nightly browser-oracle job fails every night** —
  *regression from `e16ef4b`* [probe]. See R72. Fix: check only untracked
  files: `git ls-files --others --exclude-standard tests/vectors`. Use the
  same in ci.yml:82.

### Low

- **F22 · low · `first_focusable_in_scope` walks every in-scope subtree every
  frame when nothing is focusable** (HUD-only UIs, empty modals) — *from
  `cd21ffd`* [code]. focus.rs:425. Fix: return early on an empty
  `navigable`.
- **F23 · low · Extra per-build costs from the R04–R06 fixes** [code].
  build.rs:1259-1330 copies the document source on every build without a
  reuse pass, hover restyles included. Gate-skipped builds re-parse the
  document and rebuild the spec table. `refresh_cached_handles` queues a
  command per element in every reused subtree, even when nothing moved. ADR
  0021's "no per-frame data move" no longer holds. Fix: compare without
  copying, parse lazily at retry time, and skip the refresh when the root's
  handle is unchanged.
- **F24 · low · `update_spec` strips app-attached `EditableText`/`TextInput`
  from every non-input element** — *from `bbd0959`* [code]. build.rs:2591-2596.
  The stated case ("type changed") can't occur: non-text inputs build no item.
  The branch only removes what an app's custom element attached. Fix:
  delete it.
- **F25 · low · `var()` cycle members still take their fallback** [probe].
  cascade.rs:1619-1622. `--a: var(--a, 5px)` → 5px; CSS Variables §2.3 makes
  them invalid at computed-value time, fallback included.
- **F26 · low · `var_names`' fixpoint is quadratic** [probe].
  cascade.rs:910-924. A 4000-level chain takes 4.7 s at load (debug). Fix: a
  worklist over a name → refs map.
- **F27 · low · Debug-build overflow panics from track counts (pre-existing)**
  [probe]. `repeat(40000, [a] 1fr)` gives "attempt to subtract with
  overflow" (cascade.rs:2651), and the loop iterates `count` times before the
  `u16` rejection. 32 767 `grid-template-areas` rows overflow at :2906. Fix:
  reject counts and lines over `i16::MAX` first.
- **F28 · low · `type` is compared case-sensitively** (`== "text"`,
  build.rs:3636, from `bbd0959`; `== "submit"`, :3795) [code].
  `type="Text"` now builds nothing. Fix: `eq_ignore_ascii_case`.
- **F29 · low · `refresh_cached_handles` gives up when an app-nested
  `HtmlUi` sits beside real children** [traced]. build.rs:2492: the count
  includes the nested root and the function returns early, so `:hover` stays
  dead in that subtree while reuse keeps succeeding.
- **F30 · low · `hover_signals` keeps one empty outer entry per touch**
  (R54 incomplete) [probe]. signals.rs:696-716. Fix: `entered.retain(…)`.
- **F31 · low · The Markdown cache skips a whole resource that contains any
  Junk** [probe]. l10n.rs:~478. All of that resource's messages then convert
  per frame, and the skip enables R39's duplicate-id mismatch. Fix: use the
  partial AST from `Err((resource, _))`.
- **F32 · low · The unapproved-path check ignores
  `AssetPlugin::unapproved_path_mode`** [code]. template.rs:308,
  l10n.rs:180 hard-code Forbid; apps that opted into `Allow` now fail
  includes and FTL resources. Fix: honor the mode, or document it.
- **F33 · low · `separate_statements`' own-line test reads the first line of
  `rest`, not the statement's line** [probe]. md.rs:137-140. A line-leading
  inline statement after an earlier line gets split. Fences without an info
  string, `~~~` fences, fences not at a loop line start, and indented code are
  not protected.
- **F34 · low · The `inputs` example still splices typed text into JSON**
  (R66) [probe]. Fix: serialize in `read_fields_into_preview` as `forms`
  does, and add a quote to the `content_lint` contexts.
- **F35 · low · Bounded `hot-reload` runs edit a tracked file** [probe]. With
  `--exit-after` > 140, the injected Enter cycles the palette and rewrites
  `examples/assets/hot-reload/style.css`. Fix: skip the write in bounded
  runs.
- **F36 · low · `world` panics under `--no-render` (pre-existing)** [probe].
  `ResMut<Assets<StandardMaterial>>` is missing. examples/README.md presents
  `--no-render` without exceptions. Fix: document `--offscreen` for `world`.
- **F37 · low · The outline renderer's recursion is unbounded
  (pre-existing)** [probe]. template.rs:132/167. 3000 nested divs plus
  `HtmlDebugOutline` crash. This contradicts bug_0040's "every downstream
  recursion is bounded".
- **F38 · low · Remaining `= helper(value)` erase sites** (R27): the six
  sites listed under R27.
- **F39 · low · A name on a merged `repeat()` boundary is registered twice**
  — *from `6195a88`* [probe]. `repeat(2, [a] 1fr [a])`: `3 a` resolves to
  line 2 (CSS: 3).
- **F40 · low · Per-value Markdown double-escapes string args inside code
  spans (pre-existing)** [probe]. `` Run `{ $cmd }` `` with `a & b` displays
  `a &amp; b`.
- **F41 · low · Fuzz-harness gaps** [code]. The no-ancestor cascade case was
  dropped (comment says "with and without ancestors"). `var_names` never runs
  (empty stub). The `markdown: true` `resolve_all` fallback isn't reached.
- **F42 · low · Example tests never run in CI** [code]. R67's
  `palettes_round_trip_through_the_file` lives in an example's `#[cfg(test)]`
  module. Fix: move it to `tests/content_lint.rs` or add `cargo test
  --example hot-reload --features hot-reload`.
- **F43 · low · Linear-mode modal confinement ignores visibility and z-order**
  (R69) [code]. examples/shared/input.rs:224-238. Fix: pick the modal as
  `active_modal` does.

## Records and documentation

These records claim something the code doesn't do, or are out of date:

- **Prior review:** `review_0001.md:3-7` still says "Findings are **open** —
  none has a fix yet … hold no changes". INDEX.md:8 says "80 open findings".
  INDEX.md:235 says R76–R80 are fixed (R79 and R80 aren't). Its "not filed"
  list misses R42 (folded into bug_0059) and R57 (folded into bug_0078).
- **Bug files claiming fixes that aren't:** bug_0040 (R12, "every recursion
  bounded"; F02, F05, F37), bug_0044 (R19; F06), bug_0058 (R34; F08),
  bug_0059 (R35/R42; F03, F12, F33), bug_0077 (R55; F07). bug_0047's "partly
  open" status isn't marked **open** like other open rows, so a scan misses
  it.
- **Regression coverage that doesn't guard the bug:**
  - bug_0031/0033/0036 cite the R01 property (no reuse; F04).
  - bug_0033's `gate_skipped_builds_stay_sound` passes on revert.
  - bug_0072's test guards R60's ordering, not the `try_insert`.
  - bug_0070's `manifests_validate_values` re-implements the predicate.
  - bug_0071's test lacks the wrapper case its comment claims.
  - bug_0054's "none dedicated" is stale: `descendant_combinator_matches_at_any_depth` guards it.
  - bug_0051 names "the `image_urls` unit test" (it is `border_image_sources_none_and_longhands`) and claims inline `style` URLs, which still don't load.
- **CHANGELOG [Unreleased]:**
  - the keyword entry describes `initial`/`inherit` backwards (F15);
  - the forms entry says only `type="submit"` submits (a `<button>` without a `type` does too);
  - the RemSize entry says values "resolve the same frame" (the calc pass reads the new size a frame later, build.rs:1230-1232);
  - the R55 and R34 entries describe fixes that didn't land;
  - the `maxlength` text is wrong (F19);
  - F18 is not marked breaking.
- **AGENTS.md:**
  - line 46 (overlay clamp) and line 45 ("scale factors read correctly") contradict F07 and F16;
  - :503-505 (code is Tera-literal) contradicts F08;
  - :412 ("the one queued write refreshes the `DomNode`") and ADR 0021:204/212 ("only the retry counter ticks") predate the R04/R06 fixes;
  - Limits says only `text-align`/`line-height`/`letter-spacing` are silently ignored, but every property without a match arm falls through `_ => {}` (cascade.rs:2398) unlogged;
  - :222 `inputs` "Language button".
- **Code comments:**
  - the `Skeleton` doc says the retry backs off "16, 64, 256", but the code doubles from 16 to 256;
  - build.rs:879 "see the suppression below" points at removed code;
  - the `FormSubmitButton` doc (signals.rs:209) says "inside a form" but the marker is set outside forms too (F09);
  - the anchor.rs:125-131 comment calls the off-screen result "clamped";
  - the `first_focusable_fallback_picks_document_order` comment explains `Entity` order backwards;
  - mutants.sh:45-47 says "the signal's status" (TERM exits 130).
- **Merge splice:** `transitive_var_change_restyles` (tests/html_ui.rs:2472) lost the first line of its doc comment. The md.rs test `tera_templates_survive_the_conversion` sits outside `mod tests` (it still runs).

## Checked and fine

- **Merge integrity.** Every test function from all six `fix/*` branches is
  at HEAD; nothing is lost or duplicated except the intentionally removed
  `queue_restyle`. `style_pending` is set in `decide`, survives a `Wait`, and
  is cleared only when an update runs.
- **Strengthened reuse property (probe).** 66 reuse passes covering sheet
  swap with a content change, outline toggles and gate-skipped builds, with
  no difference from fresh builds. The reuse fixes themselves hold; only the
  committed property is weak.
- **Depth cap × reuse.** `pair_node` counts depth like `collect_node` and
  never marks a capped subtree clean.
- **Combinator memo.** 200 000-case differential against the old matcher; the
  key is exact.
- **`var()` size budget.** 64 KiB per declaration with clones charged;
  fallbacks and fan-out are linear. No exponential shape found.
- **Markdown cache.** Slicing stays on char boundaries, `tera_runs` ends
  safely on unclosed input, there is one fluent-syntax version in
  `Cargo.lock`, and the original R32/R33/R37 scenarios match the per-value
  path.
- **Loaders.** The unapproved-path check covers absolute paths, normalized
  `..` and Windows prefixes; images, stylesheet `url()`s and `<img>` go
  through Bevy's `validate_path`. Nine-slice validation rejects NaN, ±inf,
  negative insets and non-positive tile/scale factors.
- **Forms.** Submit gating; exactly one Submit through a covered wrapper;
  links in forms don't submit; activation submits with an Activation source;
  a submit button outside a form is a no-op.
- **Focus and plugin wiring.** No schedule cycle; the
  `DirectionalNavigationPlugin` auto-add works in every order tried;
  `HtmlNoFocus` and modal scope are honored in the fallback walk.
- **Anchors.** `place_anchored` handles `UiScale` and the window scale factor
  consistently. Right/Below clamp inside the viewport at the far edges.
- **Examples.** Fourteen of 15 examples run headless (`--no-render
  --exit-after 200`) with no ERROR or WARN lines; `world` is F36. The forms
  receipt decodes its args once.
- **CI and scripts.** `shell: bash` gives pipefail; the fuzz scripts build
  first and treat 124 as success; `mutants.sh`'s `restore` is idempotent.
  `cargo doc --no-deps` and `cargo check --lib --features text-input` are
  clean under `-D warnings`. Every bug-file commit hash exists and touches
  its stated component.
- **No public API changes** between `b224619` and HEAD.

## Triage

## Status (fix session, same day)

Every finding is addressed: F01–F43 fixed (CI green, `-D warnings`, all
feature sets CI runs), each regression test falsified against its
specific fix in a scratch copy where the diff supports it. Not fixed
deliberately: F32 (documented as bug_0087 — no API to consult the
mode). Untestable as a lone fix: F50's `try_insert` panic needs the R60
ordering reverted too (the row above is updated). The guard-test wave
(R04, R05-Wait, R47, R58, R62, R64) landed with the fixes; the
`F`→bug-file mapping is in `docs/agents/bugs/INDEX.md`.

## Triage (what this session executed)

1. **Unblock CI first:** F01, then F02. Until both land, every push to main
   fails CI.
2. **Hangs and aborts from user content:** F03 (any `.md` with a Tera
   comment), F06 (CSS), F05 (HTML). Each breaks the fuzz contract, and F03
   already hangs committed seeds.
3. **Fix the net before trusting reuse again:** F04. The reuse fixes
   probe-verify today; the committed property would not notice a regression.
4. **Fixes that didn't land or regressed:** F07 (R55; its test must be
   rewritten, not just the code), F08 (R34), F09, F10, F11, F12, F13, F16,
   F17, F21.
5. **Correct the records in the same change as each fix.** See Records and
   documentation; in particular, the bug files and CHANGELOG lines that claim
   F07/F08/F15 behavior.
6. **Process note for the next fix pass.** Run CI's exact commands
   (`RUSTFLAGS=-D warnings`, every feature set CI runs), read the exit status
   rather than counting `ok` lines, and falsify each regression test against
   the specific fix it names, reverting only that fix. Three of this review's
   "test doesn't guard it" rows (R04, R50, R55) would have been caught that
   way.
