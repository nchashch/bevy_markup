# 48. CSS is parsed on cssparser by bevy_markup; lightningcss leaves gradually

| Field | Content |
|---|---|
| `ADR` | `0048` |
| `Title` | CSS is parsed on cssparser by bevy_markup; lightningcss leaves gradually |
| `Date` | 2026-11-24 21:17 +0400 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code. The direction is the user's decision: keep Servo's tokenizer and the selector engine, and write typed parsing only for the properties bevy_markup supports, done gradually |
| `Commit` | `a76df61 graft -> trail update` |
| `Status` | Proposed |
| `Related` | ADR 0001 (lightningcss chosen), ADR 0034 (selectors), ADR 0039 (pre-scan on cssparser), ADR 0044 (every input is untrusted); bug_0092, bug_0179, bug_0238, bug_0245; UPSTREAM U10, U14, U15, U45, U49 |

## Context

Every stylesheet, `style` attribute and substituted `var()` value is
untrusted (ADR 0044). lightningcss 1.0.0-alpha.72 parses all three:
`StyleSheet::parse` in the loader (`style.rs` `StylesheetLoader::load`),
`StyleAttribute::parse` (`cascade.rs` `inline_style`) and
`Property::parse_string` after substitution (`cascade.rs` around line
2120).

Its defects reached from that input so far:

- **U14 and U15:** nesting recursed without a bound, and nested
  `light-dark(`/`calc(var(…))` parsed in exponential time. Refused before
  parsing by `nesting_over_limit`, a walk over cssparser's tokens (ADR 0039).
- **U45:** an infinite `hsl()`/`hwb()` hue panics in debug builds.
- **U49:** a NaN argument to `min()`, `max()` or `clamp()` panics in every
  build. No scan can refuse every `calc()` route to it (bug_0245).
- **U10:** bare `grid-auto-flow: dense` fails to parse.

`cascade::guarded_parse` turns a panic into a dropped sheet. It can't help
under `panic = "abort"`, and it can't catch a stack overflow. lightningcss
accepts issues only from collaborators, and bevy_markup doesn't vendor
patched dependencies.

What bevy_markup uses from lightningcss, checked against `a76df61`:

| Layer | Use | Where |
|---|---|---|
| Rule structure | `CssRule::Style` only; `@font-face` noted and skipped, other at-rules skipped; `!important`; the `style` attribute | `HtmlStyles::from_sheet`, `inline_style` |
| Selectors | lightningcss's `SelectorList`, matched by `parcel_selectors` | `cascade/selector.rs` (ADR 0034) |
| Typed values | about 120 `Property` variants, including every shorthand bevy_markup maps (margin, padding, border, `border-image`, flex, background, outline, `list-style`, inset, transition, `grid` and the grid family) | `ElementStyle::apply` (702 lines), `apply_keyword`, `apply_resolved`, `track_sizing`, `parse_areas`, `grid_line` |
| Math | `Calc`/`MathFunction` trees, converted into bevy_markup's own `CalcExpr` | `calc_expr`, `length_percentage_val`; evaluation is already bevy_markup's (`CalcExpr::resolve`, `collapse`) |
| Colors | `CssColor` → `RGBA` | `to_color` |
| `var()` | `TokenList`/`TokenOrValue`, printed back to text with `to_css_string`, then parsed again | `substitute_inner`, `var_names`, `image_urls` |
| Public API | `Stylesheet::sheet() -> &StyleSheet<'static>`, `pub use lightningcss` | `style.rs`, `lib.rs` |

bevy_markup doesn't use minifying, transpiling, browser targets, prefixes,
nesting, media or container queries, keyframes, or the several hundred
properties it doesn't map. lightningcss still parses all of them, from
untrusted text, before bevy_markup sees the result.

## Decision

bevy_markup parses CSS itself on `cssparser` (0.37, already a direct
dependency for the pre-scan). It parses only the properties it maps, with
its own bounds on depth and work. lightningcss leaves in steps, each
shippable on its own.

### 1. bevy_markup owns rules and declarations; lightningcss gets single declarations behind a gate

- **Rules.** A `cssparser` `StyleSheetParser` builds bevy_markup's own
  sheet: style rules with their selectors and declarations, in source
  order. At-rules are consumed and skipped; the pre-scan still bounds their
  nesting.
- **Selectors.** Each rule's prelude is parsed with lightningcss's public
  `SelectorList` parser (`ParseWithOptions`), so ADR 0034's matcher and
  allowlist don't change in this step. Selector parsing has no reported
  defect, and it never reaches the value layer.
- **Declarations.** A `RuleBodyParser` records each declaration as name,
  importance and its raw value: the value's source text plus its
  `cssparser` tokens. The `style` attribute goes through the same
  declaration parser.
- **`var()`.** Substitution runs on raw tokens instead of `TokenList`. The
  existing limits stay: `MAX_VAR_DEPTH`, `MAX_SUBSTITUTE_DEPTH`, the
  64 KiB budget and cycle detection.
- **Dispatch.** Each declaration is parsed after substitution. A ported
  property goes to bevy_markup's parser. Any other goes to
  `Property::parse_string` under `guarded_parse`, but only through the
  **gate**. The gate refuses a value holding any CSS math function or
  constant (`calc`, `min`, `max`, `clamp` and the rest of CSS Values 4),
  or a numeric token that isn't finite as `f32`. A refused declaration
  drops like any unsupported value: an earlier rule's value stands.
