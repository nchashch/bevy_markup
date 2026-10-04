# p23

Bevy 0.19 library: HTML templates (Tera) + Fluent + CSS → Bevy UI, with
9-slice frames. Dependencies are limited to that; don't add unrelated crates.

- `cargo run --example quickstart` — the API in one small app.
- `cargo run --example demo` — everything: themes, languages, DOM outlines,
  scrolling, 9-slice frames.
- `cargo doc --open` — the user-facing documentation (crate docs = guide).

The library depends on `bevy` with `default-features = false` and only the
features its code uses (`ui_api`, `default_font`, `bevy_log`; see
`Cargo.toml`). Rendering, windowing, picking, widgets and image formats are the
app's choice. Examples get full Bevy via `[dev-dependencies]`, whose features
never reach library users. A missing feature shows up in `cargo check --lib`.

## Public API (`p23::prelude::*`)

| Item | Kind | Role |
|---|---|---|
| `HtmlUiPlugin` | Plugin | loaders, resources, systems; adds bevy_fluent's `FluentPlugin` if absent |
| `HtmlUiSystems::{Render, Localize, Build}` | SystemSet | chained in `PostUpdate`, before `UiSystems::Prepare` |
| `HtmlUi(Handle<HtmlTemplate>)` | Component | the UI; requires `Node`, `TemplateContext`, `RenderedHtml`, `LocalizedText` |
| `TemplateContext(tera::Context)` | Component | Tera variables; `.with(k, &v)` builder; Deref to `tera::Context`; mutate → re-render |
| `HtmlStylesheet(Handle<Stylesheet>)` | Component | per-entity stylesheet override |
| `HtmlDebugOutline` | Component | show the DOM outline (styled like `pre`) instead of the UI |
| `RenderedHtml` | Component | `Pending` / `Ready(HtmlDocument)` / `Failed(msg)` (read-only) |
| `HtmlElement { tag, id, classes }` | Component | on each spawned block and container node |
| `HtmlUiBuilt { entity }` | EntityEvent | after each (re)build; children are replaced every time, so wire behaviour here |
| `HtmlElements` | SystemParam | `iter` / `by_id` / `by_class` / `by_tag` below an `HtmlUi` |
| `DefaultStylesheet(Option<Handle<Stylesheet>>)` | Resource | stylesheet for `HtmlUi`s without an override; swap = theme |
| `ActiveLocale(Option<Handle<BundleAsset>>)` | Resource | Fluent bundle; `None` = no localization; swap = language |
| `FontFamilies` / `FontFaces` / `GenericFamily` | Resource + types | CSS `font-family` name → font handles (+ generic keyword mapping) |
| `HtmlTemplate`, `Stylesheet`, `NineSlice` | Assets | `.html`/`.htm`, `.css`, `*.slice.ron` |
| `NineSliceFrame(Handle<NineSlice>)` | Component | 9-slice image as a node's border-box background (non-HTML nodes; HTML uses CSS `border-image`) |
| `BundleAsset` | Asset (bevy_fluent) | `*.ftl.ron` locale bundle |

Re-exported crates (their types appear in the API): `tera`, `tl`,
`bevy_fluent`, `lightningcss`. Cargo feature `system_fonts` enables Bevy's
`system_font_discovery`.

## Layout

