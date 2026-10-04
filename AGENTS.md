# p23

Bevy 0.19 prototype. Current focus: UI built from assets — ratatui panels
rendered to textures, 9-slice framed Bevy UI, and an HTML → Bevy UI pipeline
(Tera templating, `tl` parsing, Fluent localization).

## Layout

```
src/
  main.rs            App: DefaultPlugins + system::SystemPlugins
  system/mod.rs      SystemPlugins: registers AssetsPlugin, TuiPlugin, UiPlugin; ClearColor; Camera2d
  consts/mod.rs      Shared paths and styles (fonts, frame, colors)
  assets/
    mod.rs           AssetsPlugin = CssPlugin + HtmlPlugin + L10nPlugin
    css.rs           .css loader (lightningcss → owned StyleSheet<'static>)
    html.rs          .html loader (Tera template), HtmlView → RenderedHtml (tl DOM), decode_entities
    l10n.rs          Fluent: Locales (all bundles, preloaded), ActiveLocale, data-l10n-id/-args → LocalizedText
  tui/
    mod.rs           TuiPlugin: bevy_tui_texture terminal inside a 9-slice frame
    panel.rs         TuiPanel marker + ratatui draw system (write ratatui code here)
  ui/
    mod.rs           UiPlugin: registers panels and systems below
    nine_slice.rs    *.slice.ron loader, NineSlice asset, NineSliceFrame component
    panel.rs         Static Bevy UI panel (header + body text)
    dom_panel.rs     Debug panels: DOM outline of test/inventory/l10n.html (max 45vh, scrollable); demo_context()
    html_ui.rs       HtmlUi: renders an HtmlView's DOM as Bevy UI nodes (rendered panel: max 50vh, scrollable)
    html_style.rs    Stylesheet → per-element style table (color, background, font) for html_ui
    scroll.rs        Shared scroll pieces: viewport_node(), spawn_scrollbar(), toggle_scrollbars
    selector.rs      Framed panel with one-of-N toggle buttons → Selector { active } (+ caller marker)
    locale_panel.rs  Language selector (Selector + LocaleSelector) sets ActiveLocale
    theme_panel.rs   Theme selector (Selector + ThemeSelector) swaps every HtmlStylesheet; THEMES list
assets/              (gitignored — see Gotchas)
  fonts/             Regular/Bold/Italic/BoldItalic of IosevkaSlabMono (TUI, debug text), IosevkaSlabQP (headers), Spectral (body)
  ui/frame.png       256x256 frame; ui/frame.slice.ron slices it (16px borders)
  ui/themes/         CSS themes for html_ui: crimson (default), parchment, terminal, large_print
  ui/content/        test.html (plain), inventory.html (Tera), l10n.html (Tera + Fluent), test.css
  locales/<id>/      main.ftl.ron (bundle manifest) + ui.ftl, for en-US, ru, de, ja
```

## Conventions

- One plugin per module; `SystemPlugins` wires them. Panels expose `spawn`
  (Startup) and their update systems as `pub(super)` fns registered in the
  module's `mod.rs`.
- Shared paths/styles live in `consts`: four faces per family —
  `MONO_FONT_PATH` / `MONO_BOLD_FONT_PATH` / `MONO_ITALIC_FONT_PATH` /
  `MONO_BOLD_ITALIC_FONT_PATH`, the same four `HEADER_*` + `HEADER_COLOR`, the
  same four `BODY_*` + `BODY_COLOR` — and `FRAME_PATH`. Don't hard-code these
  in panels. `FONT_FAMILIES` maps CSS `font-family` names to those four-face
  sets; `FONT_GENERIC_FAMILIES` maps `serif`/`monospace`.
- Typography: headers IosevkaSlabQP (red), body Spectral (off-white),
  monospace/debug Iosevka Slab Mono. For rendered HTML this lives in the CSS
  themes (`assets/ui/themes/crimson.css` is the default look); plain Bevy UI
  panels use the consts.
- Framed panels: `NineSliceFrame(asset_server.load(FRAME_PATH))` on the panel's
  root `Node`; use `padding` for the inset (the frame covers the border box).
- Asset-backed state updates react to `AssetEvent::LoadedWithDependencies` /
  `Modified` plus change detection, so hot reload works if Bevy's
  `file_watcher` feature is enabled (currently it is not).

## Systems

### 9-slice (`ui/nine_slice.rs`)

