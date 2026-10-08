# Changelog

All notable changes to bevy_markup are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Before 1.0, a minor version bump (0.2 → 0.3) may contain breaking changes.

## [Unreleased]

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

[Unreleased]: https://github.com/nchashch/bevy_markup/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/nchashch/bevy_markup/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nchashch/bevy_markup/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nchashch/bevy_markup/releases/tag/v0.2.0
[0.1.0]: https://crates.io/crates/bevy_markup/0.1.0
