# Testing p23

How to write tests, run them, and find bugs with p23's testing harness. It
covers unit tests, the headless test vectors and oracles, the property
frameworks, the fuzzers, golden images and mutation testing. `AGENTS.md`
(Testing, Testing TODO, Gotchas, Verification) has the history and the
reasons behind each piece; this file is the how-to.

## Quick reference

| What | Command | Time |
|---|---|---|
| Everything that must pass | `cargo test` | ~20–30 s |
| Library unit tests only | `cargo test --lib` | ~1 s |
| Fuzz-harness unit tests | `cargo test --lib --features fuzzing` | ~1 s |
| One test file | `cargo test --test html_ui` | seconds |
| One test | `cargo test --test html_ui layout_oracle` | seconds |
| Warning-free checks | `cargo check --lib`, `cargo build --all-targets`, `cargo doc --no-deps` | |
| Golden images (software Vulkan) | `scripts/golden.sh` (`--update` rewrites references) | ~5 s warm |
| Regenerate CSS/layout oracle data | `scripts/browser_oracle.py [tests/vectors/<name>]` | ~1 s/vector |
| Regenerate Fluent oracle data | `scripts/fluent_oracle.sh [tests/vectors/<name>]` | <1 s |
| Fuzz one target | `scripts/fuzz-libfuzzer.sh <html\|css\|ftl> [seconds]` | 60 s default |
| Other fuzzers | `scripts/fuzz-{honggfuzz,fuzzcheck,test-fuzz}.sh <target> [seconds]` | |
| Mutation testing | `scripts/mutants.sh [--full] [--file src/x.rs]` | ~15 min full run (estimate) |

Rules that always hold:
- `cargo test` and the three warning-free checks must pass before any change
  is done. CI enforces them with `-D warnings`.
