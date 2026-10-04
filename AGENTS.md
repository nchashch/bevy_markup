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
  properties.rs    proptest metamorphic properties over the pipeline (shorthand=longhands, round trips,
                   selector lists/formatting/duplicates, color notations, relative font sizes, 9-slice manifests)
  quickcheck.rs    quickcheck structured-input properties (cascade vs reference models — single rule winner and
                   selector lists × per-declaration importance × several elements — and text round trip)
  arbtest.rs       arbtest: arbitrary bytes through the pipeline (robustness) + structure-aware random documents
                   vs an HTML model (node tree, collapsed text, HtmlElements order)
  stateful.rs      proptest-stateful machine: random op sequences (default theme, own HtmlStylesheet set/remove,
                   locale, context, outline toggle, FontFamilies swap) vs a reference model
  layout_properties.rs  flexbox invariants on real headless layout (row-reverse mirror, auto margins,
                   grow shares, space-between, wrap without overlap)
  content_lint.rs  lint tests over the real assets/ content (skipped without assets/): locale parity,
                   templates render (+ every data-l10n-id resolves), no hard-coded text, CSS url()s, pseudo-locale
  golden.rs        golden images (#[ignore]d): tests/golden/<scene>/ rendered offscreen by real Bevy,
                   compared with expected.png within a tolerance (see Testing)
  common/mod.rs    shared headless harness: TestUi (temp asset root, settle, dump; with_layout = Bevy UI layout)
  vectors/<name>/  file-based vectors: page.html, style.css, browser.json (CSS oracle output);
                   Fluent vectors add messages.ftl + fluent.html (Fluent oracle output);
                   layout_* vectors also get rects in browser.json (layout oracle)
  golden/<scene>/  page.html, style.css, [messages.ftl], expected.png (text, frame, l10n)
  fixtures/        frame.png (32×24, committed; `assets/` is not)
scripts/
  browser_oracle.py  headless Chromium → tests/vectors/*/browser.json (stdlib Python only)
  fluent_oracle.sh   @fluent/dom in jsdom → tests/vectors/*/fluent.html (Node + npm;
                     pinned packages in fluent-oracle/, node_modules gitignored)
  mutants.sh         mutation testing (cargo-mutants) with cache parking; see Testing TODO 6
  golden.sh          golden-image test on Mesa lavapipe (`--update` rewrites references; fetches
                     Arch's vulkan-swrast into target/golden-lavapipe/ if no lavapipe ICD)
  fuzz-<driver>.sh   run one fuzzer over one target:
                     `scripts/fuzz-{libfuzzer,honggfuzz,fuzzcheck,test-fuzz}.sh
                     <html|css|ftl> [seconds=60]` (details in each script's
                     header; they encapsulate the per-driver workarounds
                     described under Testing TODO 5)
fuzz/
  fuzz_targets/      cargo-fuzz targets (`html`, `css`, `ftl`) over `p23::fuzz`
  corpus/, artifacts/, coverage/  gitignored
honggfuzz/
  targets/           the same three harnesses driven by honggfuzz (`cargo +nightly hfuzz run <t>`)
  vendor/honggfuzz/  vendored honggfuzz crate (patched bfd.c for current binutils)
fuzzcheck/
  tests/             the same three harnesses as fuzzcheck `#[test]`s
                     (`cargo +nightly fuzzcheck --test <name> fuzz_<name> --stop-after-duration 60`)
  vendor/            vendored fuzzcheck 0.13 git-master (edition 2024; patched
                     `__llvm_prf_data` parser for the u64 record layout of
                     LLVM 21+; see the comment in `llvm_coverage.rs`)
  .cargo/config.toml linker override: the repo's clang+mold cannot link the
                     LLVM coverage section symbols (`__start___llvm_prf_*`)
test-fuzz/
  src/lib.rs         the same three harnesses as AFLplus targets via
                     `#[test_fuzz]` (`cargo +nightly test-fuzz tests::fuzz_html`
                     from this directory; corpus seeds come from plain
                     `cargo test` runs)
vendor/fluent-syntax/  vendored fluent-syntax 0.11.1 (patched char-boundary bug, see Gotchas)
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
- A stylesheet that *fails* to load emits no asset event; `build.rs` latches
  failures per entity (`FailedSheets`) and builds unstyled once (a failed
  per-entity `HtmlStylesheet` falls back to `DefaultStylesheet`). Styling
  with a ready sheet again, or the sheet reloading, clears the latch, so
  re-selecting a broken sheet fails fresh. Only *loading* stylesheets defer
  the build. Beware: re-requesting a failed asset can flip its state to
  `Loading` for a frame, eating that frame's change signal — don't rely on
  `is_changed` alone across a load-state transition.
- `vendor/fluent-syntax` is a patched fork (wired via `[patch.crates-io]` in
  the root, `fuzz/` and `honggfuzz/` manifests): upstream fluent-syntax 0.11.1
  panics slicing FTL source at byte ranges inside multi-byte characters
  (broken `\U` escapes etc.), reachable from any FTL asset. The patch clamps
  `Slice::slice` to char boundaries. Upstream hasn't fixed it and the version
  is pinned by bevy_fluent's fluent; re-check when bumping bevy_fluent.
- `vendor/honggfuzz` is a patched fork of the honggfuzz crate used only by
  `honggfuzz/` targets: its bundled C source fails against current binutils
  (`bfd.h` no longer defines `TRUE`).
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
  (re)load, stylesheet swap or (re)load (a failed sheet rebuilds unstyled
  once, per entity), a stylesheet's `border-image` image loading,
  `FontFamilies` change, outline marker added or removed. The UI isn't built
  while its stylesheet is loading (images may arrive later; their load
  rebuilds).
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
  data-l10n-args='{"n": 3}'>fallback</p>` on any element (block, container,
  inline, walked-through); the translation replaces the children, the
  element's own content is the fallback. Args are JSON (numbers stay numbers
  for plurals); a whole map can be one Tera variable. Translations are markup
  (`walk_translation` in `build.rs` parses them as a fragment): inline
  elements in them are styled; `data-l10n-name` elements take the
  same-named, same-tag source descendant's tag/id/classes (each usable once,
  otherwise plain text). In `.ftl`: `&lt;`/`&amp;` for literal `<`/`&`,
  `{"{"}`/`{"}"}` for braces. Deliberate differences from fluent-dom: no
  sanitizing (nested markup, `class`/`id`, any element kept), string args
  HTML-escaped, fluent-rs number formatting (no grouping). CJK paragraphs go
  on one line (a wrapped line becomes a stray space).
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
  default font, 16px. Missing font faces fall back as CSS font matching does
  (style before weight): bold-italic → italic → bold → regular.
- Layout properties (`LayoutDecl` in `cascade.rs`, applied by
  `LayoutDecl::apply_to` in `build.rs` over p23's defaults): `display`
  (`none`/`block`/`flex`), `flex-*`, `justify-content`, `align-*`, sizes
  (px/%/vw…/auto), margins (px/%/auto, over `li`'s 12px indent), `gap` both
  axes, `box-sizing` — on containers and on a block's outer node (the text
  node, or its box wrapper). Containers stay flex columns unless
  `flex-direction`/`display: block` says otherwise; blocks and containers
  keep `flex_shrink: 0`; nodes default to `BoxSizing::ContentBox`.
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
- Bevy's `Query::iter_descendants` is breadth-first; use `iter_descendants_depth_first` for document order (`HtmlElements`).
- HTML white-space collapsing covers ASCII whitespace only (space, `\t`, `\n`, `\f`, `\r`); `char::is_whitespace` would also eat NBSP/U+3000.
- Tests: `TestUi::settle()` demands a new build; for a change that legitimately rebuilds nothing (e.g. a `DefaultStylesheet` swap under a ready own `HtmlStylesheet`) use `settle_quiet()`.
- CLDR plural operand `n` is the absolute value: English `-1` selects `one`.
- Offscreen rendering (`tests/golden.rs`): pipelines compile asynchronously and
  a draw whose pipeline isn't ready is silently skipped; wait until the render
  world's `PipelineCache::waiting_pipelines()` is empty (disable
  `PipelinedRenderingPlugin` to inspect it inside `app.update()`). Without
  `App::run`, poll `plugins_state()`, then `finish()` and `cleanup()`.
- wgpu's GL backend isn't usable for software rendering (Bevy 0.19 doesn't
  enable wgpu `gles`; llvmpipe GL can't compile some of Bevy's GLSL): use
  lavapipe (Vulkan).
- `rustfmt tests/<file>.rs` also formats `tests/common/mod.rs` (it follows
  `mod common;`), whose code isn't rustfmt-formatted; use
  `--config skip_children=true`, or don't format.
- `TestUi::settle` never returns for a stylesheet whose `url()` image is
  missing (the sheet loads, its dependency fails); check sheets first (as
  `content_lint` does).
- A pseudo-locale opening marker must not be a text `[` at the start of a
  line (FTL reads a variant key); use a string-literal placeable `{"["}`.
- Bevy/taffy size the border box by default; p23 nodes set
  `BoxSizing::ContentBox` (CSS initial); `box-sizing` overrides.
- `cargo mutants` copies the source tree and ignores nested `.gitignore`s:
  park the fuzz drivers' `target/`/corpus dirs first and point `TMPDIR` at
  a disk with room (each job builds its own target dir).
- Headless UI layout (no window, no renderer, verified in `tests/common`):
  `UiPlugin` sizes roots from their camera's `computed.target_info`, which
  bevy_render's `camera_system` would fill — set it by hand on a camera with
  `RenderTarget::None { size }` + `IsDefaultUiCamera`. `UiPlugin`'s picking
  and focus systems then need `InputPlugin`, a `WindowPlugin` with
  `primary_window: None`, `DefaultPickingPlugins` and `TextureAtlasPlugin`;
  text measurement needs `TextPlugin`. Missing-resource panics name the
  system only with Bevy's `debug` feature (`--features bevy/debug`).
- Bevy's text measure (`TextMeasure::measure`) ceils every text node to whole
  pixels; browsers keep 1/64 px. With a fractional line height (16px × 1.2 =
  19.2) each text block runs up to 1px taller than in a browser and the
  drift accumulates down the page.

## Testing

Two layers, both deterministic and headless. Bugs found here (or by the
fuzzers/property harnesses below) are filed in `docs/agents/bugs/` — see
**Bug reports**.

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
  is `* { all: unset }` + `P23_CSS` (p23's defaults and layout model as CSS:
  flex-column root and containers, block blocks, Bevy's default font at line
  height 1.2, white text, `li` indent and bullet, `pre` 8px padding — keep in
  sync with `build.rs`) + `style.css` + `page.html`, so vectors must be plain
  HTML (no Tera / `data-l10n-id`; the script refuses them and skips Fluent
  vectors), set `color` on
  `html` (browsers default to black, p23 to white) and `border-style: solid`
  where widths matter. Deliberate differences (root background,
  `border-style`, `li` bullets) are skipped and listed above
  `FIXTURE_SIZE` in `tests/html_ui.rs`; add new ones there with a reason. The
  oracle's first run found two real bugs (unregistered `font-family` kept the
  inherited family; weight 501–599 wasn't bold).
- **Layout** (`TestUi::with_layout` / `from_layout_vector`, `layout_dump()`):
  Bevy UI really lays out headlessly against a fixed-size camera (see
  Gotchas). Text uses Bevy's embedded default font (FiraMono, 0.6em advance,
  printable ASCII only); no fake families. `layout_column_stacking` pins
  `layout_blocks` with hand-derived rects; the **layout oracle**
  (`layout_oracle`) compares every root/block/container border box of each
  `tests/vectors/layout_*/` with Chromium's `getBoundingClientRect` (recorded
  in `browser.json` by `scripts/browser_oracle.py`: 640px viewport — headless
  Chromium widens narrower windows — and the same FiraMono file, found via
  `cargo metadata`) within 1px per value. Layout vectors: no
  `font-family`, ASCII only (the script refuses others), font sizes whose
  1.2 line height is whole (Bevy ceils text nodes, see Gotchas). Vectors:
  `layout_blocks` (padding, borders, gap, wrapping, `pre`), `layout_mixed`
  (mixed inline content in a container, mixed font sizes on one line,
  `border-image` wrapper block, root border, nested containers). Both agree
  with Chromium to the pixel.
- **Unit tests** (`cargo test --lib`; `--features fuzzing` adds `src/fuzz.rs`):
  besides the cascade tests, `template.rs` (entity decoding: five escapes,
  one level only, others untouched; Tera-autoescape round trip proptest),
  `l10n.rs` (escape/decode identity, `data-l10n-args` typing: numbers stay
  numbers for CLDR plurals, bools → strings, bad JSON → error naming the
  message; only FSI/PDI stripped), `fonts.rs` (face fallback chain
  exhaustively, `resolve` list order / case / generics), `nine_slice.rs`
  (manifest defaults and errors), `html.rs` (`HtmlElements` document order),
  `fuzz.rs` (style cache transparency proptest).
- **Layout properties** (`tests/layout_properties.rs`): flexbox invariants over
  generated sizes on real headless layout, 1px tolerance: `row-reverse` mirrors
  `row`, `margin: auto` centers, zero-basis `flex-grow` splits by factor,
  `space-between` spreads evenly, wrapped items stay inside without overlap.
- **Content lint** (`tests/content_lint.rs`): checks the real `assets/`
  content; without `assets/` (gitignored) each content test prints a skip note
  and passes (checker unit tests always run). `PAGES` lists every template with
  the examples' context, stylesheets, bundle family and an optional
  `unlocalized` reason (`every_template_is_listed` keeps it complete). Checks:
  locale message/attribute parity with en-US; templates render for every
  context × stylesheet and every `data-l10n-id` resolves in every locale; no
  visible text outside `data-l10n-id` that keeps letters once context strings
  are removed; CSS parses and `url()`s name files; a generated `en-XA`
  pseudo-locale leaves no untranslated text run. New template → add to `PAGES`.
- **Golden images** (`tests/golden.rs`, `scripts/golden.sh`; `#[ignore]`d,
  so plain `cargo test` never needs a GPU): scenes in `tests/golden/<scene>/`
  (`text`: sizes, colors, wrapping, inline runs, background, `pre`; `frame`:
  root/container/block `border-image` stretch/round/% slices; `l10n`: Fluent
  markup, plurals, fallback) are rendered by full Bevy without a window
  (DefaultPlugins minus winit/audio/gilrs/pipelined rendering) into a
  `RenderTarget::Image`, captured with `Screenshot::image` and compared with
  `expected.png`. Committed inputs only (default font, `frame.png`). The
  script pins Mesa lavapipe via `VK_DRIVER_FILES` (an installed
  `lvp_icd*.json`, else Arch's `vulkan-swrast` matching `pacman -Q mesa`,
  fetched once into `target/golden-lavapipe/`, no root): bit-identical run to
  run. Tolerance: a pixel differs above channel delta 8; a scene fails above
  0.05% differing pixels (NVIDIA ≤5, RADV ≤1 delta vs lavapipe; one changed
  letter is ~0.1%). `scripts/golden.sh --update` rewrites references;
  mismatches write `target/tmp/golden/<scene>.{actual,diff}.png`.
- **Fluent oracle** (`fluent_oracle` test): every `tests/vectors/*/` with a
  `fluent.html` must build the same dump localized by p23 (`page.html` +
  `messages.ftl` as the en-US bundle, via `TestUi::from_vector`) as
  unlocalized from `fluent.html` — the DOM Fluent's reference bindings
  (`@fluent/dom` 0.10.2 + `@fluent/bundle` 0.19.1 in jsdom, `useIsolating:
  false` because p23 strips the isolation marks) produce from the same
  inputs, `data-l10n-*` attributes removed. Failures print a line diff
  (`-` fluent-dom, `+` p23). Regenerate after changing a vector:
  `scripts/fluent_oracle.sh [tests/vectors/<name>]`; fluent-dom's warnings
  (missing messages, sanitizer and name decisions) are recorded as comments
  at the top of `fluent.html`. Vectors: `fluent_basics` (plurals,
  selectors, terms, markup values, entities, missing-message fallback, `li`,
  `pre`), `fluent_overlays` (`data-l10n-name` edge cases, translated
  containers and inline elements). Deliberate differences stay out of
  oracle vectors and are pinned by the hand-written `fluent_permissive_markup`
  and `fluent_localization` vectors. Its first run found two missing
  features, now implemented: `data-l10n-name` overlays, and `data-l10n-id` on
  containers/inline elements (translations were silently dropped).

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

- [x] **1. Real browser as the oracle** (fixes weakness 1): CSS via
  `scripts/browser_oracle.py` + `browser_oracle` (vectors `cascade`,
  `box_model`, `units`); Fluent via `scripts/fluent_oracle.sh` +
  `fluent_oracle` (vectors `fluent_basics`, `fluent_overlays`). See Testing.
  - [ ] More oracle vectors as CSS support grows (every new property gets one).
- [x] **2. Metamorphic and property-based tests** (fixes weakness 6). Check
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
  - [x] Stateful sequences (`tests/stateful.rs`, `proptest_stateful`): random
    op chains (theme × {framed, plain, broken}, locale, context, outline
    toggle) against one long-lived `HtmlUi`, model-checked after every op.
    Found two real bugs the pairwise tests missed: outline *removal* never
    rebuilt (fixed via `RemovedComponents`), and a globally latched failed
    sheet swallowed a later re-select (latch is now per entity).
  - [x] `quickcheck` over structured inputs: a generated many-rule stylesheet
    checked against a reference CSS-precedence model (`cascade_winner_matches_
    precedence_model`), and arbitrary text round-tripping through Tera
    autoescaping (`context_text_round_trips_through_template`). The cascade
    property catches a specificity mutation that all cascade unit tests miss.
    Note: quickcheck 1.1's `Gen` RNG is private (edition-2024 `gen` keyword) —
    build `Arbitrary` impls from `T::arbitrary(g)` + `g.choose`;
    the `#[quickcheck]` attribute comes from `quickcheck_macros`.
  - [x] More metamorphic/model properties: selector list ≡ expanded rules ≡
    commented/duplicated sheet; color notations (`#rgb`/hex/`rgb()`/named);
    em/%/rem bases; 9-slice manifest sides; quickcheck selector lists with
    per-declaration `!important` over several elements; stateful ops for
    per-entity `HtmlStylesheet` set/remove and `FontFamilies` swaps. The
    stateful ops found three rebuild bugs (override removal never rebuilt; a
    failed override ignored default swaps; a re-requested failed sheet's
    one-frame `Loading` ate the swap signal).
  - [ ] Shrink quality: stateful ops shrink only by removal; proptest value
    shrinking inside an op isn't supported by the framework.
- [x] **3. Headless layout checks** (fixes weakness 3, mostly): Bevy UI
  layout runs headlessly (`TestUi::with_layout`); `layout_column_stacking`
  (hand-derived) and `layout_oracle` (Chromium rects) over `layout_*`
  vectors. See Testing.
  - [x] Flex layout, sizes, margins, `box-sizing`: vectors `layout_flex`,
    `layout_sizes` (match Chromium to the pixel); `P23_CSS` models p23's
    `flex-shrink: 0`; `tests/layout_properties.rs` checks flexbox invariants.
    The oracle found Bevy's border-box default (p23 now uses content-box).
- [x] **4. Lint tests over real content** (`tests/content_lint.rs`, see
  Testing): locale parity, templates render with the examples' data and every
  `data-l10n-id` resolves, no hard-coded text, CSS `url()`s, pseudo-locale run.
  No content bugs found; each check spot-checked by breaking the content.
  - [ ] Overflow: lay out the pseudo-locale (`TestUi::with_layout`) against a
    size budget per panel.
- [x] **5. Fuzzing** (robustness). Four drivers over the same
  `#[doc(hidden)]` `p23::fuzz` harness (feature `fuzzing`), which calls the
  internal glue directly — a full Bevy app is far too slow per exec.
  Contract for every target: no panic/hang/abort; errors are values.
  Easiest entry point: `scripts/fuzz-<driver>.sh <html|css|ftl> [seconds]`.
  - `cargo fuzz` (`fuzz/`, libFuzzer/ASan, nightly):
    `cargo +nightly fuzz run <target> -- -max_total_time=60`.
  - `honggfuzz-rs` (`honggfuzz/`, hardware-counter feedback; also nightly):
    `cargo +nightly hfuzz run <target>` — needs
    `HFUZZ_BUILD_ARGS='--manifest-path honggfuzz/Cargo.toml --config target.x86_64-unknown-linux-gnu.linker="cc"'`
    (the repo's `.cargo/config.toml` forces clang+mold, which fails to link
    the hfuzz runtime) and a time bound (`timeout 120`, or
    `HFUZZ_RUN_ARGS="--run_time 60"`; args after the target name reach the
    *target's* argv, not the driver). Data lands in `hfuzz_workspace/`
    (gitignored).
  - `fuzzcheck` (`fuzzcheck/`, its own coverage sensor over `-C
    instrument-coverage`; also nightly, installed via
    `cargo +nightly install cargo-fuzzcheck`):
    `cd fuzzcheck && cargo +nightly fuzzcheck --test <name> fuzz_<name>
    --stop-after-duration 60`. The vendored copy (git master, edition 2024)
    needed: a patched `__llvm_prf_data` parser (LLVM 21+ widened the record
    fields to u64 — NumCounters sits at offset 0x38 of the 72-byte record;
    verified against a `-C instrument-coverage` section dump) and the
    counter-file filter relaxed (cargo passes absolute source paths).
    `fuzzcheck/.cargo/config.toml` overrides the repo's clang+mold linker,
    which cannot resolve `__start___llvm_prf_*` section symbols.
  - `test-fuzz` (`test-fuzz/`, AFLplus via `cargo-afl`; nightly):
    `cargo +nightly test-fuzz tests::fuzz_html` (from `test-fuzz/`). Corpus
    seeds are collected by ordinary `cargo test` runs (the `#[test_fuzz]`
    macro writes arguments to the corpus); AFL needs
    `AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1 AFL_SKIP_CPUFREQ=1` on this
    machine (the core-pattern and cpu checks require `sudo`, which we don't
    have). Install with `cargo install cargo-test-fuzz cargo-afl` (both
    nightly). Data lands in `test-fuzz/target/` (gitignored).
  - [x] cargo-fuzz's first `css` run found a real crash: a selector with a
    non-ASCII first character panicked in `Compound::parse` (`&rest[1..]`
    byte-sliced past a multi-byte char). Fixed to skip the rule (p23 idents
    are ASCII-only by design); regression test
    `multibyte_selector_characters_are_skipped`.
  - [x] honggfuzz's first `ftl` run found an *upstream* crash: fluent-syntax
    0.11.1 slices parser source at byte ranges that can land inside a
    multi-byte character (e.g. FTL `u={"\" + U-escape + replacement char`).
    Reachable from any FTL asset; not fixed upstream and the version is
    pinned by bevy_fluent's fluent. Fixed by vendoring `vendor/fluent-syntax`
    with a boundary-safe `Slice::slice` (`[patch.crates-io]` in all three
    manifests); regression tests in `src/fuzz.rs`.
  - [x] The arbtest harness (`tests/arbtest.rs`) covers the pipeline end to
    end with arbitrary bytes; its first run found the failed-stylesheet hang
    (fixed in `build.rs`, see Gotchas).
  - [x] Structure-aware generation, HTML: `generated_documents_build_the_html_model`
    (arbtest) builds random well-formed documents and compares the built
    tree with an HTML model. Found `HtmlElements` iterating breadth-first
    (fixed: depth-first), and Unicode-whitespace collapsing of NBSP/U+3000
    (fixed: ASCII whitespace only). CSS/FTL grammars remain open.
- [ ] **6. Mutation testing** (measures vector strength): `scripts/mutants.sh`
  (`--full` adds the property suites; `--file src/x.rs` narrows). Slow (each
  mutant relinks Bevy test binaries: ~1 h default, ~4 h `--full`): run
  occasionally in the background; results in `target/mutants.out/` (not
  committed, overwritten per run — copy survivors here), reruns skip caught
  mutants (`--iterate`). The script parks the fuzz caches (cargo-mutants
  copies the tree, ignoring nested `.gitignore`s) and restores them on exit.
  - [ ] Complete a `--full` run (only 29 of 396 mutants ran so far, at
    `e1b910c` + working tree; line numbers below are from then).
  - [x] Triage the survivors of the partial runs (`e1b910c`; reruns with
    unit + html_ui + stateful). Killed by new tests:
    - `BoxStyle::is_empty` `&&`→`||` (background-only block lost its
      background): `background_only_block_keeps_its_background`.
    - `sliced_image` / `apply_nine_slices` dropping `VisualBox::BorderBox`:
      the dump prints `visual-box=` for frames not over the border box;
      `nine_slice::tests::frames_get_the_sliced_image_over_the_border_box`.
    - stylesheet reload arm deleted, and `!=`→`==` in the failed-latch
      cleanup (unrelated failed UIs rebuilt on any reload):
      `stylesheet_reload_rebuilds_only_its_users` (per-entity build counts).
    - image reload arm deleted: `image_change_reslices_percent_frames`.
    - `LoadState::Failed` guard → `true` (unstyled flash while loading):
      `loading_stylesheet_defers_the_build`.
    - `||`→`&&` before the outline check: already killed by the stateful
      machine (it was excluded from the first run).
    - `flex_shrink`/`box_sizing`/`margin`/`overflow` field deletions in
      `spawn_item`/`spawn_block`: layout cases in `layout_flex` (overfull
      narrow rows: containers, plain/boxed blocks, unsized text) and
      `layout_sizes` (width + padding blocks, sized `pre`); boxed `li`
      indent in the background test; the dump prints non-default `overflow`.
    The triage also found bug_0014 (text in boxed blocks never wrapped).
    Equivalent (documented, not killable by behaviour):
    - `build.rs` `Styler::box_of` `delete !` on `if !fill`: only decides
      whether a `debug!` line is logged.
    - `style.rs` `ParserOptions { filename }` deleted: only the file name in
      CSS parse-error messages.
  - Note — cargo-mutants 27.1.0 quirk: `--re`/`--exclude-re` don't filter
    struct-field-deletion mutants (they always run); harmless, but a
    targeted rerun takes ~10 min instead of ~2.
  - [ ] After the full run: triage any new survivors the same way.
  - [ ] Optionally run it in CI on a schedule (nightly), not per commit.
  - Proptest regression seeds written while planting bugs by hand
    (`tests/layout_properties.proptest-regressions`,
    `proptest-regressions/*.txt`) are already covered: proptest replays them
    first on every run.
- [x] **7. Golden images, sparingly** (fixes the rest of weakness 3 and part of weakness 4: real font rasterization, wrapping and 9-slice drawing; CJK fallback is still untested, since only committed fonts are used). `tests/golden.rs` + `scripts/golden.sh` on Mesa lavapipe; three scenes (`text`, `frame`, `l10n`); see Testing. Brittle across drivers and font versions: a smoke check, not a spec.
  - [ ] A CJK/fallback scene would need a committed CJK font (or `system_fonts`, which isn't reproducible).

## Limits and next steps

Known limits (each skipped/ignored value is logged at `debug`):

- **Layout:** flex layout, sizes, margins and `box-sizing` work on blocks and
  containers (see the CSS subset), not on the `html` rule (the `HtmlUi` node's
  `Node` stays the app's); no `position`, grid, `order`, `gap` in `%`, or
  font-relative lengths (`em`/`rem`) for layout. The `HtmlUi` node's default `Node` is a
  flex *row*, so apps must set `flex_direction: Column` themselves (as the
  guide's example does), or blocks sit side by side.
- **Lists:** `ul`/`ol` are plain columns; `li` draws a fixed `• ` with a
  hard-coded 12px indent; `ol` isn't numbered; no `list-style`.
- **Selectors:** compound only (type/`*` + `.class` + `#id`); no combinators
  (`div p`, `>`), attribute selectors, pseudo-classes (`:hover`) or
  pseudo-elements.
- **Properties:** no `text-align`, `line-height`, `letter-spacing`,
  `text-decoration`, `opacity`, `border-radius`, `border-color`/solid
  borders, `overflow`. Lengths: px/em/rem/% for `font-size`, absolute only
  for `padding`/`border-width`/`gap`; px/%/viewport units for sizes and margins.
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
  the five escapes Tera emits (no numeric references). Mixed inline content
  directly in a container (`<div>Mixed <b>bold</b> text</div>`, also from a
  translation) becomes one anonymous block per text piece, not one line.
- **Fluent:** numbers format without locale grouping (fluent-rs; bevy_fluent's
  shared bundle exposes no custom formatter) — pre-format in Tera if needed.
- **Fonts:** no bundled CJK font; Japanese relies on `system_fonts`. Bevy's
  default font covers printable ASCII only, so with no registered family even
  the `li` bullet (`•`) comes from font fallback.

Next steps (roughly in order of value):

1. ~~Flex layout from CSS~~ (done: flex, sizes, margins, `box-sizing`).
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