```
src/
  lib.rs           crate docs (guide), HtmlUiPlugin, HtmlUiSystems, prelude, re-exports
  html.rs          HtmlUi, TemplateContext, RenderedHtml, HtmlDebugOutline, HtmlElement,
                   HtmlUiBuilt, HtmlElements; render system (Tera + tl)
  template.rs      HtmlTemplate asset + loader, HtmlDocument (+ outline), decode_entities
  l10n.rs          ActiveLocale, LocalizedText; localize system (data-l10n-id/-args)
  style.rs         Stylesheet asset + loader, DefaultStylesheet, HtmlStylesheet; CSS subset docs
  cascade.rs       (internal) stylesheet → declared style per element (+ unit tests)
  fonts.rs         FontFamilies, FontFaces, GenericFamily
  build.rs         (internal) DOM + styles → Bevy UI children; HtmlUiBuilt trigger
  nine_slice.rs    NineSlice asset + loader, NineSliceFrame
examples/
  quickstart.rs    fonts, DefaultStylesheet, ActiveLocale, one HtmlUi, click wiring, Space = language
  demo/            main.rs (setup: fonts), panels.rs (plain / rendered / outline panels),
                   scroll.rs, selector.rs, locale_panel.rs, theme_panel.rs, consts.rs
tests/
  html_ui.rs       headless test vectors: HTML/CSS/Fluent/Tera → world dump, + browser_oracle (see Testing)
  properties.rs    proptest metamorphic properties over the pipeline (shorthand=longhands, round trips, …)
  quickcheck.rs    quickcheck structured-input properties (cascade vs reference model, text round trip)
  common/mod.rs    shared headless harness: TestUi (temp asset root, settle, dump)
  vectors/<name>/  file-based vectors: page.html, style.css, browser.json (oracle output)
  fixtures/        frame.png (32×24, committed; `assets/` is not)
scripts/
  browser_oracle.py  headless Chromium → tests/vectors/*/browser.json (stdlib Python only)
assets/            (gitignored — see Gotchas)
  fonts/           Regular/Bold/Italic/BoldItalic of IosevkaSlabMono, IosevkaSlabQP, Spectral
  quickstart/      hello.html, style.css (html rule: border-image frame), locales/{en-US,de}
  ui/frame.png     256x256 frame; ui/frame.slice.ron slices it (16px borders); frame_transparent.png (clear center)
  ui/themes/       demo CSS themes: crimson (default), parchment (framed `pre` via border-image longhands), terminal, large_print
  ui/content/      test.html (plain), inventory.html (Tera), l10n.html (Tera + Fluent), test.css
  locales/<id>/    demo bundles (main.ftl.ron + ui.ftl) for en-US, ru, de, ja
```

## Conventions

- Library = HTML/CSS/Fluent/Tera → Bevy UI only. Panels, layouts, selectors,
  scrollbars, cameras, `ClearColor`, font files and asset paths belong to apps
  (the examples). Global config is resources; per-entity config is components.
- New user-facing items go in the prelude; internals stay `pub(crate)`. Keep
  rustdoc warning-free and doc examples compiling (`no_run`).
- Asset-backed state reacts to `AssetEvent::LoadedWithDependencies` /
  `Modified` plus change detection, so hot reload works with Bevy's
  `file_watcher` (not enabled in the examples).
- Demo typography: headers IosevkaSlabQP (red), body Spectral (off-white),
  code/debug Iosevka Slab Mono — in the CSS themes for HTML, in
  `examples/demo/consts.rs` for plain Bevy UI panels.

## Pipeline

```
.html (Tera) --render(TemplateContext)--> HTML --tl--> DOM            [Render]
    --data-l10n-id × ActiveLocale--> LocalizedText                     [Localize]
    --CSS (HtmlStylesheet | DefaultStylesheet) × FontFamilies--> UI    [Build] → HtmlUiBuilt
```

- Templates: compiled at load (syntax errors fail the load); name = asset path,
  so `.html` gets Tera HTML autoescaping. Plain HTML renders to itself.
- Rebuild triggers: template/context change or reload, locale change or bundle
  (re)load, stylesheet swap or (re)load, a stylesheet's `border-image` image
  loading, `FontFamilies` change, outline marker added. The UI isn't built
  while its stylesheet is loading (images may arrive later; their load rebuilds).
- Structure (`build.rs`: DOM → `Item` tree → nodes): blocks `h1`–`h6`, `p`,
  `li` (bulleted), `pre` (whitespace kept, no wrap; leading newline and
  trailing whitespace dropped), loose text; containers (`CONTAINERS`: `div`,
  `section`, `article`, `header`, `footer`, `main`, `nav`, `aside`, `ul`,
  `ol`, `blockquote`, `figure`, `form`) become column nodes holding their
  children, `row_gap` = the `HtmlUi` node's own unless CSS `gap`; other
  elements are inline in a block or walked through outside one (`html`,
  `body` included — the `HtmlUi` node is the root); `head`/`script`/`style`
  skipped. Each block is a `Text` + one `TextSpan` per styled run; whitespace
  collapses outside `pre`.