- Every real defect gets a regression test *and* a bug report in
  `docs/agents/bugs/` (see [Bug reports](#bug-reports)).
- Expected values are derived by hand from HTML/CSS/Fluent semantics, never
  pasted from actual output. If a test fails, decide whether the code or the
  expectation is wrong before changing either.
- A new test must fail when its feature is broken. Spot-check that by
  breaking the code once, then restore it.

## The layers

| Layer | Where | Catches |
|---|---|---|
| Unit tests | `#[cfg(test)] mod tests` in `src/*.rs` | logic of pure pieces: cascade matching and property mapping, entity decoding, Fluent args, font fallback, the rebuild state machine |
| Test vectors | `tests/html_ui.rs`, `tests/vectors/<name>/` | the HTML/CSS/Fluent/Tera → Bevy world mapping, through the real pipeline, headless |
| CSS oracle | `browser_oracle` test + `browser.json` | p23's computed styles vs headless Chromium |
| Layout oracle | `layout_oracle` test + `layout_*` vectors | Bevy UI's real layout rects vs Chromium's |
| Fluent oracle | `fluent_oracle` test + `fluent.html` | p23's localization vs `@fluent/dom` |
| Metamorphic properties | `tests/properties.rs` (proptest + test-strategy) | relationships that must hold for any input |
| Layout properties | `tests/layout_properties.rs` | flexbox invariants over generated sizes |
| Reference models | `tests/quickcheck.rs` | cascade results vs an independent precedence model |
| Robustness + structure | `tests/arbtest.rs` | arbitrary bytes never panic or wedge; generated documents match an HTML model |
| State machine | `tests/stateful.rs` (proptest-stateful) | rebuild/restyle bugs over random runtime operation sequences |
| Content lint | `tests/content_lint.rs` | problems in the real `assets/` content (skips when `assets/` is absent) |
| Golden images | `tests/golden.rs` (`#[ignore]`d) | real rendering: glyphs, wrapping, 9-slice drawing |
| Fuzzers | `fuzz/`, `honggfuzz/`, `fuzzcheck/`, `test-fuzz/` | panics and hangs in parsing/glue code |
| Mutation testing | `scripts/mutants.sh`, `.github/workflows/mutants.yml` | gaps in all of the above |

The bugs found so far (`docs/agents/bugs/INDEX.md`) came almost entirely from
the generated-input layers and the oracles, not from hand-written examples.
New features should get a vector *and* a property or oracle case.

## Choosing where a test goes

- **Pure function or a small module** (CSS property → Bevy value, entity
  decoding, the rebuild decision): a unit test next to the code. These run in
  milliseconds and pinpoint the arm that broke. Table-driven tests (one row
  per CSS value) work well for mappings.
- **A mapping visible in the built UI** (a block's runs and styles, a node's
  box): a test vector in `tests/html_ui.rs`.
- **Anything CSS can express**: prefer an oracle vector, so Chromium supplies
  the expected values instead of you.
- **Localization behaviour**: a Fluent oracle vector. Deliberate differences
  from fluent-dom get a hand-written vector instead (see
  `fluent_permissive_markup`).
- **"For any X, Y holds"** (shorthand = longhands, restyle = fresh build, no
  overlap after wrap): a property in `properties.rs`, `layout_properties.rs`
  or `quickcheck.rs`.
- **Runtime sequences** (swap theme, remove an override, toggle the outline,
  in any order): an op in `tests/stateful.rs`.
- **Crash resistance of a parser or glue function**: a fuzz harness in
  `src/fuzz.rs` (and the arbtest robustness properties end to end).
- **Real pixels**: a golden scene, sparingly — it's the most brittle layer.

## The harness: `TestUi` (`tests/common/mod.rs`)

Every pipeline-level test uses `TestUi`. It builds a headless Bevy app
(`MinimalPlugins` + assets + images + `HtmlUiPlugin`) with a temporary asset
root, no window and no renderer.

```rust
let mut ui = TestUi::new("name", &[("page.html", page), ("style.css", css)])
    .stylesheet("style.css")                  // DefaultStylesheet
    .locale("locales/en-US/main.ftl.ron")     // ActiveLocale (optional)
    .spawn("page.html", TemplateContext::new(), Node::default());
ui.settle().assert_dump(r#"
html-ui
  p
    "Text" serif 20px #ffffff
"#);
```

- `new(name, files)` writes the files (plus `frame.png`, a 32×24 fixture)
  into a fresh temp dir. Fonts are fake handles labelled `serif`,
  `serif-bold`, `serif-italic`, `serif-bold-italic` (family `Spectral`) and
  `mono` (family `Mono`, also `monospace`); anything else shows as `default`.
- `with_layout(name, files, viewport)` adds real Bevy UI layout and text
  measurement against a fixed-size camera. Text then uses Bevy's embedded
  default font (FiraMono, printable ASCII only); no fake families.
- `from_vector(name)` / `from_layout_vector(name, viewport)` load
  `tests/vectors/<name>/`. A `messages.ftl` there becomes the en-US bundle.
- `settle()` updates until every tracked asset is loaded or failed, a new
  update (build or restyle) happened, and nothing changed for 5 frames. It
  panics after 3000 frames with the current dump. Use `settle_quiet()` when a
  change legitimately causes no update. Use `update(n)` + `builds()` to check
  that something does *not* happen.
- `dump()` is one line per entity: `label [border=t,r,b,l] [padding=…]
  [margin=…] [gap=N] [overflow=x,y] [bg=#hex] [slice=file t,r,b,l mode]
  [visual-box=…]`, then for `Text` nodes one indented line per run
  (`"text" face size color`). Labels are `tag#id.class…`, `-` for anonymous
  nodes, `html-ui` for the root.
- `layout_dump()` prints `label x,y wxh` per node (needs `with_layout`);
  `node_rect(world, entity)` gives one rect.
- `load::<T>(path)` tracks an asset for `settle()`. `world_mut()`, `root()`
  and `with_file(path, bytes)` cover the rest.

Pitfalls:
- `settle()` never returns if a stylesheet's `url()` image is missing (the
  sheet loads, its dependency fails). Check the sheet first, as
  `content_lint` does.
- Don't run `rustfmt` on a test file: it follows `mod common;` and
  reformats the shared harness. Use `--config skip_children=true`.
- Each file in `tests/` is its own test binary and can't see the others'
  code: shared helpers go in `tests/common/mod.rs`, marked
  `#[allow(dead_code)]` (not every binary uses every helper).

## Writing test vectors (`tests/html_ui.rs`)

A vector is inputs plus the expected dump. Keep inline vectors small. Put
anything an oracle should check in `tests/vectors/<name>/`: `page.html`,
`style.css`, optionally `messages.ftl`.

Steps:
1. Write the inputs.
2. Derive the expected dump by hand. For layout, FiraMono advances 0.6 em
   per character and lines are 1.2 em tall (20 px text: 12 px per character,
   24 px lines).
3. Run it. If it fails, find out who is wrong before editing anything.
4. Break the feature once and check that the vector fails.

## The oracles

The oracles replace "the expected value comes from the same head as the code"
with an independent reference. Their outputs are committed, so running the
tests needs no browser or Node.

### CSS and layout oracle (`scripts/browser_oracle.py`)

The script loads each vector in headless Chromium as `* { all: unset }` +
`P23_CSS` + the vector's `style.css` + `page.html`, and records
`getComputedStyle` per element (and `getBoundingClientRect` for `layout_*`
vectors) into `browser.json`.

- `P23_CSS` expresses p23's own defaults and layout model as CSS: root and
  containers are flex columns, blocks are blocks, nothing shrinks, white
  text in Bevy's default font at line height 1.2, the `li` indent and
  bullet, `pre` with 8 px padding. Keep it in sync with `src/build.rs`.
- Vectors must be plain HTML (no Tera syntax, no `data-l10n-id`). Set
  `color` on `html` (browsers default to black) and `border-style: solid`
  wherever border widths matter (p23 ignores `border-style`).
- Layout vectors (`layout_*`) must not set `font-family`, must be ASCII
  only, and should use font sizes whose 1.2 line height is a whole number:
  Bevy rounds every text node up to a whole pixel, browsers don't.
- `browser_oracle` compares per character (face, size, color) and per
  root/block/container box. `layout_oracle` compares rects within 1 px.
  Deliberate differences are listed above `FIXTURE_SIZE` in
  `tests/html_ui.rs`; add new ones there with a reason.
- After changing a vector: regenerate, review the `browser.json` diff (one
  record per line), commit it.

### Fluent oracle (`scripts/fluent_oracle.sh`)

The script runs `@fluent/dom` in jsdom over each vector's `page.html` +
`messages.ftl` and writes the translated DOM to `fluent.html`, with
fluent-dom's warnings as comments at the top. The `fluent_oracle` test
requires p23's localized build to equal p23's build of `fluent.html`, and
prints a line diff (`-` fluent-dom, `+` p23).

Deliberate differences (no sanitizing of translation markup, escaped string
arguments, fluent-rs number formatting) stay out of oracle vectors and are
pinned by hand-written vectors.

## Property tests

All property tests run on every `cargo test`, so keep case counts low (the
whole suite should stay around 20–30 s). Name each test after the property
and say in its doc comment which bug class it catches.

### proptest + test-strategy (`tests/properties.rs`, `tests/layout_properties.rs`)

```rust
#[proptest(cases = 24)]
fn padding_shorthand_equals_longhands(
    #[strategy(0u8..40)] t: u8,
    #[strategy(0u8..40)] r: u8,
) {
    // build two TestUis, prop_assert_eq! their dumps
}
```

- Prefer **metamorphic** properties: two inputs that must produce the same
  world (shorthand vs longhands, a restyle vs a fresh build, a selector list
  vs expanded rules). They need no expected values.
- Layout properties assert invariants (mirroring, centring, ratios, no
  overlap) with a 1 px tolerance.
- Failures are shrunk and saved to `tests/<file>.proptest-regressions`
  (integration tests) or `proptest-regressions/<module>.txt` (unit tests).
  Commit those files: proptest replays them first on every run.
- More cases for one run: `PROPTEST_CASES=500 cargo test --test properties`.
- proptest is pinned to `=1.5.0` (proptest-stateful needs it); test-strategy
  stays at 0.3.1.

### quickcheck (`tests/quickcheck.rs`)

Use it for structured inputs checked against a small independent reference
model (`cascade_winner_matches_precedence_model`,
`selector_lists_cascade_per_element_and_property`). quickcheck 1.1's `Gen`
RNG is private: build `Arbitrary` impls from `T::arbitrary(g)` and
`g.choose`. The `#[quickcheck]` attribute comes from `quickcheck_macros`.

### arbtest (`tests/arbtest.rs`)

- **Robustness**: arbitrary bytes, lossily decoded, as HTML, CSS, FTL and
  `data-l10n-args`. The pipeline must settle and always leave output (content
  or the `failed to render:` paragraph).
- **Structure-aware**: `generated_documents_build_the_html_model` generates
  well-formed documents and compares the built tree against an HTML model.
- Each property runs for a time budget (`.budget_ms(400)`). On failure,
  arbtest prints a seed: replay with `.seed(0x…)` on the same property while
  debugging.

### proptest-stateful (`tests/stateful.rs`)

Random sequences of runtime operations (`SetN`, `SetTheme`, `SetOwnSheet`,
`RemoveOwnSheet`, `SetLocale`, `ToggleOutline`, `SwapFonts`) against one
long-lived `HtmlUi`, each checked against a reference model after it settles.

To add an operation:
1. Add the variant to `Op` and a generator in `op_generators`.
2. Update the model in `next_state` and the expected dump.
3. Add a precondition if an op could legitimately cause no update.
4. Apply it in the system-under-test runner, then `settle()` (or
   `settle_quiet()` when no update is expected).

Shrinking only removes operations; it doesn't shrink values inside one.
Some failures are timing-dependent (one was 1 in 3): stress-run with
`PROPTEST_CASES=200 cargo test --test stateful` a few times.

### Unit tests and the rebuild state machine

The decision of when to rebuild or restyle is a pure function
(`RebuildState::decide` in `src/rebuild.rs`). Test new update triggers there,
not through full apps: `rebuild::tests` checks every load-state combination
and a proptest over frame sequences ("no change is lost, none is invented").
Proptest works inside `#[cfg(test)]` modules too.

## Content lint (`tests/content_lint.rs`)

It checks the real `assets/` content: locale message parity, templates
render with the examples' data, every `data-l10n-id` resolves, no hard-coded
text, CSS parses and its `url()`s exist, and a generated pseudo-locale
(`en-XA`) leaves no untranslated text. `assets/` is gitignored, so without it
each content test prints a skip note and passes.

New template: add it to `PAGES`. A deliberately unlocalized template gets
`unlocalized: Some(reason)`.

## Golden images (`tests/golden.rs`, `scripts/golden.sh`)

Three scenes in `tests/golden/<scene>/` are rendered offscreen by full Bevy
and compared with `expected.png`. The script pins Mesa's lavapipe (software
Vulkan, bit-identical run to run); on Arch without it installed, it fetches
the matching package into `target/golden-lavapipe/`.

