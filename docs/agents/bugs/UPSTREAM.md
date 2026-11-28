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
| U1 | fluent-syntax (fluent-rs) | Invalid unicode escape before a multi-byte character panics ([bug_0005](bug_0005.md)) | panic from any FTL asset | `ftl_check::bad_escape` at load (ADR 0045); the fork no longer wired in | fix on fork; upstream PR pending |
| U2 | fluent-syntax (fluent-rs) | Unbounded expression nesting overflows the stack ([bug_0015](bug_0015.md)) | stack overflow (abort) from a few KB of FTL | `ftl_check` nesting limit at load (ADR 0045); the fork no longer wired in | fix on fork; upstream PR pending |
| U3 | honggfuzz-rs | Bundled honggfuzz doesn't build against current binutils | honggfuzz driver doesn't build | vendored patch | unreported |
| U4 | fuzzcheck | Coverage sensor misreads LLVM 21+ `__llvm_prf_data` | fuzzcheck driver gets no usable coverage | vendored patch | unreported |
| U5 | fuzzcheck | `observing_only_files_from_current_dir` drops absolute paths | fuzzcheck observes no project code | vendored patch | unreported |
| U6 | fuzzcheck | `fuzzcheck_mutators_derive` 0.13.0 uses let-chains with edition 2021 | fuzzcheck driver doesn't build | vendored, edition bumped | unreported |
| U7 | cargo-mutants | `--re` / `--exclude-re` don't filter struct-field-deletion mutants | targeted reruns run unrelated mutants (~5× slower) | none (documented) | unreported |
| U8 | cargo-fuzz | Prebuilt (musl) binary defaults `--target` to musl, which ASan rejects | CI fuzz jobs failed to build | `--target "$host"` in `scripts/fuzz-libfuzzer.sh` | unreported |
| U9 | Bevy 0.19 | `UiPlugin`'s `viewport_picking` panics without the picking plugins | headless UI layout tests need extra plugins | add `DefaultPickingPlugins` in `tests/common` | discuss first |
| U10 | lightningcss | Bare `grid-auto-flow: dense` fails to parse | the declaration is dropped (CSS means `row dense`) | none (documented; write `row dense`) | unreported; **moot for bevy_markup**: lightningcss removed (ADR 0048) |
| U11 | tl | A value-less attribute eats the next attribute's first character ([bug_0019](bug_0019.md)) | `autofocus data-on-click` lost the click hook and focusability | switched to `astral-tl` 0.8.0 (astral-sh's maintained fork, fixed there) | fixed in astral-tl; y21/tl 0.7.8 affected |
| U12 | bevy_picking / bevy_ui | Text-section hits resolve `Pickable` on the span entity, not the text node ([bug_0020](bug_0020.md)) | `pointer-events: none` on a text-bearing element leaked clicks | `Pickable::IGNORE` on the spans too (`src/build.rs`) | unreported |
| U13 | pulldown-cmark | The offset iterator panics on a list item holding only a link reference definition before a whitespace-only line ([bug_0178](bug_0178.md)) | panic from any `.md` template | minimum version raised to 0.13.5 (`Cargo.toml`) | fixed upstream (0.13.5, #1021); a residual un-hardened `unwrap()` in `OffsetIter` is latent only (not reachable in 0.13.5 per ~18M fuzz execs + instrumented replay), left unreported |
| U14 | lightningcss | Nested `light-dark(` and `calc(` around a `var()` parse in exponential time ([bug_0179](bug_0179.md)) | a few hundred bytes of CSS stall the load for minutes to hours | nesting pre-scan bounds `(…)` at 8 (`MAX_CSS_PAREN_NESTING`) | reported upstream by others (#1382, #1389); PR #1383 open for #1382; **moot for bevy_markup**: lightningcss removed (ADR 0048) |
| U15 | lightningcss | The parser recurses once per block with no bound ([bug_0092](bug_0092.md), [bug_0179](bug_0179.md)) | stack overflow (abort) from a few KB of nested blocks | nesting pre-scan bounds all blocks at 32 (`MAX_CSS_NESTING`) | reported upstream by others (#1297, #1155, #1092); **moot for bevy_markup**: lightningcss removed (ADR 0048) |
| U16 | tera | The compiler recurses once per operator of an expression; the 2.4.0 cap (256 operations, Keats/tera#1035) still overflows a 2 MB thread in debug builds from 136 ([bug_0182](bug_0182.md)) | stack overflow (abort) at template load in unoptimized builds | a tag over 64 operators fails the load before Tera compiles it (`tera_tag_over_limit`, `src/template.rs`) | unreported (follow-up to the closed #1035) **Workaround kept for MiniJinja (ADR 0049, 2026-11-25): bevy_markup no longer uses Tera, but the operator pre-check still guards the new engine.** |
| U17 | usvg (resvg) | Converter recursion through references and nesting is unbounded | stack overflow (abort) from a few KB of SVG on a 2 MB thread (`svg` feature) | static recursion-depth check before conversion (`svg_bounds::bounds`) | unreported; still overflows on 0.50, whose `max_depth` (#1223) bounds XML nesting and `<use>` only |
| U18 | roxmltree | 0.21.1's tokenizer recursed once per nesting level | stack overflow at ~150 levels in debug on a 2 MB thread (`svg` feature) | version floor `roxmltree >= 0.21.2` | fixed upstream (0.21.2) |
| U19 | usvg (resvg) | The default `ImageHrefResolver` reads any file path | an SVG reads any file the process can and embeds it (`svg` feature) | a resolver that resolves nothing | unreported (by design; worth a docs warning) |
| U20 | resvg | `feTurbulence` overflows `i32` after enough octaves or on a huge `baseFrequency` (#1185, open; #937 a similar multiply overflow) | panic where overflow checks are on (debug builds, fuzzing): a failed load, or a crash through a redraw task (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: `svg::guarded` (catch_unwind around usvg/resvg); the `svg` fuzz target lets it unwind (`fuzz::is_known_upstream_panic`) | reported upstream (#1185), open; the `baseFrequency` trigger unreported; unreachable since ADR 0051 |
| U21 | astral-tl | `/` is a name character: `<b/>` is an unclosed element named `b/` ([bug_0185](bug_0185.md)) | a `<br/>` merged the rest of the document into one block | `template::normalize_self_closing` before every `tl` parse | unreported |
| U22 | svgtypes / kurbo | An arc splits into `(11·r)^⅙` cubics at kurbo's absolute 0.1 tolerance, and svgtypes' simplifier drains them with `Vec::remove(0)` ([bug_0190](bug_0190.md)) | one 40-byte arc converts for seconds or never finishes (`svg` feature) | counted before conversion in the work bound (`svg_bounds::bounds`, ADR 0040) | unreported |
| U23 | usvg (resvg) | A pattern whose content inherits a paint naming itself recurses until the stack overflows ([bug_0191](bug_0191.md)) | stack overflow (abort) from 230 bytes (`svg` feature) | none needed: patterns are outside the icon subset (ADR 0051); was: inherited references are edges of the cycle check (`svg_bounds::bounds`) | fixed upstream in 0.50; check kept (it is part of the U17 graph); unreachable since ADR 0051 |
| U24 | tiny-skia | Anti-aliased fills shift the path's *unclipped* rows by 2 in an `i32`, which wraps past 2²⁹ ([bug_0194](bug_0194.md)) | panic in any build on paths far off the canvas (debug: `walk_edges` assertion; release: U28) — a failed load (`svg` feature) | refused before drawing (`svg::check_extent`, `MAX_SVG_EXTENT`, `SvgError::TooFarOut`) | unreported |
| U25 | usvg (resvg) | No bound on conversion work: `<use>` copies under the million-node limit, markers per vertex ([bug_0188](bug_0188.md), [bug_0189](bug_0189.md)) | seconds to minutes of conversion from a few KB (`svg` feature) | the work bound (`svg_bounds::bounds`, `MAX_SVG_WORK`, ADR 0040); markers are outside the icon subset since ADR 0051 | unreported (by design; a conversion budget option would replace the model) |
| U26 | usvg (resvg) | `feTurbulence` with a `baseFrequency` past `f32::MAX` unwraps `PositiveF32::new(inf)` ([bug_0192](bug_0192.md)) | panic in any build: a failed load (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: refused before usvg runs (`svg_bounds::check_values`, `SvgError::OutOfRange`) | unreported; unreachable since ADR 0051 |
| U27 | tiny-skia | The cubic edge clipper's bisection can't reach its 0.25 px tolerance between ~2²¹ and 2²² px and unwraps `NormalizedF32Exclusive::new(1.0)` ([bug_0194](bug_0194.md)) | panic in any build: a failed load (`svg` feature) | refused before drawing (`svg::check_extent`) | unreported |
| U28 | tiny-skia | Release builds of U24: the wrapped row range writes outside the canvas, caught by an `unwrap` in `alpha_runs.rs` or a slice in `pipeline/mod.rs` ([bug_0194](bug_0194.md)) | panic in release builds: a failed load (`svg` feature) | as U24 | unreported (report with U24) |
| U29 | usvg (resvg) | `feConvolveMatrix` multiplies its `order` sides as `u32` unchecked ([bug_0195](bug_0195.md)) | panic where overflow checks are on (debug builds, fuzzing): a failed load; release wraps (the cost estimate then refuses the kernel) (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: refused before usvg runs (`svg_bounds::check_values`, `SvgError::OutOfRange`) | unreported; unreachable since ADR 0051 |
| U30 | usvg (resvg) | `feMorphology` unwraps `PositiveF32::new(radius × scale)`, `None` when the product isn't finite ([bug_0196](bug_0196.md)) | panic in any build: a failed load (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: non-finite radii refused before usvg runs (`svg_bounds::check_values`, `SvgError::OutOfRange`); radius × box by ADR 0042 | unreported; unreachable since ADR 0051 |
| U31 | tiny-skia-path (via usvg) | `NonZeroRect::bbox_transform` unwraps `from_xywh` of an overflowed product ([bug_0197](bug_0197.md)) | panic in any build on a filter over a box wider than ~2.8·10³⁸: a failed load (`svg` feature) | none needed: filters and the `filter` property are outside the icon subset (ADR 0051); was: boxes measured by a first usvg pass, overflowing regions refused (ADR 0042, `svg_boxes`, `SvgError::RegionOverflow`) | unreported; unreachable since ADR 0051 |
| U32 | resvg | The box blur adds 2 to an `i32` box width that saturates at `i32::MAX` for a deviation of ~1.4·10⁹ px ([bug_0198](bug_0198.md)) | panic where overflow checks are on (debug builds, fuzzing): a failed draw; release wraps to a negative box (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: refused before drawing past `MAX_SVG_BLUR` (`svg::check_extent`, `SvgError::TooBlurred`) | unreported; unreachable since ADR 0051 |
| U33 | Bevy (bevy_picking 0.20) | `Location::is_in_viewport` returns false without a `PrimaryWindow`, even for an image render target: ray-based picking (mesh, sprite) hits nothing in a windowless app | any render-to-texture-only app loses mesh picking (no bevy_markup code affected); the examples' `--offscreen` mode did | the harness's `--offscreen` mode spawns a primary `Window` entity with no native window (`examples/shared/harness.rs`) | unreported |
| U34 | Bevy (bevy_ui 0.20) | The UI picking backend hit-tests a text node's runs (`run_geometry`) but never `TextLayoutInfo::inline_boxes`: a pointer over an inline image hits its text block ([bug_0199](bug_0199.md)) | an image or SVG inside a link can't be clicked through the icon | `signals::pick_inline_boxes` adds the image's hit from the UI backend's hit on its text block | unreported |
| U35 | tiny-skia-path 0.12 (via resvg 0.50) | `NonZeroRect::to_int_rect` and `IntSize::to_int_rect(x, y)` unwrap `IntRect::from_xywh`, `None` once `x + width` passes `i32::MAX`; resvg converts filter regions, subregions and offsets with them ([bug_0201](bug_0201.md)) | panic in any build: a failed draw (`svg` feature) | none needed: filters are outside the icon subset (ADR 0051); was: regions, subregions and offsets past `MAX_SVG_FILTER_REGION` refused before drawing (`svg::check_extent`) | unreported; unreachable since ADR 0051 |
| U36 | resvg 0.50 (tiny-skia 0.12) | `render_pattern_pixmap` allocates a pattern's tile at its drawn size with no limit; `Pixmap::new` allocates with `vec![0; len]`, which aborts on failure ([bug_0200](bug_0200.md)) | process abort (`svg` feature) | none needed: patterns are outside the icon subset (ADR 0051); was: the render-cost estimate charges each tile (`svg::estimate_cost`) | unreported; unreachable since ADR 0051 |
| U37 | parley 0.11 (via bevy_text 0.20) | An inline box's whole height counts as ascent and the line height is the tallest item's, so a box taller than the text's ascent gets negative leading ([bug_0203](bug_0203.md)) | an `<img>`/inline `<svg>` taller than the ascent overlaps the line above; the line is short by the text's descent | none | fixed upstream (parley 0.12.0); waiting for Bevy to depend on it |
| U38 | Bevy (bevy_ui 0.20.1) | `update_viewport_render_target_size` ends in `images.get_mut(image_handle).unwrap()`: a `ViewportNode` camera whose `RenderTarget::Image` isn't in `Assets<Image>` panics ([bug_0218](bug_0218.md)) | a canvas camera over a not-yet-loaded or removed target image panics on the compute pool | `sync_canvases` binds a camera only while its target image is in `Assets<Image>` (bug_0218) | unreported |
| U39 | Tera 2.4.0 | A render's work and memory have no limit (only `range` ≤ 10⁵ and component recursion ≤ 20): nested loops run for hours, `set_global` doubling in a loop or unrolled `set` doubling ask for terabytes and abort ([bug_0220](bug_0220.md)) | a template stalls or aborts the app | `tera_check`: a static check at load bounds steps, built values and copies of context data (ADR 0044); output capped by `template::MAX_RENDERED_BYTES` | unreported **Moot for bevy_markup since ADR 0049 (2026-11-25): it no longer uses Tera; MiniJinja's fuel bounds renders, and `tera_check` is removed.** |
| U40 | simplecss 0.2.2 (via usvg 0.50 and bevy_markup's `svg_bounds`) | `Selector::matches` tries every ancestor for a descendant combinator and recurses with no memo: a selector with k descendant combinators costs up to Σ_{j≤k} C(depth, j) calls per element when its leftmost compound never matches ([bug_0229](bug_0229.md)) | a 555-byte SVG `<style>` parses for 36 s (`svg` feature) | none needed: `<style>` sheets are outside the icon subset (ADR 0051); was: `svg_bounds::match_work` charges every rule by that count (and its class/attribute tests) before anything matches | unreported; unreachable since ADR 0051 |
| U41 | usvg 0.50 text layout (`text` feature) | `ByteIndex::code_point_at` counts characters from the chunk's start, and `resolve_relative_shifts` (and the rotate and text-on-path code) calls it per cluster: every text chunk lays out in time quadratic in its characters ([bug_0232](bug_0232.md)) | a 100 000-character `<text>` converts for 3.9 s (13.5 s with multi-byte characters) inside `MAX_SVG_WORK` (`svg-text`) | none needed: `<text>` are outside the icon subset (ADR 0051); was: `svg_bounds` charges a `<text>` by its characters² | unreported; unreachable since ADR 0051 |
| U42 | simplecss 0.2.2 (via usvg 0.50 and bevy_markup's `svg_bounds`) | `StyleSheet::parse_more` builds an error, positioned by counting lines from the text's start (`Stream::gen_text_pos`), at every declaration's end, unitless number and `#` colour, then drops it: parsing is quadratic in the sheet's length ([bug_0231](bug_0231.md)) | 40 000 rules (760 KB) parse for 0.96 s per pass, three passes per SVG (`svg` feature) | none needed: `<style>` sheets are outside the icon subset (ADR 0051); was: `svg_bounds::style_scan_bytes` charges the offsets of those places before simplecss parses | unreported; unreachable since ADR 0051 |
| U43 | usvg 0.50 text layout (`text` feature) | `process_chunk` reshapes the whole chunk once per span and merges each span's glyphs into the first run with `glyphs.iter().position(…)` and `Vec::remove`/`insert`: spans × characters ([bug_0232](bug_0232.md)) | 4 000 one-character `<tspan>`s convert for 2.0 s (`svg-text`) | none needed: `<text>` are outside the icon subset (ADR 0051); was: `svg_bounds` charges a `<text>` by its spans × characters | unreported; unreachable since ADR 0051 |
| U44 | fuzzcheck | The fuzzer unwraps `String::from_utf8` of every serialized input it keeps (for an assertion message) | a `ByteSerializer` target panics on its first non-UTF-8 input | vendored patch | unreported |
| U45 | lightningcss 1.0.0-alpha.72 / cssparser-color 0.5.0 | An infinite `hsl()`/`hwb()` hue normalizes to NaN, and `hsl_to_rgb` `debug_assert!`s its hue range | panic at parse time in debug builds, from any CSS text ([bug_0238](bug_0238.md)) | `cascade::guarded_parse` (catch_unwind around the three lightningcss parses); the fuzz targets let it unwind (`fuzz::is_known_upstream_panic`, pinned to cssparser-color 0.5.0) | unreported; **moot for bevy_markup**: lightningcss removed (ADR 0048) |
| U46 | Tera 2.4.0 | `unique` compares every pair of distinct items ([bug_0241](bug_0241.md)) | unmodeled quadratic render work | charge comparisons, refuse context-sized `unique` (ADR 0047) | unreported **Moot for bevy_markup since ADR 0049 (2026-11-25): it no longer uses Tera.** |
| U47 | fluent-syntax 0.11.1, 0.12.0 | Duplicate named-argument scan is quadratic ([bug_0243](bug_0243.md)) | flat FTL call stalls resource loading | `MAX_FTL_ARGUMENTS` = 64 in the pre-check (ADR 0047) | unreported |
| U48 | cargo-afl 0.18.2 | `target-cpu=native` also reaches proc macros and build scripts | cached test-fuzz builds hit SIGILL on runners with older CPUs | append `-C target-cpu=x86-64` in `scripts/fuzz-{setup,test-fuzz}.sh` | unreported |
| U49 | lightningcss 1.0.0-alpha.72 | A NaN argument to `min()`/`max()`/`clamp()` reaches `unreachable!()` in `Percentage::parse` | panic at parse time in every build profile, from any CSS text; an app with `panic = "abort"` aborts ([bug_0245](bug_0245.md)) | `cascade::guarded_parse` (unwinding builds only); the fuzz targets let it unwind (`fuzz::is_known_upstream_panic`, pinned to 1.0.0-alpha.72) | fixed on master ([`b0939ab`](https://github.com/parcel-bundler/lightningcss/commit/b0939ab684793d2ba3ccc1a91e0f4d3ba837a708)), awaiting release; **moot for bevy_markup**: lightningcss removed (ADR 0048) |
| U52 | bevy_ui 0.20.1 (bevy_math 0.20.0) | `ComputedNode::border_box` calls `Rect::from_center_size`, which `assert!`s a non-negative size: a node laid out to a NaN size panics ([bug_0249](bug_0249.md)) | panic in every build from a NaN `Node` value (a CSS `calc()` overflowing at layout) | CSS magnitudes bounded at 10⁸, `CalcExpr::resolve` skips non-finite results | unreported |
| U53 | parley 0.11.1 (via bevy_text) | `BreakLines::break_next_line_or_box` `assert!`s on its line widths, which an infinite or NaN layout width breaks ([bug_0249](bug_0249.md)) | panic in every build from text in a node whose width overflows (`3e38em`, width + margin near `f32::MAX`) | same bounds | unreported |
| U54 | zeno 0.3.3 / swash 0.2.10 (via bevy_text) | Glyph rasterization sizes its bitmap with `u32` arithmetic: past ~32,768 px per side it overflows ([bug_0249](bug_0249.md)) | debug: panic from `font-size: 2e5px`; release: hangs past 2 min at 1.4–2.4 GB; `50000px` costs 3.5 s and 6.9 GB | computed font size clamped to `MAX_FONT_SIZE` (8192 px) | unreported |
| U55 | bevy_text 0.20.1 | Font atlases are kept per exact font size and never freed; each new size costs at least a 512² RGBA atlas, a glyph gets an atlas as large as itself (2× the font size for tall glyphs), possibly past the device's texture limit ([bug_0250](bug_0250.md)) | memory grows with the distinct sizes and glyphs ever drawn: 2,000 sizes took 2 GB, 16 letters at 8192 px 2.3 GB; animated font sizes leak | `HtmlFontLimits` (default 2048 px, 128 distinct sizes) | unreported |
| U56 | minijinja 3.1.0 | `join`, `split`, `lines`, `escape` allocate their result before charging fuel (21×, 4.6× past the budget) | memory past the fuel budget from a template | the fueled filter wrappers in `template.rs`; patched MiniJinja (ADR 0052) | reported (email; issue to open) |
| U57 | minijinja 3.1.0 | Long flat expression chains overflow the stack in codegen | abort from a ~13 KB template | `tera_tag_over_limit` (64 operators per tag); patched MiniJinja | reported (email; issue to open) |
| U58 | honggfuzz-rs 0.5.62 (`cargo-hfuzz`) | Release builds add `-C target-cpu=native` | targets built on a runner with AVX-512 crash with SIGILL on every input on one without, so a cached build fails the nightly fuzz jobs | append `-C target-cpu=x86-64` in `scripts/fuzz-{setup,honggfuzz}.sh` (later flags win), cache suffix `-x86-64` | unreported |
| U59 | minijinja 3.1.0 | `format` panics on a non-ASCII `%(key)` | panic from a 24-byte template | `checked_format`; patched MiniJinja | reported (email; issue to open) |
| U60 | minijinja 3.1.0 | `format` panics on an integer `%f` with precision over 65535 | panic from a template | `checked_format`; patched MiniJinja | reported (email; issue to open) |
| U62 | minijinja 3.1.0 | Deep walks of containers (printing, `~`, `in`, `==`) and constant folding aren't bounded by fuel ([bug_0253](bug_0253.md)) | stall or out-of-memory at template load or render | printing and text filters checked (`streaming_formatter`); patched MiniJinja (the rest) | found 2026-11-26; issue to open |
| U64 | minijinja 3.1.0 | `tojson` takes any `usize` indent and writes `indent × depth` spaces per item before any limit ([bug_0256](bug_0256.md)) | stall or out-of-memory from a 93-byte template | `checked_tojson` counts the indent (`fits_printed`); patched MiniJinja | found 2026-11-28; issue to open |

Both are fixed in the fork [nchashch/fluent-rs](https://github.com/nchashch/fluent-rs):
branch `fix/fuzzing-bugs` (on upstream `main`, 0.12; for the PR) and branch
`fix/fuzzing-bugs-0.11` (on tag `fluent-syntax@0.11.1`; what bevy_markup
patches in, since fluent 0.16 / fluent-bundle 0.15 needs `^0.11`). One
commit on each, same
diff: `fluent-syntax/src/parser/{core,errors,expression,helper}.rs` and the
new test file `fluent-syntax/tests/robustness.rs`. Upstream's `cargo fmt`,
CI clippy command and workspace tests pass on both.

## U1 — fluent-syntax: invalid unicode escape before a multi-byte character panics

- **Project:** [projectfluent/fluent-rs](https://github.com/projectfluent/fluent-rs),
  crate `fluent-syntax`. Affected: 0.11.1 and 0.12.0 (what bevy_markup
  uses since 2026-11-25, through fluent 0.17 / fluent-bundle 0.16; latest
  release, rechecked that day).
- **Problem:** `skip_unicode_escape_sequence` reports an invalid `\u`/`\U`
  escape with the character that ended it, sliced as `ptr + 1`, one byte.
  If that character is multi-byte, `Slice::slice` (`&self[range]`) panics.
  It's the only unsafe slice: the parser's other `ptr - 1` slices follow an
  ASCII byte, and replaying ~4,900 ftl fuzz inputs with a boundary clamp
  removed found no other panic. (An earlier "second variant", a long
  multi-byte error context, was wrong: that input parses to an ordinary
  error.)
- **Reproduction:** `FluentResource::try_new("u={\"\\U\u{fffd}".to_owned())`
  panics with "byte index 7 is not a char boundary".
- **Fix (fork):** take the whole offending character
  (`chars().next().map(char::len_utf8)`) instead of one byte. The first
  local fix (vendored, now removed) clamped every slice to a char boundary.
- **Workaround now (2026-11-11, ADR 0045):** `ftl_check::bad_escape` refuses the file at load, before fluent-syntax parses it; the fork is no longer wired in (`[patch]` removed from every manifest), so dependents need nothing. Drop the check's part once a released fluent-syntax that `fluent` uses carries the fix.
- **Tests:** fork `robustness.rs` `invalid_unicode_escape_before_multibyte_char`
  (includes the exact error text); bevy_markup `src/fuzz.rs`
  `broken_unicode_escape_after_multibyte_char_does_not_panic`.

## U2 — fluent-syntax: unbounded expression nesting overflows the stack

- **Project:** as U1. Affected: 0.11.1 and 0.12.0 (latest release,
  rechecked 2026-11-25: no depth limit).
- **Problem:** inline expressions nest recursively with no limit, along two
  cycles: placeables (placeable → expression → inline expression or select
  variant → placeable) and call arguments (inline expression → call
  arguments → inline expression, `F(F(F(…)))`, no braces needed). Measured
  overflow depths for placeables: debug 200–400 levels on a 2 MB stack
  (800–1600 on 8 MB), release 1600–3200 on 2 MB (6400–12800 on 8 MB); calls
  overflow at 100–400 levels in debug on 2 MB. A few KB of FTL abort any
  process that parses it, e.g. a Bevy app loading a translation on a worker
  thread.
- **Reproduction:**
  `FluentResource::try_new(format!("x = {{ {}1{} }}\n", "F(".repeat(100_000), ")".repeat(100_000)))`
  (or the same with `{`/`}` around `"a"`) aborts with a stack overflow.
- **Fix (fork):** both cycles pass through `get_inline_expression` once per
  level; a depth counter there returns the new `ErrorKind::NestingTooDeep`
  past `MAX_NESTING_DEPTH` = 100, and the entry becomes Junk. The first
  local fix (vendored, now removed) counted placeables only and missed the
  call cycle.
- **Workaround now (2026-11-11, ADR 0045):** `ftl_check`'s port of the parser refuses nesting past `MAX_FTL_NESTING` (64) at load, before fluent-syntax parses it; the fork is no longer wired in (`[patch]` removed from every manifest), so dependents need nothing. Drop the check's part once a released fluent-syntax that `fluent` uses carries the fix.
- **Tests:** fork `robustness.rs` (100,000 nested placeables and calls on a
  2 MiB stack; the boundaries 100/101 braces and 99/100 calls, derived by
  hand; a too-deep entry leaves the next message intact); bevy_markup
  `src/fuzz.rs` `deeply_nested_expressions_are_an_error_not_a_stack_overflow`,
  `nesting_up_to_the_limit_still_works`.
- **For the PR:** the limit (100) and the new `ErrorKind` variant (a public
  enum without `#[non_exhaustive]`, so technically breaking for exhaustive
  matches) are maintainer decisions. fluent-bundle's resolver already caps
  placeables per message (`MAX_PLACEABLES`).

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
  That still observes only the harness crate (`fuzzcheck/`, ~50 counters):
  the library is a path dependency above it. The harnesses pass their own
  filter instead (`fuzzcheck/tests/common/mod.rs` `observed`, the
  repository's `src/`; 2026-11-12).

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

## U10 — lightningcss: bare `grid-auto-flow: dense` doesn't parse

- **Project:** [parcel-bundler/lightningcss](https://github.com/parcel-bundler/lightningcss),
  1.0.0-alpha.72 (the version bevy_markup uses; re-check the latest).
- **Problem:** `<GridAutoFlow as Parse>::parse` (`src/properties/grid.rs`),
  on a leading `dense`, `try_parse`s a following `row`/`column` and
  propagates its error with `?`, so `dense` alone (valid CSS: `[ row |
  column ] || dense`, = `row dense`) is an invalid declaration. `dense row`,
  `row dense`, `column dense` parse.
- **Reproduction:** `StyleSheet::parse("p { grid-auto-flow: dense }", …)`
  yields no `Property::GridAutoFlow` declaration.
- **Local workaround:** none; documented in `src/style.rs` (write
  `row dense`), the example uses `row dense`; `cascade::tests::
  grid_container_properties_map_to_bevy_values` notes it at its `dense row`
  case.
- **Upstream fix sketch:** make the trailing direction optional (`.ok()`
  instead of `?`) and keep `Row` as the default.

## U11 — tl: a value-less attribute eats the next attribute's first character

- **Project:** [y21/tl](https://github.com/y21/tl) 0.7.8 (latest release,
  checked 2026-10-05). Fixed in [astral-sh/astral-tl](https://github.com/astral-sh/astral-tl)
  0.8.0 (crates.io `astral-tl`, library name `tl`).
- **Defect:** `parser::base::parse_attributes` advances one byte after every
  attribute (to step over a quoted value's closing quote), but for a
  value-less attribute `parse_attribute` has already skipped the whitespace
  and stands on the next attribute's first byte: `<div autofocus
  data-on-click="x">` parses as `autofocus` + `ata-on-click="x"`.
- **Workaround:** bevy_markup depends on `astral-tl` (`tl = { package =
  "astral-tl", … }` in `Cargo.toml`); API-compatible, full suite passes.
  Regression test: `focus_navigation_scope_and_styles` (`autofocus` first).
- **Status:** not reported to y21/tl (fixed in the maintained fork; y21/tl
  has had no release since 0.7.8).

## U12 — bevy_picking: text-section hits resolve `Pickable` on the span, not the text node

- **Project:** [bevyengine/bevy](https://github.com/bevyengine/bevy) 0.19.1
  (`bevy_ui` 0.19.1 picking backend + `bevy_picking` 0.19.1 hover map).
- **Problem:** for a `Text` node, `bevy_ui`'s `ui_picking` reports the *text
  span entity* as the hit target and attaches the text **node**'s `Pickable`
  to the hit. `bevy_picking`'s `build_hover_map` then resolves `Pickable`
  with a query on the **target** entity — the span, which normally has no
  `Pickable` — so the node's `should_block_lower: false` /
  `is_hoverable: false` never apply: the span hovers and blocks by default,
  and the event bubbles up from it. Concretely, `pointer-events: none` on a
  UI element didn't make its text unclickable (bevy_markup
  [bug_0020](bug_0020.md)).
- **Reproduction:** the backend pushes the hit with the node's `Pickable`;
  put `Pickable::IGNORE` on a text node under a click point and observe that
  the click still reaches the node's ancestors (observers) instead of
  falling through to what's behind. bevy_markup's
  `pointer_events_none_silences_an_element` (`tests/signals.rs`) is the
  end-to-end form; it failed before the local fix and passes with spans
  carrying `Pickable::IGNORE`.
- **Local workaround:** `src/build.rs` puts `Pickable::IGNORE` on every span
  of an ignored block (`spawn_spec`), and takes it back on restyle
  (`apply_spec`).
- **Upstream question (discuss first):** should `build_hover_map` use the
  `Pickable` recorded on the hit (the text node's) rather than re-querying
  the target entity, or should the backend report the text node as the
  target? Either fixes text under `pointer-events: none`; the second also
  changes which entity `Pointer<Over>` observers see.

## U13 — pulldown-cmark: the offset iterator panics on a definition-only list item before a blank line

- **Project:** [pulldown-cmark/pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark).
  Affected: 0.13.4 (bevy_markup's lock until bug_0178) and the 0.13 branch
  before `078feb520e`; fixed in **0.13.5** (checked 2026-10-26; upstream
  `main` at `61d95cedd7`, 2026-10-23, does not panic either).
- **Problem:** `Parser::into_offset_iter()` panics with
  `called Option::unwrap() on a None value` at `src/parse.rs:2199`
  (0.13.4) when a list item contains only a link reference definition and
  the next line holds only whitespace such as a vertical tab or form feed.
  The tight paragraph left behind has no children, and the code that skips
  tight paragraphs (`tree.push(); tree.cur().unwrap()`) assumes one. The
  plain iterator (`Parser::next`) does not panic on these inputs.
- **Reproduction:**
  `Parser::new_ext("- [a]: b\n\u{0B}", Options::empty()).into_offset_iter().count()`
  panics on 0.13.4. bevy_markup reaches it from `.md` templates
  (`md::dedent_blocks` → `code_ranges`); found by the honggfuzz `html`
  target (bug_0178).
- **Upstream fix:** PR [#1021](https://github.com/pulldown-cmark/pulldown-cmark/pull/1021)
  ("Correctly parse all-spaces line between refdef and paragraph", for
  issue [#997](https://github.com/pulldown-cmark/pulldown-cmark/issues/997)),
  cherry-picked into the 0.13 branch as `078feb520e` and released in 0.13.5.
  Bisected: `96201943db` (just before) panics, `078feb520e` does not. Issue
  [#1084](https://github.com/pulldown-cmark/pulldown-cmark/issues/1084) is
  another input reaching the same `unwrap()` (tasklists + strikethrough),
  also fixed for its input.
- **Local workaround:** none needed; `Cargo.toml` requires
  `pulldown-cmark` ≥ 0.13.5 and all five lockfiles are on 0.13.5. The panic
  is genuinely fixed in 0.13.5 (bug_0178 resolved): the parse change means
  the empty tight paragraph is no longer produced, so neither iterator
  panics on the inputs above.
- **Residual asymmetry (latent, not reachable; no report warranted):** in
  0.13.5 the two iterators diverge where they skip a tight paragraph.
  `Parser::next` (plain, `src/parse.rs:2431`) was hardened — it binds
  `tree.cur()` to `maybe_cur_ix` and guards it with `if let Some(..)`, so an
  empty tight paragraph emits nothing. `OffsetIter::next`
  (`src/parse.rs:2267`, the path bevy_markup's `code_ranges` uses) still
  does `self.tree.push(); self.tree.cur().unwrap()`. So the unwrap survives,
  but the parser no longer reaches it: the hardening is defense-in-depth for
  a state the parse fixes removed. It would become a live panic again only
  if a future parse change reintroduced an empty tight paragraph, and then
  only through the offset iterator.
- **Why "not reachable" and not just "not found":** three searches on
  0.13.5, all against `Parser::new_ext(data, Options::all()).into_offset_iter()`
  (2026-10-26):
  - **Coverage-guided fuzzing** (cargo-fuzz / libFuzzer), seeded with the
    0.13.4 trigger and the near-miss cases (`issue_1095` `"- [n]:Z\r\n\t\t"`
    and `issue_1084`): two runs, ~6.6M and ~11.7M executions (~18.3M total),
    ~6240 edges covered, no panic.
  - **Instrumented replay:** a vendored 0.13.5 with that exact
    `.cur().unwrap()` replaced by a probe logging when `cur()` is `None`;
    the 40,933-input fuzz corpus replayed through it reached the
    empty-tight-paragraph branch **0 times**.
  - Hand-built refdef / tight-list / trailing-whitespace combinations and a
    token-soup random search: nothing.
  If a future upstream change reopens this, the fix is to make
  `OffsetIter::next` match the plain iterator's `if let Some(..)` guard;
  left unreported because it is not reachable today.

## U14 — lightningcss: nested `light-dark(` and `calc(var(…))` parse in exponential time

- **Project:** [parcel-bundler/lightningcss](https://github.com/parcel-bundler/lightningcss).
  Affected: 1.0.0-alpha.72 (bevy_markup's lock; the latest release,
  checked 2026-10-28) and `master`.
- **Problem:** parse time doubles per nesting level. `light-dark()`
  arguments are parsed as a color first; without a comma that fails after
  the full parse, and the generic function path parses them again
  (#1382). A typed property with nested `calc()` around a `var()` does the
  same (#1389). Measurements: bug_0179 (depth 18: 36 ms / 554 ms).
- **Reproduction:** `StyleSheet::parse(&format!("a{{color:{}", "light-dark(".repeat(24)), ParserOptions::default())`
  takes ~2 s in release; `.x{width:` + 22 nested `calc(… - 1px)` around
  `var(--x)` ~19 s (#1389).
- **Upstream:** issue [#1382](https://github.com/parcel-bundler/lightningcss/issues/1382)
  with fix PR [#1383](https://github.com/parcel-bundler/lightningcss/pull/1383)
  (open, mergeable, 2026-10-22); issue [#1389](https://github.com/parcel-bundler/lightningcss/issues/1389)
  (open, no PR). Both filed by others from fuzzing.
- **Local workaround:** `nesting_over_limit` rejects text nesting `(…)`
  deeper than 8 before lightningcss sees it (sheets fail their load,
  `style=` attributes and substituted values are dropped). Revisit the
  bound (not the block bound, U15) once both are fixed in a release.

## U15 — lightningcss: unbounded parser recursion

- **Project:** [parcel-bundler/lightningcss](https://github.com/parcel-bundler/lightningcss).
  Affected: 1.0.0-alpha.72 and `master`.
- **Problem:** the parser recurses once per block (`(`, `[`, `{`, nested
  rules) with no depth limit. ~500 nested blocks overflow a 2 MB thread in
  a debug build, ~3000 nested `calc(` or `light-dark(` in release
  (bug_0179); nested rules too (bug_0106).
- **Upstream:** issues [#1297](https://github.com/parcel-bundler/lightningcss/issues/1297)
  (selector, math-function and rule-nesting recursion),
  [#1155](https://github.com/parcel-bundler/lightningcss/issues/1155),
  [#1092](https://github.com/parcel-bundler/lightningcss/issues/1092); all
  open, no PR (checked 2026-10-28).
- **Local workaround:** `nesting_over_limit` bounds all blocks at 32
  (bug_0092, bug_0106). The css fuzz harness applies it too (bug_0179),
  so fuzzing doesn't spend time on inputs every app rejects.

## U16 — tera: expression compilation recurses once per operator

> **Workaround kept for MiniJinja (ADR 0049, 2026-11-25): bevy_markup no longer uses Tera, but the operator pre-check still guards the new engine.**

- **Project:** [Keats/tera](https://github.com/Keats/tera). Affected:
  2.4.0 (bevy_markup's lock; the latest release, checked 2026-11-03).
- **Problem:** `Compiler::compile_expr` walks the left-deep AST of an
  operator chain recursively, one frame per operator. Issue
  [#1035](https://github.com/Keats/tera/issues/1035) (closed 2026-09-09)
  reported the unbounded case; 2.4.0 bounds it with
  `MAX_EXPRESSION_COMPLEXITY` = 256, measured against an 8 MB main
  thread. On a 2 MB thread a debug build overflows from 136 operators;
  release holds up to the cap (bug_0182).
- **Reproduction:** in a debug build on a 2 MB thread,
  `Tera::new().add_raw_template("t", &format!("{{{{ n{} }}}}", " - n".repeat(136)))`
  aborts.
- **Upstream:** not reported. A PR sketch: make `compile_expr` iterative
  along the left spine of binary operations and filters, or count depth
  in the compiler and lower the cap to what a 2 MB debug thread holds.
- **Local workaround:** `template::tera_tag_over_limit` rejects a tag of
  more than 64 operators before Tera compiles it; drop it once Tera's
  compiler holds its cap on a 2 MB debug thread.

## U17 — usvg: converter recursion through references and nesting is unbounded

- **Project:** [linebender/resvg](https://github.com/linebender/resvg)
  (`usvg` crate). Affected: 0.49.0 and 0.50.0 (the latest release,
  retested 2026-11-23). 0.50 includes PR
  [#1223](https://github.com/linebender/resvg/pull/1223), which makes the
  XML nesting limit configurable (`Options::max_depth`, default 1024) and
  counts `<use>` instances, but not references followed by the converter.
  On 0.50, on a 2 MB thread, 1,000 chained patterns, markers or `feImage`s
  overflow debug and release; 1,000 masks overflow debug; 5,000 masks or
  clip paths overflow both.
- **Problem:** `parser/converter.rs` recurses once per nested element
  and once per reference it follows into a target's content
  (`paint_server::convert_pattern` → `convert_children` → … →
  `convert_path` → back into `paint_server::convert`; likewise masks,
  clip paths, markers, `feImage`). On a 2 MB thread (Bevy's task-pool
  default) a debug build overflows at 121 chained patterns, 113 `feImage`
  hops, 123 markers, 251 `<use>` hops, 293 masks, 370 nested `<svg>`,
  ~689 nested `<g>` (below the 1024 nesting check); release at about 3×
  those (probe report `docs/agents/performance/adr-0038-probe.md` §6).
  A few KB of SVG abort the process.
- **Reproduction:** on a 2 MB thread, `usvg::Tree::from_str` of
  `<svg xmlns="http://www.w3.org/2000/svg">` followed by 130 patterns,
  each `<pattern id="pN" width="8" height="8"
  patternUnits="userSpaceOnUse"><rect width="8" height="8"
  fill="url(#pN-1)"/></pattern>`, and a rect filled with the last.
- **Upstream:** not reported. A PR sketch: carry a depth in the
  converter's `State` (already threaded through every recursive call),
  increment it on each element and each followed reference, and fail
  with the existing `NodesLimitReached` past `Options::max_depth` — the
  option #1223 introduced, so callers on small stacks set one number.
- **Local workaround:** `svg::parse` parses the XML with roxmltree,
  bounds the recursion with `svg_bounds::bounds` (longest path over
  nesting and `url(#…)`/`href` edges, `<style>` rules matched with
  simplecss as usvg matches them, cycles refused) and only then calls
  `Tree::from_xmltree` on the same document. Keep it until a usvg
  release bounds the converter's recursion; then set `max_depth` and
  drop the check (keep `accepted_depths_fit_a_two_megabyte_stack`).

## U18 — roxmltree: the tokenizer recursed once per nesting level

- **Project:** [RazrFalcon/roxmltree](https://github.com/RazrFalcon/roxmltree).
  Affected: 0.21.1 and earlier; fixed in 0.21.2 (2026-10-24: "Element
  content is now parsed iteratively instead of recursing once per
  nesting level").
- **Problem:** `tokenizer::parse_element` / `parse_content` recursed per
  level, ~14 KB of stack each in a debug build: a 2 MB thread overflowed
  at ~150 nested elements. usvg 0.49 accepts `roxmltree ^0.21.1`, so an
  older lockfile keeps the bad version (bevy_markup's did).
- **Local workaround:** `roxmltree = "0.21.2"` in `Cargo.toml` (the `svg`
  feature), a version floor. Status: fixed upstream; the floor stays as
  long as usvg allows 0.21.1.

## U19 — usvg: the default image resolver reads any file path

- **Project:** linebender/resvg (`usvg`). Affected: 0.49.0.
- **Problem:** `ImageHrefResolver::default_string_resolver` treats every
  non-`data:` `href` as a file path (absolute, cwd-relative without
  `resources_dir`, `..` without containment) and `std::fs::read`s it,
  embedding bitmaps and parsing SVGs (probe report §2, under `strace`).
  Reasonable for a command-line converter; a hazard for libraries that
  render untrusted SVG, and the docs don't warn.
- **Upstream:** not reported. Suggest a docs warning, or a resolver that
  confines paths to `resources_dir`.
- **Local workaround:** `svg::options` installs a resolver whose closures
  return `None`.

## U20 — resvg: `feTurbulence` overflows `i32` after enough octaves

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg. Affected: 0.49.0, 0.50.0. Reported upstream as
  [#1185](https://github.com/linebender/resvg/issues/1185) ("Panic
  attempt to multiply with overflow in filter/turbulence.rs", open); #937
  (a multiply overflow in `lib.rs`) is open too.
- **Problem:** `filter/turbulence.rs` `noise2` computes
  `bx0 = (x + 4096) as i32` and `bx1 = bx0 + 1`; each octave doubles `x`,
  so after enough octaves the cast saturates and `+ 1` overflows. A panic
  where overflow checks are on; release builds wrap (the lattice index is
  masked, so the noise is merely wrong).
- **Reproduction:** draw `<filter id="f"><feTurbulence baseFrequency="0.05"
  numOctaves="100"/></filter><rect width="16" height="16"
  filter="url(#f)"/>` in a debug build. A huge finite frequency reaches
  the same line in one octave: `baseFrequency='1e20'` on a 64 × 64 image
  (found 2026-11-08; about 3e7 is enough there). Not in #1185 yet: add it
  when reporting.
- **Impact on bevy_markup:** Bevy turns a panicking asset loader into a
  failed load, but the `<img>` redraws run as `AsyncComputeTaskPool`
  tasks, and polling a panicked task re-raises the panic in the poller,
  the app's main thread (probe report §8).
- **Local workaround:** `svg::guarded` wraps usvg's parse and resvg's
  draw in `catch_unwind`: the panic becomes `SvgError::Panicked` (the load
  fails, or the size isn't redrawn). Keep it while resvg can panic on
  input; it costs nothing on the normal path. The `svg` fuzz target
  (debug assertions on) lets this panic unwind into `guarded` instead of
  aborting (`fuzz::is_known_upstream_panic`, matched by file and
  message in `resvg-0.50.0/src/filter/turbulence.rs`), so it no longer
  turns the nightly red; `fuzz::svg_tests` fails once a resvg release
  stops panicking, as a reminder to drop the excuse.

## U21 — astral-tl: `/` is a name character, so `<b/>` never closes

- **Project:** [astral-sh/astral-tl](https://github.com/astral-sh/astral-tl)
  0.8.1 (crates.io `astral-tl`, library `tl`; the latest release, checked
  2026-11-07). y21/tl 0.7.8 has the same `IDENT_CHARS` table.
- **Problem:** `util.rs` `IDENT_CHARS` includes `/`, and `parse_tag`
  reads a tag name with `read_ident`: `<b/>` is an element named `b/`,
  the self-closing check finds no `/`, and the element stays open over
  the rest of the document. The same goes for a value-less attribute
  before `/>` (bug_0185).
- **Reproduction:** `tl::parse("<p>A <br/> B</p><p>C</p>")`: the second
  `p` is a child of `br/`.
- **Upstream:** not reported. A PR sketch: end `read_ident` for tag and
  attribute names at `/` (keep it for unquoted values), or check for
  `/>` after the name.
- **Local workaround:** `template::normalize_self_closing` (a space
  before such a `/>`) in `HtmlDocument::parse` and `walk_translation`.

## U22 — svgtypes / kurbo: arc subdivision grows with the radius, and the drain is quadratic

- **Project:** RazrFalcon/svgtypes 0.16.2 (`SimplifyingPathParser`) with
  linebender/kurbo 0.13.1 (`Arc::to_cubic_beziers`); usvg 0.50 builds
  shapes' arcs through tiny-skia's `PathBuilder::arc_to` with the same
  kurbo call.
- **Problem:** the tolerance is an absolute 0.1 user units, and
  `append_iter` makes `ceil(max((1.1163·r/0.1)^⅙, 4) · sweep/2π)` cubics:
  1.5·10⁵ for one arc at `r = 1e30`, ~7·10¹¹ at `r = 1e70` (past ~1e77,
  `rx·ry` squared can overflow and the count be NaN, so 0). The sweep is
  computed from where the previous arc's last cubic ended, a rounding
  error off the written point, and a large radius amplifies that error:
  `A1e92 1e9 0 1 1 0 0` after `A1e9 1e9 0 1 1 1e9 0` is ~3·10¹⁵ cubics,
  one from the exact point (bug_0193). The simplifier
  pushes every cubic into `buffer` and returns them with
  `self.buffer.remove(0)`, which moves the rest each time: quadratic.
- **Reproduction:** `<path d='M0 0 A1e30 1e30 0 1 1 1e30 0'/>` converts
  for 4.3 s (ASan); `A3e38 3e38 0 1 1 3e38 0` runs past 100 s.
- **Local workaround:** `svg_bounds::path_segments` counts each arc's
  cubics with kurbo's arithmetic (walking svgtypes' raw `PathParser`), and
  the work bound refuses the document before usvg runs (ADR 0040). After
  an arc of radius past ~1.5·10⁶ the simplifier's point may be far off
  the written one (3.5·10¹⁵ after a 2·10¹⁷ arc), so the count bounds that
  distance and charges the following arcs for it until an absolute
  coordinate (bug_0254).
- **PR sketch:** svgtypes: drain with a `VecDeque` (or an index), and cap
  the cubics per arc (a curve is never visibly better past a few hundred).
  kurbo/usvg: a tolerance relative to the radius, or to the output scale.

## U23 — usvg: a pattern inheriting a paint naming itself recurses forever

> **Unreachable since ADR 0051 (2026-11-26):** patterns are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49.0; fixed in
  0.50.0 (the reproduction below converts in debug and release).
- **Problem:** converting a pattern converts its content. A shape inside
  that inherits `fill`/`stroke` naming the pattern from an ancestor of the
  pattern converts the pattern again, without end: the paint cache is
  filled only after the content converts. svgtree's recursive-pattern
  check looks at the content's own attributes, not inherited ones.
- **Reproduction:** `<g fill='url(#p)'><pattern id='p' width='10'
  height='10' patternUnits='userSpaceOnUse'><rect width='5'
  height='5'/></pattern><rect width='64' height='64'/></g>` overflows the
  stack.
- **Local workaround:** inherited paint servers are edges of
  `svg_bounds::bounds`' graph, so the document is a refused cycle. Kept
  on 0.50: the same edges measure the depth U17's check needs.
- **PR sketch:** carry the patterns being converted in `State` (as
  `parent_markers` does for markers) and resolve a nested reference to
  one of them as `none`.

## U24 — tiny-skia: the anti-aliased row range wraps on paths far off the canvas

- **Project:** linebender/tiny-skia 0.12.0.
- **Problem:** `scan/path.rs` (around line 164) takes the path's integer
  bounds before clipping them to the canvas (the code's own `TODO:
  SkScanClipper`; Skia clips first) and, for anti-aliasing, shifts them
  left by 2 (`start_y <<= shift_edges_up`). Past |y| ≥ 2²⁹ the `i32`
  wraps and the edge walk starts at a garbage row. Debug builds then fail
  `walk_edges`' `debug_assert!(edges[curr_idx].last_y >= curr_y as i32)`;
  release builds write outside the canvas (U28). Lines alone trigger it;
  non-anti-aliased fills don't.
- **Reproduction:** a 64 × 64 SVG with `<path d='M603553400 -786566100
  L-232962910 303603170 L554380750 -829689350 Z'/>`, or `<path d='M0 0
  A1e9 1e9 0 1 1 1e9 0 A1e9 1e9 0 1 1 0 0'/>` (seed `004a6ba5…` in
  `fuzz/seeds/svg/`). Through tiny-skia alone: fill that triangle at
  identity with `anti_alias = true` (halving the coordinates still panics,
  quartering them doesn't).
- **Impact on bevy_markup:** before the workaround, a failed load
  (`svg::guarded` catches the panic). An earlier version of this entry
  said release builds were unaffected; a release replay showed U28
  ([bug_0194](bug_0194.md)).
- **Local workaround:** `svg::check_extent` refuses a drawing that reaches
  more than `MAX_SVG_EXTENT` (2¹⁹ px) outside the image, measured on
  control points with strokes padded, through every transform resvg
  applies (`SvgError::TooFarOut`). Drop it, and the limit, once tiny-skia
  clips first.
- **PR sketch:** clip the integer bounds to the clip rect (Skia's
  `SkScanClipper`) before the shift, or compute the shift in `i64` and
  reject what doesn't fit.

## U25 — usvg: no bound on conversion work

- **Project:** linebender/resvg, crate usvg. Affected: 0.49.0.
- **Problem:** usvg's only size limit counts its tree's nodes after `<use>`
  copies (a million). Conversion is ~7 µs per node in release, so a fan-out
  under the limit converts for seconds (bug_0188). Markers are converted per
  path vertex and aren't counted at all (bug_0189). Not a defect in usvg's
  terms: it has no notion of a budget.
- **Local workaround:** `svg_bounds::bounds` estimates the conversion's work
  from the XML (instances over the reference graph, inherited markers and
  paint servers, path segments), refused over `MAX_SVG_WORK` (ADR 0040).
- **PR sketch:** an `Options::max_work`-style budget counted by the
  converter itself (nodes built plus segments), failing with an error past
  it. With it, the model here could go.

## U26 — usvg: an infinite `baseFrequency` panics

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49.0, 0.50.0.
- **Problem:** `parser/filter.rs` `convert_turbulence` parses
  `baseFrequency` as a number list, converts to `f32` (a value past
  `f32::MAX` becomes `inf`), checks only `is_sign_positive()` and calls
  `PositiveF32::new(x).unwrap()`, which is `None` for infinity. An
  unconditional `unwrap`: release builds panic too.
- **Reproduction:** `<filter id='f'><feTurbulence
  baseFrequency='1e40'/></filter><rect width='8' height='8'
  filter='url(#f)'/>`. Found by a local `svg` fuzz run on 2026-11-08
  (artifact `crash-05abaafb…`, a 557-byte filter chain).
- **Impact on bevy_markup:** before the workaround, the load failed with
  `SvgError::Panicked` (`guarded`), and libFuzzer counted it a crash
  ([bug_0192](bug_0192.md)).
- **Local workaround:** `svg_bounds::check_values` refuses an
  `feTurbulence` whose `baseFrequency` holds a non-finite `f32` before usvg
  runs (`SvgError::OutOfRange`). A number past `f64` fails svgtypes' parse
  and usvg ignores the attribute, so that isn't refused. Drop the check
  once usvg handles it.
- **PR sketch:** treat a non-finite frequency like a negative one (keep the
  default), or use `PositiveF32::new(x).unwrap_or_default()`.

## U27 — tiny-skia: the cubic clipper unwraps a `t` of 1.0

- **Project:** linebender/tiny-skia 0.12.0.
- **Problem:** `edge_clipper.rs` (around line 568) `mono_cubic_closest_t`
  bisects for the `t` where a monotonic cubic crosses a clip edge, until
  the point is within 0.25 px. Between ~2²¹ and 2²² px the f32 spacing is
  0.25 px or coarser, so the search never gets within tolerance and `t`
  walks up to exactly 1.0. `NormalizedF32Exclusive::new(1.0).unwrap()` then
  panics, in every build. Above 2²² `too_big_for_reliable_float_math`
  draws such a cubic as a line, so only that band is affected.
- **Reproduction:** `svg::tests::paths_far_outside_the_image_are_refused`'s
  cubic (three cubics around 2·10⁶ px, `crispEdges`, stroke width 3), or
  artifact `crash-13d520f9…` (a seed in `fuzz/seeds/svg/`).
- **Local workaround:** as U24 (`MAX_SVG_EXTENT` is below 2²¹).
- **PR sketch:** stop the bisection after a fixed number of steps and
  clamp `t` into the exclusive range, or use an inclusive type there.

## U28 — tiny-skia: release builds write outside the canvas (U24)

- **Project:** linebender/tiny-skia 0.12.0.
- **Problem:** with U24's wrapped row range and no debug assertion, the
  scan converter writes coverage outside the canvas. Later code catches
  it: `alpha_runs.rs` (around line 189) unwraps a `None`, or
  `pipeline/mod.rs` (around line 205) slices out of range.
- **Reproduction:** U24's inputs in a release build (`cargo fuzz run -O -s
  none … svg <seed>`); artifact `crash-c7219239…` (a seed).
- **Local workaround:** as U24.
- **PR sketch:** fixed by U24's fix; report together.

## U29 — usvg: `feConvolveMatrix` `order` overflows `u32`

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49.0, 0.50.0.
- **Problem:** `parser/filter.rs` `convert_convolve_matrix` reads `order`
  as one or two numbers saturated to `i32`, keeps them when positive, and
  multiplies them as `u32` unchecked: `order_x * order_y` when a
  `kernelMatrix` is present (line 674), and `columns * rows` in
  `tree/filter.rs` `ConvolveMatrixData::new` (line 459). Debug builds
  panic (`attempt to multiply with overflow`). Release builds wrap. From
  reading the code: `order='65536'` with no `kernelMatrix` wraps to 0, which
  equals the empty matrix's length, so usvg accepts a 65536 × 65536 kernel
  with no data. resvg would then index that empty matrix; bevy_markup's
  cost estimate multiplies the sides in `f64` and refuses it first.
- **Reproduction:** `<filter id='f'><feConvolveMatrix
  order='65536'/></filter><rect width='8' height='8' filter='url(#f)'/>`
  (seed `ddf57483…` in `fuzz/seeds/svg/`). Found by a local `svg` fuzz run
  on 2026-11-09 (three artifacts, all filter chains whose `order` was
  mangled into a huge number).
- **Local workaround:** `svg_bounds::check_values` refuses an `order` whose
  product (read as usvg reads it) doesn't fit in `u32`
  (`SvgError::OutOfRange`). Drop it once usvg checks.
- **PR sketch:** `checked_mul` in both places, treating an overflow like a
  mismatched `kernelMatrix` (the primitive is dropped); or cap the order at
  a sane size, since a large kernel is quadratic per pixel anyway.

## U30 — usvg: `feMorphology` radius × scale unwrapped as `PositiveF32`

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49.0, 0.50.0.
- **Problem:** `parser/filter.rs` `convert_morphology` (lines 1028–1029)
  reads `radius` as `f32`s and, when both are non-negative, unwraps
  `PositiveF32::new(rx * scale.width())` (and `ry`). `PositiveF32` refuses
  infinity, so a radius past `f32::MAX` (`1e49` parses to infinity) panics
  in every build. Under `primitiveUnits="objectBoundingBox"` the scale is
  the element's bounding box, so a finite radius over a huge box panics
  too (radius 2, box width 2·10³⁸).
- **Reproduction:** `<filter id='f'><feMorphology
  radius='1e49'/></filter><rect width='8' height='8' filter='url(#f)'/>`
  (seed `67bfa887…` in `fuzz/seeds/svg/`). Found by a local `svg` fuzz run
  on 2026-11-09.
- **Local workaround:** `svg_bounds::check_values` refuses a radius that
  isn't finite as `f32` (`SvgError::OutOfRange`); a finite radius counts in
  the region factors of U31's workaround (ADR 0042). Drop both once usvg
  checks.
- **PR sketch:** treat a non-finite product like a negative radius (the
  primitive's default, or a transparent result as the spec says for a
  disabled effect) instead of unwrapping; `PositiveF32::new(…).unwrap_or(…)`.

## U31 — tiny-skia-path: `NonZeroRect::bbox_transform` unwraps an overflowed rect

> **Unreachable since ADR 0051 (2026-11-26):** filters and the `filter` property are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/tiny-skia, crate tiny-skia-path. Affected:
  0.12.0 (via usvg 0.49.0, 0.50.0).
- **Problem:** `rect.rs` `bbox_transform` (line 691) multiplies the rect by
  the bounding box and unwraps `NonZeroRect::from_xywh`, which is `None`
  when a side overflows to infinity. usvg calls it for every
  `objectBoundingBox` region; the default filter region (−10%, 120%)
  overflows on a box wider than ~2.8·10³⁸.
- **Reproduction:** `<filter id='f'><feOffset/></filter><rect width='3e38'
  height='8' filter='url(#f)'/>`. Found while probing the U30 workaround on
  2026-11-09.
- **Local workaround:** ADR 0042: `svg_boxes::largest_box` converts a
  copy of the document that reaches no `bbox_transform` (filters, masks,
  clip paths off, paints plain, `defs`/`mask`/`clipPath`/`pattern` content
  renamed to `g`) and reads usvg's boxes; `svg::check_regions` refuses
  (`SvgError::RegionOverflow`) when the largest box × the largest region
  factor could overflow. Drop it once tiny-skia-path or usvg checks.
- **PR sketch:** return `Option<Self>` from `bbox_transform` (a breaking
  change), or a checked variant that usvg's callers use, dropping the
  element as they do for other invalid regions.

## U32 — resvg: box blur width overflows `i32`

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate resvg. Affected: 0.49.0, 0.50.0.
- **Problem:** `filter/box_blur.rs` `create_box_gauss` computes the ideal
  box width `sqrt(12σ²/5) + 1` in `f32`, casts it to `i32` (saturating at
  `i32::MAX` once σ² is infinite, from σ ≈ 1.8·10¹⁹; the cast alone reaches
  `i32::MAX` from σ ≈ 1.4·10⁹) and adds 2 unchecked (line 48). Overflow
  checks panic; release wraps to a negative box, which `apply` turns into a
  huge `usize` radius [reading the code]. σ is the deviation × the
  transform's scale (`resolve_std_dev`), so a `viewBox` can grow it.
- **Reproduction:** `<filter id='f'><feGaussianBlur
  stdDeviation='1.4e31'/></filter><rect width='40' height='40'
  filter='url(#f)'/>` (seed in `fuzz/seeds/svg/`, bug_0198). Found by the
  nightly `svg` fuzz job on 2026-11-09.
- **Local workaround:** `svg::check_extent`'s walk refuses a deviation
  past `MAX_SVG_BLUR` (2²⁰ px at the drawn size; `SvgError::TooBlurred`).
- **PR sketch:** clamp σ to the image's larger side (a wider blur gives the
  same result: the loops already stop at the image size), or compute the
  box sizes in `i64` / with `saturating_add`.

## U33 — bevy_picking: ray picking needs a primary window, even for image targets

- **Project:** bevyengine/bevy, crate bevy_picking. Affected: 0.20.0.
- **Problem:** `pointer.rs` `Location::is_in_viewport` normalizes the
  camera's `RenderTarget` against `primary_window.single()` and returns
  `false` when there is no primary window, before comparing targets. An
  image target needs no window to normalize, so a pointer located on an
  image (a `ViewportNode`'s forwarded pointer, or the mouse of an app
  rendering offscreen) never gets a ray (`RayMap::repopulate`), and mesh
  and sprite picking hit nothing.
- **Reproduction:** the `canvas` example under `--offscreen` (no winit, no
  window): the forwarded pointer reaches the portrait image at the right
  position (`viewport_picking` works), but `RayMap` stays empty for every
  pointer. Found while playtesting ADR 0041's example on 2026-11-09.
- **Local workaround:** none needed in the library. The examples' harness
  (`examples/shared/harness.rs`) gives its `--offscreen` composition a
  primary `Window` entity with no native window (without winit it never
  gets a `RawHandleWrapper`, so the renderer never extracts it): dragging
  and clicking the `canvas` example's hero then work offscreen. The test
  harness has such a window too (`tests/canvas.rs`
  `pointer_input_reaches_the_cameras_scene`). Drop the harness's window
  once Bevy checks the target first.
- **PR sketch:** normalize with `primary_window.single().ok()` and let
  `RenderTarget::normalize` fail only for `WindowRef::Primary` targets.

## U34 — bevy_ui: UI picking never hits inline boxes

- **Project:** bevyengine/bevy, crate bevy_ui. Affected: 0.20.0.
- **Problem:** `picking_backend.rs` resolves a text node's hit to a span by
  `pick_ui_text_section` over `TextLayoutInfo::run_geometry`. Inline boxes
  (`InlineBox` children: `<img>`/inline `<svg>` images in bevy_markup) are
  laid out in `TextLayoutInfo::inline_boxes` (entity, kind, rect) but never
  hit-tested, so the pointer over one hits the text node itself.
- **Reproduction:** `<p><a href="next.html"><img src="frame.png"></a>
  text</p>`, click the image: the hit is the paragraph's
  (`signals::an_image_inside_a_link_opens_it` passes through the
  workaround).
- **Local workaround:** `signals::pick_inline_boxes`, a picking system
  after the backends: for a hit on a text node, it tests the hit's point
  (normalized position × node size, relative to the content box) against
  `inline_boxes` and adds a hit on a bound, pickable box's entity just in
  front of the text node (bug_0199). Drop it once Bevy hit-tests inline
  boxes.
- **PR sketch:** in `pick_ui_text_section`'s caller, test
  `inline_boxes`' rects before the runs and report the box's entity.

## U35 — tiny-skia-path: `to_int_rect` unwraps an overflowing rect

> **Unreachable since ADR 0051 (2026-11-26):** filters are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/tiny-skia, crate tiny-skia-path. Affected:
  0.12.0 (via resvg 0.49.0, 0.50.0).
- **Problem:** `NonZeroRect::to_int_rect` (`rect.rs:700`) and
  `IntSize::to_int_rect(x, y)` (`size.rs:87`) unwrap `IntRect::from_xywh`,
  which checks `x + width` in `i32`. resvg's `filter/mod.rs` converts the
  filter region and every primitive subregion with the first, and
  `apply_offset`/`apply_drop_shadow` position a `draw_pixmap` (the second)
  at the scaled offset saturated to `i32`.
- **Reproduction:** `<filter id='f' primitiveUnits='objectBoundingBox'>
  <feFlood width='4e5'/></filter><rect width='6000' height='10'
  filter='url(#f)'/>`; or `<feOffset dx='1e8'/>` under `scale(2e2)`.
  Found by bevy_markup's `hostile_magnitudes_never_panic` proptest,
  2026-11-09.
- **Local workaround:** `svg::check_extent` refuses regions, subregions
  and offsets reaching past `MAX_SVG_FILTER_REGION` (2²⁴ px).
- **PR sketch:** return `Option<IntRect>` (or clamp) and let resvg treat
  an unrepresentable region as empty, as it does for invalid ones.

## U36 — resvg: pattern tiles are allocated without a limit

> **Unreachable since ADR 0051 (2026-11-26):** patterns are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg. Affected: 0.49.0, 0.50.0 (tiny-skia 0.12.0).
- **Problem:** `path.rs` `render_pattern_pixmap` sizes the tile pixmap as
  the pattern rect × the scale of the path's transform with the
  pattern's, rounded, and creates it with `Pixmap::new`, which allocates
  `vec![0; len]` (tiny-skia's own note: it can't check the allocation).
  A tile of 10⁶ px a side asks for 4 TB and aborts the process.
- **Reproduction:** `<pattern id='p' width='1e4' height='1e4'
  patternUnits='userSpaceOnUse'><rect width='1' height='1'/></pattern>
  <rect width='8' height='8' fill='url(#p)' transform='scale(100)'/>`.
  Found by bevy_markup's `hostile_magnitudes_never_panic` proptest,
  2026-11-09 (133 TB).
- **Local workaround:** `svg::estimate_cost` charges each tile at its
  drawn size, so such a document exceeds the cost budget and is refused.
- **PR sketch:** cap the tile at the canvas size (a larger tile shows at
  most the canvas) or use `try_reserve` and skip the paint on failure.

## U37 — parley 0.11: tall inline boxes get negative leading

- **Project:** linebender/parley. Affected: 0.11.1 (Bevy 0.20's bevy_text
  depends on `^0.11`). Fixed in 0.12.0 (2026-10-09): line boxes are
  computed CSS-style from baseline-relative extents, with
  `vertical-align`.
- **Problem:** `layout/line_break.rs` `finish_line` sets
  `ascent = max(ascent, box.height)` for an in-flow inline box and takes
  the line height from `running_line_height` (the tallest cluster or
  box). For a box taller than the line height, `leading = line_height −
  (ascent + descent)` is −descent: the box rises by half the descent and
  the text's descent is missing from the line.
- **Reproduction:** `<p>Tall <img src="frame.png" style="…"> icon</p>`
  with `html { font-size: 20px }` and the image 20 × 40: Chromium's
  paragraph is 45px with the image at its top; Bevy's is 40px with the
  image at −3 (`a_tall_inline_image_grows_the_line_like_css`, ignored).
- **Local workaround:** none (Bevy places the box from parley's metrics).
  The `layout_svg` vector keeps its images below the font's ascent.
- **On a Bevy bump:** if bevy_text depends on parley ≥ 0.12, run
  `cargo test --test html_ui -- --ignored a_tall_inline_image`, un-ignore
  it, add a tall image to `layout_svg` (and re-record it), and mark this
  entry removed.

## U38 — bevy_ui: a viewport camera's missing target image panics

- **Project:** bevyengine/bevy, crate bevy_ui. Affected: 0.20.1.
- **Problem:** `widget/viewport.rs` `update_viewport_render_target_size`
  resizes the camera's target with `images.get_mut(image_handle)
  .unwrap()` (line 188). A camera whose `RenderTarget::Image` handle has
  no image in `Assets<Image>` (reserved, still loading, or removed)
  panics.
- **Reproduction:** a `ViewportNode` (here a `<canvas>` with
  `CanvasSource::Camera`) over a camera targeting a reserved handle:
  `called Option::unwrap() on a None value` on the first frame
  (review_0007 K15, bug_0218).
- **Local workaround:** `canvas.rs` `sync_canvases` binds a camera's
  viewport only while its target image is in `Assets<Image>` (bug_0218,
  `tests/canvas.rs` `a_camera_without_its_image_waits_for_it`). Drop the
  check once Bevy skips a missing image.
- **PR sketch:** skip (or warn once) when `images.get_mut` returns
  `None`, as the render-target extraction does for missing images.

## U39 — Tera: no limit on a render's work or memory

> **Moot for bevy_markup since ADR 0049 (2026-11-25): it no longer uses Tera; MiniJinja's fuel bounds renders, and `tera_check` is removed.**

- **Project:** Keats/tera. Affected: 2.4.0.
- **Problem:** the interpreter bounds `range` (`MAX_RANGE_LEN`, 10⁵) and
  component recursion (20) but nothing else: instructions run without a
  budget, `~` concatenation, filters (`replace`, `join`, `indent`) and
  captures allocate without a cap, and a list nested in itself (`[a, a]`,
  shared `Arc`s) is exponential when written. A failed allocation aborts
  the process.
- **Reproduction** (release, 2026-11-11): `{% set_global s = "x" %}{% for i
  in range(end=24) %}{% set_global s = s ~ s %}{% endfor %}` builds 16 MB in
  5 ms (40 iterations: 1 TB); two nested `range(end=100000)` loops with an
  empty body run 10¹⁰ iterations.
- **Local workaround:** `src/tera_check/` (ADR 0044) ports Tera's lexer and
  parser and refuses, at load, a template whose bounds pass the limits;
  `template::MAX_RENDERED_BYTES` caps the output. A vendored Tera with an
  instruction budget and a value cap was tried and dropped (~10% render
  cost; `[patch]` doesn't reach dependents).
- **PR sketch:** an optional render budget on `Tera` (instructions, bytes
  written, largest value), checked in the interpreter loop with batched
  counters; and expose the parser (`parsing::parser`) so checks like ours
  don't have to port it.

## U40 — simplecss: descendant selectors match in exponential time

> **Unreachable since ADR 0051 (2026-11-26):** `<style>` sheets are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/simplecss. Affected: 0.2.2 (used by usvg 0.50's
  `resolve_css` and by `svg_bounds::Graph::new`).
- **Problem:** `Selector::matches_impl` (`selector.rs:174`) matches a
  descendant combinator by trying each ancestor in turn and recursing
  into the rest of the selector, with no memo. When the leftmost
  compound never matches and the others always do, an element `d` deep
  costs Σ_{j≤k} C(d, j) calls for k descendant combinators (the count is
  exact: `svg_bounds::tests::descendant_calls_count_simplecss_backtracking`
  replays it).
- **Reproduction** (release, 2026-11-12): one `<style>` rule
  `q g g g g g g` over 60 nested `<g>`s around a `<rect>` (553 bytes):
  usvg's conversion takes 5.2 s; `q g g g g g g g` (555 bytes) 35.7 s
  (review_0008 L07, bug_0229).
- **Local workaround:** `svg_bounds::match_work` charges every rule by
  that count over the document's depth histogram, times its class,
  attribute and pseudo-class tests, into `MAX_SVG_WORK` before anything
  matches (bug_0229, bug_0231).
- **PR sketch:** for a selector of descendant combinators only, match
  greedily (the nearest ancestor matching a compound is always the best
  choice, so no backtracking is needed); with child combinators mixed
  in, memoize (compound index, element) failures for the duration of
  one `matches` call. Either makes a match O(depth × compounds).

## U41 — usvg: text layout is quadratic in a chunk's characters

> **Unreachable since ADR 0051 (2026-11-26):** `<text>` are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49, 0.50 (`text`
  feature).
- **Problem:** `ByteIndex::code_point_at` (`text/layout.rs:1871`)
  converts a byte offset to a character index by counting
  `char_indices()` from the start of the chunk's text, and
  `resolve_relative_shifts` (`layout.rs:563`, run for every chunk) calls
  it once per cluster, as do the rotation (`layout.rs:610`) and
  text-on-path (`layout.rs:716`) code. A chunk of n characters costs
  O(n²); multi-byte characters decode slower (×3.4).
- **Reproduction** (release; Bevy's default font; one `<text>` at 10 px;
  conversion only): 20 000 `M` → 0.17 s, 40 000 → 0.65 s, 100 000 →
  3.93 s; with a character the font lacks, 0.55 s and 13.5 s. 20 000
  one-letter `<text>` elements take 0.10 s (review_0008 L10, bug_0232).
  Located by reading (no profiler on this machine); the doubling
  measurements fit n².
- **Local workaround:** `svg_bounds::Graph::text_work` charges a `<text>`
  by its characters² / 32 (a chunk is at most the whole element) into
  `MAX_SVG_WORK`.
- **PR sketch:** compute each cluster's code-point index once, with a
  single pass over `char_indices()` building a byte → code point map
  (or walk clusters in byte order with a running count).

## U42 — simplecss: parsing a style sheet is quadratic in its length

> **Unreachable since ADR 0051 (2026-11-26):** `<style>` sheets are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/simplecss. Affected: 0.2.2.
- **Problem:** errors carry a `TextPos` computed eagerly by
  `Stream::gen_text_pos`, which counts the lines from the start of the
  text (`stream.rs:281`). The parser raises (and drops) such errors on
  ordinary input: `consume_declaration` ends every declaration with a
  failing `consume_term` (`InvalidIdent` at the `;` or `}`), a number
  without a unit tries `consume_ident` for its suffix, a `#` colour
  starting with a digit too. Each costs O(offset), so a sheet costs
  O(length × declarations).
- **Reproduction** (release, `StyleSheet::parse_more`, N rules of
  `rect { fill: red }`): 5 000 → 16.5 ms, 10 000 → 61.5 ms, 20 000 →
  245 ms, 40 000 → 962 ms. usvg parses each `<style>` again, and
  bevy_markup's box pass a third time (review_0008 L09's 3.25 s,
  bug_0231).
- **Local workaround:** `svg_bounds::style_scan_bytes` sums the offsets
  of the bytes where those errors arise (`:`, `{`, `#`, the start of a
  digit run) and charges them into `MAX_SVG_WORK` before simplecss
  parses the text.
- **PR sketch:** store the byte offset in the error and compute the
  row/column only in `Display` (or when asked); or have
  `consume_term`/`consume_ident` peek instead of erroring on the normal
  end of a value.

## U43 — usvg: each text span reshapes the whole chunk

> **Unreachable since ADR 0051 (2026-11-26):** `<text>` are outside the icon subset and refused before usvg runs; the workaround described below was removed with it. Kept for the record and for reporting upstream.

- **Project:** linebender/resvg, crate usvg. Affected: 0.49, 0.50 (`text`
  feature).
- **Problem:** `process_chunk` (`text/layout.rs:897`) shapes the chunk's whole
  text once per span (each `<tspan>` or text piece with its own font),
  then overwrites the first run's glyphs with the span's by looking each
  one up with `glyphs.iter().position(…)` and inserting or removing with
  `Vec::insert`/`remove`. A chunk of n characters in s spans costs s
  shapings of n characters plus up to n² for the merge.
- **Reproduction** (release; Bevy's default font; one `<text>` of n
  one-character `<tspan>`s alternating two font sizes): 1 000 → 0.12 s,
  2 000 → 0.48 s, 4 000 → 1.93 s; the same characters in one span take
  1.4–8.4 ms (bug_0232).
- **Local workaround:** `svg_bounds::Graph::text_work` charges a
  `<text>` by its spans × characters × 2 into `MAX_SVG_WORK`.
- **PR sketch:** shape each span's own byte range where the bidi runs
  allow it, or shape once per distinct font rather than per span; merge
  through a byte-index map instead of a linear search.

## Not upstream bugs (recorded so they aren't re-filed)

- **Bevy's text measure rounds up** (`TextMeasure::measure` ends in
  `.ceil()`): a deliberate pixel-snapping choice, not a defect; it makes
  layouts with fractional line heights drift from browsers. Layout vectors
  use whole line heights.
- **`rui314/setup-mold` has no effect on Rust builds:** rustc ≥ 1.90 links
  through its bundled `rust-lld` and bypasses `/usr/bin/ld`.

## U44 — fuzzcheck: the fuzzer unwraps every kept input as UTF-8

- **Project:** as U4.
- **Problem:** `fuzzer.rs`, after adding an input to a pool, builds the
  message of its complexity-consistency assertion eagerly:
  `String::from_utf8(serializer.to_data(&input.value)).unwrap()`. With
  `ByteSerializer` (raw `Vec<u8>` corpus files) the first kept input that
  isn't UTF-8 panics the fuzzer itself (`FromUtf8Error`), within
  milliseconds on the `svg` target.
- **Local fix:** `fuzzcheck/vendor/fuzzcheck/src/fuzzer.rs`:
  `String::from_utf8_lossy(..)` (2026-11-12, with the `svg` target).
- **PR sketch:** the same one-line change, or format the bytes lazily
  inside the assertion's message.

## U45 — lightningcss / cssparser-color: an infinite hue fails a debug assertion

- **Project:** [parcel-bundler/lightningcss](https://github.com/parcel-bundler/lightningcss)
  1.0.0-alpha.72 (`src/values/color.rs`) calling
  [servo/rust-cssparser](https://github.com/servo/rust-cssparser)'s
  `cssparser-color` 0.5.0 (`hsl_to_rgb`).
- **Problem:** `hsl(1e39 50% 50%)` — any `hsl()`/`hwb()` hue that is
  infinite as `f32`, also `calc(infinity)` — panics in debug builds:
  `assertion failed: (0.0..=1.0).contains(&hue)`. lightningcss
  normalizes with `(h - 360.0 * (h / 360.0).floor()) / 360.0`, NaN for an
  infinite `h`; `hsl_to_rgb` `debug_assert!`s the range. Release builds
  compute with the NaN.
- **Reproduction:** `StyleSheet::parse("p { color: hsl(1e39 50% 50%) }",
  ParserOptions::default())` in a debug build ([bug_0238](bug_0238.md)).
- **Local workaround:** `cascade::guarded_parse` (`src/cascade.rs`), used
  by the sheet loader, `inline_style` and the `var()` re-parse.
- **PR sketch:** in lightningcss, treat a non-finite hue as 0 (or
  `rem_euclid` and clamp) before `hsl_to_rgb`; CSS says an infinite
  angle in a colour clamps like any other out-of-range value.
- **On a bump:** `fuzz::…::known_upstream_panics_still_panic_as_excused`
  fails when the panic is gone or moved; drop the excuse (and consider
  dropping the guard) then.

## U46 — Tera: `unique` checks all previously accepted values per item

> **Moot for bevy_markup since ADR 0049 (2026-11-25): it no longer uses Tera.**

- **Project/version:** Tera 2.4.0, `src/filters.rs::unique`.
- **Status:** unreported; observed in the installed version, latest
  upstream release/default branch not checked in this fix session.
- **Reproduction:** `{% set x = range(end=40000) | unique %}{{ x | length }}`
  renders for about 2 seconds in release on the review machine, with
  4× time per doubling. `Vec::contains` makes n(n−1)/2 comparisons for
  distinct inputs. See [bug_0241](bug_0241.md).
- **Local workaround:** `tera_check::Analyzer::filter` charges the pair
  comparisons plus element scan cost and refuses context-sized inputs
  (`AppSquared`). Small constant inputs are accepted (ADR 0047).
- **PR sketch:** implement stable uniqueness with equality-compatible
  hashing, or provide an enforceable comparison/work budget. Tera's own
  comment says a simple ordered set does not preserve map semantics.
- **On a bump:** inspect the new algorithm and its equality semantics
  before reducing the cost charge or relaxing the refusal.

## U47 — fluent-syntax: wide calls have quadratic duplicate-name scans

- **Project/version:** fluent-syntax 0.11.1 and 0.12.0 (checked
  2026-11-25: the same `Vec::contains` scan),
  `src/parser/expression.rs::get_call_arguments`.
- **Status:** unreported.
- **Reproduction:** `m = { FUNC(a000000: 0, a000001: 0, ...) }` with 20,000
  distinct names. Every name is compared with all earlier names through
  `argument_names.contains`. See [bug_0243](bug_0243.md); the port of
  this scan in `ftl_check` reproduced 4× time per doubling in release.
- **Local workaround:** the shared load-time checker refuses the 65th
  argument of each call, before this upstream parser runs. The same
  limit protects the port's scan, the fuzz harness and Markdown paths.
- **PR sketch:** use a hash set for duplicate names while retaining the
  ordered output vector and the current duplicate-name error position.
- **On a bump:** compare the port with upstream again and reconsider
  `MAX_FTL_ARGUMENTS` after both scans have bounded cost.

## U48 — cargo-afl: `target-cpu=native` reaches proc macros and build scripts

- **Project/version:** cargo-afl 0.18.2, `src/main.rs` (the `RUSTFLAGS` it
  builds, `-C target-cpu=native` at line 287).
- **Status:** unreported; latest upstream release/default branch not
  checked.
- **Reproduction:** build a crate using a proc macro with `cargo afl test
  --no-run` (no `--target`, so `RUSTFLAGS` also apply to host artifacts),
  restore `target/` on a machine whose CPU lacks an instruction the build
  machine had. rustc loads the macro and dies with SIGILL. Nightly runs
  2026-11-16 to 2026-11-21 failed this way in `test-fuzz-macro`, on a
  different target each night, depending on the runner's CPU.
- **Local workaround:** `scripts/fuzz-setup.sh` and
  `scripts/fuzz-test-fuzz.sh` export `RUSTFLAGS` ending in `-C
  target-cpu=x86-64` for the AFL builds; cargo-afl appends the user's flags
  after its own, and rustc's last `target-cpu` wins. The `-x86-64` suffix on
  the `fuzz-test-fuzz` cache key retires the caches built with `native`.
- **PR sketch:** make `target-cpu=native` opt-out (an environment variable
  like `AFL_OPT_LEVEL`), or pass `--target` so the flags skip host artifacts.
- **On a bump:** check whether cargo-afl still sets `native`; if not, drop
  the override.

## U49 — lightningcss: a NaN math argument reaches `unreachable!()`

- **Project/version:** [parcel-bundler/lightningcss](https://github.com/parcel-bundler/lightningcss)
  1.0.0-alpha.72, `src/values/percentage.rs:30` (`Percentage::parse`).
- **Status:** fixed on master, awaiting release. [`b0939ab`](https://github.com/parcel-bundler/lightningcss/commit/b0939ab684793d2ba3ccc1a91e0f4d3ba837a708)
  (#1329, 2026-09-26) returns a parse error instead. Checked 2026-11-22: the
  newest release is still 1.0.0-alpha.72 (npm v1.33.0), and master
  `8823beb` parses all the reproductions below without a panic.
- **Problem:** `min()`, `max()` and `clamp()` can't order a NaN, so they
  stay unevaluated, and `Percentage::parse` treats any result other than
  `Calc::Value` as `unreachable!()`. This panics in release builds too.
- **Reproduction:** `StyleSheet::parse("p { opacity: min(1, NaN) }",
  ParserOptions::default())`. NaN can also be computed: `min(1, sqrt(-1))`,
  `min(1, mod(1, 0))`, `min(1, calc(infinity - infinity))`
  ([bug_0245](bug_0245.md)).
- **Local workaround:** `cascade::guarded_parse` catches the unwind. It can't
  help an app built with `panic = "abort"`, which aborts on this CSS.
- **PR sketch:** none needed; wait for the release containing `b0939ab`.
- **On a bump:** `fuzz::svg_tests::known_upstream_panics_still_panic_as_excused`
  fails when the panic is gone or moved; drop the excuse and the README's
  `panic = "abort"` warning then.

## U52 — bevy_ui: a NaN node size panics in `Rect::from_center_size`

- **Project/version:** [bevyengine/bevy](https://github.com/bevyengine/bevy)
  0.20.1: `bevy_ui::ui_node::ComputedNode::border_box`
  (`ui_node.rs:281`) → `bevy_math::Rect::from_center_size`
  (`rects/rect.rs:100`, `assert!(0. <= size.x && 0. <= size.y)`), reached from
  `bevy_ui::widget::text::text_system`.
- **Problem:** a `Node` whose computed size is NaN fails the assertion: the
  app panics, in release builds too. NaN reaches the layout from a value
  like `inf - inf` (here a CSS `calc()` that overflowed).
- **Reproduction:** a text node inside a node with `top: Val::Px(f32::NAN)`
  and `position_type: Absolute` (bevy_markup:
  `position: absolute; top: calc((3e38px + 1em + 3e38px) + (-3e38px + 1em - 3e38px))`).
- **Local workaround:** bevy_markup never writes a non-finite `Val`
  (`css::value::MAX_CSS_MAGNITUDE`, `CalcExpr::resolve`).
- **PR sketch:** sanitize non-finite style values when converting to taffy
  (treat NaN/infinite lengths as `auto` or 0), or make `border_box` tolerate
  NaN; at least `debug_assert!` plus a clamp.

## U53 — parley: an infinite or NaN layout width fails the line breaker's assertion

- **Project/version:** [linebender/parley](https://github.com/linebender/parley)
  0.11.1, `layout/line_break.rs:453`
  (`assert!(layout_max_advance == INFINITY || line_max_advance - layout_max_advance < 1.0)`),
  via `bevy_text::pipeline::layout_with_bounds`.
- **Problem:** a max advance that is NaN, or a line advance that overflows to
  infinity, fails the assertion: the app panics, in release builds too.
- **Reproduction:** text inside a Bevy UI node with `width: Val::Px(f32::MAX)`
  and `margin.left: Val::Px(f32::MAX)`, or `width: Val::Em(3e38)`.
- **Local workaround:** as U52.
- **PR sketch:** parley: treat a non-finite max advance as unbounded (or
  clamp it) instead of asserting; Bevy: don't pass non-finite bounds.

## U54 — zeno/swash: huge glyphs overflow the bitmap size

- **Project/version:** [dfrg/zeno](https://github.com/dfrg/zeno) 0.3.3
  (`mask.rs:34`, `Format::buffer_size`: `width * height * 4` in `u32`; also
  `raster.rs:339`), via [dfrg/swash](https://github.com/dfrg/swash) 0.2.10
  `scale::Render::render_into`, from `bevy_text::font_atlas::get_outlined_glyph_texture`.
- **Problem:** a glyph past ~32,768 px per side overflows `u32`. Debug builds
  panic (`attempt to multiply with overflow`); release builds wrap and then
  run for minutes at gigabytes (measured: `font-size: 2e5px`, `3e6px`,
  `1e8px` each over 2 min at 1.4–2.4 GB). Even without overflow the cost is
  unbounded: a 50,000 px glyph took 3.5 s and 6.9 GB.
- **Reproduction:** a Bevy `Text` with `TextFont { font_size: 200_000.0, .. }`.
- **Local workaround:** `cascade::MAX_FONT_SIZE` (8192 px) caps the computed
  CSS font size.
- **PR sketch:** zeno: compute buffer sizes in `usize` with checked
  arithmetic and refuse oversized masks; Bevy: cap the rasterized glyph size
  (render larger text as scaled outlines or refuse it).

## U55 — bevy_text: font atlases per exact size, never freed

- **Project/version:** [bevyengine/bevy](https://github.com/bevyengine/bevy)
  0.20.1, `bevy_text`: `FontAtlasSet` (`font_atlas_set.rs`, keyed by
  `FontAtlasKey { font_size_bits, … }`) and `FontAtlas::add_glyph`
  (`font_atlas.rs:161–173`: a glyph that fits no atlas gets a new one, the
  smallest power of two holding it, at least 512 px; RGBA8, kept in the main
  world and uploaded to the GPU).
- **Problem:** nothing ever removes an atlas (`FontAtlasSet::clear` drops
  everything, and nothing calls it). Every exact font size (the `f32` bits:
  `16.0` and `16.000001` are two) gets its own atlases, so memory grows with
  every size and glyph ever drawn: text whose size animates, or UI built from
  untrusted content, grows without bound. Large glyphs each take an atlas of
  their own; for tall glyphs that's twice the font size, which past 4096 px
  exceeds the 8192 px texture limit WebGPU and many GPUs have (a wgpu
  validation error is fatal in Bevy; not run on a GPU here).
- **Measurements** (release, headless, Bevy's default font; main-world
  bytes, the GPU holds the same again):

  | Text | Atlas memory |
  |---|---|
  | 1 glyph at 1024 / 4096 / 8192 / 16384 px | 4 MiB / 64 MiB / 256 MiB / 1 GiB |
  | 26 letters at 4096 px | 1.1 GiB |
  | 16 letters at 8192 px | 2.3 GiB |
  | 2,000 nodes, one glyph each, sizes 10, 10.001, 10.002, … px | 2.0 GiB |
  | 4,000 nodes, two glyphs each, sizes 16–1016 px in 0.25 px steps | 10.6 GiB (11.5 s) |

- **Reproduction:** spawn 2,000 `Text` nodes with `TextFont::from_font_size(10.0 + i as f32 * 0.001)`
  and one character each; read `FontAtlasSet::total_bytes`.
- **Local workaround:** `HtmlFontLimits` caps the size (default 2048 px) and
  the distinct sizes bevy_markup's text uses (default 128; past it a size
  reuses the nearest in use).
- **To investigate with Bevy:** eviction of atlases no live text uses
  (reference-counting glyphs per atlas, or an LRU with a memory budget),
  sharing atlases across sizes (quantized sizes, or a size-independent
  rasterization such as SDF/MSDF for large text), packing large glyphs
  instead of one atlas each, and refusing or downscaling a glyph larger than
  the device's `max_texture_dimension_2d`.

## U58 — honggfuzz-rs: `cargo hfuzz build` compiles for the build machine's CPU

- **Project/version:** [rust-fuzz/honggfuzz-rs](https://github.com/rust-fuzz/honggfuzz-rs)
  0.5.62, `src/bin/cargo-hfuzz.rs` (the release and instrumented branches
  push `-C target-cpu=native` before the user's `RUSTFLAGS`).
- **Problem:** a target binary uses whatever the build machine's CPU
  supports. Built once and reused elsewhere (a CI build cache), it dies on
  the first unsupported instruction. Nightly run 45522952063
  (2026-11-25): the `ftl` and `svg` honggfuzz jobs saved 22 and 2 "crashes",
  all `SIGILL` on `vmovups …, %zmm0` (AVX-512) within seconds of starting;
  every input replays cleanly on a normal build, debug and release.
- **Local workaround:** `RUSTFLAGS` gains `-C target-cpu=x86-64` in
  `scripts/fuzz-setup.sh` and `scripts/fuzz-honggfuzz.sh` (cargo-hfuzz
  appends the user's flags last, and the last `target-cpu` wins); the
  honggfuzz build cache gets the suffix `-x86-64` so native builds aren't
  restored. Same class as U48 (cargo-afl).
- **PR sketch:** make `target-cpu=native` opt-in (an `HFUZZ_NATIVE` switch),
  or put it before a documented override point.

## U56 — MiniJinja: `join`, `split`, `lines` and `escape` allocate past the fuel budget

- **Status (2026-11-26):** reported to the maintainer by email (2026-11-25/26), who asked for a public issue; issue and PR to open. Fixed on `fix/fuel-before-allocation` (from `main`), and in the MiniJinja bevy_markup patches in (`bevy_markup/3.1.0`, ADR 0052). Local guard for crates using bevy_markup: `template::fueled_join`, `fueled_split`, `fueled_lines`, `fueled_escape` charge the result up front.

| field | value |
|---|---|
| Upstream | [mitsuhiko/minijinja](https://github.com/mitsuhiko/minijinja) 3.1.0, `src/filters.rs` `join` (`charge_join`, `join_plain`) |
| Severity | high for users of `fuel` with untrusted templates: the budget is the advertised protection, and this defeats it (memory DoS) |
| Found | 2026-11-25, Claude Code session (`claude-opus-5-5`), measuring fuel for ADR 0049 |
| bevy_markup | not affected: it doesn't use MiniJinja (ADR 0049 is a proposal) |

### Problem

`fuel.rs` says allocations of known size are checked before they are made.
`join` checks the growing output only through `charge_join`:

```rust
ok!(state.charge_items(1));
if !joiner.is_empty() {
    ok!(state.ensure_bytes(output.len().saturating_add(joiner.len())));
}
```

1. With an empty joiner (`| join`, the default) nothing is checked per item:
   the whole result is built, then charged once it exists.
2. With a joiner, the check counts the output and the joiner but not the
   item about to be appended, so it overshoots by up to one item (an item
   can be 100 MB: `MAX_REPEATED_STRING_LEN`).

### Reproduction

Release build, `minijinja = { version = "3.1.0", features = ["fuel"] }`,
`env.set_fuel(Some(10_000_000))` (≈ 640 MB of byte charges):

```jinja
{% set a = "x" * 100000000 %}{{ [a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a,a] | join | length }}
```

Peak RSS 5,821 MiB, then `OutOfFuel` after 0.71 s (`/usr/bin/time`,
`prlimit --as=8 GiB`). With fuel 10⁶ it fails at once (4 MiB): the 100 MB
repetition alone exceeds the budget. A repeated list (`[a] * 30`) behaves
the same.

### Suggested fix

Check before every append, with the item's size: `ensure_bytes(output.len()
+ joiner.len() + item_len)` regardless of the joiner (for non-string items,
format into a scratch buffer and check before copying, or charge the
formatted length). Worth auditing the other filters that build a result in
a loop for the same pattern (`replace`, `indent`, `format`, `wordwrap`,
`center`, `map`/`list` conversions).

### Draft report (not sent)

> Subject: fuel: `join` can allocate far past the fuel budget
>
> With `fuel` enabled, `join` without a joiner builds its whole result
> before it is charged (`charge_join` only calls `ensure_bytes` when the
> joiner is non-empty, and never counts the next item). A template joining
> 30 references to a 100 MB string reaches 5.8 GB on a budget of 10⁷
> fuel (~640 MB). Reproduction and suggested fix above.

### Audit, 2026-11-25 (ADR 0049 branch)

Every built-in filter that can produce more than it takes in, at the
default bevy_markup budget (2²² fuel ≈ 268 MB of data), release, peak RSS
(`/usr/bin/time`), inputs sized to cost well under the budget:

| Filter | Template | Peak |
|---|---|---|
| `join` (no joiner) | 30 × a 100 MB string | 5,821 MiB (21×) |
| `split` | `("x " * 50000000) \| split` | 1,244 MiB (4.6×) |
| `lines` | `("x\n" * 50000000) \| lines` | 1,244 MiB (4.6×) |
| `escape` | `("\"" * 100000000) \| escape` | 1,245 MiB (4.6×) |
| `list`, `tojson`, `reverse`, `title`, `batch`, `slice`, `string`, `pprint`, `chain`, `zip`, `map`, `items`, `join` with a joiner, `~` doubling | — | ≤ 290 MiB (within ~1.1×) |

All four build their result and charge it afterwards (`split`/`lines`
charge nothing per piece before allocating the list; `escape` writes the
escaped string before it is charged). bevy_markup's workaround on the
branch: wrappers registered over the four names charge the result's size
through `State::consume_fuel` before calling `minijinja::filters::*`;
`tests/template_fuel.rs` (a counting allocator) keeps every case under 1×
of a 2¹⁸ budget (`join` reached 22.5× without the wrappers).

Draft report addition: "The same pattern affects `split`, `lines` and
`escape` (4.6× the budget each, table above)."

### Fix ready (2026-11-25): `../minijinja` branch `fix/fuel-before-allocation` (434c658f, local, not pushed)

Rust and Go. `join` checks output + separator + next item before growing
(with or without a separator); `split`/`lines` count pieces and check slots
first (Rust: `filters::split_checked`/`lines_checked`, registered instead,
public signatures kept, as `length_checked` on main; Go: in
`FilterSplit`/`FilterLines`, which now take the state); Rust `escape` checks
the escaped size first (`utils::html_escaped_len`, unit-tested against
`HtmlEscape`). Both `test_fuel_limits` bomb lists gain the cases. Without
the fix (100,000 fuel ≈ 6.4 MB, limit 4×): Rust `join` 357 MB, `split`/
`lines` 107 MB; Go `join` 101 MB, `split`/`lines` 75 MB. With it: Rust
≤ 17 MB, Go ≤ 3 MB. Suites: Rust 435, Go 8 packages, all pass.

Correction to the audit above: `escape` stays within MiniJinja's own 4×
tolerance (its input is charged twice before it runs, so its 6× output is
at most ~3× the budget); its change is hardening, not a violation. The
Go port's `escape` is fine.

Once released, bevy_markup can drop the wrappers in `template::new_environment`
and move `U56-template_fuel.rs` into `tests/`.

## U57 — MiniJinja: long flat expression chains overflow the stack in codegen

- **Status (2026-11-26):** reported to the maintainer by email (2026-11-25/26), who asked for a public issue; issue and PR to open. Fixed on `fix/chain-depth` (from `main`), and in the MiniJinja bevy_markup patches in (`bevy_markup/3.1.0`, ADR 0052). Local guard for crates using bevy_markup: `template::tera_tag_over_limit` refuses a tag holding more than 64 operators (bug_0182).

| field | value |
|---|---|
| Upstream | [mitsuhiko/minijinja](https://github.com/mitsuhiko/minijinja) 3.1.0, `src/compiler/codegen.rs` (expression compilation) |
| Severity | high for apps compiling untrusted templates: a ~20 KB template aborts the process (stack overflow is not a panic; it can't be caught) |
| Found | 2026-11-25, Claude Code session (`claude-opus-5-5`), measuring MiniJinja for ADR 0049 |
| bevy_markup | not affected: it doesn't use MiniJinja. Tera had the same class (bug_0182, guarded by `template::tera_tag_over_limit`) |

### Problem

The parser limits nesting depth (`MAX_RECURSION`, 500: 499 nested `(`,
`{% if %}` or `{% for %}` are refused with "template exceeds maximum
recursion limits"). A flat chain isn't nesting to the parser — it loops —
but it builds a left-deep tree, and code generation recurses down that tree
once per link. Parsing and dropping the tree are fine (checked with
`machinery::parse` on the same input); `Environment::template_from_str`
overflows.

### Reproduction

Thread with a 2 MB stack (Rust's default for spawned threads), minijinja
3.1.0, `template_from_str` only (no render):

| Chain | Debug: fine / overflow | Release: fine / overflow |
|---|---|---|
| `{{ 1\|string\|string… }}` | 800 / 1,600 filters | 6,400 / 12,800 |
| `{{ 1+1+1… }}` | 1,600 / 3,200 | 3,200 / 6,400 |
| `{{ 'a'~'a'~… }}` | 1,600 / 3,200 | 3,200 / 6,400 |
| `{{ true and true and … }}` | 1,600 / 3,200 | 3,200 / 6,400 |
| `{{ x.y.y… }}` | 800 / 1,600 | 6,400 / 12,800 |
| `{{ 1 is number == true … }}` | fine at 51,200 | fine at 51,200 |

`format!("{{{{ 1{} }}}}", "+1".repeat(6400))` (about 13 KB) aborts a
release build: "thread has overflowed its stack / fatal runtime error:
stack overflow, aborting".

### Suggested fix

Count chain length in the parser against the same depth budget it uses for
nesting (each binary operator, filter or attribute link adds one level), or
make codegen iterative over left-deep chains. The `stacker` feature
probably helps but isn't on by default [not tested].

### Draft report (not sent)

> Subject: stack overflow compiling long flat expression chains
>
> `template_from_str` on `{{ 1+1+…+1 }}` with 6,400 terms (13 KB) overflows
> a 2 MB stack in a release build and aborts the process; 1,600 chained
> filters do in debug. The parser's recursion limit covers nesting but not
> chains, which become left-deep trees that codegen recurses down. Table,
> reproduction and a suggested fix above.

### Fix ready (2026-11-25): `../minijinja` branch `fix/chain-depth` (33c387e2, local, not pushed)

`compiler/parser.rs`: the parser tracks the expression tree's depth
(nesting plus chain links, `MAX_EXPR_DEPTH` = 500) and refuses deeper trees
with the existing "template exceeds maximum recursion limits" error;
binary-operator, postfix (`.`, `[]`, call) and filter/test chains count
their links (`chain_link`), unary chains (`not not …`, `- - …`) go through
`with_recursion_guard`. ~40 lines. Tests: `test_regressions.rs`
(10,000-link chains of ten kinds refused on a 2 MiB thread in debug — abort
without the fix; 300-link chains, with 60 levels of nesting, still compile
and evaluate). MiniJinja's suite: 436 tests pass. The Go port is not
affected (goroutine stacks grow; 2,000,000-link chains compile in 0.3 s).

## U59 — MiniJinja: `format` panics on a non-ASCII `%(key)`

- **Status (2026-11-26):** reported to the maintainer by email (2026-11-25/26), who asked for a public issue; issue and PR to open. Fixed on `fix/format-key-utf8` (from `main`), and in the MiniJinja bevy_markup patches in (`bevy_markup/3.1.0`, ADR 0052). Local guard for crates using bevy_markup: `template::checked_format` refuses a non-ASCII `%(key)`.

- **Project / versions:** minijinja 3.1.0 and `main` at a372dfbd (Rust only;
  Go slices bytes and doesn't panic). Reachable through the built-in
  `format` filter (printf style, `builtins` feature, on by default); the
  `str.format` path (`{a[é}`) is only in minijinja-contrib's pycompat.
- **Severity:** high for bevy_markup — any template, 24 bytes, panics the
  render, which runs in a Bevy system: the app's main thread.
- **Discovery:** honggfuzz `html` target, Nightly run 45557989538
  (2026-11-25, the first run with 4 workers), at d54f8ba. One unique
  input (SIGABRT), hit 688 times. Replayed and minimized locally
  2026-11-26 by Claude Opus 5.5.
- **Minimal reproduction:** `{{ "%(é" | format(1) }}` (also `"%(aé"`,
  any non-ASCII character inside an unterminated or terminated key) →
  `end byte index 1 is not a char boundary` at formatting.rs:871
  (`Cursor::advance`).
- **Root cause:** `parse_till` (used for `%(key)` and `[key]`) steps
  through the key with `cursor.advance(1)`, and `advance` slices the
  source `&str` at that byte offset.
- **Fix (upstream):** `../minijinja` branch `fix/format-key-utf8`
  (67132834): advance by the character's UTF-8 length; regression test
  `test_format_key_with_non_ascii_characters` in test_regressions.rs
  (fails without the fix, full suite passes with it). Patch:
  `minijinja-report/0003-fix-step-over-whole-characters-in-format-keys.patch`.
- **Workaround (bevy_markup):** `template.rs` `ascii_keyed_format`
  replaces the `format` filter and refuses a format string whose
  `%(key)` holds a non-ASCII byte before MiniJinja parses it
  (`format_keys_are_ascii`; `%%` is literal). Test
  `template::tests::format_keys_must_be_ascii` (panics with the
  wrapper unregistered). Remove the wrapper once a fixed release is
  pinned.

## U60 — MiniJinja: `format` panics on an integer `%f` precision past 65,535

- **Status (2026-11-26):** reported to the maintainer by email (2026-11-25/26), who asked for a public issue; issue and PR to open. Fixed on `fix/format-integer-precision` (from `main`), and in the MiniJinja bevy_markup patches in (`bevy_markup/3.1.0`, ADR 0052). Local guard for crates using bevy_markup: `template::checked_format` refuses widths and precisions over 65535.

- **Project / versions:** minijinja 3.1.0 and `main` at a372dfbd (Rust).
  Reachable through the built-in `format` filter.
- **Severity:** high for bevy_markup: a 28-byte template panics the
  render, which runs in a Bevy system on the app's main thread.
- **Discovery:** honggfuzz `html` target, Nightly run 45706083357
  (2026-11-25, at 7896c42, which already had the U59 wrapper). One unique
  input (SIGABRT), 218 crashes counted. Replayed and minimized locally
  2026-11-26 by Claude Opus 5.5.
- **Minimal reproduction:** `{{ "%.70000f" | format(1) }}` (an integer;
  `%.65535f` works; floats, `%e`, `%g`, `%s` and widths don't panic) →
  `Formatting argument out of range` at formatting.rs:541.
- **Root cause:** the integer branch of `%f` formats
  `format!("{}.{:0prec$}", val, 0)`; Rust's formatting machinery panics
  for a dynamic precision above `u16::MAX`. MiniJinja already clamps the
  other paths (`MAX_NATIVE_FORMAT_PRECISION`), not this one.
- **Fix (upstream):** `../minijinja` branch `fix/format-integer-precision`:
  write the zeros out (`"0".repeat(prec)`); regression test
  `test_format_integer_with_a_large_precision` (panics without the fix,
  full suite passes with it). Patch:
  `minijinja-report/0004-fix-write-out-the-zeros-of-a-large-integer-f-precisi.patch`.
- **Workaround (bevy_markup):** `template.rs` `checked_format` (formerly
  `ascii_keyed_format`) refuses a width or precision over 65,535
  (`check_format`, `MAX_FORMAT_NUMBER`). Test
  `template::tests::format_specifiers_are_checked` (panics with the limit
  disabled). Remove with U59's wrapper once a fixed release is pinned.

## U62 — MiniJinja: deep walks of containers and constant folding aren't bounded by fuel

- **Status (2026-11-26):** found by the Nightly; the maintainer asked for
  public issues (see U56). Issue and PR to open. Fixed on
  `fix/fuel-for-deep-walks` (from `main`) and in `bevy_markup/3.1.0`
  (ADR 0052).
- **Project/version:** [mitsuhiko/minijinja](https://github.com/mitsuhiko/minijinja)
  3.1.0 and `main` at a372dfbd: `value/ops.rs` (`binop_alloc_estimate`,
  `repeat_iterable`), `compiler/ast.rs` (`eval_binop`, `eval_compare`),
  `vm/mod.rs` (`Emit`, `StringConcat`, `In`, comparisons), `filters::string`.
- **Problem:** `[x] * n` makes a lazily repeated sequence of up to 10⁸
  items for nothing (`binop_alloc_estimate` returns 0 for it: "repeating
  lists is lazy"), and a container holding the same container shares its
  items, so one costs far less to make than to walk. Formatting it (printing,
  `~`, `string`), comparing it or searching it walks every item, and those
  charge fuel only for the top level or only afterwards. Constant folding
  does the same walks at compile time with no fuel at all: the template
  stalls or exhausts memory while it *loads*.
- **Measurements** (release, 3.1.0, default fuel 2²²): compiling
  `{{ [none] * 18446744 ~ 'x' }}` builds a 110 MB string (0.6 s);
  `{{ 1 in ([2] * 100000000) }}` compiles in 3.6 s, `{{ ([1] * 100000000) ==
  ([1] * 100000000) }}` in 6.0 s; with variables instead of literals the
  render runs out of fuel, except `~`, which builds the string first.
  Nested repetitions (`[[n] * 100000] * 100000 ~ 'x'`) walk 10¹⁰ items, at
  load or at render. Printing `[x] * 87408739` allocated 2.2 GB in
  `write_with_html_escaping` (it formats the value into a `String` before
  escaping).
- **Discovery:** cargo-fuzz `html`, Nightly run 45740232198 (2026-11-26),
  41 inputs (37 timeouts at 8–94 s, 4 out-of-memory); see
  [bug_0253](bug_0253.md).
- **Fix (branch):** `ops::deep_item_count(value, limit)` counts the items
  a deep walk visits, stopping past `limit` (lists without a known length
  aren't walked: they print without iterating and may be one-shot).
  Folding declines operations walking (or `*` creating) more than
  `MAX_FOLDED_SIZE / SLOT_COST` items; `State::charge_walk` charges the walk
  against the remaining fuel before `Emit` of a container, `~` with one,
  `in` with a container needle, comparisons (the smaller side) and the
  `string` filter. Tests: `test_deep_walks_consume_fuel_first`,
  `test_constant_folding_declines_large_containers`. Not covered: `tojson`
  and `pprint` take no `State` (charging them changes public signatures).
- **Local guard for crates using bevy_markup** (they get 3.1.0 from
  crates.io): `template::streaming_formatter` refuses printing a value
  holding more items than the output could take (`fits_printed`, without
  iterating a too-long list) and escapes as it writes; `string`, `tojson`,
  `pprint` and `json` check the same, `join` charges before collecting.
  Constant folding and `~`/`in`/`==` at render have no hook: only the
  MiniJinja fix closes them.

## U64 — MiniJinja: `tojson`'s indent is unbounded

- **Status (2026-11-28):** found by the Nightly; public like U56–U62 (the
  maintainer asked for public issues). Issue and PR to open. Fixed on
  `fix/tojson-indent-fuel` (from `main`) and on `bevy_markup/3.1.0`
  (`db625d2e`, pushed 2026-11-28; every lock file builds with it).
- **Project/version:** [mitsuhiko/minijinja](https://github.com/mitsuhiko/minijinja)
  3.1.0 and the `bevy_markup/3.1.0` patch branch: `filters::tojson_impl`,
  `json::JsonWriter::{begin_item, end_container, write_indent}`.
- **Problem:** the indent is any `usize` (`usize::try_from` accepts
  `5E13`), and pretty printing pushes `indent × depth` spaces into a
  `String` before every item and every non-empty container's close.
  Nothing bounds it while it is written: `tojson_fueled` (the patch
  branch) charges the result's length only after building it. In release,
  `indent * depth` can also wrap.
- **Reproduction:** `{{ [1] | tojson(5E13) }}` tries to allocate 50 TB
  (ASan: over 10 s and 2.6 GB before the timeout).
- **Discovery:** cargo-fuzz `html`, Nightly run 45997844575 (2026-11-27),
  one timeout; see [bug_0256](bug_0256.md).
- **Local workaround:** `template::checked_tojson` parses the indent as
  MiniJinja does and refuses a value whose pretty-printed size could pass
  `MAX_RENDERED_BYTES` (`fits_printed`: `3 + 2 × indent × depth` bytes an
  item) before calling it.
- **Fix (branch):** with fuel, `json::to_json_within` stops with
  `OutOfFuel` before indenting past what the remaining fuel could pay for
  at `charge_serialized`'s rate (`State::serialized_limit`), and
  `indent × depth` saturates. Without fuel the indent stays unbounded, like
  the rest of an unfueled render. Test: `test_tojson_indent_is_bounded_by_fuel`.

## U65 — MiniJinja: deep walks ignore string bytes and serialization filters/operators charge fuel only after allocating

- **Status (2026-11-28):** discovered during [review_0010](../reviews/review_0010.md) N02 and corroborated in its verification addendum ([bug_0258](bug_0258.md)). Fixed locally on `../minijinja`'s `bevy_markup/3.1.0` branch (`fbbefc56`, not yet pushed / lockfiles not yet bumped); upstream open.
- **Project/version:** [mitsuhiko/minijinja](https://github.com/mitsuhiko/minijinja)
  3.1.0 and `bevy_markup/3.1.0` before `fbbefc56`: `value/ops.rs` (`deep_item_count`),
  `compiler/ast.rs` (`exceeds_fold_walk`),
  `vm/state.rs` (`charge_serialized`), `vm/mod.rs` (`Emit`, `StringConcat`),
  `filters.rs` (`string`, `tojson`, `pprint`, `join`, `format`, `safe`, `escape`).
- **Problem:**
  1. `ops::deep_item_count` counts container items, not the byte lengths of
     strings held inside containers. A lazily repeated list of strings
     `[s] * N` (e.g. `s = 'x' * 100000`, `N = 3000`) has only 3,000 items
     but serializes to 300+ MB. Constant folding (`(['x' * 50000] * 1000) ~ ''`)
     also folded large string-holding containers at compile time.
  2. `State::charge_serialized` charges fuel for serialized bytes only
     *after* the `String` has already been allocated (`string` filter and
     `~` operator). At `fuel = 10_000` (~640 KB budget),
     `([s'] * 600) | string | length` (`s' = 'x' * 50000`) peaks at 51.3 MB
     before failing fuel, and `(([s] * 300) ~ '') | length` peaks at
     81.3 MB before failing fuel.
  3. Built-in `join` stringifies nested non-string items without charging
     for their serialized size first: `[[s] * 300] | join | length` at
     `fuel = 10_000` allocates 51.3 MB before failing fuel.
  4. `format`, `safe`, and `escape` serialize container arguments/values
     without charging estimated output bytes up front (and repeated large
     width specifiers such as `'%(a)65535d' * 500 | format(a=1)` accumulate
     tens of MBs in one call).
- **Reproduction:** see [bug_0258](bug_0258.md) and the heap-measurement
  table in [review_0010](../reviews/review_0010.md) (e.g.
  `{% set s = 'x' * 100000 %}{{ ([s] * 3000) | tojson | length }}` peaks
  at 709.7 MB before failing fuel; `[[s] * 300] | join | length` at
  `fuel = 10_000` peaks at 51.3 MB).
- **Local workaround (bevy_markup):** `template.rs` (`estimate_printed_bytes`,
  `charge_serialized_value`, `checked_safe`, `fueled_join`, `fueled_escape`,
  `checked_format`, `checked_tojson`, `checked_pprint`, `checked_string`,
  `json_filter`) protects all filter and formatter paths even for downstream
  crates on released MiniJinja 3.1.0; `~` has no local hook.
- **Fix (branch `bevy_markup/3.1.0` `fbbefc56`):** added
  `ops::deep_serialized_bytes` (counting string and byte-slice lengths plus
  container overhead up to a limit), extended `compiler/ast.rs`
  `exceeds_fold_walk` with it, added `State::ensure_serialized` /
  `ensure_serialized_pair` (checking at `SERIALIZED_BYTE_COST` before
  allocating), and wired them into `Instruction::Emit`,
  `Instruction::StringConcat` (`~`), `filters::safe_fueled`, `escape`,
  `join` (nested object items), `string`, `tojson_impl`, and
  `pprint_fueled`. Test: `test_container_of_strings_serialization_fails_before_allocating`
  in `minijinja/tests/test_fuel.rs` (failed at 254 ms before the fix,
  passes under 50 ms after; full `--all-features` and `--no-default-features`
  suites green with `-D warnings`).