- Localization (fluent-dom convention): `<p data-l10n-id="key"
  data-l10n-args='{"n": 3}'>fallback</p>`; the translation replaces the
  content, the element's own content is the fallback. Args are JSON (numbers
  stay numbers for plurals); a whole map can be one Tera variable. Translations
  are markup: inline elements in them are styled. In `.ftl`: `&lt;`/`&amp;`
  for literal `<`/`&`, `{"{"}`/`{"}"}` for braces. String args are
  HTML-escaped. CJK paragraphs go on one line (a wrapped line becomes a stray
  space).
- CSS subset (`style.rs` docs): compound selectors (type or `*` + `.class` /
  `#id` parts, comma lists; combinators/attributes/pseudo-classes skipped at
  `debug`), matched per element (`HtmlElement` tag/id/classes, cached per
  combination) with CSS precedence: `!important`, then specificity (ids,
  classes, type), then source order. Inherited `color`, `font-family` (first name registered in
  `FontFamilies`, generics via `set_generic`; no registered name → Bevy's
  default font, as browsers do), `font-size` (`px`, `em`/`%`, `rem`, keywords
  with medium = 16px, `smaller`/`larger`), `font-weight` (bold above 500, CSS
  font matching), `font-style`; blocks:
  `background-color`. `html` is the
  starting point even without `<html>`; nothing declared → white, Bevy's
  default font, 16px. Missing font faces fall back bold-italic → bold →
  italic → regular.
- Box properties (`cascade.rs` → `build.rs`): `border-image` (shorthand +
  `-source`/`-slice`/`-repeat`), `border-width`, `padding` (absolute lengths)
  on blocks, containers and the `html` rule; `background-color` on blocks and
  containers; `gap`/`row-gap` on containers.
  `url()` resolves relative to the `.css`; the `Stylesheet` loader loads the
  images as dependencies. Maps to Bevy's sliced `ImageNode`
  (`VisualBox::BorderBox`) + `Node::border`/`padding`. Slice numbers = image px,
  `%` = of image size (needs the image loaded). `stretch` → Stretch, other
  repeats → `Tile`; one mode for all sides. Bevy always draws the center;
  `border-image-width`/`-outset` ignored (corners at image size). Blocks with
  box properties get a wrapper node (a node can't be both `Text` and
  `ImageNode`); the `HtmlElement` is on the wrapper. The `html` rule's box is
  applied to the `HtmlUi` node itself, with the replaced border/padding kept in
  `CssRootBox` and restored if a later stylesheet drops them.
- Demo locales: every locale needs the same message ids; item names arrive as
  English data (`$item`) and non-English bundles map them with an `item-name`
  message. Every visible string in `l10n.html` has a key.

## Gotchas (verified)

- `assets/` is gitignored: new asset files are not committed.
- Bevy `ImageNode` defaults to `VisualBox::ContentBox` (draws inside padding);
  frames need `BorderBox` (`NineSliceFrame` sets it).
- Scrollable panels (demo `scroll.rs`): framed row node with `max_height` and
  `column_gap: SCROLLBAR_GAP` → `[viewport, scrollbar]`. Viewport: `ScrollArea`
  + `viewport_node()` (`overflow: scroll_y`, `flex_grow: 1`, `min_height: 0` —
  else its content sizes it and nothing scrolls). Viewport children need
  `flex_shrink: 0.0` (`HtmlUi` blocks set it). The `Scrollbar` must be a
  *sibling* of the viewport; it starts `Display::None` and `toggle_scrollbars`
  shows it only while content overflows. A framed node that scrolls itself
  needs `overflow_clip_margin: OverflowClipMargin::content_box()`.
- Every UI `Node` is pickable and blocks pointer input below it. Invisible
  layout-only wrappers must carry `Pickable::IGNORE` (a full-window wrapper
  once broke all scrolling and clicking).
- Bevy 0.19: `BorderRadius` is a `Node` field, not a component.
- `tl::VDom` borrows its input; `HtmlDocument` uses `tl::parse_owned` (unsafe
  fn, sound per its docs) → `VDomGuard`, which only hands out shared borrows,
  so the DOM is never mutated; translations live beside it keyed by
  `tl::NodeHandle`.
