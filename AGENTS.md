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
    mod.rs           AssetsPlugin = HtmlPlugin + L10nPlugin
    html.rs          .html loader (Tera template), HtmlView → RenderedHtml (tl DOM), decode_entities
    l10n.rs          Fluent: ActiveLocale, data-l10n-id/-args → LocalizedText
  tui/
    mod.rs           TuiPlugin: bevy_tui_texture terminal inside a 9-slice frame
    panel.rs         TuiPanel marker + ratatui draw system (write ratatui code here)
  ui/
    mod.rs           UiPlugin: registers panels and systems below
    nine_slice.rs    *.slice.ron loader, NineSlice asset, NineSliceFrame component
    panel.rs         Static Bevy UI panel (header + body text)
    dom_panel.rs     Debug panels: DOM outline of test/inventory/l10n.html; demo_context()
    html_ui.rs       HtmlUi: renders an HtmlView's DOM as Bevy UI nodes
assets/              (gitignored — see Gotchas)
  fonts/             Regular/Bold/Italic/BoldItalic of IosevkaSlabMono (TUI, debug text), IosevkaSlabQP (headers), Spectral (body)
  ui/frame.png       256x256 frame; ui/frame.slice.ron slices it (16px borders)
  ui/content/        test.html (plain), inventory.html (Tera), l10n.html (Tera + Fluent)
  locales/en-US/     main.ftl.ron (bundle manifest), ui.ftl (messages)
```

## Conventions

- One plugin per module; `SystemPlugins` wires them. Panels expose `spawn`
  (Startup) and their update systems as `pub(super)` fns registered in the
  module's `mod.rs`.
- Shared paths/styles live in `consts`: `FONT_PATH` (mono), `HEADER_FONT_PATH`
  / `HEADER_BOLD_FONT_PATH` / `HEADER_ITALIC_FONT_PATH` /
  `HEADER_BOLD_ITALIC_FONT_PATH` + `HEADER_COLOR`, the same four
  `BODY_*_FONT_PATH` + `BODY_COLOR`, `FRAME_PATH`. Don't hard-code these in
  panels.
- Typography: headers IosevkaSlabQP (red), body Spectral (off-white),
  monospace/debug Iosevka Slab Mono.
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
- `HtmlUi` on a node with `HtmlView` rebuilds its children on change. Supported:
  `h1`–`h6` (header font, 28/24/22/20px), `p` (body 20px), `li` (bulleted body),
  loose text (body). Other elements are traversed; `head`/`script`/`style`
  skipped. Each block is a `Text` with one `TextSpan` per styled run:
  `b`/`strong` → Bold face, `i`/`em` → Italic face of the block's family, both
  nested → BoldItalic face. Fluent translations are plain text (one
  regular run). Whitespace collapses across run boundaries as in HTML.
- `HtmlDocument::outline(&LocalizedText)` gives an indented debug tree (used by
  `dom_panel.rs`, also logged at `info`).

## Gotchas (verified)

- `assets/` is gitignored: new asset files are not committed.
- Bevy `ImageNode` defaults to `VisualBox::ContentBox` (draws inside padding);
  frames need `BorderBox`.
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
