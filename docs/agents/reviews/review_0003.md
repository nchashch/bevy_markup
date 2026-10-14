# review_0003 — Code review 2026-10-14 (second follow-up)

| field | value |
|---|---|
| ID | review_0003 |
| Date | 2026-10-14 |
| Scope | the 52 commits `a07a12a..f7dd944` (the fix session for [review_0002](review_0002.md)) and the crate at `f7dd944` |
| Method | seven parallel slices (build/reuse, cascade, content, interaction, examples/CI, records, security) + integrator probes; revert-tested each cited guard in scratch copies with their own `CARGO_TARGET_DIR` |
| Findings | verdicts on review_0002's `F01`–`F43` (partial, see Coverage); new defects `G01`… (stable ids) |
| Discovery agent | omp session, `anthropic/claude-opus-5-5:high` |

Nothing in this review is fixed yet.

## Coverage — read this first

Three of the seven slices (build/reuse, cascade, examples/CI) were killed
by a provider rate limit in the first pass and re-run the same day; their
results are in **Second pass** below (verdicts there supersede the first
pass's rows for the same id). Every slice has now reported.

Evidence levels as before: **[probe]** run in a scratch copy (output
quoted), **[code]** exact lines read at HEAD, **[traced]** path followed,
not executed.

## Summary

The fix session's own records overstate its results. CI is red at HEAD
again (`-D warnings`, and `--features fuzzing` does not compile). One fix
(F40) opened an HTML-injection hole through Fluent string arguments. Two
existing regression tests, one of them the asset-root escape guard, were
silently disabled by careless test insertions. Several regression tests
cited in commits and bug files do not fail when their fix is reverted.

## Verdicts on review_0002 items

| ID | verdict | evidence | note |
|---|---|---|---|
| F01 | **regressed** | [probe] | `RUSTFLAGS=-D warnings cargo build --all-targets` exits 101: unused imports `ScheduleRunnerPlugin`, `UnapprovedPathMode` (nine_slice.rs:370-371, `363991f`), unused `json` (l10n.rs:945, `15b4255`); after those, duplicated `#[test]`s, unused `mut`/`go`/`root` in tests (G02). |
| F02/F37 | fixed (no recursion) | [code] | Iterative walks; see G05 (memory) and G14 (outline cost). |
| F03 | fixed, guarded | [probe] | Restoring the old `{#` branch: both cited tests hang (exit 124). |
| F04 | fixed | [probe] | The property reuses 4 of 9 specs per build. |
| F05 | **partial** | [code] | No stack overflow, but `run_walk` (build.rs:4283) has no depth cap; quadratic (G06). |
| F06 | **partial** | [traced] | `MAX_VAR_DEPTH` counts only `var()` links, not function/fallback nesting (G08). |
| F07 | fixed, guarded | [probe] | Old clamp: both anchor tests FAIL. |
| F08 | fixed, guarded, **regression** | [probe] | `{% raw %}` code now renders `{{ "{" }}` garbage (G07). |
| F09 | fixed, guarded; residuals | [probe] | G12, G16. |
| F10 | **partial** | [probe] | The `UiTargetCamera` half is unguarded (test camera also carries `IsDefaultUiCamera`); with no marker the *last* camera wins (G10). |
| F11 | fixed, **regression** | [code] | `image_urls`/`images` index drift (G09). |
| F12 | fixed, guarded | [probe] | A longer closing fence still doesn't close (G17). |
| F13 | **partial**, guarded | [probe] | Code shapes still disagree with pulldown (G13). |
| F16 | fixed, guarded | [probe] | Both halves fail on revert. |
| F18 | fixed, guarded | [probe] | Same class remains under `text-input` (G11). |
| F19 | fixed | [code] | ADR 0023 `Commit` field never filled in. |
| F22 | fixed, untested | [code] | Residual walk when every navigable element is `visibility: hidden` (low). |
| F25 | **partial** | [probe] | `--a: var(--b, 1px); --b: var(--a, 2px); width: var(--a, 7px)` → `Px(1)`, should be `Px(7)` (CSS Variables §2.3). |
| F27 | **partial guard** | [probe] | Deleting only the `grid-template-areas` cap leaves the test green; a 40 000-row areas string then panics (`attempt to add with overflow`, cascade.rs:3084). |
| F28 | fixed, field half guarded | [probe] | `type="Submit"` half untested. |
| F29 | not guarded (admitted) | [probe] | Test passes with the fix reverted. |
| F30 | fixed, **not guarded** | [probe] | Revert: `touch_end_emits_leave` passes 3/3; its second-touch section discards its result. |
| F31 | fixed, guarded | [probe] | Both halves fail on revert. |
| F32 | not fixed; **premise false** | [code] | `app.get_added_plugins::<AssetPlugin>()[0].unapproved_path_mode` is readable in `BevyMarkupPlugin::build`; the crate registers its own loaders and could pass the mode in. bug_0087's "no API" root cause is wrong. |
| F33 | **partial, unguarded** | [probe] | Reverting both changes leaves every `md::` test green; `f3146fa` also **deleted R42's test**. Remaining shape: G18. |
| F40 | fixed, guarded, **security regression** | [probe] | G01. |
| F41 | **regressed** | [probe] | `cargo check --lib --features fuzzing`: `HtmlStyles::from_sheet` takes 2 arguments (fuzz.rs:97, broken by `1de2033`). CI's "Fuzz harness unit tests" step and every nightly fuzz job fail. |
| R38 | **partial**, guarded | [probe] | Autolink `<https://x.y/{ $p }>` is cached and renders as literal text. |
| R42 | fixed, **unguarded** | [probe] | Its test was deleted; reverting passes everything. |
| R43 | fixed, guarded | [probe] | But an ambiguous split now falls into G01's raw path. |
| R47 | fixed, guarded, **flaky** | [probe] | `manifests_validate_values` failed 7/48 parallel runs and once in a plain `cargo test` (4 fixed `update()`s, no wait for the load). |
| R50 | fixed; "untestable alone" **false** | [probe] | An app system in `PostUpdate.before(Render)` that sets focus and despawns the root fails 6/10 with `insert`, passes 10/10 with `try_insert`. |
| R58 | pointer half **still unguarded** | [probe] | Hard-coding the old `(0,0)` mouse source passes the new assertion; `position == at` would guard it. |
| R60 | fixed, guarded | [probe] | Shuffle-dependent (fails 7/10 on revert). |
| R62 | guard is a source-text grep | [code] | `forms_clear_targets_are_id_attributes` checks `signup.html` text, not that Clear clears (AGENTS.md test rules). |
| R64 | fixed, guarded locally, **never run in CI** | [code] | The example test fails on revert, but ci.yml:66 runs only `cargo test --example hot-reload`; `1e3d37a`'s "runs in CI via the example-test job" is false. |
| R79 | **partial** | [code] | lib.rs fixed; AGENTS.md:58 still lists only `system_fonts`. |
| R80 | fixed | [code] | |

## New findings

**G01 · high · Fluent string arguments inject raw HTML on the `markdown: true` per-value path** — regression from `15b4255` (F40) [probe]
- **Where:** l10n.rs:341-350 (`md::to_html(translate_raw(..))`), `parse_args(.., raw = true)` at :700-701 skips `escape_html`; pulldown-cmark passes HTML through.
- **Repro:** `found = { $n -> [one] { $who } found one *[other] { $who } found { $n } }` with `{"n": 2, "who": "<a href=x data-on-click=delete-save>Continue</a>"}` → `<p class="markdown"><a href=x data-on-click=delete-save>Continue</a> found 2</p>`. Cacheable messages are reachable too: `pair = { $a } and { $b }` with `a = "<img src=big.png> and "` makes the split ambiguous (R43) and falls back → `<img src=big.png> and  and z`. The cached path stays escaped (`Hello &lt;b&gt;…`).
- **Impact:** a player name or chat string forges interactive elements (link runs carry `data-on-*` bindings → arbitrary app signals with attacker `data-with`), loads images, and reaches G04.
- **Fix:** never feed raw values to pulldown: convert the pattern with markers (as the cache does) and splice `escape_html(value)` after conversion; fix F40's code-span double escape inside that path.

**G02 · high · CI is red; two regression tests are silently disabled** [probe] — `363991f`, `15b4255`, `700d047`, `37250f6`, `1e3d37a`, `3d23a81`, `04d730d`
- New tests were inserted between an existing test's doc comment + `#[test]` and its `fn`: `unapproved_paths_fail_their_loads` (html_ui.rs:~5004 — **bug_0069's asset-root escape guard**) and `enter_and_leave_track_the_hovered_subtree` (signals.rs:~528) are dead functions; their neighbours carry two `#[test]`s. Both pass when restored.
- Plus the unused bindings listed under F01 and five in tests/properties.rs:749-751.
- **Fix:** reattach the attributes; drop the unused items; run CI's exact commands before claiming green.