- `tl` misparses raw Tera (`{% if a < b %}` becomes a `<b>` tag) — hence
  render-then-parse. `tl` keeps character references as written (use
  `decode_entities`) and drops `<!DOCTYPE>`.
- Tera 2 (not 1.x): unknown functions/filters fail at template *compile* time,
  so custom ones must be registered in the loader before `add_raw_template`.
  Tera prints maps as `{"k": v}` (valid JSON except control characters).
- Fluent wraps placeables in U+2068/U+2069; bevy_fluent's bundle is behind an
  `Arc`, so `l10n.rs` strips them. Fluent term arguments only accept literals;
  message references share the caller's variables.
- rustdoc: a bare `[`template`]` link is ambiguous (Bevy has a `template` fn);
  write `[`template`](mod@template)`.
- Iosevka ligatures render `<!--`/`-->` as arrows in outline text.
- CSS generic keywords are case-insensitive: unquoted `font-family: Serif` is
  the generic `serif`, not a family named "Serif" (quote it, or map the
  generic with `FontFamilies::set_generic`).
- Headless apps: `ImagePlugin` only pre-registers its loader; `bevy_render`
  registers the real one. Without rendering, register
  `ImageLoader::new(CompressedImageFormats::empty())` yourself.

## Testing

Two layers, both deterministic and headless:

- **Cascade unit tests** (`src/cascade.rs`, `cargo test --lib`): CSS text →
  declared style for an `HtmlElement` (specificity, compound matching, comma
  lists, importance, unsupported selectors).
- **Test vectors** (`tests/html_ui.rs`, `cargo test --test html_ui`): input
  files (HTML/Tera template, CSS, Fluent bundles) → the Bevy world they
  produce. `TestUi::new(name, files)` writes the files plus `frame.png` into a
  fresh temp asset root and builds `MinimalPlugins + AssetPlugin + ImagePlugin
  + HtmlUiPlugin` (no window/renderer; fonts are fake `Handle::Uuid`s labelled
  `serif`, `serif-bold`, …, `mono`). `.stylesheet()`, `.locale()`,
  `.spawn(template, context, node)`, then `settle()` updates until all tracked
  assets are loaded and a new build has been stable for 5 frames.
  `assert_dump(expected)` compares a text dump of the `HtmlUi` subtree: one
  line per entity (`tag#id.class` / `-` / `html-ui`, then `border=`,
  `padding=`, `margin=` as t,r,b,l, `gap=`, `bg=`, `slice=file t,r,b,l
  stretch|tile`), one indented line per text run (`"text" face size color`).
  Vectors: Tera structure, CSS cascade + fonts, Fluent, box model, runtime
  changes (context / locale / stylesheet swaps, root box restore). Plain-HTML
  vectors live as files in `tests/vectors/<name>/` (`TestUi::from_vector`).
