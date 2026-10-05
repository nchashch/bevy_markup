# Upstream issues

Defects in dependencies and tools that bevy_markup works around locally, tracked so
they can be reported upstream (issue and/or pull request) and the local
workarounds removed once upstream releases a fix. bevy_markup's own bugs are in
`INDEX.md`; entries here link to a bug report when one exists.

Status per entry: **unreported** → **reported** (issue link) → **PR open**
(link) → **fixed upstream** (version) → **workaround removed** (commit).
Before reporting, re-check the latest upstream release and default branch:
the issue may already be fixed or reported.

## Summary

| # | Project | Issue | Impact on bevy_markup | Local workaround | Status |
|---|---|---|---|---|---|
| U1 | fluent-syntax (fluent-rs) | Slicing panics inside multi-byte characters ([bug_0005](bug_0005.md)) | panic from any FTL asset | vendored patch | unreported |
| U2 | fluent-syntax (fluent-rs) | Unbounded placeable nesting overflows the stack ([bug_0015](bug_0015.md)) | stack overflow (abort) from a few KB of FTL | vendored patch | unreported |
| U3 | honggfuzz-rs | Bundled honggfuzz doesn't build against current binutils | honggfuzz driver doesn't build | vendored patch | unreported |
| U4 | fuzzcheck | Coverage sensor misreads LLVM 21+ `__llvm_prf_data` | fuzzcheck driver gets no usable coverage | vendored patch | unreported |
| U5 | fuzzcheck | `observing_only_files_from_current_dir` drops absolute paths | fuzzcheck observes no project code | vendored patch | unreported |
| U6 | fuzzcheck | `fuzzcheck_mutators_derive` 0.13.0 uses let-chains with edition 2021 | fuzzcheck driver doesn't build | vendored, edition bumped | unreported |
| U7 | cargo-mutants | `--re` / `--exclude-re` don't filter struct-field-deletion mutants | targeted reruns run unrelated mutants (~5× slower) | none (documented) | unreported |
| U8 | cargo-fuzz | Prebuilt (musl) binary defaults `--target` to musl, which ASan rejects | CI fuzz jobs failed to build | `--target "$host"` in `scripts/fuzz-libfuzzer.sh` | unreported |
| U9 | Bevy 0.19 | `UiPlugin`'s `viewport_picking` panics without the picking plugins | headless UI layout tests need extra plugins | add `DefaultPickingPlugins` in `tests/common` | discuss first |

## U1 — fluent-syntax: slicing panics inside multi-byte characters