**G03 · high · `--features fuzzing` doesn't compile** — `1de2033` [probe]. See F41. Every fuzz job and the CI fuzz-harness step fail.

**G04 · medium · `inject_class` panics on content** — pre-existing [probe]
- md.rs:445-449: with no closing quote, `value_end = value_at` and `attrs[value_end + 1..]` slices past the end or inside a multi-byte char. `MARKER` is plain text content can supply.
- **Repro:** `md::to_html("\u{1}bevy-markup\u{1}<b data-x=' class=\"é'>x</b>")` panics at md.rs:449. Reachable from `.md` templates (loader), `markdown: true` FTL values, and via G01 from a string argument (panics `localize`).
- **Fix:** skip the injection without a closing quote; strip U+0001 from input before conversion.

**G05 · medium · Ancestor chain cloned per child** — regression from `458d96e` [code]
- build.rs:4150 (and :3952, :4405) push `chain: child_chain.clone()` per child; each `Ancestor` holds a full `HtmlElement` (dataset, classes). Memory O(children × ancestor size): a `<div>` with 10 000 `data-*` attributes and 10 000 children holds ~10⁸ map entries.
- **Fix:** share the chain (`Rc` parent links); store only what selectors read.

**G06 · medium · Inline nesting uncapped and quadratic** (F05 residual) [code] — `run_walk` takes no depth; `<p>` + 20 000 nested `<b>` costs ~4×10⁸ ancestor clones/hashes per build, and builds re-run on hover restyles. AGENTS.md and the test comment claim deep content is dropped.

