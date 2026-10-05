# Bug Reports — Index

Upstream defects (in dependencies and tools) are also tracked in
[UPSTREAM.md](UPSTREAM.md) until they're reported and fixed upstream.

The crate was called `p23` until 2026-10-05 and is now `bevy_markup`; the
reports keep the name, paths and commands as they were at the time
(`p23::fuzz` is now `bevy_markup::fuzz`, `P23_*` variables are
`BEVY_MARKUP_*`).

Bugs found by the automated testing infrastructure. One file per bug,
`bug_NNNN.md`, numbered in discovery order. This page is the title/metadata
record; each discovery session has its own metadata section below
(bug_0001–0005: property testing, stateful testing and fuzzing — the fuzzers
immediately paid for themselves; bug_0006–0007: the Fluent oracle; bug_0008–0013: property tests and the layout oracle; bug_0014: mutation-testing triage; bug_0015: the fuzz-corpus coverage replay; bug_0016–0017: demo crashes with nested `HtmlUi`s; bug_0020: the picking-driven signals tests).

## Bugs

| ID      | Title | Component | Severity | Status | Found by |
|---------|-------|-----------|----------|--------|----------|
| [bug_0001](bug_0001.md) | Stylesheet that fails to load leaves the `HtmlUi` permanently blank | `build.rs` (pipeline) | high | fixed (`9e87b67`) | `arbtest` (tests/arbtest.rs) |
| [bug_0002](bug_0002.md) | Non-ASCII selector panics `Compound::parse` (byte slice across char boundary) | `cascade.rs` | high | fixed (`017cfac`) | cargo-fuzz `css` target |
| [bug_0003](bug_0003.md) | Removing `HtmlDebugOutline` never triggers a rebuild | `build.rs` (pipeline) | medium | fixed (`06b86ae`) | `proptest-stateful` (tests/stateful.rs) |
| [bug_0004](bug_0004.md) | Global failed-stylesheet latch swallows a later re-select of the same broken sheet | `build.rs` (`FailedSheets`) | medium | fixed (`06b86ae`) | `proptest-stateful` (tests/stateful.rs) |
| [bug_0005](bug_0005.md) | fluent-syntax panics on an invalid unicode escape before a multi-byte character | upstream `fluent-syntax` 0.11.1 | high | fixed in the fluent-syntax fork (git patch); **upstream unfixed**, PR pending | honggfuzz `ftl` target |
| [bug_0006](bug_0006.md) | `data-l10n-id` on containers and inline elements is silently ignored | `build.rs` (translation walk) | medium | fixed (`94a4fb9`) | Fluent oracle (`fluent_oracle`) |
| [bug_0007](bug_0007.md) | `data-l10n-name` overlays unsupported: source attributes lost | `build.rs` (translation walk) | medium | fixed (`94a4fb9`) | Fluent oracle (`fluent_oracle`) |
| [bug_0008](bug_0008.md) | Removing a per-entity `HtmlStylesheet` never rebuilds | `src/build.rs` | medium | fixed (`0d9d6cb`) | targeted probe while designing proptest-stateful ops |
| [bug_0009](bug_0009.md) | Entity with a failed `HtmlStylesheet` ignores `DefaultStylesheet` swaps | `src/build.rs` | medium | fixed (`0d9d6cb`) | probe, then proptest-stateful |
| [bug_0010](bug_0010.md) | Re-requesting a failed stylesheet eats the change signal | `src/build.rs` | medium | fixed (`0d9d6cb`) | proptest-stateful, minimal `[SetLocale(En), SetOwnSheet(Broken), SetTheme(Broken)]`, ~1 in 3 runs |
| [bug_0011](bug_0011.md) | NBSP and other Unicode spaces were collapsed and trimmed | `src/build.rs` | medium | fixed (`0d9d6cb`) | arbtest structure-aware document model |
| [bug_0012](bug_0012.md) | `HtmlElements` lookups were breadth-first, not document order | `src/html.rs` | medium | fixed (`0d9d6cb`) | unit test + arbtest model |
| [bug_0013](bug_0013.md) | CSS padding on a boxed `pre` added to the default 8px | `src/build.rs` | low | fixed (`0d9d6cb`) | code reading while adding layout support; confirmed by the layout oracle |
| [bug_0014](bug_0014.md) | Text in a boxed block never wraps: it overflows the box | `src/build.rs` | medium | fixed (`311c808`) | mutation-testing triage (layout oracle case) |
| [bug_0015](bug_0015.md) | Deeply nested FTL expressions overflow the stack | upstream `fluent-syntax` 0.11.1 | medium | fixed in the fluent-syntax fork (git patch; first fix missed nested calls); **upstream unfixed**, PR pending | fuzz-corpus coverage replay |
| [bug_0016](bug_0016.md) | Rebuilding an `HtmlUi` under a rebuilding ancestor `HtmlUi` panics (commands on despawned entities) | `src/build.rs` | high | fixed (`9d21b17`) | user report (demo locale click), minimized to a vector |
| [bug_0017](bug_0017.md) | Restyle fallback rebuild despawned app-nested `HtmlUi`s (same_shape counted them as shape mismatches) | `src/build.rs` (`same_shape`) | high | fixed (`d079b08`) | startup crash of the demo, traced via `bevy/debug` |
| [bug_0018](bug_0018.md) | An app `ImageNode` on a built element turned every restyle into a rebuild (rebuild every frame) | `src/build.rs` (`same_shape`) | high | fixed (`f99c6ec`) | prototype_19 integration (entity-id sampling over BRP) |
| [bug_0019](bug_0019.md) | A value-less attribute (`autofocus`) ate the next attribute's first character | upstream `tl` 0.7.8 (UPSTREAM.md U11) | high | fixed (`f81acce`): dependency switched to `astral-tl` 0.8.0 | prototype_19 integration (gamepad smoke run) |
| [bug_0020](bug_0020.md) | `pointer-events: none` didn't stop clicks on a block's text (spans carried no `Pickable`) | `src/build.rs` (text spans) | medium | fixed (`96f2cd3`) | new picking-driven signals tests (`tests/signals.rs`) |

