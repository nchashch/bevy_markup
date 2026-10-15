# review_0004 — Code review 2026-10-15 (third follow-up)

| field | value |
|---|---|
| ID | review_0004 |
| Date | 2026-10-15 |
| Scope | the 26 commits `4717e13..4776aea` (the fix session for [review_0003](review_0003.md)) and the crate at `4776aea` (`528e5cb`, the hotpath tooling, is out of scope) |
| Method | one session: CI's exact commands (`ci.yml` env: `-D warnings`, debug info off) in a worktree at `4776aea` with its own `CARGO_TARGET_DIR`; behaviour probes and guard reverts in a second scratch worktree; commit diffs and records read against review_0003's findings |
| Findings | verdicts on review_0003's `G01`–`G44` (plus F25, R64); new defects `H01`… (stable ids) |
| Discovery agent | Claude Code session, `claude-opus-5-5` |

Status: fixed by the same-day fix session (same model); H11 after the
user chose the dedent (ADR 0024) — see **Fix session** at the end for
every item's outcome, report and guard.

## Coverage — read this first

Every G finding got a verdict, but not all at the same depth. These had
their fix or guard **reverted** in a scratch copy: G01, R04, R05 (G35).
These were **probed** with review_0003's repro shape or a new one: G03,
G07, G08, G13, G17, G18, G27, G28, G29, G31, G32, G33, G34, F25. The rest
were read at `4776aea` ([code]). G44's five sub-items were not re-checked.

Evidence levels are unchanged: **[probe]** run in a scratch copy (output
quoted), **[code]** exact lines read at `4776aea`, **[traced]** path
followed but not executed.

## Summary

The session fixed most of what it took on. Twenty-two of the 44 findings
are fixed or fixed with a residual. Highlights:

- **CI:** the `-D warnings` build and both test runs are green again.
- **G01 injection:** HTML can no longer be injected through a string
  argument, and the guard fails on the revert.
- **R04/R05:** the two reuse fixes finally have guards that fail when
  the fix is reverted.
- **Performance:** the quadratic paths dropped from seconds to
  milliseconds (G28 3.0 s → 31 ms, G29 3.45 s → 13 ms).

Six problems remain or are new:

- **CI is red again.** `--features fuzzing` fails to compile, this time
  broken by the G05 fix (`944e8fa`). CI's fuzz-harness step and every
  fuzz, cache and coverage job that builds the feature fail (H01).
- **G08 is not closed.** A stylesheet of ~1–3 KB of nested rules still
  aborts the app, because the new pre-scan counts only parentheses (H02).
- **G01 is not closed for Markdown.** String arguments can no longer
  inject HTML, but they can still inject *Markdown*. On the per-value
  path that forges real links and images (H03).
- **G13 broke new shapes.** Its new indented-code rule disagrees with
  CommonMark in both directions, and its "escaped backticks were already
  fixed" claim is false (H04).
- **Debug prints shipped.** Two `eprintln!`s are in library code (H05).
- **The records are incomplete.** Eight of the session's bug reports
  were never committed, and the committed ones cite tests that don't
  exist (H06).

Eighteen G findings were not attempted (G10–G12, G16–G20, G22, G36, G38,
G40–G43). Review_0003 itself still says nothing in it is fixed.

## Verdicts on review_0003 items