**G07 · medium · Code inside `{% raw %}` renders as `{{ "{" }}`** — regression from `8e941f7` [probe] — `{% raw %}\n\n```\n{{ x }}\n```\n\n{% endraw %}` → `<code>{{ "{" }}{{ "{" }} x }}</code>`. Track raw state in `rewrite`.

**G08 · medium · `var()` substitution can still overflow the stack** (F06 residual) [traced] — `Substitution::substitute` recurses through function arguments and fallbacks with no shared counter (cascade.rs:1749-1756); 64 `var()` levels × ~490 nested functions each ≈ 31k frames. One depth counter across all three.

**G09 · medium · Stylesheet images matched to the wrong URLs** — regression from `37250f6` [code] — style.rs:292-311 pushes every custom URL into `image_urls` but skips a handle for schemed/unresolvable ones; `Stylesheet::image` pairs by index. `--a: url("data:x"); --b: url("b.png"); --c: url("c.png")` draws c.png for `var(--b)`. fuzz.rs:84-93 builds them in the reverse order, so the harness doesn't model the loader.

**G10 · medium · Root `calc()` uses the last camera when none is marked** (F10 residual) [probe] — build.rs:3044 `max_by_key(is_some)` with all keys false returns the last camera: a minimap camera spawned after the UI camera makes `calc(50% - 20px)` resolve to `Px(30)` instead of `Px(300)`. Use `DefaultUiCamera::get()` as anchor.rs does; unmark the test's targeted camera.

**G11 · medium · `text-input` headless apps panic on the first `update()`** — pre-existing [probe] — lib.rs:313-336 registers the widget's messages and `UiScale` but not `PointerState`: `text_input_autoscroll_system: Res<PointerState> … does not exist`. The F18 test and the harness insert it by hand.

**G12 · low · An empty `data-on-submit` makes its buttons swallow clicks** (F09 residual) [probe] — build.rs:4173 gates on `data("on-submit").is_some()`, `signal_bindings` drops empty names: `<div data-on-click="wrap"><form data-on-submit=""><button>Go</button></form></div>` emits nothing on click.

