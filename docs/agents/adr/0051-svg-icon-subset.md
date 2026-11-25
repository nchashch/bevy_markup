# 51. SVG is drawn in an icon subset; filters, masks, patterns, markers, text and style sheets are refused

| Field | Content |
|---|---|
| `ADR` | `0051` |
| `Title` | SVG is drawn in an icon subset; filters, masks, patterns, markers, text and style sheets are refused |
| `Date` | 2026-11-26 01:49 +0400 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code |
| `Commit` | `7896c42 Fix the first night of the new fuzz setup; ASCII-only format keys` + uncommitted: the subset (branch `svg-allowlist`) |
| `Status` | Accepted (2026-11-26) |
| `Related` | ADR 0038, 0040, 0042 (superseded), 0044; bug_0188–bug_0248 (the SVG reports) |

## Context

ADR 0038 drew `.svg` files and inline `<svg>` with resvg, and ADR 0044
made every input untrusted. resvg renders nearly all of SVG 1.1 and parts
of SVG 2; each feature it draws was a feature an untrusted document could
aim at. What that cost (2026-11-25, `main` at `d54f8ba`):

- **Bugs.** 43 reports name SVG, 21 of high severity. Of the 24 caused by
  the engine or its guards, 18 (75%) are in features icons don't use:
  filters (6: turbulence, convolve, morphology, blur and region overflows),
  `<text>` (4: quadratic layout, glyph cost), patterns (3), `<style>`
  sheets (3: quadratic parsing, exponential descendant matching), markers
  and `<use>` fan-out (2).
- **Upstream entries.** About 28 in `UPSTREAM.md` against usvg (12), resvg
  (11), tiny-skia and tiny-skia-path (6), simplecss, svgtypes, roxmltree
  and kurbo; most are reached through those features.
- **Guards.** About 1,700 of the 3,017 logic lines of the SVG modules exist
  to bound usvg and resvg before they run: `svg_bounds` (931: the reference
  graph, `check_values` for filter primitives, the `<style>` parse and
  match charges, text work, region factors), `svg_boxes` (153: a first usvg
  pass measuring object bounding boxes for filter, mask and pattern
  regions), and the filter, mask, pattern and text parts of the cost and
  extent models in `svg.rs`. Plus `svg_fonts` (297) and the `svg-text`
  feature, a second font stack (fontdb, harfrust, skrifa: 10 crates) beside
  Bevy's.
- **Use.** The repository's own SVG content (11 files and the examples'
  inline icons) uses shapes, paths, groups, `defs`/`use`, gradients and one
  `<text>` badge.

## Evidence

Seven public icon and emoji sets, fetched 2026-11-26 (now kept in
`../icons`, with their licenses): Lucide (2,123), Tabler (6,563), Material
Design Icons (7,447), game-icons.net (4,239), Font Awesome Free (2,883),
Twemoji (4,009), OpenMoji (4,569): 31,833 documents.

- **Coverage.** Every one uses only `svg`, `path`, `g`, `circle`, `line`,
  `rect`, `polyline`, `polygon`, `ellipse`, `defs` and `clipPath` (4 files
  use the last two). None uses a `filter` property, `style` declarations,
  `mask`, or `<style>`; one has a dangling `marker-end` (no `<marker>`).
- **Rendering.** Each document through the loader at production limits,
  drawn at 4× (a throwaway probe over `fuzz`, not committed): `main` and
  the branch give the same size and pixel hash for all 31,833, with no
  error on either side, before and after the guards were removed; 55.3 s
  and 55.5 s for the whole corpus.
- **Size.** SVG logic 3,017 → 2,011 lines (−33%), its tests 2,036 →
  1,227; `svg_boxes` and `svg_fonts` gone; `svg_bounds` 932 → 595. The
  `svg` feature keeps its 329 crates; the text stack's 10 are gone (what
  `svg-text` and `fuzzing` pulled in).