| ID | verdict | evidence | note |
|---|---|---|---|
| G01 | fixed, guarded; **residual** | [probe] | Restoring the raw args (`args.set(key, string)`) fails `string_args_cannot_inject_markup_on_the_per_value_path`. Markdown syntax in arguments still converts (H03). Three comments in `l10n.rs` (:687, :925, :937) still describe the reverted `to_html_value`. |
| G02 | fixed | [probe] | `cargo build --all-targets`, `cargo test` and `cargo test --features text-input` exit 0 under CI's env. No dead test functions: `-D warnings` would flag them. |
| G03 | **regressed** | [probe] | `cargo test --lib --features fuzzing` exits 101. `944e8fa` (G05) replaced `Ancestor { element }` with `tag`/`id`/`classes`/`submit`, but `src/fuzz.rs:157,166,182` still builds `element:` (H01). |
| G04 | fixed, guarded | [code] | Narrow fix: a class run with no closing quote now leaves the tag verbatim. `MARKER` is still plain text, so content can still put the class into another attribute's value. The commit notes this as cosmetic. |
| G05 | fixed | [code] | `Ancestor` is now slim and the chain is shared through `Rc<Vec<Ancestor>>`. The regression is H01. The guard is a 20 s wall-clock bound (H09). |
| G06 | fixed | [code] | `run_walk` now threads depth. The guard is a 10 s wall-clock bound (H09). |
| G07 | fixed | [probe] | The G07 shape, a raw-tag code span outside raw, and a raw tag inside a fence outside raw all render correctly. A debug `eprintln!` shipped with the fix (H05). |
| G08 | **partial** | [probe] | The substitution depth counter works. The parse pre-scan counts only `(`, so 1000 nested style rules still overflow a 2 MiB thread and abort the process (H02). |
| G09 | fixed | [code] | `Stylesheet.images` is keyed by URL (`style.rs:241`), so drift between URLs and handles can't happen any more. |
| G10 | open | [code] | `build.rs:3072` still uses `max_by_key(is_some)`. |
| G11 | open | [code] | `src/` registers no `PointerState`. |
| G12 | open | [code] | Still `data("on-submit").is_some()` (`cascade.rs:4037`, `:4434`). The new `Ancestor::submit` doc says "non-empty submit hook", which the code doesn't do. |
| G13 | **partial, regression** | [probe] | The five listed shapes are fixed, but the new indented-code rule drops real includes and misses tab-indented code. Escaped backticks still drop a real include, despite the commit's "already fixed" claim (H04). |
| G14 | fixed; **guard partial** | [code] | The scanners cache their stop positions. The guard only *prints* the two `resolve_references` timings (`G14 DBG …`); only `separate_statements` is asserted (H09). |
| G15 | fixed | [code] | Compares through `Deref` before writing (`build.rs`, `resolve_calc`). |
| G16 | open | [code] | `type` must still equal `submit` (`build.rs:4245`). |
| G17 | open | [probe] | `` ``` `` + ` ```` ` closer: the include after it is dropped, and statements after it merge into one `<p>`. A shorter closer turns a code sample's include into a reference. |
| G18 | open | [probe] | `Hello {# c #} {% if a %}\nShown\n{% endif %}` → `<p>Hello {# c #}\n{% if a %}</p>` + `<p>Shown</p>`. |
| G19 | open | [code] | Border-image URLs with a scheme still go to `load_context.load`. Since G09, custom-property URLs with a `:` are skipped, so the two paths now disagree. |
| G20 | open | [code] | README.md:29 and quickstart's `hello.html` are unchanged. |
| G21 | fixed | [code] | `-timeout=10` in `scripts/fuzz-libfuzzer.sh:30`. |
| G22 | open | [code] | E.g. `CustomElements.warned` is unchanged. |
| G23 | **partial** | [code] | review_0002 and INDEX were corrected. The session's own records repeat the problem (H06). |
| G24 | **partial** | [code] | bug_0081–0087 now name the right agent and commit. AGENTS.md:618 still gives `repeat(4000000000, auto)` as the overflow example, and review_0003 says that one never panicked. |
| G25 | **partial** | [code] | Fixed: INDEX, the feature list, the walks wording, the backoff text, the CHANGELOG lines for F14/F27 and the submit line, and the nine_slice doc. Still stale: review_0001's header ("Findings are **open**"), CHANGELOG:42's RemSize "same frame", and G39's detached docs. |
| G26 | fixed, guarded | [code] | The reuse gate excludes outline-removal builds. The revert was not re-run; the commit reports it fails. |
| G27 | fixed | [probe] | With the test tightened to `restyled == 0` it still passes: a content change no longer restyles. The committed guard allows `<= 1` (H10). |
| G28 | fixed | [probe] | `repeat(1000, [a0 … a999] 1px)`: 31 ms per element style, release, parse included (was 3.0 s). |
| G29 | fixed | [probe] | A consumed 20 000-link chain: 13 ms (was 3.45 s). |
| G30 | fixed | [code] | Names now come from lightningcss's tokens (`collect_var_names`), and the fragment sets are merged. |
| G31 | fixed | [probe] | `left: calc(50% - 10px)` then `margin: initial` keeps `(Left, Sum(50%, -10px))`. |
| G32 | fixed | [probe] | `pointer-events: none` then `initial` → `None`. |
| G33 | fixed | [probe] | `grid-column: 2 / 3` then `span x / 5` → `[Line(2), Line(3)]`. An all-or-nothing `border-radius` with an `em` calc also keeps the earlier rule. |
| G34 | fixed | [probe] | Depth failures aren't memoized. A sticky `depth_failure` could in principle disable memoization of later failures, so I probed for exponential blow-up: refuted (see below). |
| G35 | fixed, guarded | [probe] | Reverting R04 (the `None` arm back to ticking `since_try` only) fails `gate_retry_after_all_dirty_builds_stays_current`, `wait_swallows_two_changes_then_reuses_currently` and `render_change_during_outline_frames_stays_in_step`. Reverting R05 (`if false && !unchanged`) fails the same three. The record calls the Wait test a characterization that no shape fails (H10). |
| G36 | open | [code] | `build.rs:4071` still requires `type` to equal `text`. |
| G37 | **partial** | [code] | The F26 shape is right now (a backward chain) and fails on revert, as the commit reports. It still asserts wall time (2 s), and the session added four more wall-clock guards (H09). |
| G38 | open | [code] | `total_lines` still multiplies `usize`s unchecked (`cascade.rs:3077-3081`). |
| G39 | **partial** | [code] | Only nine_slice's doc was moved. `apply_keyword`'s doc still sits on `calc_props` (`cascade.rs:~1895`), and `track_sizing`'s doc is still merged into `MAX_GRID_TRACKS`'s (`cascade.rs:2918-2926`). |
| G40 | open | [code] | `examples/shared/input.rs:209-213` still uses `.ok()?` on both z-index components. |
| G41 | open | [code] | `step_linear`'s doc and `#[allow(clippy::too_many_arguments)]` still sit on `z_order` (`input.rs:204-209`). |
| G42 | open | [code] | `close_dialog_on_escape` still sets no focus. |
| G43 | open | [code] | Both scripts still map 124 to `exit 0`. |
| G44 | not re-checked | — | |
| F25 | open | [probe] | `--a: var(--b, 1px); --b: var(--a, 2px); width: var(--a, 7px)` → `Px(1)` (CSS: `Px(7)`). |
| R64 | open | [code] | ci.yml:66 still runs only `cargo test --example hot-reload`. |

**CI** (`ci.yml`'s exact env, at `4776aea`):

| command | exit |
|---|---|
| `check --lib` | 0 |
| `check --lib --features text-input` | 0 |
| `build --all-targets` | 0 |
| `test` | 0 |
| `test --features text-input` | 0 |
| `check --examples --features hot-reload,dev-tools,text-input` | 0 |
| `test --example hot-reload --features hot-reload` | 0 |
| `test --lib --features fuzzing` | **101** |
| `doc --no-deps` | 0 |

The fuzzing step is CI's second-to-last step, so the job fails there.

## New findings

**H01 · high · `--features fuzzing` doesn't compile again** — regression from `944e8fa` (G05) [probe]
- **Where:** `src/fuzz.rs:157,166,182` builds `cascade::Ancestor { element: HtmlElement { … }, … }`. `944e8fa` replaced that field with `tag`/`id`/`classes`/`submit`.
- **Impact:**
  - CI's "Fuzz harness unit tests" step fails (`error[E0560]: struct cascade::Ancestor has no field named element`, ×3).
  - Every nightly cargo-fuzz job, `cache.yml`'s fuzz build and the nightly coverage job (which exits on the first failing layer) fail with it.
  - This is the same failure as F41/G03.
- **Why it slipped through:** `944e8fa`'s message reports "Full lib suite 143 green", which was run without the feature. AGENTS.md's gotcha ("API changes that touch `src/fuzz.rs`'s imports … need `cargo check --lib --features fuzzing`") applies directly.
- **Fix:** build the slim `Ancestor` in `fuzz.rs`. Run CI's full command list, not a subset, before claiming green.

**H02 · high · Nested style rules still overflow lightningcss's parser and abort the app** (G08 residual) [probe]
- **Where:** `nesting_over_limit` (`cascade.rs:1733`) counts only `(`. The loader (`style.rs:293`) relies on it before `StyleSheet::parse`, but CSS nesting recurses per `{`.
- **Repro:** `format!("p {{ {} }}", "a {".repeat(n))`, through `nesting_over_limit` and then `StyleSheet::parse(.., ParserOptions::default())`, on a 2 MiB thread:
  - n = 300 parses in a debug build; n = 500 already aborts.
  - Release aborts at n = 1000 (3 KB): `thread '<unknown>' has overflowed its stack … SIGABRT`.
  - Control: 50 000 nested `[` or `{` inside a custom property's value gives a clean parse error, not an abort.
- **Impact:** a mod's or user's `.css` file a few KB long kills the process at load. That is G08's impact by another route.
- **Fix:** count `{` (and `[`, cheaply) in the pre-scan, so the bound covers every block kind the parser recurses into. Add the nested-rule shape to the regression test, on a 2 MiB thread.

**H03 · medium · String arguments still forge links and images on the Markdown per-value path** (G01 residual) [probe]
- **Where:**
  - `resolve_all` (`l10n.rs:~341-352`) converts the *formatted* text with `md::to_html` whenever the cache can't take the message: a selector, a reference, adjacent placeables, or an ambiguous split.
  - `parse_args` escapes only HTML. A value's `[`, `]`, `(`, `)` and `!` are Markdown.
- **Repro:**
  - `found = { $n -> [one] { $who } found one *[other] { $who } found { $n } }` with `{"n": 2, "who": "[Continue](https://evil.example) ![x](../../save.png)"}` → `<p class="markdown"><a class="markdown" href="https://evil.example">Continue</a> <img class="markdown" src="../../save.png" alt="x" /> found 2</p>`.
  - The cacheable `hi = Hello { $who }!` with the same value renders it as literal text, so the result depends on the message's shape.
- **Impact:**
  - A player name or chat string becomes a clickable link. The link span emits the `open` signal with an attacker-chosen href.
  - It can also become an `<img>` that loads an attacker-chosen asset path.
  - AGENTS.md's gotcha ("their characters are still Markdown") covers emphasis but not this.
- **Fix:** review_0003's original suggestion. Convert the pattern with placeable markers, as the cache does, and splice the escaped values in after the conversion. Alternatively, backslash-escape Markdown punctuation in string values before the per-value conversion. Pin both message shapes in one test.

**H04 · medium · The template scanner's code rules still disagree with CommonMark; G13's indented-code rule added new shapes** — `2e14ba8` [probe]

`resolve_references` (`page.md`) compared with `md::to_html` of the same text:

| shape | scanner | pulldown | effect |
|---|---|---|---|
| `Intro\n    {% include "real.md" %}` (lazy paragraph continuation) | protected | paragraph text | real include dropped → "Unknown template" at render |
| `- item\n\n    {% include "real.md" %}` (a list item's paragraph) | protected | paragraph in `<li>` | real include dropped |
| `Intro\n\n\t{% include "gone.md" %}` (tab-indented code) | reference | code block | load fails NotFound |
| `` \`{% include "real.md" %}\` `` (escaped backticks) | protected | text | real include dropped (G13's commit: "already fixed by 57c45a2" — false) |

- G17's two directions and G18 still reproduce (verdicts above).
- **Fix:** indented code exists only where a paragraph can't continue: after a blank line or a non-paragraph block, outside list-item content. Expand tabs to the next multiple of four columns. Treat `\`` as a literal backtick. The shared root cause is that the scanner re-implements CommonMark block structure by hand. Consider deriving the protected byte ranges from pulldown-cmark's offset iterator (`into_offset_iter`) on the same source instead.

**H05 · medium · Debug prints shipped in library code** [code]/[probe]
- `src/md.rs:518` (`bde26f8`, G07): `eprintln!("DBG count arm text={text:?} raw_before={raw}")` runs for every Markdown text or HTML event containing `raw` and `%}`. Seen in a probe's output.
- `src/template.rs:346` (`2e14ba8`, G13): `eprintln!("G13 DBG indented line taken: …")` runs for every 4-space-indented line of every `.md` template at load.
- Both write to every application's stderr.
- Test-only leftovers:
  - `cascade::tests::tmp_probe` (`cascade.rs:5637`, `6343345`) asserts nothing.
  - The `G14 DBG` prints in `scanners_stay_linear_on_pathological_input`.
  - `AREAS:` (`cascade.rs:3997`).
- **Fix:** remove them. Add a CI grep for `eprintln!`/`println!`/`dbg!` in `src/` outside `#[cfg(test)]` and `fuzz.rs`.

**H06 · medium (records) · The session's bug reports are incomplete and cite tests that don't exist** [code]
- `INDEX.md` lists and links bug_0088–bug_0095 (G01, G02, G03, G04, G08, G13, G14, G15), but the eight files are not in the tree. Every high-severity G fix is among them.
- bug_0102 names `build::tests::ancestor_chain_cloned_per_child` and `build::tests::inline_nesting_uncapped_and_quadratic`. The real guards are `deep_inline_nesting_chain_is_bounded` and `attribute_heavy_pages_stay_bounded` (`tests/html_ui.rs`).
- bug_0103 names `md::tests::raw_block_code_renders_verbatim`, which doesn't exist.
- bug_0104 names `skipped_custom_urls_dont_shift_the_pairing`; the test is `…_do_not_…`.
- bug_0096–bug_0101 give a commit hash as "Regression coverage" and "See the commit" as root cause and fix. AGENTS.md (Bug reports) requires a minimal repro, root cause, fix and regression test, which is G24's complaint again.
- review_0003:12 still says "Nothing in this review is fixed yet."
- **Fix:** commit the eight files and correct the test names. When filing, run `grep -rn "fn <name>"` for each cited test.

**H07 · medium · `calc()` on a direct child of a top-level `HtmlUi` root never resolves** — pre-existing since `f086b11` [probe]
- **Where:** `resolve_calc` (`build.rs:~3080`) needs both `boxes.get(parent)` and `parents.get(parent)`. A top-level root has no `ChildOf`, so its children hit `continue` every frame.
- **Repro:** layout harness, 400×300 viewport, root `Node { width: Px(400), flex_direction: Column }`. Inline `width: calc(50% - 10px)` on a root-level `<div>` gives `Node.width = Auto` (laid out at 400). Results per shape:

  | shape | width |
  |---|---|
  | `calc(50% - 10px)` on a root-level `<div>` | `Auto` (400) |
  | `calc(50vw - 10px)` on a root-level `<div>` | `Auto` |
  | `calc(10em - 10px)` on a root-level `<div>` | `Auto` |
  | a stylesheet `.s` rule with `calc(50vw - 10px)` | `Auto` |
  | `calc(50vw - 10px)` on a root-level `<p>` | `Auto` |
  | the same `calc(50% - 10px)` on a `<p>` inside a `<div>` | `Px(190)` (correct) |

- The calc example only works because its calc elements sit inside a container.
- **Fix:** a parent without `ChildOf` is the top. Take its box as the content box, and the camera viewport (as for roots) as the viewport.

**H08 · low · Viewport units in a nested mixed `calc()` resolve against the root's box, not the viewport** — pre-existing [probe]
- **Where:** `resolve_calc` walks to the topmost node and takes its box as the viewport. Its comment ("a UI root measures the window") holds only for full-window roots.
- **Repro:** 400 px viewport, root `width: Px(100)`, `<div><p style="width: calc(50vw - 10px)">`. Results:

  | root width | `calc(50vw - 10px)` | plain `50vw` (Bevy's `Vw`) |
  |---|---|---|
  | 100 px | `40` (correct: 190) | `200` |
  | 400 px | `190` | `200` |

- **Impact:** tooltips, nameplates and HUD panels, which are small roots, size `vw`/`vh` calcs wrongly. Plain and `calc()` viewport units disagree.
- **Fix:** use `viewport_of_camera(top)` for the viewport, falling back to the top box only without a camera.

**H09 · low (tests) · New guards assert wall time, and one asserts nothing on its main path** [code]
- `scanners_stay_linear_on_pathological_input` times the two `resolve_references` shapes but only prints them. The commit's "bounds all four shapes at 2 s" is false, and reverting only `resolve_references`'s cache would pass.
- G37 asked for structural assertions instead of wall time. The session added or kept four wall-clock guards:
  - `deep_inline_nesting_chain_is_bounded` (10 s)
  - `attribute_heavy_pages_stay_bounded` (20 s)
  - `scanners_stay_linear…` (2 s)
  - `var_names_closure_is_linear` (2 s)

  They are the same class that flaked under parallel load (G37).
- **Fix:** assert the `resolve_references` halves. Where possible, count work (stop-position refreshes, chain clones, closure iterations) instead of timing it.

**H10 · low (records/tests) · Guards and records that disagree with the code** [probe]/[code]
- G35's record says `wait_swallows_two_changes_then_reuses_currently` is a characterization that no shape fails. Reverting the R05 re-parse fails it, so the record understates the guard.
- G27's guard asserts `restyled <= 1`, but the fixed code restyles 0 times. A regression to one spurious restyle per content change would pass. Assert `== 0`.
- `Ancestor::submit`'s doc ("a non-empty submit hook") describes G12's fix, which was never made.
- `l10n.rs` comments still name the removed `to_html_value` (G01 row).

**H11 · low (design) · Indented content inside an own-line Tera block becomes a code block** — found by the fix session [probe]
- `md::to_html_template("{% if a %}\n    Some *text*\n{% endif %}\n")`
  → `{% if a %}\n<pre class="markdown"><code>Some *text*\n</code></pre>\n{% endif %}`
  (`*text*` shows literally, in a code block).
- `separate_statements` gives an own-line statement its own lines, with a
  blank line after it, and after a blank line CommonMark reads a 4-space
  line as indented code. Without the statement the line would be a lazy
  paragraph continuation. Indenting inside `{% if %}`/`{% for %}` is the
  usual Tera/Jinja habit, so templates written that way render code
  blocks.
- The include scanner agrees with the conversion here (bug_0108: an
  `{% include %}` on such a line is code, not a reference), so nothing
  fails. It's a rendering surprise, not a crash.
- **Options:** (a) keep it and document "don't indent Markdown inside
  Tera blocks"; (b) dedent the lines between a statement pair before the
  conversion; (c) separate with a single newline where the next line is
  indented (a paragraph continuation, so the statement would merge into
  the paragraph, which is the case separation exists to prevent). Needs a
  decision, and an ADR if not (a).

## Refuted or unconfirmed

- **Exponential `var()` evaluation after G34's change** ([probe], release). Once a declaration has hit the depth cap, `depth_failure` stays set and later failures aren't memoized. Empty fallbacks (`var(--x,)`) consume no budget, so a doubling chain `--aᵢ: var(--aᵢ₊₁,) var(--aᵢ₊₁,) var(--missing)` looked like 2ⁿ work. Measured after a 70-link depth-failing chain: n = 12: 1.2 ms, n = 16: 3.5 ms, n = 20: 4.8 ms (control without the deep chain: 0.11–0.14 ms). It grows, but not exponentially. Not filed.
- **`{% raw %}` + backtick interplay** ([probe]): `` {% raw %}`{% endraw %}` tail {{ x }} `` renders as `tail SEVEN`, losing the raw content. Tera's textual raw handling makes the "right" output debatable for this adversarial shape. Not filed.

## Checked and fine

- R04 and R05 are guarded now. Each revert fails three tests (G35 row).
- G01's guard fails on the revert. The escaped values are safe against HTML in both message shapes.
- G28 and G29 are linear in practice (timings above). G31–G33 behave as CSS specifies for the review's shapes.
- G07's three raw/code combinations render correctly end to end (Markdown template → Tera → build).
- G09's handles are keyed by URL text, so a skipped URL can't shift a pairing.
- G15 writes only on change.
- `-D warnings` is clean for `build --all-targets`, both test runs, `doc`, and the gated examples.

## Triage

1. **CI:** H01, and H05's two library prints. Both are one-line fixes. Run every `ci.yml` step locally before claiming green.
2. **Security / DoS:** H02, then H03.
3. **Correctness:** H07 (any root-level `calc()` is dead), H04, then the open G10/G12/G16/G36/G17/G18.
4. **Records:** H06 and H10, in the same change as the fixes. Add the H-series fixes to `INDEX.md` as their reports land.
5. **Tests:** H09. Tighten G27's assertion. Finish G39/G41's detached docs.

## Fix session (2026-10-15, Claude Code `claude-opus-5-5`)

The model that wrote this review also fixed it; the changes were
uncommitted at filing. Each fix's regression test was checked by
reverting exactly that fix and watching the test fail, unless the row
says "unguarded" or notes otherwise. Reports are in `docs/agents/bugs/`.

| ID | outcome | report | guard (fails on revert) |
|---|---|---|---|
| H01 | fixed by the user's session (`ce19a69`); CI now checks `--features fuzzing` early | bug_0105 (severity corrected to high) | CI's feature check |
| H02 | fixed: the pre-scan mirrors cssparser's block matching for `(`, `[`, `{` (a `)` inside a string had also hidden paren nesting) | bug_0106 | `deeply_nested_style_rules_fail_the_load_instead_of_aborting` (old scanner: `IO Task Pool` overflow, SIGABRT); `pathological_nesting_is_rejected_before_the_parser` |
| H03 | fixed: string args ride the per-value conversion as Private Use Area stand-ins | bug_0107 | `string_args_cannot_inject_markdown_on_the_per_value_path` |
| H04 + G17 + G18 | fixed: pulldown's code ranges replace both scanners' hand-written rules | bug_0108 | `scanner_code_is_commonmark_code`, `separation_follows_fences_and_lines` |
| H05 | fixed: prints and assertion-less tests removed | bug_0109 | none (grep) |
| H06 | fixed: bug_0088–0095 written; bug_0096–0104's coverage corrected; review_0001's and review_0003's status lines updated | — | — |
| H07 | fixed | bug_0110 | `calc_resolves_on_direct_children_of_a_top_level_root` |
| H08 | fixed | bug_0111 | `viewport_units_in_nested_calc_measure_the_viewport` |
| H09 | fixed for the scanner guard (see below) | bug_0094 | `scanners_stay_linear_on_pathological_input` |
| H10 | fixed: G27's guard asserts 0 restyles; `Ancestor::submit`'s doc and the `to_html_value` comments corrected. G35's record (that the Wait test guards nothing) stands corrected here | bug_0098 | — |
| H11 | fixed as the user chose (option b, dedent): `md::dedent_blocks`, ADR 0024; extended to raw HTML containers, ADR 0025 (bug_0127) | bug_0126 | `indented_tera_bodies_are_not_code`, `indented_markdown_templates_build_like_flat_ones`, `dedent_maps_offsets_back`, `dedent_survives_token_soup` |
| G10 | fixed (`DefaultUiCamera`) | bug_0112 | `root_calc_without_a_marked_camera_uses_the_window_camera` |
| G11 | fixed; the harness and test workarounds removed | bug_0113 | `update_without_finish_drives_the_focus_systems` (the original error on revert) |
| G12 | fixed | bug_0114 | `an_empty_submit_hook_does_not_swallow_clicks` |
| G16 | fixed | bug_0115 | `invalid_button_types_submit` |
| G19 | fixed (`http`/`https` sources refused) | bug_0118 | `remote_includes_are_refused`, `manifests_validate_values`, `remote_asset_sources_are_refused` |
| G20 | fixed: map-literal args in the README, the crate docs and 15 example templates | bug_0119 | `map_literal_args_cannot_be_injected`; `content_lint` |
| G22 | fixed | bug_0120 | `undefined_name_warnings_are_bounded` (the FTL dedupe and the allocation-free hash are unguarded) |
| G24 | fixed: AGENTS.md's overflow example | — | — |
| G25 | fixed: review_0001's header, CHANGELOG's RemSize line | — | — |
| G36 | fixed | bug_0116 | `invalid_input_types_are_text_fields` |
| G38 | fixed (saturating arithmetic) | bug_0121 | pinned only; it can't wrap on 64-bit |
| G39 | fixed: `apply_keyword`'s and `track_sizing`'s docs, plus `resolve_references`', `tera_raw_end`'s and `separate_statements`' docs, which were detached the same way | — | — |
| G40 + G41 | fixed (`walk_roots` extracted) | bug_0122 | `linear_walk_confines_to_the_topmost_visible_modal` |
| G42 | fixed | bug_0123 | unguarded (example code); a bounded headless run exits 0 |
| G43 | fixed (crash files counted before and after) | bug_0124 | unguarded; the counting was checked with a planted crash file |
| G44 | not re-checked (as in the review) | — | — |
| F25 | fixed (every cycle member is invalid) | bug_0117 | `var_cycle_fallbacks_dont_apply` (old logic gives `Px(1)`) |
| R64 | fixed (`test = true` on grid-names) | bug_0125 | the example's tests run under `cargo test` |

**H09 in detail.** The scanner guard is now a scaling test: 4× the input
may cost at most 8× the time, best of three runs. It passed 8/8 under 4×
parallel load. It caught a quadratic regression in this session's own
`separate_statements` rewrite (1.25 s on the 1 MB line), which the old
2 s bound let through. The remaining wall-clock guards, measured under 4×
parallel load:

| guard | bound | measured |
|---|---|---|
| `var_names_closure_is_linear` | 2 s | 0.09 s (whole test) |
| `name_heavy_repeats_stay_linear` | 2 s | 0.16 s (whole test) |
| `consumed_custom_declarations_arent_substituted_either` | 150 ms on the chain | 0.08 s (whole test); the tightest, so convert it next |
| `deep_inline_nesting_chain_is_bounded` | 10 s | 0.7 s (both html_ui tests together) |
| `attribute_heavy_pages_stay_bounded` | 20 s | (included above) |

**Gate:** CI's command list (`-D warnings`, debug info off) plus
`cargo check --lib --features fuzzing`, run on the working tree. The
session's final report has the exit codes.
