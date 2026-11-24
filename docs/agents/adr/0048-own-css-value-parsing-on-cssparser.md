# 48. CSS is parsed on cssparser by bevy_markup; lightningcss is removed

| Field | Content |
|---|---|
| `ADR` | `0048` |
| `Title` | CSS is parsed on cssparser by bevy_markup; lightningcss is removed |
| `Date` | 2026-11-24 21:17 +0400; revised 2026-11-24 22:10 +0400 (single pass, implemented) |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code. The direction is the user's decision: keep Servo's tokenizer and the selector engine, and write typed parsing only for the properties bevy_markup supports; then "rip lightningcss out entirely … keeping all the tests, with them failing and then iteratively rebuilding the functionality until tests pass" |
| `Commit` | `22a04b0 Propose ADR 0048: parse CSS on cssparser, phase out lightningcss` + uncommitted: the parser (branch `css-own-parser`) |
| `Status` | Accepted |
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

bevy_markup parses CSS itself, in `src/css/`, on `cssparser` 0.37 (already a
direct dependency for the pre-scan) and parcel_selectors' selector parser.
lightningcss and its re-export are removed in one change. The proposed
gradual plan (below, under Alternatives) was dropped at the user's request:
the existing tests are the specification, and the parser was rebuilt until
they pass.

### 1. Sheets and declarations (`css/sheet.rs`)

`cssparser`'s `StyleSheetParser` and `RuleBodyParser` build a `Sheet` of
style rules in source order. A syntax error fails the sheet or `style`
attribute, as lightningcss did without error recovery. At-rules and nested
rules are accepted and skipped without parsing their contents. Each
declaration becomes one of:

- `Value(Decl)`: parsed once, at load.
- `Keyword(name)`: `initial`/`inherit`/`unset`/`revert`.
- `Var { name, value: Raw }`: a value holding `var()`, kept as source text
  and parsed per element after substitution. Substitution walks
  cssparser's tokens and wraps each substituted value in empty comments,
  so it never merges with a neighbouring token. The existing bounds stay:
  `MAX_VAR_DEPTH`, `MAX_SUBSTITUTE_DEPTH`, the 64 KiB budget, cycle
  detection.
- `Custom { name, value: Raw }`: a `--name` declaration.

### 2. Selectors (`css/select.rs`)

parcel_selectors parses into bevy_markup's own `SelectorImpl` (owned
`String` names; its `into_owned` feature, with the `static-self` crate,
lifts a parsed list to `'static`). Unknown pseudo-classes and
pseudo-elements parse and are skipped by ADR 0034's allowlist, so they skip
their rule instead of failing the sheet. The matcher in
`cascade/selector.rs` is unchanged apart from its types.

### 3. Properties and values (`css/property.rs`, `value.rs`, `color.rs`, `grid.rs`)

One parser per property the cascade maps produces a `Decl` that
`cascade::apply` writes into an `ElementStyle`. Every other property is
dropped without parsing. A value inside the grammar that bevy_markup can't
map carries `None` and keeps the earlier value, as before.

- **Math.** `calc()`, `min()`, `max()` and `clamp()` parse into a tree and
  evaluate with CSS's types: numbers fold, a length-percentage becomes
  `CalcExpr` with same-unit terms combined. Depth is bounded
  (`MAX_CALC_DEPTH` 32, under the pre-scan's 8 parentheses). Every literal
  and result must be finite, so no value carries NaN or infinity. The other
  math functions parse as unsupported.
- **Lengths.** A unitless number is pixels, as lightningcss read it:
  `TemplateContext` numbers reach `var()` unitless (`--px: 24`), and the
  tests pin that.
- **Colors.** CSS Color 4's formulas, quantized to 8-bit sRGB like
  lightningcss's `RGBA`.
- **Grid.** lightningcss's grammar, with one fix: a bare
  `grid-auto-flow: dense` is `row dense` (U10).

## Alternatives considered

- **The gradual plan this record first proposed.** Own the rule layer first,
  gate lightningcss's value parser away from math and non-finite numbers,
  then port property groups and selectors. Dropped by the user in favour of
  one pass: the intermediate states needed throwaway gating code, and the
  test suite could drive a full rewrite directly.
- **Keep lightningcss and keep guarding it.** The guard doesn't work under
  `panic = "abort"` or for a stack overflow. Scans before the parse can't
  cover every math route (bug_0245), and every lightningcss upgrade
  re-opens the question.
- **Take over `calc()` first, leaving the sheet to lightningcss.** Ruled
  out: `StyleSheet::parse` parses every declaration's value before
  bevy_markup sees any of it.
- **Stylo (Servo and Firefox's style engine, crate `stylo`).** The most
  heavily used Rust CSS code, but a whole style engine built around a
  browser DOM, MPL-2.0, with frequent breaking releases. The input surface
  would grow, not shrink.
- **swc_css_parser, Biome's CSS parser, raffia.** Syntax trees for build
  tools, with mostly untyped values and no published security or fuzzing
  record. The typed value parsing would still be bevy_markup's.
- **simplecss.** No value parsing, and its own quadratic and exponential
  defects (U40, U42).
- **A patched lightningcss.** bevy_markup bounds dependencies with checks
  of its own rather than vendored patches.

## Consequences

- **Gained:** untrusted CSS reaches only the tokenizer, parcel_selectors'
  selector parser and bevy_markup's own value code. lightningcss's panics
  (U45, U49) can't be reached; a non-finite value drops its declaration in
  every build, where it used to fail the whole sheet or abort a
  `panic = "abort"` app. `cascade::guarded_parse` is gone.
- **Measured:** all 657 tests pass with `--features svg-text,fuzzing`, as
  do the plain and `text-input` suites. The golden images are identical.
  The pre-existing tests changed only where they pinned lightningcss
  itself: the two crash-path tests now expect only the declaration to
  drop, and the fuzz harness no longer excuses lightningcss panics. The
  parser is 4,378 lines (`src/css/`); the cascade lost 1,700 lines of
  lightningcss mapping.
- **Breaking:** the `lightningcss` re-export and `Stylesheet::sheet()` are
  gone (`Stylesheet::check`, hidden, validates a sheet for tooling).
- **Behavior:** math functions other than `calc()`/`min()`/`max()`/
  `clamp()` aren't evaluated (lightningcss folded constant ones); errors
  inside an at-rule no longer fail a sheet; `color()` knows `srgb`,
  `srgb-linear`, `display-p3` and XYZ only; `color-mix()` has no fixed
  color; negative widths, heights and paddings are invalid, as in CSS.
- **Open:** whether to fuzz the CSS target in CI again. ADR 0039's
  pre-scan stays, since bevy_markup's parsers recurse on cssparser's
  nested blocks too.
