# 27. Add the missing text properties, list numbering, scroll-if-needed and full entity decoding

| Field | Content |
|---|---|
| ADR | 0027 |
| Title | Add the missing text properties, list numbering, scroll-if-needed and full entity decoding |
| Date | 2026-10-15 06:01 +0400 |
| Author | GLM 5.3 Flash (Z.ai), via omp |
| Commit | `e2081e8 Release 0.7.0` (the decisions below are uncommitted) |
| Status | Accepted (fully implemented; status corrected 2026-10-16 — all six items shipped: `TextAlignSpec`/`JustifyText`, `LineHeightSpec`, `ListStyleSpec` + ol numbering, the `promote_auto_scroll` system, `src/entity_data.rs` + `scripts/generate_entities.py`, `LetterSpacingSpec`; oracle vectors `text_props`/`layout_text_props`) |
| Related | AGENTS.md "Limits" (text-align/line-height/letter-spacing, lists, inline, `overflow: auto`); ADR 0011 (link spans carry no box — inline borders); ADR 0020 (Markdown conversion); `scripts/browser_oracle.py` (`BEVY_MARKUP_CSS`, the model CSS the oracles assume); bug_0087 (include-path check) is unrelated |

## Context

Six silent gaps in the CSS/HTML subset are documented in AGENTS.md's
Limits, three of them *silently* — `text-align`, `line-height` and
`letter-spacing` have no arm at all, so a declaration is ignored without a
debug log. They are the most common authoring stumbling blocks after
layout (which 0.6 closed): centered text and numbered lists are the first
things a new template reaches for. Each gap has a concrete cost today:

- **`text-align`** — blocks build a `Text` node with `TextLayout::default()`
  (justify `Left`, `build.rs` ~2443); nothing maps the property. Centered
  text is impossible without app-side `Node` writes, which updates overwrite
  on elements (AGENTS.md Gotchas: only the root keeps app values).
- **`line-height`** — every text node gets Bevy's required `LineHeight`
  component at its default, `RelativeToFont(1.2)` (`bevy_text-0.20.0`
  `text.rs:1366-1369`). The layout oracles agree with Chromium *because*
  the default happens to match the browser default; any author who writes
  `line-height` gets silence, and the match is accidental, not mapped.
- **Lists** — `ul`/`ol` are plain columns; every `li` draws a hard-coded
  `"• "` prefix with a hard-coded 12 px indent (`build.rs:2147`,
  `build.rs` margin default). `ol` numbering is impossible; `list-style`
  doesn't exist.
- **`overflow: auto`** — an `Overflow` value maps for `visible`/`hidden`/
  `scroll`/`clip`; `auto` is skipped, and the docs tell authors `auto`
  *scrolls always* (Bevy has no scroll-if-needed).
- **Entities** — `decode_entities` (`template.rs:110-119`) handles the five
  escapes Tera emits (`&quot; &#39; &lt; &gt; &amp;`), one level, and
  leaves every numeric reference (`&#8212;`, `&#x2014;`) and named one
  (`&mdash;`, `&nbsp;`) verbatim. Fluent values must hand-write the
  character; Markdown text with a numeric reference shows the reference.
- **`letter-spacing`** — the sixth silent gap (also no arm at all): a
  declaration is ignored without a log. Bevy already spaces at
  `LetterSpacing`'s default (0 px), so the oracle agreement is again
  accidental, not mapped.

Constraint carried from ADR 0020 / the crate's dependency policy: no new
crates for this (a full HTML5 entity table is data, not logic).

Bevy 0.20 capability check (registry source, `bevy_text-0.20.0/src/text.rs`):
`TextLayout` has `justify: JustifyText`; `LineHeight` is a required
component with `Px(f32)` / `RelativeToFont(f32)`; `LetterSpacing` is a
required component (px). `bevy_ui`'s `Overflow` is per-axis
(`Visible`/`Hidden`/`Scroll`); `InlineNode` reserves space in text flow but
placing content in it is unproven (the documented `inline-block` limit).

## Decision

Add the six, with these mappings — **inline borders stay unsupported**
(Bevy blocks it; see Alternatives). One release, one
implementation wave, each item with its own vector.

1. **`text-align`** (`cascade.rs` → `build.rs`): inherited property on the
   text declarations; `left`/`right`/`center`/`justify` →
   `JustifyText::{Left, Right, Center, Justified}` on the block's
   `TextLayout` (and the root/pre paths that build `TextLayout`), applied
   through the same `put`/compare-fields path as `no_wrap`
   (`build.rs:2913-2915` compares `justify` already). Other values drop the
   declaration only. Blocks that aren't text (boxed blocks' wrappers)
   don't take it; the text node inside does.
2. **`line-height`** (`cascade.rs` → `build.rs`): inherited; `normal` →
   *don't write* (Bevy's default `RelativeToFont(1.2)` is CSS's
   initial value); a unitless number or `%` → `LineHeight::RelativeToFont`;
   `px` → `LineHeight::Px`. Applied as the `LineHeight` component next to
   `TextFont`/`TextColor` in `TextSpec`. This makes the oracle agreement
   *by mapping*, not by accident: extend `BEVY_MARKUP_CSS` with
   `line-height: 1.2` on blocks so the model stays in sync.