- A pixel differs above channel delta 8; a scene fails above 0.05%
  differing pixels. One changed letter is about 0.1%.
- Failures write `target/tmp/golden/<scene>.{actual,diff}.png`.
- `scripts/golden.sh --update` rewrites the references. Look at the PNGs
  before committing them.
- Use only committed inputs: Bevy's default font and `tests/fixtures/`.
- Keep it to a few scenes: it's a smoke check, not a spec.

## Fuzzing

All fuzzers drive the `#[doc(hidden)]` `p23::fuzz` harness (feature
`fuzzing`), which calls the internal glue directly: `render_html`,
`cascade`, `translate`. Contract: arbitrary input may return `Err` or any
output, but must never panic, hang or abort.

Input formats (bytes are decoded lossily, capped at 64 KiB):
- `html`: template source, optionally `\n---\n` + a JSON context.
- `css`: the whole input is a stylesheet.
- `ftl`: `FTL body\n---\nmessage id\n---\nargs JSON`.

| Driver | Run | Notes |
|---|---|---|
| cargo-fuzz (libFuzzer + ASan) | `scripts/fuzz-libfuzzer.sh <target> [s]` | the main one; also nightly in CI. Passes `--target` (a prebuilt cargo-fuzz defaults to musl, which ASan rejects) |
| honggfuzz | `scripts/fuzz-honggfuzz.sh <target> [s]` | vendored, patched crate; needs the linker override the script sets |
| fuzzcheck | `scripts/fuzz-fuzzcheck.sh <target> [s]` | vendored, patched for LLVM 21+ coverage records |
| test-fuzz (AFLplus) | `scripts/fuzz-test-fuzz.sh <target> [s]` | corpus seeds come from plain `cargo test` runs |