`*.slice.ron` → `NineSlice { image, slicer }`. Format:

```ron
(
    image: "frame.png",                  // relative to the .ron file
    border: (left: 16.0, right: 16.0, top: 16.0, bottom: 16.0),  // image px, floats
    sides: Stretch,                      // or Tile(1.0)
    center: Stretch,
    max_corner_scale: 1.0,
)
```

`NineSliceFrame(handle)` inserts/updates the entity's `ImageNode` once loaded,
keeping any existing tint, with `visual_box: BorderBox`.

### TUI (`tui/`)

`TuiRequest::ui(cols, rows, TuiFontSource::Asset { .. })` spawned as a child of
a framed node (the terminal owns its own `ImageNode`, so the frame must be on
the parent). `Tui` appears once the font loads; draw systems run in
`TerminalSystemSet::Render` and must tolerate `Tui` not existing yet. Panel
aspect ratio = `Tui::size_px()` (cols × cell width / rows × cell height).

### HTML pipeline (`assets/html.rs`, `assets/l10n.rs`, `ui/html_ui.rs`)

```
.html (Tera template) --Tera render(context)--> HTML string --tl parse--> DOM
    --data-l10n-id + ActiveLocale bundle--> LocalizedText --HtmlUi--> Bevy UI nodes
```

- `.html`/`.htm` load as `HtmlTemplate` (compiled Tera; syntax errors fail the
  load). Template name = asset path, so `.html` gets Tera HTML autoescaping.
  Plain HTML is a template that renders to itself.
- `HtmlView { template, context: tera::Context }` → `RenderedHtml`
  (`Pending | Ready(HtmlDocument) | Failed(msg)`) and `LocalizedText`.
  Mutating `context` at runtime re-renders → re-parses → re-localizes.
- Localization follows Fluent's DOM convention:
  `<p data-l10n-id="key" data-l10n-args='{"n": 3}'>fallback</p>`. The
  translation replaces the element's content; on error the element's own text is
  the fallback. Args are a JSON object (numbers stay numbers for plurals).
  A whole args map can be one Tera variable: `data-l10n-args='{{ my_args }}'`
  with a map/struct in the context.
- Locales: `LOCALES` in `l10n.rs` lists (dir, native name); add a row plus
  `assets/locales/<dir>/{main.ftl.ron,ui.ftl}` to add a language. Every locale
  needs the same message ids. Item names come in as English data (`$item`);
  non-English bundles map them with an `item-name` message selecting on
  `$item`, referenced from `item-count`.
- Fonts: Spectral/IosevkaSlabQP/IosevkaSlabMono cover Latin + Cyrillic, not
  CJK. Japanese renders via Bevy's `system_font_discovery` feature (Parley
  falls back per script to installed system fonts) — depends on the player's
  OS having a CJK font; bundle one for shipping.
- `HtmlUi` on a node with `HtmlView` rebuilds its children on change (in the
  rendered panel that node is the scroll viewport). Tags only decide
  structure: blocks `h1`–`h6`, `p`, `li` (bulleted), `pre` (whitespace and
  newlines kept, no wrap; a newline right after `<pre>` and trailing
  whitespace are dropped), and loose text; other elements are inline within a
  block or walked through outside one; `head`/`script`/`style` skipped. Each
  block is a `Text` with one `TextSpan` per styled run. Fluent translations
  are one run in the block's style. Outside `pre`, whitespace collapses across
  run boundaries as in HTML.
- All `HtmlUi` styling comes from its `HtmlStylesheet(Handle<CssStyleSheet>)`
  (initially `THEMES[0]`, swapped by the theme panel; a changed handle or a
  (re)loaded sheet rebuilds) — including what browsers do by default (bold `b`,
  italic `em`, mono `code`/`pre`, heading sizes). Subset (`ui/html_style.rs`):
  type selectors only (comma lists ok; others skipped at `debug`);
  inherited `color`, `font-family` (first name in `FONT_FAMILIES`, or a mapped
  generic), `font-size` (`px`, `em`/`%`, `rem`, keywords with medium = 16px,
  `smaller`/`larger`), `font-weight` (bold at 600+), `font-style`;
  non-inherited `background-color` (blocks). Later rules win, `!important`
  beats normal. An `html` rule is the starting point, also for fragments
  without `<html>`; nothing declared → white, Bevy's default font, 16px. The
  UI isn't built until the sheet loads, and rebuilds on its load/modify events.