3. **Lists** (`build.rs`, `cascade.rs`): `list-style-type`
   (`none` | `disc` | `circle` | `square` | `decimal` → the marker glyphs
   `•`/`◦`/`▪`/arabic numerals; other values drop the declaration),
   inherited from the `ul`/`ol` to the `li` like CSS marker inheritance.
   The item walk already knows the ancestor `ul`/`ol` chain: number each
   `li` from its list's counter (nested lists restart; a `value` attribute
   is out of scope). The hard-coded 12 px indent becomes the `li`'s
   initial `padding-left` (12 px), overridable by CSS — a layout-default
   change, so re-derive the affected vectors and mirror the indent in
   `BEVY_MARKUP_CSS`. Numbered markers render in the `li`'s own style (the
   list's, inherited), same as the bullet today.
4. **`overflow: auto`** (`build.rs` + one system): per-axis. `auto` maps to
   `OverflowAxis::Hidden` (clip) at build time, and a system running where
   the other layout-time systems run (after `Build`, before
   `UiSystems::Prepare`, reading the same previous-frame layout the calc
   resolver does) promotes the axis to `Scroll` when the content's
   measured size exceeds the box and demotes it when it doesn't.
   [INFERENCE] on the mechanics: promote/demote from the scroll range the
   same way `size_images` reads used sizes; a build that sets `auto`
   resets the promotion so a rebuilt element re-measures (no stale
   `Scroll` from a previous rule).
5. **Entities** (`template.rs`): numeric references (decimal and hex,
   invalid → U+FFFD per HTML) plus the full WHATWG named-reference table,
   generated once into `src/entity_data.rs` from the spec's `entities.json`
   (data, not a dependency; the generator script lives in `scripts/`).
   `decode_entities` keeps its `&`-fast path and one-decode semantics (a
   decoded `&amp;lt;` stays `&amp;lt;`), applied at the same call sites —
   including the Markdown path's HTML containers and Fluent values, so a
   translation can write `&nbsp;` where today it cannot.

6. **`letter-spacing`** (`cascade.rs` → `build.rs`): inherited;
   `normal` → *don't write* (Bevy's `LetterSpacing` default, 0 px, is
   CSS's initial); `px` → the raw value; `em`/`rem` resolved at cascade
   time against the element's computed font-size (`LetterSpacing` is a
   raw f32 in px and has no font-relative arm), `rem` against the root
   font-size the cascade already resolves. Applied as the `LetterSpacing`
   component next to `LineHeight` in `TextSpec` — one compare-fields arm
   like the others. Other values drop the declaration only.

**Not decided here:** nothing — the six silent gaps above are all mapped.

## Alternatives considered

- **Inline borders on inline elements** (part of the original ask): Bevy
  0.20 `TextSpan` has no box — a span can't draw a border, the same
  constraint that keeps `background-color` a run paint (AGENTS.md Limits:
  inline). The escape hatch would be `InlineNode` boxes in text flow, but
  placing *content* in an `InlineNode` is the documented-unsupported
  `inline-block` path (Bevy reserves the space, doesn't place content),
  and a bordered run would need its text moved into that box — an
  unproven interaction with span ordering and picking (link spans).
  Rejected for now; revisit when Bevy grows real inline boxes. ADR 0011's
  contract (links are spans, not boxes) stays.
- **`overflow: auto` = always `Scroll`** (a lie in the other direction —
  every `auto` panel gets a scrollbar): rejected; the docs' whole complaint
  is "auto scrolls always".
- **A `text-align`-only subset without `justify`**: rejected; `Justified`
  is the same mapping arm, no extra risk.
- **A small hand-picked entity table (~50 common names)**: rejected; the
  generated full table costs nothing at runtime (a match on the name
  slice) and avoids the "why doesn't `&thinsp;` work" class. The generator
  makes the table's size a build-time concern, not a maintenance one.
- **Adopting an entity-decoding crate**: rejected against the crate's
  dependency policy (HTML parsing in, UI out; `html5ever`-class deps for
  one table are the kind of unrelated crate AGENTS.md forbids).

## Consequences

- **Author-visible:** centered/justified text, explicit line heights,
  numbered lists and `list-style: none` become possible in templates and
  Fluent markup; numeric and named entities render as characters
  everywhere text is decoded. `overflow: auto` stops scrolling-empty
  panels. Templates that relied on `ol` *not* numbering (the documented
  deviation) change appearance — a breaking, documented-by-ADR change for
  0.8.0.
- **The `li` indent change** re-derives the layout vectors and the oracle
  model CSS in the same change (the oracles must keep agreeing to the
  pixel).
- **Tests:** a vector each for `text-align` (all four values on a wrapped
  line), `line-height` (`normal`/unitless/%/px, including the
  whole-pixel-fraction caveat from Bevy's text measure), `list-style-type`
  (numbered `ol` across a restart at a nested list, `none`, marker
  inherits the list's style), `overflow: auto` (promote on overflow,
  demote without, reset on rebuild), entities (numeric, hex, named,
  invalid, one-level decode), `letter-spacing` (`normal`/px/`em` against
  the element's font-size, inheritance down a container); cascade unit
  tests per mapping arm;
  `content_lint` keeps passing (templates may now use entities).
- **Cost:** one extra system (the auto-scroll promotion) in `PostUpdate`;
  a ~100 KB generated data module. No new dependencies. The pipeline's
  shape is unchanged.

## Also since 0026

- 0.7.0 (ADR-less, all fix-session work with their own reports):
  the H02 pre-scan (bug_0106), the PUA arg stand-ins (bug_0107), the
  pulldown-derived code ranges (bug_0108), root-level `calc()` (bug_0110)
  and viewport-unit fixes (bug_0111), the `var()` cycle rule (bug_0117),
  the JSON-escape rewrite (bug_0128), the testbench example.
