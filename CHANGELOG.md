# Changelog

All notable changes to bevy_markup are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0, a minor version bump (0.2 → 0.3) may contain breaking changes.

## [Unreleased]

### Added
- CSS `transition` (the shorthand): animates a restyled property's change
  — `color` (the element's text color; runs that shared its old value fade
  with it), `background-color` and `border-color`, over `linear`, the
  named curves, `cubic-bezier()` or `steps()`, with optional delay. A
  spawn never animates; the clock is `Time`'s virtual one.
- CSS `var()`: `var(--name)` resolves the UI's `TemplateContext` first
  (insert a value under `--name`; strings interpolate as written, numbers
  and bools as their CSS spelling), then the stylesheet's own `--name:`
  declarations (any top-level rule, last wins — sheet-level, not
  per-element inherited), then the reference's fallback. An unresolvable
  reference without a fallback drops the declaration.
- The `hot-reload` example (`--features hot-reload`, which enables Bevy's
  file watcher): editing a template or stylesheet restyles the UI without
  a restart; the example also rewrites its own stylesheet on `H` / B to
  demonstrate it without an editor.
- `visibility: hidden`: the element and its descendants become invisible
  (Bevy's propagating `Visibility`) but keep their layout space — a
  subtree-hide approximation of CSS (a `visibility: visible` child cannot
  re-appear under a hidden ancestor). `collapse` is skipped; the root
  rule's visibility is left to the app (`HtmlWorldAnchor` owns it).
- `calc()` in layout properties (sizes, `flex-basis`, margins, insets:
  `width: calc(50% - 40px)`, `top: min(2em, 10%)`, …). Expressions that
  resolve to a single unit are computed at style time; mixed-unit ones are
  resolved per frame — `%` against the containing block's content box per
  axis, `em`/`rem` against the font sizes, viewport units against the UI
  root's size. Supported math functions: `calc()`, `min()`, `max()`,
  `clamp()`; `round()`/`mod()`/`abs()` and the rest are skipped.

### Changed
- **Changed-content frames reuse unchanged subtrees** (ADR 0021): a
  rebuild pairs the fresh template output against the previous build and
  hands every element whose tag, attributes and text are unchanged to the
  update pass by reference and skips it entirely — no re-styling, no
  text-run construction, no copying, no diff visit. Measured on
  600-element documents: −46% per frame with one changing paragraph, −33%
  on a mostly-static Markdown page; documents whose every element changes
  (a fully localized page, a full re-render) are within a percent or two
  of before. Style-side changes
  (`:hover`/`:active`, `var()` values, fonts, image loads) still restyle
  the whole document, as before.
- **Breaking:** a `.md` template's Tera interpolations are no longer
  parsed as Markdown. Templates convert to HTML once, at load (Tera
  syntax passes through verbatim), so the per-frame path is Tera-only —
  a value interpolated through `{{ … }}` renders as literal text (its
  `*`/`_`/`&lt;` no longer become emphasis/tags), while the template's
  own Markdown is unaffected. `markdown: true` Fluent values' arguments
  are literal too: the message's Markdown converts once per bundle load
  (with placeholders) and the formatted arguments substitute into the
  cached HTML, escaped — messages with select expressions or two
  adjacent placeables convert per frame, as before.
- **Breaking:** `<img>` sizes come from the CSS `width`/`height`
  properties instead of the `width`/`height` attributes (the attributes
  are removed). `width: 100%` fills the containing block's content box —
  this is what keeps a large image inside a fixed-width panel (the
  `inline` example's notes cat); one dimension unset keeps the aspect
  ratio; undeclared, the image keeps its intrinsic size. Sizes resolve at
  layout time (percentages against the containing block's used content
  box), so templated values (like every CSS property) update in place on
  re-render.

### Fixed
- **A Tera statement sharing a line with Markdown content broke the
  document's structure.** The load-time conversion parsed `{% for %}##
  Heading` (e.g. from Rust `\`-continued template strings) as one
  paragraph, wrapping the loop in it — headings vanished, paragraphs
  stayed unclosed, every loop iteration repeated the opening tag. Tera
  statements (`{% … %}`, `{# … #}`) are now placed on their own lines
  before the Markdown conversion, matching the shape the old
  expand-then-convert pipeline produced.
- **Markdown documents printed their entire converted HTML to stderr on
  every conversion** — a debug print left in `md::to_html` formatted the
  whole output per frame (a 600-paragraph document paid ~30% of its
  per-frame cost on it). Removed.
- **Scrolling panels stopped working: the root grew to its content.** Taffy
  (in Bevy 0.20) applies the automatic minimum size to root nodes, so a
  root with undeclared `min-height` grew to its tallest content instead of
  the window — nothing overflowed its scrolling panel any more. The root
  now follows CSS: `min-width`/`min-height: auto` compute to zero for it
  (unless the root rule declares a minimum), restoring the Bevy 0.19
  behavior the scroll recipe relies on.
- **`repair_focus` cleared focus the app set on an activation target.**
  An entity with signals (a link span — no `Focusable`, no `Node`) was
  treated as stale every frame, so app-driven keyboard focus on links (as
  in the `links` example) died instantly. Repair now keeps the focus.
- **Interaction styles on translated markup.** `:hover`/`:active`/`:focus`
  never matched elements whose markup comes from a Fluent value (the
  `links` example's inline links didn't highlight on hover, while the
  template's links did). Translated fragments now style against their own
  element states, and `a:hover { background-color }` /
  `text-decoration` apply to link spans too.

### Added
- **Named grid lines and areas.** `[name]` line names in
  `grid-template-columns`/`-rows` and `grid-template-areas` (which also
  implies the explicit tracks it spans, sized `auto`) resolve at
  collection: children place by name — `grid-column: main / main-end`,
  `grid-area: sidebar`, with occurrence indexes (`2 main-start`). Names
  the template doesn't have — or ones in `auto-fill`/`auto-fit` repeats —
  place `auto`, logged. (Browsers extend the implicit grid with the
  repeated pattern instead; taffy auto-places, so bevy_markup does too.)
- **CSS combinators.** Selectors join compound selectors with descendant
  and child combinators: `.panel p`, `.panel > p`, `.a .b > .c` — matched
  against the ancestor chain, with specificity summed over the compounds
  and ancestor pseudo-classes working (`.panel:hover p`). Sibling
  combinators (`+`, `~`), attribute selectors and other pseudo-classes
  still skip the rule. This also activates CSS the crate silently skipped
  before — the `inline` example's `.quest.done span` (strike-through a
  done quest's own span) now applies.
- **Text input.** `<input type="text" value maxlength>` renders an editable
  text field (Bevy 0.20's `EditableText` + `TextInput` behind the new
  `text-input` cargo feature): typed text, cursor and selection from the
  widget, focusing through the crate's own focus system (press-to-focus,
  `HtmlFocus` navigation, `HtmlModal`), the text styled by the cascade and
  the box by CSS. The app reads the value from the entity's `EditableText`.
  Without the feature `<input>` stays an unknown tag.
- **Links.** `<a href>` spans become picked entities: clicking emits the
  `open` signal with the href payload (the app decides what opening a link
  means — Bevy has no URL opener), the deepest-bound-element rule applies,
  `HtmlElements` finds links, and `:hover` styles work on them. A link
  without an href stays plain styled text.
- **Font fallback lists.** A CSS `font-family` list resolves to *every*
  registered family in order (not just the first hit), and the text composes
  them into Bevy 0.20's fallback list — fontique picks the first face
  covering each glyph, so Latin and CJK render from different fonts without
  bundling (with the `system_fonts` feature, the OS's fonts join the
  chain). Weight/style selection stays per family.
- **Font-relative layout lengths.** `em` and `rem` now work for sizes,
  margins, insets, border radii and grid tracks (Bevy 0.20's
  `Val::Em`/`Val::Rem`): `em` resolves against the element's own computed
  font-size, `rem` against the root rule's font-size (Bevy's `RemSize`
  resource, synced by the build; a change restyles). `ex`/`ch` and
  font-relative `padding`/`border-width`/`gap` remain unsupported.
- **Markdown templates and Fluent values.** A `.md`/`.markdown` template is
  Tera plus pulldown-cmark: the rendered output converts to HTML (CommonMark
  + tables, strikethrough, task lists, footnotes) before parsing — Markdown
  every element it constructs carries `class="markdown"` — images included,
  so documents style their Markdown content (say,
  `img.markdown { width: 100% }`) apart from their raw-HTML `<img>` tags.
  Headings, emphasis, lists
  and `![images](…)` become the ordinary blocks, and raw HTML passes
  through, so a Markdown page can embed real UI. Fluent
  bundles opt their message values into Markdown with the manifest's
  `markdown: bool` (default off — HTML-authored bundles keep their exact
  behavior). Autoescaping is forced for Markdown templates (Tera's default
  list doesn't include `.md`); Markdown's per-bundle opt-in mirrors ADR
  0014.

- **`dev-tools` feature** for the examples: `--features dev-tools` serves
  the example over localhost BRP + MCP (`bevy_mcp_harness`) so an agent can
  playtest it — screenshots, the laid-out UI tree (`data-on-click` elements
  report `clickable`), mocked keyboard/gamepad/mouse input. Inert without
  the feature; the library never touches the dependency.

- **Three new examples:** `inputs` (text input fields with a live
  preview and reset buttons), `links` (`<a href>` opening as an `open`
  signal, with keyboard cycling) and `forms` (form submission to a receipt
  panel, including a non-submitting `type="button"`). Run with
  `--features text-input` for the fields.
- **Form submission.** A `<form data-on-submit="name">` whose
  `<button type="submit">` is clicked emits the form's `Submit`
  [`ElementSignal`](src/signals.rs) with a JSON object of the named
  inputs' live values (`{name: value}`; unnamed inputs excluded). Route it
  with `on_html_signal` or drain the message queue like any other signal.
  Submit buttons render as ordinary button blocks (focusable, styleable,
  clickable) even without their own `data-on-click`. With the `text-input`
  feature the payload carries each input's live text; without it the form
  still submits, with an empty payload.

## [0.5.1] - 2026-10-10

### Added

- **Elliptical `border-radius`.** The CSS `h / v` slash syntax
  (`border-radius: 20px / 8px`, and the four corner longhands, including
  per-corner `25% 10px` pairs) now draws elliptical corners; `%` resolves
  per-axis against the node's dimensions, as CSS does. Previously a corner
  whose two radii differed dropped the whole `border-radius` declaration.
- **`position: fixed`.** Positions against the viewport (Bevy 0.20's
  `FixedNode`) instead of the parent, and does not inherit the parent's
  clipping or transform — like CSS's `position: fixed`. The insets apply
  against the viewport. (For `absolute`, nothing changed: still placed in
  the parent's padding box, as Bevy has no containing-block search.)
  `position: sticky` remains unsupported.
- **Inline styling.** `background-color` on an inline element paints behind
  its text run (Bevy 0.20's `TextBackgroundColor`), and `text-decoration`
  (`underline`/`line-through` via `-line`, plus `text-decoration-color`;
  `overline`/`blink`/`-thickness`/`-style` are unsupported) draws on it.
  Borders, frames and padding on inline elements remain unsupported (Bevy
  `TextSpan` has no box).
- **`<img src="…" width height>`.** An image flowing inline with the text
  (Bevy 0.20's `InlineBox`/`InlineImage`). `src` is an asset path relative
  to the template, like `{% include %}` (`/` from the asset root; URLs and
  data URIs are unsupported); `width`/`height` are px attributes, keeping
  the image's aspect ratio when one is unset. The image's load restyles the
  UI and its reload re-sizes the box.

## [0.5.0] - 2026-10-10

### Changed

- **Breaking:** bevy_markup now depends on Bevy 0.20 (was 0.19), following
  Bevy's changes: the picking events behind `data-on-*` are flattened
  (`Pointer<Press>` → `PointerPress`; apps reading `SignalSource::Pointer`
  are unaffected), and `FontSource`'s generic-family variants became
  constructor methods (`FontSource::Serif` → `FontSource::serif()`, same for
  `sans_serif()`/`monospace()`). The bevy_markup API itself is unchanged.

## [0.4.0] - 2026-10-08

### Added

- **`LocaleBundle` asset.** Locale bundles are now bevy_markup's own asset:
  a `*.ftl.ron` manifest naming the locale and its FTL resources (paths
  relative to the manifest, read as dependencies so they hot-reload). FTL
  parse errors are logged and the resource still loads (Fluent keeps the
  valid messages), as before.

### Changed

- **Breaking:** bevy_markup no longer depends on (or re-exports)
  `bevy_fluent` — localization goes through `fluent` directly. `ActiveLocale`
  holds a `Handle<LocaleBundle>` instead of `Handle<bevy_fluent::BundleAsset>`:
  change the type in `load` calls; asset paths and the manifest format are
  unchanged, so existing bundle files need no edits. Apps that use
  `bevy_fluent` themselves can keep depending on it directly.

### Removed

- The `bevy_fluent` dependency and re-export. With it, bevy_fluent's
  `.ftl.yaml`/`.ftl.yml` bundle format stops loading (bevy_markup documents
  `*.ftl.ron` only), and the never-documented `unic-langid` cargo feature is
  gone (the crate now always depends on `unic-langid` for the manifest's
  `locale` field).

## [0.3.0] - 2026-10-06

Interactive UIs: bevy_markup goes from rendering documents to building menus,
HUDs and in-world overlays that work with a mouse, a keyboard and a gamepad,
and that update in place every frame.

### Added

- **Interaction from HTML.** `data-on-click`, `data-on-auxclick` (right and
  middle button), `data-on-press`, `data-on-release`, `data-on-enter` and
  `data-on-leave` turn pointer input into `ElementSignal` messages, with an
  optional JSON payload (`data-with`) and the element's `data-*` attributes
  (`ElementSignal::data`). Each signal says what produced it
  (`SignalSource`: which pointer and button, or which key or gamepad button
  activated the element).
- **Signal routing.** `app.on_html_click("name", system)` and
  `app.on_html_signal("name", system)` run a system for a named signal,
  instead of matching names in a `MessageReader`.
- **Focus and navigation.** Elements with `data-on-click` or `tabindex` are
  focusable, `autofocus` picks the first focused element, and
  `HtmlFocus::navigate` moves focus in a direction (Bevy's directional
  navigation). `HtmlFocus::activate` presses the focused element, sending the
  same signal a click does. `HtmlModal` keeps focus inside a dialog,
  `HtmlNoFocus` keeps a UI out of it, and `FocusEdge` reports navigating past
  the last element. Input bindings stay the app's.
- **CSS pseudo-classes:** `:hover`, `:active`, `:focus` and `:focus-visible`.
- **CSS grid layout:** `display: grid`, track lists with `repeat()`
  (`auto-fill`, `auto-fit`), `minmax()`, `fr`, `fit-content()`, line and
  `span` placement, `grid-auto-flow` (including `dense`).
- **More CSS properties:** `position` with `top` / `right` / `bottom` /
  `left` / `inset`, `z-index`, `border-radius`, `border-color`, `outline` and
  `outline-offset`, `pointer-events`, `overflow`, `opacity` (group opacity),
  and inline `style="…"` attributes.
- **Templates styling their own root:** `<html class="…" id="…">` and the
  `html` rule now place, size and stack the `HtmlUi` entity itself (layout,
  position, `z-index`, background, borders, frames, pickability). Values the
  app sets on the root's `Node` and CSS doesn't declare are left alone.
- **Custom elements.** `<div is="name">` runs a system registered with
  `app.define_html_element("name", system)` once per spawned element, to
  attach components, observers or children from Rust.
- **Template composition.** `{% include %}` and `{% extends %}` take paths
  relative to the template's file, and Tera 2 `{% component %}`s defined in
  an included file can be used by the including template, so a library of
  widgets can be shared between templates.
- **Built-in tooltips.** Insert `HtmlTooltips` with a tooltip template, and
  any element with `data-tooltip="<Fluent key>"` shows it while hovered.
- **Anchored overlays.** `HtmlAnchor` keeps a UI beside an element (for
  tooltips and popups), on screen and despawned with the element.
  `HtmlWorldAnchor` keeps a UI over a point in the 3D world (for nameplates
  and markers), hidden while the point is off screen, behind the camera or
  invisible; `HtmlWorldAnchorView` reports its distance from the camera.
- **New examples:** `menu` (a settings screen for mouse, keyboard and
  gamepad), `live` (data updated every frame), `world` (nameplates over 3D
  units). Every example now works with a keyboard and a gamepad, `grid`
  shows three different grid layouts, and each example has a README with a
  screenshot.

### Changed

- **Updates happen in place.** A changed template value, translation or
  stylesheet now updates the existing UI entities instead of respawning the
  UI: elements are matched by `id` (or by position), and only components
  whose values changed are written. Focus, hover state and components the app
  attached survive updates. Rendering the same output again is skipped
  entirely, so writing a `TemplateContext` every frame is cheap.
- **`HtmlUiBuilt` now also fires for updates that keep elements.** Code that
  attached observers or children in an `HtmlUiBuilt` handler must not assume
  a fresh set of entities, or it will attach them again on every update. Use
  `data-on-*` attributes or custom elements (`is="…"`) instead.
- **The `html` rule's layout properties now apply to the `HtmlUi` entity**
  (see "Templates styling their own root" above). A stylesheet that set
  layout on `html` while the app laid the root out in Rust may now override
  the app's values.
- The HTML parser dependency moved from `tl` to `astral-tl` (a maintained
  fork), which fixes the attribute bug below.

### Fixed

- A value-less attribute (such as `autofocus` or `hidden`) no longer eats the
  first character of the attribute after it.
- An image the app put on a built element (`ImageNode`) no longer turns every
  restyle into a full rebuild of the UI.
- Updating an `HtmlUi` nested inside another `HtmlUi` that is updating in the
  same frame no longer panics, and restyles no longer despawn nested UIs.

## [0.2.0] - 2026-10-05

### Changed

- **Breaking:** `HtmlUiPlugin` is renamed `BevyMarkupPlugin`.

## [0.1.0] - 2026-10-05

### Added

- First release: HTML templates rendered with Tera, localized with Fluent
  (`data-l10n-id`, arguments, plurals, inline markup) and styled with CSS
  (type, class, id and compound selectors, specificity, inherited text
  properties, flex layout and sizing, 9-slice `border-image` frames), built
  into native Bevy UI entities. Stylesheets, translations and fonts can be
  switched at runtime.

[Unreleased]: https://github.com/nchashch/bevy_markup/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/nchashch/bevy_markup/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/nchashch/bevy_markup/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/nchashch/bevy_markup/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nchashch/bevy_markup/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nchashch/bevy_markup/releases/tag/v0.2.0
[0.1.0]: https://crates.io/crates/bevy_markup/0.1.0