- `HtmlDocument::outline(&LocalizedText)` gives an indented debug tree (used by
  `dom_panel.rs`, also logged at `info`).

## Gotchas (verified)

- `assets/` is gitignored: new asset files are not committed.
- Bevy `ImageNode` defaults to `VisualBox::ContentBox` (draws inside padding);
  frames need `BorderBox`.
- Scrollable panels (use `ui/scroll.rs`): framed row node with `max_height` and
  `column_gap: SCROLLBAR_GAP` → children `[viewport, scrollbar]`. Viewport:
  `ScrollArea` (wheel/trackpad, clamped) + `viewport_node()` (`overflow:
  scroll_y`, `flex_grow: 1`, `min_height: 0` — else its content sizes it and
  nothing scrolls). Every viewport child needs `flex_shrink: 0.0` (HtmlUi
  blocks set it) or the column squashes them. `spawn_scrollbar(parent,
  viewport)` adds `bevy::ui_widgets::Scrollbar` as a *sibling* (a child of the
  scrolled node would scroll away) with a `ScrollbarThumb` (no `Node`; style via
  `BackgroundColor` + its `border_radius`). It starts `Display::None`;
  `toggle_scrollbars` puts it in layout only while content overflows, so short
  content keeps the full width. Track click pages; thumb drags. Widgets are in
  `DefaultPlugins` via Bevy's default `ui` feature. If a framed node itself
  scrolls, add `overflow_clip_margin: OverflowClipMargin::content_box()` or
  content draws over the frame.
- Every UI `Node` is pickable and blocks pointer input to nodes below by
  default. Invisible layout-only wrappers (e.g. the full-window root in
  `tui/mod.rs`) must carry `Pickable::IGNORE`, or they swallow wheel/click/drag
  for everything underneath — this is what broke `ScrollArea`/`Scrollbar`.
- Bevy 0.19: `BorderRadius` is a `Node` field (`border_radius`), not a
  component.
- `tl::VDom` borrows its input; assets use `tl::parse_owned` (unsafe fn, sound
  per its docs) → `VDomGuard`. The guard only hands out shared borrows, so the
  DOM is never mutated; derived data (translations) lives beside it, keyed by
  `tl::NodeHandle`.
- `tl` misparses raw Tera (`{% if a < b %}` becomes a `<b>` tag; block tags in
  attribute position become junk attributes) — hence render-then-parse.
- `tl` keeps character references as written (`&lt;`, `&quot;`); use
  `html::decode_entities` on text and attribute values. `tl` drops `<!DOCTYPE>`.
- Tera 2 (not 1.x API): unknown functions/filters are errors at template
  *compile* time, so any custom function must be registered in the loader
  before `add_raw_template`. Tera prints maps as `{"k": v}` with Rust-debug
  string escaping — valid JSON except for control characters.
- Fluent wraps placeables in U+2068/U+2069; `bevy_fluent`'s bundle sits behind
  an `Arc`, so `l10n.rs` strips them instead of `set_use_isolating(false)`.
- Fluent term arguments only accept literals (`-term(x: "a")`); passing a
  variable fails to parse ("Expected a string or number literal"). Message
  references (`{ other-msg }`) share the caller's variables, so use a message
  for variable-driven lookups.
- `Locales` is inserted by an L10nPlugin Startup system; anything reading it
  at startup (e.g. `locale_panel::spawn`) runs in `PostStartup`.
- Iosevka ligatures render `<!--`/`-->` as arrows in debug text.

## Verification

- `cargo build` must be warning-free.
- For visual changes, run the app and capture an in-app screenshot (desktop
  screenshots grab whatever workspace is visible). Throwaway system, removed
  afterwards:

  ```rust
  app.add_systems(Update, |mut c: Commands, t: Res<Time>, mut done: Local<bool>| {
      if !*done && t.elapsed_secs() > 4.0 {
          *done = true;
          c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
              .observe(bevy::render::view::screenshot::save_to_disk("/tmp/p23_shot.png"));
      }
  });
  ```

- The HTML pipeline logs each DOM outline at `info`; `timeout 10 cargo run`
  and read the log to check templating/localization without a screenshot.
- Pointer interaction can be tested in-app by writing
  `bevy::window::WindowEvent::{CursorMoved, MouseWheel, MouseButtonInput}`
  messages (picking reads these) from a throwaway system, then logging
  `HoverMap` / `ScrollPosition`.
