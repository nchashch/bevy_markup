# 15. Font fallback lists via Bevy 0.20's `FontSource`

| Field | Content |
|---|---|
| ADR | 0015 |
| Title | Font fallback lists via Bevy 0.20's `FontSource` |
| Date | 2026-10-10 13:52 +0400 |
| Author | GLM-5.3-Flash (Z.ai), via omp, at the project owner's request |
| Commit | `1bbab7d` Add support for em and rem + uncommitted schedule shuffling (tests) |
| Status | Accepted |
| Related | 0001 (font model), 0002 (dependency discipline); the Fonts limits in AGENTS.md and README |

## Context

A CSS `font-family: A, B, serif` list currently resolves to the **first
registered family** (`src/fonts.rs` `FontFamilies::resolve`, `list.iter()`
`find_map`): the text gets that one family's face for the requested
weight/style, and a glyph the face lacks renders as tofu. The crate's
documented limits carry the consequence: "no bundled CJK font; Japanese
relies on `system_fonts`" — a document mixing Latin and Japanese needs the
author to register one font covering both, or lose glyphs.

Bevy 0.20 turned `FontSource` into a fallback-capable structure:
`FontSource::families("Arial, 'Noto Sans', sans-serif")` (a CSS-style list)
and `FontSource::list([...])` (an ordered combination of handles, family
names, generics — `flatten()` flattens nested lists). Selection is per
*glyph* by parley/fontique: the first source covering the character wins,
falling through the list automatically. With the `system_fonts` feature
(bevy's `system_font_discovery`, which bevy_markup's `system_fonts` feature
enables) the OS's installed fonts join the chain.

## Decision

1. **Compose the whole CSS list into one `FontSource::list`.**
   `FontFamilies` gains a resolver that walks the CSS `font-family` list in
   order and, for each *registered* family (generics through their mapped
   names), takes that family's face for the requested weight/style — the
   crate's existing per-family face selection (`FontFaces::face`, CSS font
   matching: bold-italic → italic → bold → regular). The resulting sources
   combine into `FontSource::list([...])`; a single registered family keeps
   today's exact output. Unregistered names are skipped (as `resolve` does
   today); an empty list falls back to Bevy's default font (unchanged).
2. **`Style.family` carries the resolved family indexes** (the first-match
   `Option<usize>` becomes a bounded list — a `[Option<usize>; 4]` copy
   array or `Vec` with `Style` dropping `Copy` for `Clone`; decided at
   implementation). Per-glyph fallback needs the whole chain at
   `text_font` time, not just the first hit.
3. **Weight/style stays per family.** The crate requests bold/italic
   per-face exactly as today (`TextFont` weight/style for system sources;
   file faces carry their own) — fontique picks the first list entry that
   covers the glyph, which is CSS's fallback semantics.
4. **Per-glyph OS fallback stays feature-gated.** Without the
   `system_fonts` feature only registered file fonts participate (still an
   improvement: two registered fonts with disjoint coverage now chain);
   with it, the OS's fonts complete the chain (CJK works without bundling).

## Alternatives considered

- **Hand the CSS list string to `FontSource::families` directly** and drop
  the crate's registry for lists: loses the registry's file-font families
  (handles), the generics mapping, and the case-insensitive matching —
  the two layers compose instead of compete.
- **Per-glyph fallback inside the crate** (measure, detect tofu, re-shape
  with another face): re-implements fontique badly; rejected.
- **No change** (register one comprehensive font): the status quo the
  limits describe; rejected as the trigger was real authoring pain.

## Consequences

- **A CJK document works without bundling a CJK font** when
  `system_fonts` is on: `font-family: "Iosevka Slab Mono", sans-serif`
  renders Latin from the registered face and CJK from the OS's.
- **The dump/labels change** for multi-family lists: `face_label`
  (tests/common) maps `FontSource::List` element-wise; existing
  single-family vectors are unaffected.
- **`Style` may drop `Copy`** if the resolved list outgrows a copyable
  array — run-merging only needs `PartialEq`, which `Vec` provides.
- **The system-font weight/style nuance stays**: a system face in the list
  keeps `TextFont` weight/style requests (fontique resolves the real
  face); the first *registered file face* wins for its glyphs regardless
  of weight (as today).
- **Tests at two levels**: unit tests for the list composition
  (`face_list` over registered/unregistered/generic mixes); end-to-end
  per-glyph fallback needs real fonts with disjoint coverage — the
  examples (the demo's Japanese locale) are the manual check, plus a
  `system_fonts`-gated example screenshot. The golden scenes (embedded
  default font only) are unaffected.