All need a nightly toolchain. Corpora and crashes live in gitignored
directories (`fuzz/corpus/`, `fuzz/artifacts/`, `hfuzz_workspace/`,
`test-fuzz/target/`).

To add a fuzz target: add the glue function to `src/fuzz.rs`, then a target
per driver. Regression tests for fuzz-found crashes go in `src/fuzz.rs`'s
tests (run with `--features fuzzing`) or next to the fixed code.

## Mutation testing (`scripts/mutants.sh`)

cargo-mutants makes small changes to the library code and reports any that no
test catches.

- `scripts/mutants.sh` runs unit + `html_ui` + `stateful` (the same set as
  the weekly CI run); `--full` adds the property suites; `--file src/x.rs`
  narrows it. Results land in `target/mutants.out/`
  (`caught/missed/timeout/unviable.txt`); reruns skip caught mutants.
- It's fast because of three measured settings (see `[profile.mutants]` in
  `Cargo.toml`): only the tests that run are built (`--cargo-arg`, not
  `--cargo-test-arg`, which reaches only the test run), no debug info with
  optimized dependencies, and Bevy linked dynamically. A mutant costs about
  4–8 s build + 0.5 s test after a one-time ~150 s dependency build per job.
- The script moves the fuzz drivers' build caches aside during the run
  (cargo-mutants copies the tree and ignores nested `.gitignore`s) and puts
  them back on exit.