## Discovery session metadata: bug_0001–0005

### When / where

- **Date:** 2026-10-04, 09:00–10:30 +04:00
- **Machine:** hostname `anne`; Arch Linux, kernel `7.2.7-arch1-1`
  (`#1 SMP PREEMPT_DYNAMIC Mon, 21 Sep 2026 18:51:14 +0000`), x86_64
- **CPU:** AMD Ryzen 7 9800X3D 8-Core (16 threads)
- **Memory:** 60 GiB

### Repository state

- **Repo:** `p23` (Bevy 0.19 HTML/CSS/Fluent UI library), working tree at
  `/home/a/fs/Bevy/PROTOTYPE_23/p23`
- **Session start HEAD:** `969556c` — "Check test vectors against real browser"
- All bugs were found in uncommitted working-tree state built on top of that
  commit; the fixes landed in the session's commits:
  `c59357f` → `042782f` (see each bug file for its exact fix commit).

### Toolchain

- rustc stable `1.98.1 (48a229cea 2026-09-01)` (library, tests)
- rustc nightly `1.100.0-nightly (4aa1fbcf4 2026-09-08)` (fuzz drivers)
- cargo-fuzz `0.13.2` (libFuzzer + AddressSanitizer)
- honggfuzz-rs `0.5.62` (vendored native honggfuzz, patched `bfd.c`)

### Dependency versions under test

- bevy `0.19.1` (minimal features: `ui_api`, `default_font`, `bevy_log`)
- bevy_fluent `0.15`, fluent `0.16.1`, fluent-syntax `0.11.1`
- lightningcss `1.0.0-alpha.72`, tera `2.4.0`, tl `0.7.8`

### Discovery agent

- Model: **`zai/glm-5.3-flash:high`** (LLM coding assistant), driving the
  fuzzers/tests interactively; every reported crash was reproduced
  deterministically and minimized before a fix was written.

### Common contract for the discovery harnesses