- **Project:** [projectfluent/fluent-rs](https://github.com/projectfluent/fluent-rs),
  crate `fluent-syntax`. Affected: 0.11.1 (pinned by bevy_fluent 0.15 via
  fluent 0.16 / fluent-bundle 0.15) and 0.12.0 (latest checked, 2026-10-04).
- **Problem:** the parser computes some byte ranges by guessing (`ptr - 1`,
  fixed windows like `[..100]` for error context), and `Slice::slice` for
  `String`/`&str` applies them with `&self[range]`. A range that lands
  inside a multi-byte character panics.
- **Reproduction:** parse `u={"\U` followed by a multi-byte character
  (e.g. U+FFFD): `FluentResource::try_new("u={\"\\U\u{fffd}".to_owned())`
  panics with "byte index … is not a char boundary". A second variant: a
  long error context of multi-byte characters
  (`format!("x = {}{{{}", "é".repeat(48), "é")`).
- **Local fix:** `vendor/fluent-syntax/src/parser/slice.rs` (`PATCH(bevy_markup)`)
  rounds ranges up to the next char boundary, wired via `[patch.crates-io]`
  in the root, `fuzz/`, `honggfuzz/` and `fuzzcheck/` manifests.
  Regression tests: `src/fuzz.rs`
  `broken_unicode_escape_after_multibyte_char_does_not_panic`,
  `long_multibyte_error_context_does_not_panic`.
- **Upstream PR sketch:** the clamp in `Slice::slice` is the minimal fix;
  the better fix is to compute char-aligned ranges at the call sites. Add
  the two inputs above as parser test fixtures.

## U2 — fluent-syntax: unbounded placeable nesting overflows the stack

- **Project:** as U1. Affected: 0.11.1 and 0.12.0 (latest checked,
  2026-10-04; `get_placeable` recurses without a depth check).
- **Problem:** placeables nest recursively (placeable → expression → inline
  expression or select variant → placeable) with no limit. Measured overflow
  depths: debug 200–400 levels on a 2 MB stack (800–1600 on 8 MB), release
  1600–3200 on 2 MB (6400–12800 on 8 MB). A few KB of `{{{…}}}` abort any
  process that parses it, e.g. a Bevy app loading a translation on a worker
  thread.
- **Reproduction:**
  `FluentResource::try_new(format!("x = {}\"a\"{}\n", "{".repeat(100_000), "}".repeat(100_000)))`
  aborts with a stack overflow.
- **Local fix:** `vendor/fluent-syntax/src/parser/core.rs` (`PATCH(bevy_markup)`):
  a depth counter in `get_placeable`, the single choke point for both
  `parse` and `parse_runtime`, returns the new
  `ErrorKind::PlaceableNestingTooDeep` past `MAX_PLACEABLE_DEPTH` = 100; the
  entry becomes Junk. Regression tests: `src/fuzz.rs`
  `deeply_nested_placeables_are_an_error_not_a_stack_overflow`,
  `nesting_up_to_the_limit_still_works`.
- **Upstream PR sketch:** the patch as is. Open questions for maintainers:
  the limit (100) and whether a new `ErrorKind` variant is acceptable
  (it's a public enum without `#[non_exhaustive]`, so adding a variant is
  technically a breaking change for exhaustive matches). Also check whether
  fluent-bundle's resolver needs its own limit for deep references.

## U3 — honggfuzz-rs: bundled honggfuzz fails against current binutils

- **Project:** [rust-fuzz/honggfuzz-rs](https://github.com/rust-fuzz/honggfuzz-rs),
  crate `honggfuzz` 0.5.62 (latest on crates.io, 2026-10-04). The C code
  comes from upstream honggfuzz (google/honggfuzz).
- **Problem:** `honggfuzz/linux/bfd.c` compares `bfd_find_nearest_line`'s
  result with `TRUE`; current binutils' `bfd.h` no longer defines it (it
  uses C `bool`), so the build fails.
- **Local fix:** `honggfuzz/vendor/honggfuzz/honggfuzz/linux/bfd.c`: `== TRUE`
  → `== true` (two lines, 170 and 176).
- **Upstream:** check whether google/honggfuzz already fixed `bfd.c`. If so,
  honggfuzz-rs only needs to update its bundled copy; else PR to
  google/honggfuzz first.

## U4 — fuzzcheck: coverage sensor misreads LLVM 21+ profile data

- **Project:** [loiclec/fuzzcheck-rs](https://github.com/loiclec/fuzzcheck-rs),
  crate `fuzzcheck` 0.13.0 (latest on crates.io). Vendored here from git
  master (edition 2024). Check maintenance status before investing in a PR.
- **Problem:** the coverage sensor parses `__llvm_prf_data` with the old
  record layout. With LLVM 21+ (rustc 1.100 nightly) records are 9 × u64 =
  72 bytes and NumCounters sits at offset 0x38 (verified against a
  `-C instrument-coverage` section dump), so counter counts and offsets are
  garbage.
- **Local fix:** `fuzzcheck/vendor/fuzzcheck/src/code_coverage_sensor/llvm_coverage.rs`
  (`PATCH(bevy_markup)`): read the relative counter pointer as a signed offset,
  track `counter_offset` per record, read NumCounters at the new offset.
  It also prints a `DIAG prf_data` line (a debugging leftover to drop
  before upstreaming).
- **Upstream PR sketch:** detect the profile format version
  (`__llvm_prf_data` layout differs by LLVM version) rather than hard-coding
  the new layout.

## U5 — fuzzcheck: current-dir filter rejects absolute paths

- **Project:** as U4.
- **Problem:** `CodeCoverageSensor::observing_only_files_from_current_dir`
  keeps only files whose path `is_relative()`. Cargo passes absolute source
  paths, so no project file passes the filter and the sensor observes
  nothing.
- **Local fix:** `fuzzcheck/vendor/fuzzcheck/src/code_coverage_sensor/mod.rs`:
  also keep `file.starts_with(current_dir)`.

## U6 — fuzzcheck: derive crate doesn't compile on current Rust

- **Project:** as U4, crate `fuzzcheck_mutators_derive` 0.13.0.
- **Problem:** it uses let-chains but declares edition 2021, where they
  don't compile (let-chains are edition-2024 only).
- **Local fix:** vendored with `edition = "2024"` (`fuzzcheck/Cargo.toml`
  `[patch.crates-io]`). Upstream git master may already be on edition 2024;
  then the request is just a crates.io release.

## U7 — cargo-mutants: regex filters skip field-deletion mutants

- **Project:** [sourcefrog/cargo-mutants](https://github.com/sourcefrog/cargo-mutants),
  27.1.0.
- **Problem:** `--re` and `--exclude-re` select mutants by name, but
  "delete field `x` from struct `Y` expression" mutants are always included,
  even when their name matches neither filter. Reproduction in bevy_markup: a regex
  matching no mutant at all, e.g.
  `cargo mutants --list --re '^src/nonexistent\.rs:'`, still lists every
  struct-field deletion in the crate (31 on 2026-10-04).
- **Local workaround:** none (documented in AGENTS.md, Testing TODO 6);
  targeted reruns take ~10 min instead of ~2.

## U8 — cargo-fuzz: prebuilt binary defaults to a musl target

- **Project:** [rust-fuzz/cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz),
  0.13.2 as installed by `taiki-e/install-action` (via cargo-binstall, a
  musl build).
- **Problem:** cargo-fuzz's default `--target` is the triple cargo-fuzz
  itself was compiled for. The prebuilt binary is
  `x86_64-unknown-linux-musl`, so fuzz targets are built for musl, where
  AddressSanitizer is unsupported ("sanitizer is incompatible with
  statically linked libc").
- **Local workaround:** `scripts/fuzz-libfuzzer.sh` passes
  `--target "$(rustc +nightly -vV | sed -n 's/^host: //p')"`.
- **Upstream suggestion:** default to the active rustc's host triple instead
  of the build triple.

## U9 — Bevy 0.19: UI picking systems require picking resources (discuss first)

- **Project:** [bevyengine/bevy](https://github.com/bevyengine/bevy), 0.19.1.
- **Problem:** `UiPlugin` adds `bevy_ui::widget::viewport::viewport_picking`,
  which takes `Res<HoverMap>` / `Res<PointerState>`. Without
  `DefaultPickingPlugins` (e.g. a headless app with `UiPlugin` only) the
  first update panics on the missing resource. It's arguably intended
  (`UiPlugin` assumes picking); worth asking whether the system should be
  conditional or the dependency explicit.
- **Local workaround:** `tests/common/mod.rs` (`TestUi::with_layout`) adds
  `DefaultPickingPlugins` and `TextureAtlasPlugin`.

## Not upstream bugs (recorded so they aren't re-filed)

- **Bevy's text measure rounds up** (`TextMeasure::measure` ends in
  `.ceil()`): a deliberate pixel-snapping choice, not a defect; it makes
  layouts with fractional line heights drift from browsers. Layout vectors
  use whole line heights.
- **`rui314/setup-mold` has no effect on Rust builds:** rustc ≥ 1.90 links
  through its bundled `rust-lld` and bypasses `/usr/bin/ld`.
