# Bug Reports — Index

Bugs found by the automated testing infrastructure. One file per bug,
`bug_NNNN.md`, numbered in discovery order. This page is the title/metadata
record; each discovery session has its own metadata section below
(bug_0001–0005: property testing, stateful testing and fuzzing — the fuzzers
immediately paid for themselves; bug_0006–0007: the Fluent oracle).

## Bugs

| ID      | Title | Component | Severity | Status | Found by |
|---------|-------|-----------|----------|--------|----------|
| [bug_0001](bug_0001.md) | Stylesheet that fails to load leaves the `HtmlUi` permanently blank | `build.rs` (pipeline) | high | fixed (`9e87b67`) | `arbtest` (tests/arbtest.rs) |
| [bug_0002](bug_0002.md) | Non-ASCII selector panics `Compound::parse` (byte slice across char boundary) | `cascade.rs` | high | fixed (`017cfac`) | cargo-fuzz `css` target |
| [bug_0003](bug_0003.md) | Removing `HtmlDebugOutline` never triggers a rebuild | `build.rs` (pipeline) | medium | fixed (`06b86ae`) | `proptest-stateful` (tests/stateful.rs) |
| [bug_0004](bug_0004.md) | Global failed-stylesheet latch swallows a later re-select of the same broken sheet | `build.rs` (`FailedSheets`) | medium | fixed (`06b86ae`) | `proptest-stateful` (tests/stateful.rs) |
| [bug_0005](bug_0005.md) | fluent-syntax panics slicing FTL source inside a multi-byte character | upstream `fluent-syntax` 0.11.1 | high | fixed locally via vendored patch (`042782f`); **upstream unfixed** | honggfuzz `ftl` target |
| [bug_0006](bug_0006.md) | `data-l10n-id` on containers and inline elements is silently ignored | `build.rs` (translation walk) | medium | fixed (uncommitted, on `1169ca4`) | Fluent oracle (`fluent_oracle`) |
| [bug_0007](bug_0007.md) | `data-l10n-name` overlays unsupported: source attributes lost | `build.rs` (translation walk) | medium | fixed (uncommitted, on `1169ca4`) | Fluent oracle (`fluent_oracle`) |

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
  found and fixed in the uncommitted working tree on top of it.
- **Harness:** the Fluent oracle — `scripts/fluent_oracle.sh` runs
  `@fluent/dom` 0.10.2 + `@fluent/bundle` 0.19.1 in jsdom 30.1.1 (Node
  v26.10.0) over `tests/vectors/fluent_*`, writing `fluent.html`; the
  `fluent_oracle` test requires p23's localized build to equal p23's build of
  that reference DOM.
- **Discovery agent:** model **`anthropic/claude-opus-5-5:high`**. Of the
  first run's eight differences, two were these bugs; the other six were
  triaged with the user as deliberate (no sanitizing, escaped string args,
  fluent-rs number formatting) and are pinned by hand-written vectors.