- **Math and lengths are ported in this step.** Without them, the gate
  would drop `calc()` from the 17 properties that support it today
  (`calc_props`: sizes, `flex-basis`/`flex`, margins, insets, the `calc`
  example). The ported grammar is lengths in every unit, percentages,
  numbers, and `calc()`/`min()`/`max()`/`clamp()` parsed straight into
  `CalcExpr`. Parenthesis depth is already bounded by
  `MAX_CSS_PAREN_NESTING` (8), and every operation is checked for NaN and
  infinity.
- **Public API.** `Stylesheet::sheet()` returns bevy_markup's own sheet type
  or is removed. That is a breaking change, marked in the CHANGELOG.

After this step, no `calc()` and no non-finite number reaches lightningcss's
value layer. U49 can't be reached, and neither can U45 (an infinite hue
needs a non-finite number) [INFERENCE: check U45's inputs against the gate
when implementing].

### 2. Properties are ported group by group

Each group replaces lightningcss for its properties in one change. That
change adds a differential property test that compares bevy_markup's parse
with lightningcss's on generated values: same `ElementStyle` fields,
lightningcss under `guarded_parse` and skipped where it fails. The browser
oracle vectors cover the group, and the group's `cascade::tests` keep
passing unchanged.

1. **Colors:** hex, the named colors, `rgb()`/`hsl()`/`hwb()` in both
   syntaxes, `lab`/`lch`/`oklab`/`oklch`, `color()`, `currentColor`,
   `transparent`.
2. **Box, flex, text and keywords:** padding, borders and radii,
   `border-image`, background, outline, display, position, overflow,
   visibility, `z-index`, opacity, alignment, the flex family, the font and
   text properties, `list-style`, `white-space`, transitions and easing.
3. **Grid:** track lists with `repeat()`, `minmax()`, `fit-content()`,
   named lines, `grid-template-areas`, placements and the `grid`,
   `grid-template` and `grid-area` shorthands. This fixes U10 along the way.

### 3. Selectors, then the dependency

Selectors are parsed with `parcel_selectors`' parser directly. bevy_markup
implements its `SelectorImpl` and `Parser`, because lightningcss's are
private (`cascade/selector.rs` already works around that). Then
lightningcss, the `lightningcss` re-export and `cssparser-color` leave
`Cargo.toml`, and `parcel_selectors` stays.

### Estimates

[INFERENCE: sized from the code each step replaces; new code including its
tests, one session ≈ one agent working day with the suite run per change.]

| Step | Replaces | New code | Sessions |
|---|---|---|---|
| 1. Rules, declarations, raw `var()`, gate, math and lengths | `from_sheet`, `inline_style`, `substitute_inner`, `var_names`, `image_urls`, `calc_expr`, `length_percentage_val`, the loader, the fuzz glue | 1,500–2,100 lines | 3–5 |
| 2.1 Colors | `CssColor` paths | 700–1,000 | 2–3 |
| 2.2 Box, flex, text, keywords | most of `apply`, `apply_keyword`, `apply_resolved` | 1,200–1,800 | 3–5 |
| 2.3 Grid | `track_sizing`, `parse_areas`, `grid_line`, `grid_placement`, `min_breadth` | 900–1,300 | 2–4 |
| 3. Selectors, removal | the `SelectorList` parse, `cascade/selector.rs`'s workaround | 400–700 | 1–2 |
| **Total** | | **4,700–6,900** | **11–19** |

## Alternatives considered

- **Keep lightningcss and keep guarding it.** The guard doesn't work under
  `panic = "abort"` or for a stack overflow. Scans before the parse can't
  cover every math route (bug_0245), and every lightningcss upgrade
  re-opens the question.
- **Take over `calc()` first, leaving the sheet to lightningcss.** Ruled
  out: `StyleSheet::parse` parses every declaration's value before
  bevy_markup sees any of it, so the panic happens inside lightningcss.
  The rule layer has to move first.
- **Stylo (Servo and Firefox's style engine, crate `stylo`).** The most
  heavily used Rust CSS code, but it is a whole style engine built around
  a browser DOM. Its supporting crates are described upstream as
  implementation details, it is MPL-2.0, and it makes frequent breaking
  releases. The input surface would grow, not shrink.
- **swc_css_parser, Biome's CSS parser, raffia.** Syntax trees for build
  tools, with mostly untyped values and no published security or fuzzing
  record. The typed value parsing would still be bevy_markup's.
- **simplecss.** No value parsing, and its own quadratic and exponential
  defects (U40, U42).
- **A patched lightningcss.** bevy_markup bounds dependencies with checks
  of its own rather than vendored patches.

## Consequences

- **Gained:** the CSS that runs on untrusted input is the tokenizer, the
  selector parser and bevy_markup's own value code, bounded like the
  template, FTL and SVG checks (ADR 0044–0047). After step 1, lightningcss's
  math layer can't be reached.
- **Cost:** 4,700–6,900 lines of parser code to own, and bugs of its own.
  The browser oracle, `tests/properties.rs`, the quickcheck cascade models,
  the fuzz targets and the differential tests are what catch them.
- **Breaking:** `Stylesheet::sheet()` in step 1, and the `lightningcss`
  re-export in step 3.
- **Behavior:** a value lightningcss simplified but bevy_markup doesn't
  port, such as `calc()` in `padding` or `border-width` (absolute lengths
  only today), is dropped by the gate until its group lands. Step 1 lists
  each such drop in the CHANGELOG.
- **Open:** whether to fuzz the CSS target in CI again after step 1 is a
  separate decision. ADR 0039's pre-scan stays, since bevy_markup's own
  parsers recurse on cssparser's nested blocks too.