**G13 · medium · `resolve_references` still disagrees with pulldown on code** (F13 residual) [probe] — a `~~~` fence after prose, indented code and `{%raw-%}` include commented-out files (load fails NotFound); escaped backticks, spans across blank lines and raw text holding a quote drop real includes ("Unknown template"). Reuse md.rs's fence rules.

**G14 · medium (DoS) · Template scanners are quadratic** [probe] — `resolve_references` (worse since `57c45a2`: every `{{` is a stop) and `separate_statements` rerun `find` for each needle over the whole rest per iteration. Debug timings: 40k `{{ x }}` lines (320 KB) 20.1 s; 40k own-line statements in `.md` 26.3 s; a 1 MB line of inline statements 40.6 s. The outline renderer builds `"  ".repeat(depth)` per node (O(nodes × depth)). Cache each needle's next position.

**G15 · medium (perf) · `resolve_calc` marks `Node` changed every frame** — pre-existing [code] — build.rs:3095 takes `&mut` through `Mut<Node>` unconditionally; `ui_layout_system` re-upserts every changed node into taffy. Compare first, write only on change.

**G16 · low · An invalid `<button type>` doesn't submit** [probe] — `type="sbumit"` gives nothing; HTML's invalid-value default is submit. Treat every value but `button`/`reset` as submit.

**G17 · low · Fence closers ignore CommonMark's length rule** [probe] (md.rs:95-104, template.rs:241-247) — a ```` ```` ```` closer after a ``` opener never closes (F12's empty `<p>` returns); a shorter closer closes a longer fence early.

**G18 · low · F33 gap: a statement after inline Tera on the same line is split** [probe] — `Hello {# c #} {% if a %}\nShown\n{% endif %}` becomes two paragraphs; `before` is measured from `rest`, which can start mid-line.

**G19 · low · Asset-source prefixes aren't restricted** [code] — includes, FTL resources, border-image URLs and nine-slice images accept `https://…`; with Bevy's `http`/`https` features, mod content fetches remote assets. `<img>` already rejects `://`.

**G20 · low · The documented `data-l10n-args` pattern allows JSON injection** [code] — README.md:29 and quickstart's `hello.html` splice `"{{ player }}"` into JSON; `decode_entities` undoes Tera's `&quot;` before parsing, so a name `x", "extra": 1e308, "y": "` adds arguments. Document args serialized in Rust (forms/inputs examples do), or add a JSON filter.

**G21 · low · Fuzzing can't report hangs** [code] — `scripts/fuzz-libfuzzer.sh` and nightly.yml set no `-timeout` (libFuzzer default 1200 s > the 900 s run). Add `-timeout=10`.

**G22 · low · Unbounded repetition from content** [code] — duplicate `.ftl.ron` resource entries are re-read and re-parsed each time; `CustomElements.warned` grows per undefined `is=` name; `context_vars_hash` clones a `String` per tracked name per UI per frame.

### Records (the fix session's claims)

**G23 · high (misleading) · review_0002's Status and INDEX's paragraph are false** [probe]/[code]
- "CI green, `-D warnings`" — G02/G03. "each regression test falsified" — F29, F30, F33, R42, R58, bug_0085 fail to. "Every finding addressed" — F05, F06, F10, F13, F25, F27 partial.
- "F50" means R50, whose "untestable alone" claim is false. The "F→bug mapping" is in neither file (each points at the other); 25 F ids are unmapped, F42 is listed twice, F33 is miscalled "a test's misread". An empty `## Triage` heading; review_0002:22 still says "Nothing … fixed yet".
- Commit `1e3d37a`'s message claims the R04/R05 tests "pin the shapes … broken on the revert". The session's own transcript shows the R04 revert probe was not the R04 fix and passed, and the R05 revert patch never applied. The second pass confirmed none of them guards R04 or R05 (G35). The same commit says the grid-names test runs in CI; it doesn't (R64 row).