- **Fuzzing.** cargo-fuzz on the `svg` target, 20 minutes with two
  workers from the local corpus and the committed seeds (5,686 inputs,
  the old filter, pattern and text bombs among them): 2.6 million runs,
  coverage 14,751 → 14,930, no crash, timeout or out-of-memory, with the
  turbulence excuse removed (any panic would abort). The eight crashing
  inputs saved locally on 2026-11-08 and 09 all run cleanly.

## Decision

1. **The subset.** `svg::ALLOWED_ELEMENTS`: `svg`, `g`, `defs`, `symbol`,
   `use`, `a`, `path`, `rect`, `circle`, `ellipse`, `line`, `polyline`,
   `polygon`, `linearGradient`, `radialGradient`, `stop`, `clipPath`, and
   the ignored `title`, `desc`, `metadata`. Presentation attributes and
   `style` declarations stay (fill, stroke, opacity, transforms, dashes,
   `currentColor`, blend modes).
2. **Refused before usvg runs** (`check_subset`, right after the XML
   parse): any other element in the SVG namespace (or none) — filters and
   their primitives, `mask`, `pattern`, `marker`, `text`/`tspan`/
   `textPath`, `style`, `image`, `foreignObject`, `switch`, `script` — and
   a `filter` property other than `none`, as an attribute or in `style`
   (usvg turns CSS filter functions into filters with no `<filter>`
   element, and their region math overflowed during conversion, U31).
   Other namespaces' elements (an editor's) pass: usvg ignores them. The
   error is `SvgError::Unsupported`, naming what was refused.
3. **Refused after conversion** (`check_tree`): a tree holding a filter,
   mask, pattern, text or image node, should one get past the document
   check. The cost and extent models no longer price them.
4. **Removed:** `svg_boxes` (ADR 0042's first pass), `check_values`, the
   `<style>` charges, text work and region factors in `svg_bounds`, the
   filter, mask, pattern and text paths of the cost and extent walks, the
   `TooBlurred`, `RegionOverflow` and `OutOfRange` errors, the
   `svg-text` feature and `svg_fonts`, and the fuzz harness's excuse for
   resvg's turbulence overflow (U20): every panic in the `svg` target is a
   finding now.
5. **Kept:** the reference graph (cycles through `use`, clip paths and
   inherited gradients; fan-out; depth), the path, arc and segment work,
   the dash and stroke costs, the isolated-layer and clip costs, the
   far-off-canvas extent check (tiny-skia, U24/U27/U28), the side and
   raster-memory budgets, the refusing image resolver, and `guarded`.

## Alternatives considered

- **Keep everything; document SVG as trusted only.** No work, but against
  ADR 0044, and every guard stays to maintain against each resvg release.
- **Draw the subset with our own renderer on tiny-skia.** Would drop usvg
  and resvg (~18,000 lines) but means writing the parser, style handling,
  gradients and `use`, and losing resvg's conformance (its ~1,600 rendering
  tests); tiny-skia's panics would still need the extent check. More work
  and more risk for less than this gains.
- **Refuse less** (keep masks or patterns). Each brings back its guards
  (regions, `svg_boxes`, tile costs) for content no surveyed set uses.

## Consequences

- **Breaking.** An SVG using a refused feature fails to load (an `<img>`
  or CSS image shows nothing; an inline `<svg>` is skipped) with an error
  naming the feature. The `svg-text` cargo feature is gone: a project
  enabling it must switch to `svg`, and SVG text is refused; draw text
  with HTML over the image, or convert it to paths.
- **Less to defend.** 75% of the SVG bug classes can't be reached, about a
  thousand lines of guards and their tests are gone, and so is a second
  font stack.
- **Fidelity is unchanged inside the subset**: resvg still draws it, and
  the 31,833 surveyed documents render pixel for pixel as before.
- **Watch** what usvg accepts that the element list doesn't name (a new
  element in a resvg release is refused until added here), and keep
  `check_tree` in step with usvg's node types.
