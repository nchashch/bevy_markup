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

Three of the seven slices were killed by a provider rate limit before
reporting: **build/reuse** (`build.rs`, `rebuild.rs`), **cascade**
(`cascade.rs`, `style.rs`, `fonts.rs`) and **examples/CI**. What survived
of them: the examples/CI slice's command logs (used below), and one
build-slice probe showing the F04 property now reuses (`reused=4 of 9`
specs per build). No guard checks were run for F14, F15, F20, F23, F24,
F25 (beyond the records slice's probe), F26, F38, F39, R04 and R05. **Re-run
those three slices** before trusting the fixes there.

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
| R64 | fixed, guarded | — | The example test fails on revert (fix session's probe; CI runs `cargo test --example grid-names`? see G03). |
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
- Commit `1e3d37a`'s message claims the R04/R05 tests "pin the shapes … broken on the revert". The session's own transcript shows the R04 revert probe was not the R04 fix and passed, and the R05 revert patch never applied. Those guards are **unverified** (this review's build slice died before checking).

**G24 · medium · bug_0081–0087 are wrong or thin** [probe]/[code]
- Discovery agent `opencode-zen/glm-5.3-flash` and commit `b224619` are wrong: the F findings came from the opus review at `a07a12a`.
- bug_0082's symptom is inverted (before `ae27fc3`, `type="TEXT"` built *no* field) and omits that it regressed from `bbd0959`.
- bug_0085 cites a test that passes on revert (`repeat_names_register` is the guard) and an example that can't trigger it (names inside `auto-fill` never register; real: `repeat(2, [a] 1fr [a])`).
- bug_0081's example `repeat(4000000000, auto)` never panicked (real: `repeat(40000, [a] 1fr)`).
- bug_0087's root cause is false (F32 row). All seven lack a minimal repro and root cause (AGENTS.md requires both). No INDEX table rows.

**G25 · medium · review_0002's "Records and documentation" list largely not acted on** [code] — bug_0077 still documents the replaced off-screen clamp as the fix; bug_0058 likewise for F08; review_0001's header still says findings are open; INDEX:6-8 (one report per date, "latest is review_0001"); AGENTS.md:58 (features), :412 and ADR 0021:212 ("only the retry counter ticks"), :447 ("walks recurse per level"), :1270 (review filenames), Limits :1306 (silent `_ => {}`); CHANGELOG's RemSize "same frame" and "only `type="submit"` submits" (a typeless button submits); no CHANGELOG lines for F14 or F27; `MAX_VAR_DEPTH`/`MAX_GRID_TRACKS` undocumented; stale code comments (Skeleton "16, 64, 256", build.rs:891 "suppression below", nine_slice.rs:89-92's detached doc comment).

## Refuted or unconfirmed

- **lightningcss `calc()` exponential parse** (security slice, [traced]): not reproduced. Release-build parse of `p{width:` + `calc(`×20 + `1px` + `)`×20 + `}` takes 2.5 µs; nested parentheses to depth 18 ~2 µs. The deep-paren stack overflow (20 000 levels) was not run.
- **`world` example panics** in the examples slice's bounded run (`ResMut<Assets<Mesh>>` missing): the run mode wasn't recorded; under `--no-render` it is F36's documented limit.

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

1. **CI:** G02, G03 (and make R47's test wait for the load).
2. **Security:** G01, then G04, G08, G14.
3. **Re-run the three lost slices** (build/reuse incl. the R04/R05 guards, cascade, examples/CI).
4. **Regressions:** G05, G07, G09, G10.
5. **Records:** G23–G25 — correct them in the same change as each fix; claim a guard only after reverting exactly that fix and watching the test fail.