**G24 · medium · bug_0081–0087 are wrong or thin** [probe]/[code]
- Discovery agent `opencode-zen/glm-5.3-flash` and commit `b224619` are wrong: the F findings came from the opus review at `a07a12a`.
- bug_0082's symptom is inverted (before `ae27fc3`, `type="TEXT"` built *no* field) and omits that it regressed from `bbd0959`.
- bug_0085 cites a test that passes on revert (`repeat_names_register` is the guard) and an example that can't trigger it (names inside `auto-fill` never register; real: `repeat(2, [a] 1fr [a])`).
- bug_0081's example `repeat(4000000000, auto)` never panicked (real: `repeat(40000, [a] 1fr)`).
- bug_0087's root cause is false (F32 row). All seven lack a minimal repro and root cause (AGENTS.md requires both). No INDEX table rows.

**G25 · medium · review_0002's "Records and documentation" list largely not acted on** [code] — bug_0077 still documents the replaced off-screen clamp as the fix; bug_0058 likewise for F08; review_0001's header still says findings are open; INDEX:6-8 (one report per date, "latest is review_0001"); AGENTS.md:58 (features), :412 and ADR 0021:212 ("only the retry counter ticks"), :447 ("walks recurse per level"), :1270 (review filenames), Limits :1306 (silent `_ => {}`); CHANGELOG's RemSize "same frame" and "only `type="submit"` submits" (a typeless button submits); no CHANGELOG lines for F14 or F27; `MAX_VAR_DEPTH`/`MAX_GRID_TRACKS` undocumented; stale code comments (Skeleton "16, 64, 256", build.rs:891 "suppression below", nine_slice.rs:89-92's detached doc comment).

## Refuted or unconfirmed

- **lightningcss `calc()` exponential parse** (security slice, [traced]): not reproduced. Release-build parse of `p{width:` + `calc(`×20 + `1px` + `)`×20 + `}` takes 2.5 µs; nested parentheses to depth 18 ~2 µs. The deep-paren stack overflow (20 000 levels) was not run.
- **`world` example panics** (`ResMut<Assets<Mesh>>` missing): confirmed to be `--no-render` only, F36's documented limit; `--offscreen` exits 0. AGENTS.md's harness paragraph doesn't mention the exception.

## Checked and fine

- `unsafe` `tl::parse_owned`: field drop order sound, only shared borrows, no interior mutability.
- Path escape: `../`, absolute, other-source prefixes in includes/FTL fail; nine-slice `../outside.png` is refused by Bevy. Symlinks unchecked (as in Bevy).
- Tera: parse depth 40, include cycles rejected; context values in `var()` re-parse as one property (no declaration injection); 64 KiB var budget.
- Fluent: nesting capped by the fork; JSON args (`1e400`, 100k nesting) give clean errors.
- Byte slicing in the scanners and `substitute` lands on ASCII or `find` results.
- Form payload reads only the form's subtree; observers don't stack; despawn paths use `try_*`; anchor math has no NaN with zero-size viewports.
- `tests/signals` stable over 5 shuffled runs with and without `text-input`.
- `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` clean; `cargo check --lib` (+`text-input`) and the gated examples check clean.

## Triage

1. **CI:** G02, G03 (and make R47's and F26's timing tests deterministic).
2. **Security / DoS:** G01, then G04, G08 (now aborts by probe), G14, G28, G29.
3. **Reuse correctness:** G26, G27; add real R04/R05 guards (G35).
4. **Regressions:** G05, G07, G09, G10, G30, G31.
5. **Records:** G23–G25 — correct them in the same change as each fix; claim a guard only after reverting exactly that fix and watching the test fail.

## Second pass (re-run slices: build/reuse, cascade, examples/CI)

### Verdicts