- `-j` and `--minimum-test-timeout` are set by the script: use
  `P23_MUTANTS_JOBS` and `CARGO_MUTANTS_MINIMUM_TEST_TIMEOUT`.
- To rerun specific survivors: `--re '^(src/build\.rs:625:9|…):'` anchored on
  `file:line:col`. cargo-mutants 27.1 doesn't apply `--re` to
  struct-field-deletion mutants; they always run.
- Under heavy parallel load a mutant can hit the test timeout without being
  a hang. Rerun timeouts alone with a longer limit before triaging.

Triage every survivor: write a test that catches it, or document in
`AGENTS.md` (Testing TODO 6) why the mutant changes no behaviour (typically:
it only changes whether a `debug!` line is logged). Copy survivors into
`AGENTS.md`: `target/mutants.out/` is overwritten by every run. For the
weekly CI run: `gh run download <run-id>` and read each shard's
`missed.txt`.

## Discovering bugs: the workflow

1. **Find a candidate.** A failing property, an oracle difference, a fuzz
   crash, a mutation survivor, or a stateful sequence.
2. **Reproduce it deterministically.**
   - proptest: the shrunk case is printed and saved to the regressions file.
   - arbtest: replay the printed seed.
   - cargo-fuzz: `fuzz/artifacts/<target>/crash-…`; minimize with
     `cargo +nightly fuzz tmin <target> <artifact>`.
   - stateful: the minimal op sequence is printed; replay it as a small
     throwaway test if it's timing-dependent.
   - oracle diff: the failure names the element and both values.
3. **Decide who's wrong.** Oracle and spec beat p23 unless the difference is
   deliberate. If it is, list it as an allowed difference with a reason.
4. **Write the regression test first**, watch it fail, then fix the code.
   Prefer the fastest layer that catches it (a unit test over a full app).
5. **Check the fix doesn't just move the bug.** Run the whole suite; for
   rebuild/load-state bugs also stress the state machine.
6. **File the bug report** (below).

Typical bug classes found so far, and the layer that found them:
- Change detection and rebuild triggers (removal events, failed or reloading
  assets, signals lost while loading): proptest-stateful.
- Byte slicing across multi-byte characters: the fuzzers.
- Semantic mismatches with CSS/Fluent (font fallback, `box-sizing`, missing
  overlays): the oracles.
- Whitespace and document order: the structure-aware arbtest model.
- Coverage gaps and a wrapping bug: mutation-testing triage.

## Bug reports

Every real defect gets `docs/agents/bugs/bug_NNNN.md` (next number in
discovery order) and a row in `docs/agents/bugs/INDEX.md`. Include:
- status and severity (reachable from user data = high, even if it only
  panics), component, symptoms (what's observable, not the cause)
- discovery: tool or target, seed or op sequence, commit, agent/model, date
- a minimal reproduction (minimized, not a raw fuzzer artifact)
- root cause, fix (with commit), and the regression test that guards it
- for upstream bugs: affected and pinned versions, and where the local patch
  is wired (`[patch.crates-io]` + `vendor/`)

Use the existing reports as templates.

## CI

- `.github/workflows/ci.yml` (every push and PR, blocks merges): the
  warning-free checks, `cargo test`, the `fuzzing`-feature tests, and that
  `fluent.html` matches what the script generates.
- `nightly.yml`: golden images on Ubuntu's lavapipe, the browser oracle
  against the runner's Chrome, 60 s of cargo-fuzz per target, and builds of
  the other fuzz drivers. Failures are reports to triage, not blockers.
- `mutants.yml` (weekly, 4 shards): informational. Survivors are in each
  shard's job summary and the `mutants-shard-N` artifacts.

Trigger the scheduled ones by hand with `gh workflow run nightly.yml` or
`gh workflow run mutants.yml`, and read logs with
`gh run view --job=<id> --log-failed`.