- Adding a vector: write the inputs, derive the expected dump **by hand from
  HTML/CSS/Fluent semantics** (don't paste actual output), run. When a vector
  fails, decide whether the code or the expectation is wrong before editing
  either. Keep the dump format stable; extend it only for newly mapped
  properties. A vector should fail when its feature is broken (spot-check by
  breaking the code once).
- **Browser oracle** (`browser_oracle` test): every `tests/vectors/*/` with a
  `browser.json` is built and compared with Chromium's computed styles — per
  non-whitespace character (face from CSS font matching over the harness's
  registered families, size, color) and per root/block/container (padding,
  border widths, border-image file/slices/repeat with `%` resolved on the
  32×24 fixture, background, container gap). Regenerate after changing a
  vector's inputs: `scripts/browser_oracle.py [tests/vectors/<name>]`, then
  review the `browser.json` diff (one record per line) and commit it. The page
  is `* { all: unset }` + `style.css` + `page.html`, so vectors must be plain
  HTML (no Tera / `data-l10n-id`; the script refuses them), set `color` on
  `html` (browsers default to black, p23 to white) and `border-style: solid`
  where widths matter. Deliberate differences (`pre` padding, root background,
  `border-style`, `li` bullets) are skipped and listed above
  `FIXTURE_SIZE` in `tests/html_ui.rs`; add new ones there with a reason. The
  oracle's first run found two real bugs (unregistered `font-family` kept the
  inherited family; weight 501–599 wasn't bold).

### Testing TODO

Work through in order. Weakness numbers refer to the current methodology's
known gaps:

1. Expectations come from the same head as the code (shared misreadings of
   CSS/Fluent pass).
2. The dump is a projection: unprinted components (`flex_shrink`,
   `TextLayout::no_wrap`, `Overflow`, entity counts, …) are unchecked.
3. Components, not layout or pixels: positions, wrapping and drawing are
   untested.
4. Fake font handles: font loading, glyph fallback, CJK untested.
5. `settle()` polls with a sleep and a frame cap (possible flakiness).
6. Coverage by example only.

- [x] **1. Real browser as the oracle** (fixes weakness 1) — CSS part done:
  `scripts/browser_oracle.py` + `browser_oracle` test + vectors `cascade`,
  `box_model`, `units` (see Testing).
  - [ ] Fluent part: `@fluent/dom` (needs npm + a DOM, e.g. jsdom) translating
    the same DOM, as the reference for the overlay and markup behaviour.
  - [ ] More oracle vectors as CSS support grows (every new property gets one).
- [ ] **2. Metamorphic and property-based tests** (fixes weakness 6). Check
  relationships that must always hold, over generated inputs (`proptest`,
  `quickcheck`, `arbtest`, `test-strategy` are dev-dependencies; pipeline
  properties live in `tests/properties.rs`, written with `#[proptest]` /
  `#[strategy]` over the shared `tests/common` harness):
  - [x] Shorthand = longhands: `padding: 1px 2px` builds the same world as the
    four longhands; likewise `border-image`. (`border-width` has no longhands.)
  - [x] Unmatched rules don't matter: adding a rule that matches nothing changes
    nothing (even `!important`).
  - [x] Order only breaks ties: swapping a class rule and a type rule never
    changes the result; equal-specificity ties stay covered by unit tests.
  - [x] Round trips: locale A → B → A, theme X → Y → X, context v → w → v end in
    a dump identical to the start; a forced rebuild with no input change is
    idempotent (proxy for "no duplicate children").
  - [x] `quickcheck` over structured inputs: a generated many-rule stylesheet
    checked against a reference CSS-precedence model (`cascade_winner_matches_
    precedence_model`), and arbitrary text round-tripping through Tera
    autoescaping (`context_text_round_trips_through_template`). The cascade
    property catches a specificity mutation that all cascade unit tests miss.
    Note: quickcheck 1.1's `Gen` RNG is private (edition-2024 `gen` keyword) —
    build `Arbitrary` impls from `T::arbitrary(g)` + `g.choose`;
    the `#[quickcheck]` attribute comes from `quickcheck_macros`.
  - Remaining: `arbtest` robustness runs (arbitrary HTML/CSS/FTL through the
    pipeline: no panic, always some output).
- [ ] **3. Headless layout checks** (fixes weakness 3, mostly). Run Bevy UI's
  layout headless with a fixed viewport; assert node rects (`ComputedNode`,
  `UiGlobalTransform`). Essential once flex layout lands (next step 1); then
  Chromium's `getBoundingClientRect` is the oracle for the supported subset.
  Unverified whether Bevy 0.19 UI layout runs without a camera/window target —
  spike first.
- [ ] **4. Lint tests over real content** (content bugs, the ones you'll hit):
  - every locale defines the same message ids
  - every visible text node in a template sits under a `data-l10n-id`
  - every template compiles with representative data
  - every CSS file parses; every `url()` resolves to an existing file
  - a pseudo-locale (e.g. `[Ĩñvéñtöŕý~~~]`, longer and accented) exposing
    untranslated strings and overflow in the UI
- [ ] **5. Fuzzing** (robustness). cargo-fuzz or proptest-generated arbitrary
  HTML, CSS, FTL and JSON args through the pipeline: no panic, no hang, always
  some output (e.g. the error paragraph). Targets our glue: entity decoding,
  whitespace collapsing, slice arithmetic, `data-l10n-args` handling.
- [ ] **6. Mutation testing** (measures vector strength). `cargo-mutants`
  mutates the code and reports mutations no test catches. Run occasionally;
  each survivor is a missing vector or dump field.
- [ ] **7. Golden images, sparingly** (fixes the rest of weakness 3, and
  weakness 4). Render two or three reference scenes with a software adapter
  (Mesa lavapipe/llvmpipe with wgpu) and compare screenshots within a
  tolerance — the only layer that sees real fonts, wrapping, 9-slice drawing
  and CJK fallback. Brittle across drivers and font versions: a smoke check,
  not a spec.

## Limits and next steps

Known limits (each skipped/ignored value is logged at `debug`):

- **Layout:** containers are always vertical columns; no `display`,
  `flex-direction`, `width`/`height`, `margin`, alignment, `position`. Text
  blocks and containers only stack.
- **Lists:** `ul`/`ol` are plain columns; `li` draws a fixed `• ` with a
  hard-coded 12px indent; `ol` isn't numbered; no `list-style`.
- **Selectors:** compound only (type/`*` + `.class` + `#id`); no combinators
  (`div p`, `>`), attribute selectors, pseudo-classes (`:hover`) or
  pseudo-elements.
- **Properties:** no `text-align`, `line-height`, `letter-spacing`,
  `text-decoration`, `opacity`, `border-radius`, `border-color`/solid
  borders, `overflow`. Lengths: px/em/rem/% for `font-size`, absolute only
  for box properties.
- **border-image:** center always drawn, `-width`/`-outset` ignored, one
  repeat mode for all sides (Bevy `TextureSlicer` limits).
- **Inline:** no inline boxes — `background`, borders, frames and padding on
  inline elements are ignored (Bevy `TextSpan` has no box); inline elements
  have no entity, so `HtmlElements` can't find them (only blocks/containers).
- **Interactivity:** none built in; apps wire behaviour on `HtmlUiBuilt`
  (children are rebuilt on every change, so state on them doesn't persist).
  No forms/inputs, no links.
- **Rebuilds:** any change rebuilds the whole `HtmlUi` subtree (no diffing).
  Fine for panel-sized UIs; large or per-frame-updated documents will churn.
- **Text:** `pre` has a fixed 8px padding; whitespace collapsing doesn't know
  CJK (wrapped CJK source lines become spaces); `decode_entities` handles only
  the five escapes Tera emits (no numeric references).
- **Fonts:** no bundled CJK font; Japanese relies on `system_fonts`.

Next steps (roughly in order of value):

1. Flex layout from CSS: `display: flex`, `flex-direction`, `justify-content`,
   `align-items`, `width`/`height`/`min-`/`max-`, `margin` → `Node`.
2. Descendant/child combinators (`.panel p`, `.panel > p`) — needs the
   ancestor chain during matching; specificity sums.
3. `:hover` / `:active` via `Interaction` or picking, re-styling without a full
   rebuild.
4. Lists done properly: `list-style-type`, `ol` numbering, CSS-driven indent.
5. Text properties: `text-align` (`Justify`), `line-height` (`LineHeight`).
6. Incremental rebuilds: keep entities for unchanged elements (key by DOM
   path) so app-attached state survives and large documents stay cheap.
7. More test vectors alongside each of the above.

## Verification

- Warning-free: `cargo check --lib` (minimal Bevy features), `cargo build
  --all-targets`, `cargo doc --no-deps`. `cargo test` must pass: cascade unit
  tests, the headless test vectors, and the `no_run` doc examples (see
  Testing). Library changes that alter the mapping need a vector.
- Visual changes: run an example and capture an in-app screenshot (desktop
  screenshots grab whatever workspace is visible). Throwaway system, removed
  afterwards:

  ```rust
  app.add_systems(Update, |mut c: Commands, t: Res<Time>, mut done: Local<bool>| {
      if !*done && t.elapsed_secs() > 5.0 {
          *done = true;
          c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
              .observe(bevy::render::view::screenshot::save_to_disk("/tmp/p23_shot.png"));
      }
  });
  ```

- DOM outlines (`HtmlDebugOutline`) are logged at `debug`:
  `RUST_LOG=p23=debug timeout 15 cargo run --example demo`.
- Pointer interaction: write `bevy::window::WindowEvent::{CursorMoved,
  MouseWheel, MouseButtonInput}` messages from a throwaway system (move, then
  press and release in later frames), then check state. Real input sometimes
  reaches the window during runs; prefer setting resources directly when
  clicks aren't what's under test.