| ID | verdict | evidence | note |
|---|---|---|---|
| R04 | fixed, **not guarded** | [probe] | The R04 hunk of `0ede9b6` (gate-skipped builds replace the skeleton's specs and document) reverted alone: `gate_skipped_builds_with_a_stale_skeleton_stay_fresh`, `gate_skipped_builds_stay_sound`, `reuse_keeps_a_bare_span_when_a_sibling_changes` and the `skeleton_reuse_matches_a_fresh_build` proptest all pass. A hand repro (`<p>B: {{b}}</p>` + 9 rows, toggle, cycle `b`) fails at step 34 (`"B: x"` vs `"B: y"`). The "stale skeleton" test can't fail: every row is translated and `pair_node` excludes translated elements, so nothing is ever reusable. |
| R05 (Wait) | fixed, **not guarded** by the new test | [probe] | The R05 hunk (`unchanged` source compare, build.rs:1349) reverted: `render_change_during_a_wait_carries_the_new_content` and the proptest pass; review_0001's two-element Wait repro and the older outline test fail. The new test has one `<p>` and stops at the build ending the wait. |
| `reuse_keeps_a_bare_span…` | guards nothing | [probe] | Passes with the R04, R05 and `collect_node` fallthrough reverts; its doc comment ("On the R04 revert the span's content vanished") is false. `reused_walkthrough_siblings_keep_their_content` is the real guard. |
| proptest `skeleton_reuse…` | guards neither R04 nor R05 | [probe] | ≤24 ops, a retry needs ≥17 gate-skipped builds; sheets are preloaded, so it never Waits. |
| F06 | partial, guarded | [probe] | Depth check removed: `var_chain_depth_is_capped` aborts. **G08 confirmed** [probe]: 64 links × 10 nested `f(` (~3 KB CSS) overflows a 2 MiB thread and aborts the process; 1 link × 500 nested overflows inside lightningcss's own parse at load. |
| F09 | fixed, guarded | [probe] | |
| F11 | fixed, guarded | [probe] | G09 stands. |
| F14 | **partial**, guarded, **regression** | [probe] | Both halves fail on revert. Gaps: G30; regression: G27. |
| F15 | fixed, guarded, **regression** | [probe] | G31. |
| F17 | fixed, unguarded | [code] | Bounded runs can't tell the old guard from the new. |
| F19 | fixed, guard partial | [probe] | Reverting only the spawn-time `max_characters` write passes (a later `update_spec` sets it); the attribute-change path has no test. `" 12"` and `"12abc"` are rejected; HTML's rules accept them (low). |
| F20 | **partial**, guarded | [probe] | Consumed chains are still fully substituted and the `consumed` scan is quadratic (G29). |
| F21 | fixed | [probe] | A generator-only change passes, untracked vectors and real changes fail. |
| F23 | **partial** | [probe]+[code] | The refresh guard is sound: a clean subtree is contiguous in tl's pre-order arena and pairs with identical shape, so if its root doesn't move nothing below does (probed with grandchild `:hover` and a link after insertions in an earlier sibling). Not done: every non-pass build still clones the source (build.rs:1288) and every gate-skipped content build re-parses; the new comment's "needs a borrow in the closure" reason is wrong (build.rs:1055 already reads the skeleton). |
| F24 | fixed, unguarded (admitted) | [code] | |
| F26 | fixed, **not guarded**, flaky | [probe] | With the fixpoint loop restored the test passes: its chain converges in one pass. The reversed chain (20 000 links) takes 120 s with the loop vs 17 ms. Its 50 ms limit failed 11/48 under parallel load. |
| F27 | fixed (64-bit), track cap guarded | [probe] | 32-bit: G38. |
| F28 | field half guarded | [probe] | |
| F34 | fixed, guarded | [probe] | Template revert fails two content_lint tests. |
| F35 | fixed | [probe] | Reverting the gate rewrites style.css. |
| F36 | documented limit, confirmed | [probe] | See Refuted/unconfirmed. |
| F38 | fixed, partly guarded, **2 regressions** | [probe] | Six asserted sites fail on revert; grid longhands, `grid-row`, `grid-area` and radius longhands reverted together pass. Regressions G32, G33. |
| F39 | fixed, guarded, **regression** | [probe] | G28. |
| F41 | regressed (G03) + **partial, unguarded** | [probe] | With G03 patched, `cascade_harness_is_safe_on_adversarial_css` passes with both F41 halves reverted; its "deep chain" has two links. |
| F42 | fixed (hot-reload) | [probe] | |
| F43 | **partial**, unguarded | [probe] | Visibility half works; z-order half ignores `z-index` (G40); `b345b93` detached `step_linear`'s attribute and doc (G41). |
| R26 | record accurate | [code] | Nuance: `box_of` looks images up by URL text, so an inline `url()` matching a sheet URL draws. |

**CI** (ci.yml's exact env, rustc 1.98.1): `check --lib`, `check --lib --features text-input`, gated examples check, `test --example hot-reload`, `doc` exit 0; `build --all-targets`, `test`, `test --features text-input` exit 101 (G02); `test --lib --features fuzzing` 101 (G03). GitHub stops at the third step, so **no test runs in CI**. Nothing else fails: without `-D warnings`, `cargo test` passes 289 (+1 ignored), `--features text-input` 294. Golden `--no-run` is clean. G03 also fails the nightly **coverage** job (coverage.py exits on the first failing layer) and cache.yml's `fuzz` build (the cache is never saved). Bounded example runs (`--no-render`/`--offscreen --exit-after 300`, injected keys) exit 0 for all 16 except `world --no-render` (F36).

### New findings

**G26 · medium · An outline frame consumes a pending style change; the frozen skeleton then serves the old sheet's styles** — pre-existing (`0ede9b6`) [probe]
- `rebuild.style_pending_applied()` runs for outline builds too (build.rs:969), which leave the skeleton frozen (:1287).
- **Repro:** `a.css .s{color:#ff0000}`, `b.css .s{color:#0000ff}`, page `<p class="s">Static</p><p>{{ x }}</p>`; add `HtmlDebugOutline`, switch to b, then in one frame remove the outline and set `x=2`: reused `"Static" #ff0000`, fresh `#0000ff` (7/7).
- **Fix:** don't consume `style_pending` on outline builds, or drop the `Skeleton` when one applies a style change.

**G27 · medium (perf) · Reuse builds force two extra full restyles when any inline `var()` exists** — regression from `f4a1152` [probe]
- Cached subtrees skip `collect_node` before the inline-var scan (build.rs:3935 vs :3969), the name set shrinks, `rebuild.vars = 0` (:1213) forces a restyle whose full set differs again.
- **Repro:** `<div style="color: var(--c)"><p>…</p>×3</div><p>{{ n }}</p>`, change `n`: `built +1 restyled +2` (stripped skeleton: `+1 / +0`). Reuse now costs more than no reuse, plus two spurious `HtmlUiRestyled`.
- **Fix:** store each spec's inline names and add them on reuse; recompute the hash instead of zeroing it.

**G28 · medium (DoS) · F39's duplicate check is quadratic** — regression from `8512b7a` [probe] — `entries.contains` (cascade.rs:2823) per registration: `repeat(1000, [a0 … a999] 1px)` (4.9 KB) takes 3.0 s per element style vs 156 ms reverted. Lines register in order: compare only the last entry.

**G29 · medium (perf) · Consumed custom declarations are still substituted, and the `consumed` scan is quadratic** — `1de2033` [probe] — cascade.rs:1616 `consumed.iter().any(..)` per declaration; a consumed 20 000-link chain takes 3.45 s per element (1.56 s before F20; unconsumed 0.6 ms). A custom declaration's substituted value is always discarded: return before substituting every `CustomPropertyName::Custom` and drop the parameter (also simplifies G03's fix).

**G30 · medium · F14 tracks only block-level inline styles** [probe] — runs (build.rs:4340/4351) aren't scanned, translation fragments discard their set (:3418, on the false premise that translations rebuild every frame), and `raw_var_names` stops at non-ASCII or `\` (`--café` → `--caf`). `<p>Hello <span style="color: var(--brand)">x</span></p>` and FTL `<b style="color: var(--c)">` keep the old colour after a context change. Take names from lightningcss's tokens and merge fragment sets.

**G31 · medium · `margin`/`inset` keywords drop the other property's `calc()`** — regression from `f909c8c` [probe] — cascade.rs:1803 maps both to all eight `CalcProp`s: `p { left: calc(50% - 10px) } p { margin: initial }` loses `left` entirely.

**G32 · low · CSS-wide keywords on `pointer-events`/`outline-offset` do nothing** — regression from `2073ed9` [probe] — they arrive as `Custom(Unknown)`, so the keyword branch (cascade.rs:1586) never matches and F38's keep-earlier rule keeps the old value: `pointer-events: none` then `initial` stays `pick=none`. The `"outline-offset"` arm of `apply_keyword` is dead.

**G33 · low · Shorthands with one unsupported side half-apply** — `2073ed9` [probe] — `grid-column: 2 / 3` then `span x / 5` → `[2, 5]`; `grid-area` and `border-radius` alike. `padding`/`border-width` drop the whole declaration (the documented rule).

**G34 · low · The `var()` depth cap makes results order-dependent** — `9dec0b9` [probe] — a name failed at depth 63 is memoized as `None`: `margin: 2px var(--x)` → `[2,1,2,1]` but `margin: var(--v62, 2px) var(--x)` drops the declaration. Don't memoize depth failures.

**G35 · medium (records/tests) · R04 and R05 have no guard in the suite** [probe] — see the verdicts; the three tests `1e3d37a` added guard nothing and their comments are false. Commit the two hand repros as tests.

**G36 · low · Invalid `<input type>` values build no field** — pre-existing (`bbd0959`) [code] — `type=""`/`type="txet"` build nothing; HTML's invalid-value default is text (the input side of G16).

**G37 · low · F26 and R47 timing/settling tests are flaky** [probe] — `var_names_closure_is_linear` (50 ms) failed 11/48 and `manifests_validate_values` repeatedly under load. Build the failing shape (reversed chain) and assert structure, not wall time; poll the load state.

**G38 · low · `total_lines` can wrap on 32-bit** [traced] — `tracks * count` in `usize` (cascade.rs:2929): on wasm32, `repeat(1431655766, [a] 1px 1px 1px)` wraps under the cap. Use checked arithmetic.

**G39 · low · Doc comments detached again** [code] — `f909c8c` (`calc_props` between `apply_keyword`'s doc and fn), `d723f6a` (`MAX_GRID_TRACKS`/`track_sizing`), `bad8820` (deleted a test's doc), `b345b93` (G41). Same insertion pattern as G02.

**G40 · low · The examples' linear-mode "topmost modal" ignores `z-index`** — `b345b93` [probe] — examples/shared/input.rs:209-213 uses `.ok()?` on `GlobalZIndex` (usually absent), so every modal ties and the last iterated wins. Use `map_or(0)` per component as focus.rs's `active_modal` does; exclude `HtmlNoFocus`; filter hidden roots when no modal is open (pre-existing).

**G41 · low · `step_linear` lost its `#[allow(clippy::too_many_arguments)]` and doc** — `b345b93` [probe] — `fn z_order` was inserted between them (input.rs:204-216).

**G42 · low · Menu: Esc in the Reset dialog sends focus to Volume, not Reset** — pre-existing [probe] — `close_dialog_on_escape` (examples/menu/main.rs:341) relies on focus repair, which falls back to the menu's autofocus. Set focus to Reset as `answer_dialog` does.

**G43 · low · honggfuzz/test-fuzz scripts exit 0 after finding crashes** — pre-existing [code] — both map `timeout`'s 124 to success, and neither fuzzer stops on a crash. Fail when `*.fuzz` / `crashes/` is non-empty.

**G44 · low · Smaller example/CI gaps** [code] — content_lint's forms contexts have no `"`/`\` values (R66 unguarded); AGENTS.md:164/:170 promise Language buttons grid-names and combinators don't have; nightly.yml's `coverage` (:189) and `fuzz-drivers` (:316) save their own caches against cache.yml's consumer rule; markdown/main.rs:92 and inline/main.rs:118 hard-code the reported input; demo's PageUp/PageDown have no gamepad mirror.

### Checked and fine (second pass)

- Positional pairing needs equal child counts below the top level; the cascade's only combinators (descendant, child) can't make a clean subtree's inherited style or F09's `submit` flag stale; comments keep arena offsets aligned; `remap_spec_handles`'s `Arc::get_mut` holds; Failed/Pending renders don't leave a reusable stale skeleton.
- var() resolution order matches AGENTS.md (memo → cycle/depth → context → sheet → fallback); the F25 cycle flag doesn't leak; `raw_var_names` and the selector parser don't panic on Unicode/escapes; substitution budget amplification is linear (1000 × 32 KB in 28 ms); the style-cache key folds a full 64-bit SipHash per ancestor.
- Examples: no double handling; every gamepad mirror and `:focus-visible` rule present; no letter hotkeys in inputs/forms; workflow toolchain pins agree; golden mirrors ci.yml's flags.