All fuzz targets drive the feature-gated `p23::fuzz` harness
(`#[doc(hidden)]`, `--features fuzzing`): arbitrary `&str` inputs may return
`Err` or produce any output, but must never panic, hang, or abort. The
state-machine tests (`tests/stateful.rs`) require each applied operation to
settle (rebuild observed) and the resulting dump to match a reference model.

## Discovery session metadata: bug_0006–0007

- **Date:** 2026-10-04, ~12:00 +04:00; same machine (`anne`) and Rust
  toolchain/dependency versions as above.
- **Session start HEAD:** `1169ca4` — "Add scripts for running fuzzers";
  found and fixed in the working tree on top of it, committed as `94a4fb9`.
- **Harness:** the Fluent oracle — `scripts/fluent_oracle.sh` runs
  `@fluent/dom` 0.10.2 + `@fluent/bundle` 0.19.1 in jsdom 30.1.1 (Node
  v26.10.0) over `tests/vectors/fluent_*`, writing `fluent.html`; the
  `fluent_oracle` test requires p23's localized build to equal p23's build of
  that reference DOM.
- **Discovery agent:** model **`anthropic/claude-opus-5-5:high`**. Of the
  first run's eight differences, two were these bugs; the other six were
  triaged with the user as deliberate (no sanitizing, escaped string args,
  fluent-rs number formatting) and are pinned by hand-written vectors.

## Discovery session metadata: bug_0008–0013

- **Date:** 2026-10-04, ~12:00–13:00 +04:00; same machine and toolchain.
- **Session start HEAD:** `e1b910c` — "Add browser based layout oracle".
- **Harnesses:** proptest-stateful ops for `HtmlStylesheet`/`FontFamilies`, a structure-aware arbtest document model (PropertyTests subagent), and the layout oracle extended to flex/sizes/margins.
- **Discovery agent:** `anthropic/claude-opus-5-5:high` with subagents.

## Discovery session metadata: bug_0020

- **Date:** 2026-10-05; same machine (`anne`) and toolchain.
- **Session start state:** working tree mid-development (uncommitted
  signals/focus work on top of the signals bindings).
- **Harness:** the new picking-driven test harness — `TestUi::with_pointer`
  (`tests/common/mod.rs`) spawns a primary window and aims the layout camera
  at it, then the tests write real `WindowEvent`s (`CursorMoved`,
  `MouseButtonInput`) so Bevy's full picking stack runs end to end
  (`PointerInputPlugin` → UI picking backend → pointer events → bevy_markup's
  observers and hover tracking). The `pointer-events: none` test failed on
  its first run; component dumps (`Pickable` was correctly on the block and
  its text node) pinned the miss to the span entities.
- **Discovery agent:** model `zai/glm-5.3-flash:high`.

## Discovery session metadata: bug_0016

- **Date:** 2026-10-05, ~01:50 +04:00; same machine (`anne`) and toolchain.
- **Session start HEAD:** `955e6ea` — "Use system fonts for examples, add
  ./examples/assets", plus the uncommitted HTML-only demo rewrite (a
  full-screen shell `HtmlUi` with content documents nested into its slots).
- **Harness:** the user ran `cargo run --example demo` and clicked a language
  button; the agent minimized it to a headless vector
  (`nested_ui_under_a_rebuilding_ancestor_is_skipped_not_panics`) that
  panicked identically.
- **Discovery agent:** model `zai/glm-5.3-flash:high`.

## Discovery session metadata: bug_0017

- **Date:** 2026-10-05, morning +04:00; same machine (`anne`) and toolchain.
- **Session start HEAD:** `8952749` — "Add ElementSignal bindings in HTML",
  plus the uncommitted `:hover`/`:active` work; fixed with it in `d079b08`.
- **Harness:** the demo crashed at startup (no input); traced with
  `--features bevy/debug` and `bevy_markup=debug` rebuild-decision logs, then
  pinned by the vector `restyle_with_nested_uis_keeps_them`.
- **Discovery agent:** model `zai/glm-5.3-flash:high`.
