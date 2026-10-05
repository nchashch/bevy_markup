# bevy_markup

Bevy 0.19 library: HTML templates (Tera) + Fluent + CSS → Bevy UI, with
9-slice frames. Dependencies are limited to that; don't add unrelated crates.

- `cargo run --example quickstart` — the API in one small app.
- `cargo run --example grid` — CSS grid layout (page grid, responsive slots).
- `cargo run --example demo` — everything: themes, languages, DOM outlines,
  scrolling, 9-slice frames.
- `cargo doc --open` — the user-facing documentation (crate docs = guide).

The library depends on `bevy` with `default-features = false` and only the
features its code uses (`ui_api`, `default_font`, `bevy_log`, `bevy_picking`
for interaction signals and `:hover`/`:active`; see `Cargo.toml`). Rendering,
windowing, picking backends, widgets and image formats are the
app's choice. Examples get full Bevy via `[dev-dependencies]`, whose features
never reach library users. A missing feature shows up in `cargo check --lib`.

## Public API (`bevy_markup::prelude::*`)

| Item | Kind | Role |
|---|---|---|
| `BevyMarkupPlugin` | Plugin | loaders, resources, systems; adds bevy_fluent's `FluentPlugin` if absent |
| `HtmlUiSystems::{Render, Localize, Build}` | SystemSet | chained in `PostUpdate`, before `UiSystems::Prepare` |
| `HtmlUi(Handle<HtmlTemplate>)` | Component | the UI; requires `Node`, `TemplateContext`, `RenderedHtml`, `LocalizedText` |
| `TemplateContext(tera::Context)` | Component | Tera variables; `.with(k, &v)` builder; Deref to `tera::Context`; mutate → re-render (rebuild only if the HTML changed) |
| `HtmlStylesheet(Handle<Stylesheet>)` | Component | per-entity stylesheet override |
| `HtmlDebugOutline` | Component | show the DOM outline (styled like `pre`) instead of the UI |
| `RenderedHtml` | Component | `Pending` / `Ready(HtmlDocument)` / `Failed(msg)` (read-only) |
| `HtmlElement { tag, id, classes }` | Component | on each spawned block and container node |
| `HtmlUiBuilt { entity }` | EntityEvent | after each (re)build; children are replaced every time, so wire behaviour here |
| `HtmlUiRestyled { entity }` | EntityEvent | after a style-only change applied in place (entities and attached components kept) |
| `HtmlElements` | SystemParam | `iter` / `by_id` / `by_class` / `by_tag` below an `HtmlUi` |
| `DefaultStylesheet(Option<Handle<Stylesheet>>)` | Resource | stylesheet for `HtmlUi`s without an override; swap = theme |
| `ActiveLocale(Option<Handle<BundleAsset>>)` | Resource | Fluent bundle; `None` = no localization; swap = language |
| `FontFamilies` / `FontFaces` / `GenericFamily` | Resource + types | CSS `font-family` name → faces: font files or system families (any `FontSource`; for system families bold/italic are requested via `TextFont` weight/style), + generic keyword mapping |
| `HtmlTemplate`, `Stylesheet`, `NineSlice` | Assets | `.html`/`.htm`, `.css`, `*.slice.ron` |
| `NineSliceFrame(Handle<NineSlice>)` | Component | 9-slice image as a node's border-box background (non-HTML nodes; HTML uses CSS `border-image`) |
| `BundleAsset` | Asset (bevy_fluent) | `*.ftl.ron` locale bundle |
| `HtmlCustomElementsExt::define_html_element(name, system)`, `ElementConnected { entity, root, name, dataset }` | App ext, system input | `is="<name>"` customized built-ins: the system (`In<ElementConnected>`, `data-*` attributes as `dataset`) runs on every spawn of the element, in document order, before `HtmlUiBuilt`; not on restyles |
| `HtmlAnchor { element, placement, gap }`, `AnchorPlacement::{Right, Left, Above, Below}` | Component | keeps an (absolute) overlay node beside `element` each frame: insets from its rect, clamped to the viewport by the overlay's size, `UiTargetCamera` copied, despawned with the element |
| `ElementSignal`, `ElementSignals`, `SignalBinding`, `SignalTrigger` | Message, Component | `data-on-<trigger>`/`data-with` hooks: buffered interaction signals (click/press/release/enter/leave); deepest bound element wins |
| `PseudoState { hovered, active, focused, focus_visible }` | Component | `:hover`/`:active` (from picking) and `:focus`/`:focus-visible` (from `InputFocus`/`InputFocusVisible`) per element; a change restyles in place; apps may set it |
| `Focusable { autofocus }` | Component | on focusable elements (`data-on-click` or `tabindex >= 0`, not `tabindex="-1"`) |
| `HtmlFocus` | SystemParam | `navigate(CompassOctant)` (shows focus; `FocusEdge` at an edge), `activate()`, `focused()` — the app binds its own input |
| `HtmlModal`, `HtmlNoFocus` | Component | on an `HtmlUi` root: confine focus to it while visible / never take focus |
| `FocusEdge { entity, direction }`, `ActivateElement { entity }` | EntityEvent | no neighbour in that direction / emit the element's click signals |

Re-exported crates (their types appear in the API): `tera`, `tl` (the
`astral-tl` fork, bug_0019 / UPSTREAM.md U11),
`bevy_fluent`, `lightningcss`. Cargo feature `system_fonts` enables Bevy's
`system_font_discovery`.

## Layout

```
README.md        human-facing overview: what bevy_markup is, how it works, testing strategy, Bevy
                 compatibility table (add a row on every release or Bevy/bevy_fluent bump; keep in sync)
LICENSE-MIT, LICENSE-APACHE  dual license (MIT OR Apache-2.0, Cargo.toml `license`)
src/
  lib.rs           crate docs (guide), BevyMarkupPlugin, HtmlUiSystems, prelude, re-exports
  html.rs          HtmlUi, TemplateContext, RenderedHtml, HtmlDebugOutline, HtmlElement,
                   HtmlUiBuilt, HtmlElements; render system (Tera + tl)
  template.rs      HtmlTemplate asset + loader, HtmlDocument (+ outline), decode_entities
  l10n.rs          ActiveLocale, LocalizedText; localize system (data-l10n-id/-args)
  style.rs         Stylesheet asset + loader, DefaultStylesheet, HtmlStylesheet; CSS subset docs
  cascade.rs       (internal) stylesheet → declared style per element (+ unit tests)
  fonts.rs         FontFamilies, FontFaces, GenericFamily
  build.rs         (internal) DOM + styles → Bevy UI children; HtmlUiBuilt trigger
  rebuild.rs       (internal) pure rebuild decision: Frame (load phases + change signals) → Build/Skip/Wait
  signals.rs       `data-on-*`/`data-with` → ElementSignal messages (picking observers, hover tracking)
  anchor.rs        HtmlAnchor/AnchorPlacement: place_anchored (PostUpdate, after Build, before
                   UiSystems::Prepare; reads the previous frame's layout)
  custom_elements.rs  `is="…"` + `data-*` dataset → app-defined systems run on spawn
                   (define_html_element, ElementConnected; CustomElements registry)
  focus.rs         Focusable/HtmlFocus/HtmlModal/HtmlNoFocus: focus scope sync, repair (id restore,
                   autofocus), focus pseudo-state, press-to-focus, activation (InputFocus-based)
  nine_slice.rs    NineSlice asset + loader, NineSliceFrame
examples/
  quickstart.rs    fonts, DefaultStylesheet, ActiveLocale, one HtmlUi, click wiring, Space = language
  grid.rs          CSS grid: page track template switched via context (Space), auto-fill slots
                   with a 2×2 span and dense packing, small grids in slots/stats; L = language
  demo/            main.rs (setup: fonts, window), shell.rs (the full-screen shell HtmlUi:
                   content slots, scroll wiring, contexts), controls.rs (language/theme
                   selection), consts.rs (fonts)
tests/
  html_ui.rs       headless test vectors: HTML/CSS/Fluent/Tera → world dump, + browser_oracle (see Testing)
  signals.rs       picking-driven signal tests: real WindowEvent input → Bevy picking → ElementSignal/PseudoState
                   (needs TestUi::with_pointer; see Testing)
  signal_properties.rs  proptest: random pages (nested hooks, pointer-events none) × random pointer-op
                   sequences vs a reference signal model over the real picking stack (see Testing)
  properties.rs    proptest metamorphic properties over the pipeline (shorthand=longhands, round trips,
                   selector lists/formatting/duplicates, color notations, relative font sizes, 9-slice manifests)
  quickcheck.rs    quickcheck structured-input properties (cascade vs reference models — single rule winner and
                   selector lists × per-declaration importance × several elements — and text round trip)
  arbtest.rs       arbtest: arbitrary bytes through the pipeline (robustness) + structure-aware random documents
                   vs an HTML model (node tree, collapsed text, HtmlElements order)
  stateful.rs      proptest-stateful machine: random op sequences (default theme, own HtmlStylesheet set/remove,
                   locale, context, outline toggle, FontFamilies swap, pointer moves/presses/releases) vs a
                   reference model (dump + ElementSignals)
  layout_properties.rs  flexbox invariants on real headless layout (row-reverse mirror, auto margins,
                   grow shares, space-between, wrap without overlap)
  content_lint.rs  lint tests over the examples' content in examples/assets/: locale parity,
                   templates render (+ every data-l10n-id resolves), no hard-coded text, CSS url()s, pseudo-locale
  golden.rs        golden images (#[ignore]d): tests/golden/<scene>/ rendered offscreen by real Bevy,
                   compared with expected.png within a tolerance (see Testing)
  fuzz_corpus.rs   replays fuzz/seeds/* + local fuzz/corpus/* through bevy_markup::fuzz (#[ignore]d, --features fuzzing) for coverage
  common/mod.rs    shared headless harness: TestUi (temp asset root, settle, dump; with_layout = Bevy UI layout)
  vectors/<name>/  file-based vectors: page.html, style.css, browser.json (CSS oracle output);
                   Fluent vectors add messages.ftl + fluent.html (Fluent oracle output);
                   layout_* vectors also get rects in browser.json (layout oracle)
  golden/<scene>/  page.html, style.css, [messages.ftl], expected.png (text, frame, l10n)
  fixtures/        frame.png (32×24)
.github/workflows/
  ci.yml           per push to main / PR: -D warnings check/build/doc, cargo test, fuzzing-feature
                   tests, Fluent oracle references current
  nightly.yml      golden images (Ubuntu lavapipe), browser oracle vs current Chrome, 15 min
                   cargo-fuzz per target, other fuzz drivers build (fuzz on pinned BEVY_MARKUP_NIGHTLY)
  mutants.yml      twice-weekly mutation testing, 4 shards, --in-place, fast settings, informational
  cache.yml        builds the shared `mutants` and `fuzz` dependency caches once (push to main,
                   daily); scheduled jobs only restore them (see Testing, CI)
scripts/
  browser_oracle.py  headless Chromium → tests/vectors/*/browser.json (stdlib Python only)
  fluent_oracle.sh   @fluent/dom in jsdom → tests/vectors/*/fluent.html (Node + npm;
                     pinned packages in fluent-oracle/, node_modules gitignored)
  mutants.sh         mutation testing (cargo-mutants), fast settings + cache parking; see Testing TODO 6
  coverage.py        line coverage of src/ per testing layer and merged (cargo-llvm-cov); see Testing
  golden.sh          golden-image test on Mesa lavapipe (`--update` rewrites references; fetches
                     Arch's vulkan-swrast into target/golden-lavapipe/ if no lavapipe ICD)
  fuzz-<driver>.sh   run one fuzzer over one target:
                     `scripts/fuzz-{libfuzzer,honggfuzz,fuzzcheck,test-fuzz}.sh
                     <html|css|ftl> [seconds=60]` (details in each script's
                     header; they encapsulate the per-driver workarounds
                     described under Testing TODO 5)
fuzz/
  fuzz_targets/      cargo-fuzz targets (`html`, `css`, `ftl`) over `bevy_markup::fuzz`
  seeds/<target>/    committed minimized seed corpora (read-only extra input to every fuzz run)
  corpus/, artifacts/, coverage/  gitignored (corpus/ = working corpus; CI carries it over nightly)
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
examples/assets/   the examples' content (AssetPlugin file_path; no fonts: system fonts)
  src/             artwork sources (frame.kra) for the UI images
  quickstart/      hello.html, style.css (html rule: border-image frame), locales/{en-US,de}
  grid/            grid.html, style.css (all of the grid example's layout), locales/{en-US,de}
  ui/frame.png     256x256 frame; frame_transparent.png (clear center; the
                   parchment/terminal panels frame themselves with it)
  ui/themes/       demo CSS themes: crimson (default), parchment (framed `pre` via border-image longhands), terminal, large_print;
                   each carries the identical `Demo chrome` block styling the shell's panels/buttons/viewports
  ui/content/      test.html (plain), inventory.html (Tera), l10n.html (Tera + Fluent),
                   shell.html (the demo app itself: panels, buttons, slots), test.css
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
- When to update is decided by the pure `rebuild` module (`RebuildState`,
  a component every `HtmlUi` requires): `build_html_ui` only gathers a
  `Frame` per entity (own/default stylesheet load phase, document ready,
  change signals) and acts on the `Decision` (`Build`/`Restyle`/`Skip`/
  `Wait`). Change update triggers there, and unit-test them in
  `rebuild::tests`, not via full apps. A stylesheet that *fails* to load
  emits no asset event: a change that arrives while a needed sheet loads is
  kept pending and updates the frame the sheet resolves (ready or failed) —
  that's what turns a failure into an (unstyled) update, and why a failed
  `HtmlStylesheet` falls back to `DefaultStylesheet` without a separate
  latch. Re-requesting a failed asset can flip its state to `Loading` for a
  frame, so never read change signals only after resolving load states.
- Build vs restyle (`build.rs`): items become a `NodeSpec` tree (plain data:
  `Node`, element, background, frame, text spans), which is either spawned
  (`spawn_spec`, `HtmlUiBuilt`) or, for style-only changes, applied onto the
  existing children in place (`apply_spec`, `HtmlUiRestyled`) when they have
  the same shape (`same_shape`: text/element/frame presence, span and child
  counts) — otherwise it rebuilds. Restyles keep entities and app-attached
  components. Custom elements (`is`): `spawn_spec` collects an
  `ElementConnected` per spawned `is` element; the build queues
  `custom_elements::connect` for each (document order) after the spawn
  commands and before the `HtmlUiBuilt` trigger, so `HtmlUiBuilt` observers
  see what definitions attached; `apply_spec` (restyle) never runs them.
  An undefined name warns once per name. Shape compares only what the spec owns: a frame is the
  `CssFrame` marker spawned with its `ImageNode` (an app `ImageNode` is app
  state, bug_0018), nested `HtmlUi` children are skipped (bug_0017).
  `restyle_matches_a_fresh_build` (properties) guards that both
  paths agree; keyed reconciliation can later extend `NodeSpec` with keys.
- fluent-syntax comes from a fork, `nchashch/fluent-rs` branch
  `fix/fuzzing-bugs-0.11` (upstream's 0.11.1 tag plus fixes; Cargo.lock pins
  the commit), wired via `[patch.crates-io]` in the root and all four fuzz
  manifests. It fixes two bugs reachable from any FTL asset:
  - bug_0005: an invalid `\U` escape before a multi-byte character panicked
    (the error quoted the escape plus one *byte* of the next character).
  - bug_0015: expression nesting recursed without a limit (placeables *and*
    call arguments), so a few KB of `{{{…}}}` or `F(F(…))` overflowed the
    stack; `get_inline_expression` now fails past `MAX_NESTING_DEPTH` = 100
    with `ErrorKind::NestingTooDeep` (the entry becomes Junk).
  The same fixes are proposed upstream from branch `fix/fuzzing-bugs` (on
  upstream `main`, 0.12). `[patch]` doesn't reach crates that depend on
  bevy_markup: they need the same entry (README). Re-check when bumping
  bevy_fluent; drop the patch once upstream releases the fixes in the
  fluent-syntax version bevy_fluent uses (UPSTREAM.md U1/U2).
- `vendor/honggfuzz` is a patched fork of the honggfuzz crate used only by
  `honggfuzz/` targets: its bundled C source fails against current binutils
  (`bfd.h` no longer defines `TRUE`).
- Demo typography: system fonts (Bevy `system_font_discovery`, a
  dev-dependency feature) — headers `sans-serif` (red), body `serif`
  (off-white), code/debug `monospace`. CSS themes use the generic keywords;
  the examples map them to `FontSource::{SansSerif, Serif, Monospace}`
  families (`FontFaces::new(FontSource::Serif)` etc.,
  `examples/demo/consts.rs`).

## Pipeline

```
.html (Tera) --render(TemplateContext)--> HTML --tl--> DOM            [Render]
    --data-l10n-id × ActiveLocale--> LocalizedText                     [Localize]
    --CSS (HtmlStylesheet | DefaultStylesheet) × FontFamilies--> UI    [Build] → HtmlUiBuilt
```

- Templates: compiled at load (syntax errors fail the load); name = asset path,
  so `.html` gets Tera HTML autoescaping. Plain HTML renders to itself.
- Update triggers: rebuild on template/context change or reload when the
  rendered HTML differs from the previous render (`render_templates` compares
  `HtmlDocument::source`; an identical render leaves `RenderedHtml`
  untouched, a repeated identical error is logged once), locale
  change or bundle (re)load, outline marker added or removed; restyle on
  stylesheet swap or (re)load (a failed sheet updates unstyled once, when it
  resolves), a stylesheet's `border-image` image loading, `FontFamilies`
  change. The UI isn't built while its stylesheet is loading (images may
  arrive later; their load restyles).
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
  `#id` parts, comma lists, plus `:hover`/`:active`/`:focus`/`:focus-visible`; combinators/attributes/
  other pseudo-classes skipped at `debug`), matched per element (`HtmlElement`
  tag/id/classes + `PseudoState`, cached per combination) with CSS
  precedence: `!important`, then specificity (ids,
  classes, type), then source order. Inherited `color`, `font-family` (first name registered in
  `FontFamilies`, generics via `set_generic`; no registered name → Bevy's
  default font, as browsers do), `font-size` (`px`, `em`/`%`, `rem`, keywords
  with medium = 16px, `smaller`/`larger`), `font-weight` (bold above 500, CSS
  font matching), `font-style`, `pointer-events` (`none`/`auto`; arrives as
  an unknown property — lightningcss has no typed one); blocks:
  `background-color`. `html` is the
  starting point even without `<html>`; nothing declared → white, Bevy's
  default font, 16px. Missing font faces fall back as CSS font matching does
  (style before weight): bold-italic → italic → bold → regular.
- Layout properties (`LayoutDecl` in `cascade.rs`, applied by
  `LayoutDecl::apply_to` in `build.rs` over bevy_markup's defaults): `display`
  (`none`/`block`/`flex`/`grid`), `flex-*`, `justify-content`, `align-*`,
  `justify-items`/`-self`, sizes (px/%/vw…/auto), margins (px/%/auto, over
  `li`'s 12px indent), `gap` both axes, `box-sizing` — on containers and on a
  block's outer node (the text node, or its box wrapper). Containers stay flex
  columns unless `flex-direction`/`display: block`/`display: grid` says
  otherwise; blocks and containers keep `flex_shrink: 0`; nodes default to
  `BoxSizing::ContentBox`. Grid (onto Bevy's `Node` grid fields, laid out
  by taffy): track lists (`grid-template-*`, `grid-template`, `grid`;
  lengths/%/viewport units, `fr`, `auto`, `min-`/`max-content`, `minmax()`,
  `fit-content()`, `repeat()` incl. `auto-fill`/`auto-fit`; one unsupported
  track drops the declaration), `grid-auto-rows`/`-columns`/`-flow`, and
  numeric line/`span` placement (`grid-row`/`-column` + longhands,
  `grid-area`; `GridLineDecl` pairs → `GridPlacement`). Named lines and
  `grid-template-areas` are skipped (Bevy places by number only). Bare
  `grid-auto-flow: dense` fails lightningcss's parser (UPSTREAM.md U10).
  `position` (`CssPosition`: static/relative/absolute) + insets
  (`top`/`right`/`bottom`/`left`/`inset`), and `border-radius` corners, are
  `LayoutDecl` too: `static`/undeclared leaves `PositionType::Relative` with
  insets at `auto` (CSS ignores insets there); Bevy places `absolute` in the
  parent's padding box (every parent acts positioned).
- Element components (`build.rs` `apply_css_owned`): `border-color` →
  `BorderColor`, `z-index` → `ZIndex`, computed `pointer-events: none` →
  `Pickable::IGNORE` (also on a boxed block's inner text node, and on the
  block's `TextSpan`s: bevy_picking resolves text hits against the span
  entity, so an ignored block's spans must ignore too, bug_0020). An app may
  set these itself, so `CssOwned` records what CSS set and a restyle without
  the declaration resets only that (`BorderColor`/`ZIndex` are `Node`'s
  required components: reset to default, never removed).
- Box properties (`cascade.rs` → `build.rs`): `border-image` (shorthand +
  `-source`/`-slice`/`-repeat`), `border-width`, `padding` (absolute lengths)
  on blocks, containers and the root rule; `background-color` and
  `gap`/`row-gap` on containers and the root rule, `background-color` also on
  blocks.
  `url()` resolves relative to the `.css`; the `Stylesheet` loader loads the
  images as dependencies. Maps to Bevy's sliced `ImageNode`
  (`VisualBox::BorderBox`) + `Node::border`/`padding`. Slice numbers = image px,
  `%` = of image size (needs the image loaded). `stretch` → Stretch, other
  repeats → `Tile`; one mode for all sides. Bevy always draws the center;
  `border-image-width`/`-outset` ignored (corners at image size). Blocks with
  box properties get a wrapper node (a node can't be both `Text` and
  `ImageNode`); the `HtmlElement` is on the wrapper.
- Root rule (`build.rs` `apply_root`, queued per build/restyle): the `HtmlUi`
  entity is styled as the document's top-level `<html>` element (`root_element`:
  its `id`/`class`; a bare `html` for fragments, outline mode and failures) —
  box, `gap`, the whole `LayoutDecl` on its `Node`, and `background-color`,
  `border-color`, `z-index` (`ZIndex`: Bevy sorts roots by `(GlobalZIndex,
  ZIndex)`), `pointer-events: none` (`Pickable::IGNORE`), `border-image`
  (`ImageNode`) as components. `CssRoot` keeps the app's `Node` (`base`) and
  what was declared; each application first restores the previously declared
  fields from `base` (`LayoutDecl::restore_from`, `restore_sides`), takes the
  result as the new `base`, then applies the new declarations — so undeclared
  fields the app changes (per-frame positioning) are never touched. Components
  go through `claim`: the app's value is saved the first time CSS sets one and
  given back (or removed) when CSS stops. Nested containers' default `gap` is
  the root's effective `row-gap`.
- Demo locales: every locale needs the same message ids; item names arrive as
  English data (`$item`) and non-English bundles map them with an `item-name`
  message. Every visible string in `l10n.html` and the demo shell has a key.

## Gotchas (verified)

- Bevy `ImageNode` defaults to `VisualBox::ContentBox` (draws inside padding);
  frames need `BorderBox` (`NineSliceFrame` sets it).
- Scrollable panels (demo `shell.rs`): a `.viewport` element gets `ScrollArea` +
  `overflow: scroll_y` wired on each build (bevy_markup's CSS subset has no
  `overflow`, so scrollability is app behaviour, like clicks); its CSS gives it
  `flex-grow: 1` and `min-height: 0` — else its content sizes it and nothing
  scrolls. Viewport children need `flex_shrink: 0.0` (`HtmlUi` blocks set it).
  For hand-built scrollbars, the `Scrollbar` must be a *sibling* of the
  viewport. A framed node that scrolls itself needs `overflow_clip_margin:
  OverflowClipMargin::content_box()`.
- Every UI `Node` is pickable and blocks pointer input below it. Invisible
  layout-only wrappers must not catch the pointer: `pointer-events: none` in
  CSS (or `Pickable::IGNORE` on app nodes) — a full-window wrapper once broke
  all scrolling and clicking.
- Nested `HtmlUi`s: an ancestor's rebuild replaces its whole subtree and
  despawns nested UIs with it — spawn them into slots on `HtmlUiBuilt`
  (the demo's `wire_shell_build`). The build system skips a nested UI whose
  ancestor rebuilds the same frame (bug_0016): its own queued commands would
  otherwise hit the despawned entity and panic. Restyles don't despawn:
  `same_shape` ignores nested-UI children (bug_0017), so a restyle keeps
  them and their app-attached state.
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
- API changes that touch `src/fuzz.rs`'s imports (`Styler`, `cascade`) need
  `cargo check --lib --features fuzzing`: the module only compiles under the
  feature, and plain `cargo test`/`check` don't see it (CI runs the
  fuzzing-feature tests).
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
- Bevy/taffy size the border box by default; bevy_markup nodes set
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

How-to guide for writing and running tests and hunting bugs across the
whole harness: `docs/agents/skills/testing.md`. This section is the
reference and history. Bugs found here (or by the fuzzers/property
harnesses below) are filed in `docs/agents/bugs/` — see **Bug reports**.

- **Cascade unit tests** (`src/cascade.rs`, `cargo test --lib`): CSS text →
  declared style for an `HtmlElement` (specificity, compound matching, comma
  lists, importance, unsupported selectors).
- **Test vectors** (`tests/html_ui.rs`, `cargo test --test html_ui`): input
  files (HTML/Tera template, CSS, Fluent bundles) → the Bevy world they
  produce. `TestUi::new(name, files)` writes the files plus `frame.png` into a
  fresh temp asset root and builds `MinimalPlugins + AssetPlugin + ImagePlugin
  + BevyMarkupPlugin` (no window/renderer; fonts are fake `Handle::Uuid`s labelled
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
  is `* { all: unset }` + `BEVY_MARKUP_CSS` (bevy_markup's defaults and layout model as CSS:
  flex-column root and containers, block blocks, Bevy's default font at line
  height 1.2, white text, `li` indent and bullet, `pre` 8px padding — keep in
  sync with `build.rs`) + `style.css` + `page.html`, so vectors must be plain
  HTML (no Tera / `data-l10n-id`; the script refuses them and skips Fluent
  vectors), set `color` on
  `html` (browsers default to black, bevy_markup to white) and `border-style: solid`
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
- **Signals** (`tests/signals.rs`, `cargo test --test signals`): interaction
  end to end over the real picking stack. `TestUi::with_pointer` extends the
  layout harness with a primary `Window` entity and aims the camera at it
  (`RenderTarget::Window(WindowRef::Primary)`; the camera's `computed.target_info`
  is still set by hand, so layout is unchanged). The tests then write
  `WindowEvent::CursorMoved`/`MouseButtonInput` messages — what winit sends —
  one action per frame (`move_pointer`/`press_pointer`/`release_pointer`/
  `click_at`; a click spans three frames, like a real one), and Bevy's full
  chain runs: `PointerInputPlugin` → UI picking backend → `Pointer<Click>`-style
  entity events (bubbling through text spans) → `HoverMap` → bevy_markup's
  observers, `hover_signals` and `update_pseudo_states`. Assertions read the
  drained `ElementSignal`s (`take_signals`) and `PseudoState`s, plus element
  rects (`node_rect`) for click positions. Vectors: click/press/release with
  payload and position, deepest-bound-element-wins (nesting), enter/leave over
  a subtree, the despawn-leave after a rebuild under a stationary cursor,
  hover/active pseudo states, `pointer-events: none` (bug_0020), inline hooks
  staying dead.
  Gotchas learned here: Bevy's message updates are gated on fixed ticks
  (`signal_message_update_system` runs in `FixedPostUpdate`), and at ~1 ms
  test frames those are rare — per-cycle message iteration
  (`iter_current_update_messages`) re-serves old signals, so the harness
  drains both buffers (`Messages::drain`). Cursor-based `MessageReader`s
  (the real picking path) are unaffected. Pressing a focusable element
  focuses it (press-to-focus), so `PseudoState.focused` is set after a click
  even where the test only cares about hover/active.
- **Signal properties** (`tests/signal_properties.rs`, also in the
  coverage script's `signals` layer): random pages of bound containers
  (nested up to two levels, random trigger subsets, random
  `pointer-events: none` subtrees) × random pointer-op sequences
  (move/press/release, 1–12 ops), compared op by op with a reference model
  of the real semantics: deepest non-ignored node owns the hit and the
  deepest chain element bound for a trigger emits it; enter/leave track the
  chain's covered set; a release clicks only if the hovered node is (still)
  pressed — bevy_picking keeps presses across releases — and always runs
  the hovered chain's release hooks. Pages are text-free so hit chains are
  pure rect containment (the text-span path is pinned by `tests/signals.rs`).
  Both model halves were spot-checked by breaking the library (disabling
  `covered_by_deeper`, cutting the hover ancestor walk): the property fails
  under each and passes restored.
- **Content lint** (`tests/content_lint.rs`): checks the examples' content
  in `examples/assets/` (committed, so it runs everywhere, CI included).
  `PAGES` lists every template with
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
  `fluent.html` must build the same dump localized by bevy_markup (`page.html` +
  `messages.ftl` as the en-US bundle, via `TestUi::from_vector`) as
  unlocalized from `fluent.html` — the DOM Fluent's reference bindings
  (`@fluent/dom` 0.10.2 + `@fluent/bundle` 0.19.1 in jsdom, `useIsolating:
  false` because bevy_markup strips the isolation marks) produce from the same
  inputs, `data-l10n-*` attributes removed. Failures print a line diff
  (`-` fluent-dom, `+` bevy_markup). Regenerate after changing a vector:
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
    toggle, `FontFamilies` swap, pointer moves/presses/releases over a bound
    button — via `TestUi::with_pointer`, real `WindowEvent` input) against
    one long-lived `HtmlUi`, model-checked after every op: dump *and*
    `ElementSignal`s (`SignalLog`, a durable `Last`-schedule reader, since
    buffered messages expire across a multi-frame `settle()`). The pointer
    model covers rebuilds and restyles under a stationary cursor: content
    changes replace every element, so a hovered button leaves (from the
    enter snapshot) and its replacement enters; theme swaps shift the root
    box and move the button out from under the pointer; the unstyled button
    is 0×0 and unhittable.
    Found two real bugs the pairwise tests missed: outline *removal* never
    rebuilt (fixed via `RemovedComponents`), and a globally latched failed
    sheet swallowed a later re-select (since replaced by the pending-change
    rule in `src/rebuild.rs`).
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
    per-entity `HtmlStylesheet` set/remove and `FontFamilies` swaps; random
    pages × pointer-op sequences vs a signal reference model
    (`tests/signal_properties.rs`, over the real picking stack). The stateful
    ops found three rebuild bugs (override removal never rebuilt; a
    failed override ignored default swaps; a re-requested failed sheet's
    one-frame `Loading` ate the swap signal).
  - [ ] Shrink quality: stateful ops shrink only by removal; proptest value
    shrinking inside an op isn't supported by the framework.
- [x] **3. Headless layout checks** (fixes weakness 3, mostly): Bevy UI
  layout runs headlessly (`TestUi::with_layout`); `layout_column_stacking`
  (hand-derived) and `layout_oracle` (Chromium rects) over `layout_*`
  vectors. See Testing.
  - [x] Flex layout, sizes, margins, `box-sizing`: vectors `layout_flex`,
    `layout_sizes` (match Chromium to the pixel); `BEVY_MARKUP_CSS` models bevy_markup's
    `flex-shrink: 0`; `tests/layout_properties.rs` checks flexbox invariants.
    The oracle found Bevy's border-box default (bevy_markup now uses content-box).
  - [x] CSS grid: vector `layout_grid` (fixed/%/`fr`/content-sized tracks,
    spans, explicit and swapped line placement, implicit rows, `auto-fill`
    with `minmax()`, column dense flow, `grid-area`, item alignment, container
    and boxed-block items) matches Chromium to the pixel; the mapping is
    pinned per value by `cascade::tests::grid_*`.
  - [x] Focus: `focus_navigation_scope_and_styles` (focusability incl.
    `tabindex`, `autofocus`, `:focus`/`:focus-visible` + `outline` restyles,
    activation, restore by `id` across a rebuild, modal confinement,
    `HtmlNoFocus`; restore and modal spot-checked by ablation) and
    `html_focus_navigates_by_layout_and_reports_edges` (layout harness; it
    propagates `InheritedVisibility` itself, which Bevy's navigator needs).
  - [x] Positioning: vector `layout_position` (`absolute` by `top`/`left` and
    by `right: %`/`bottom`, `relative` offset, `static` ignoring insets, plus
    `z-index`/`border-radius`/`border-color` that must not move anything)
    matches Chromium to the pixel (spot-checked by dropping the `top`
    mapping); component mapping and the CSS-owned reset are pinned by
    `positioning_radius_border_color_z_index_and_pointer_events`
    (`tests/html_ui.rs`, dump keys `pos=` `inset=` `radius=` `bcolor=` `z=`
    `pick=none`) and `cascade::tests::{position_and_insets,
    radius_border_color_z_index_and_pointer_events}`.
- [x] **4. Lint tests over real content** (`tests/content_lint.rs`, see
  Testing): locale parity, templates render with the examples' data and every
  `data-l10n-id` resolves, no hard-coded text, CSS `url()`s, pseudo-locale run.
  No content bugs found; each check spot-checked by breaking the content.
  - [ ] Overflow: lay out the pseudo-locale (`TestUi::with_layout`) against a
    size budget per panel.
- [x] **5. Fuzzing** (robustness). Four drivers over the same
  `#[doc(hidden)]` `bevy_markup::fuzz` harness (feature `fuzzing`), which calls the
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
    byte-sliced past a multi-byte char). Fixed to skip the rule (bevy_markup idents
    are ASCII-only by design); regression test
    `multibyte_selector_characters_are_skipped`.
  - [x] honggfuzz's first `ftl` run found an *upstream* crash: fluent-syntax
    0.11.1 slices parser source at byte ranges that can land inside a
    multi-byte character (e.g. FTL `u={"\" + U-escape + replacement char`).
    Reachable from any FTL asset; not fixed upstream and the version is
    pinned by bevy_fluent's fluent. Fixed in the fluent-syntax fork (first
    vendored as `vendor/fluent-syntax`; see Gotchas); regression tests in
    `src/fuzz.rs`.
  - [x] The arbtest harness (`tests/arbtest.rs`) covers the pipeline end to
    end with arbitrary bytes; its first run found the failed-stylesheet hang
    (fixed in `build.rs`, see Gotchas).
  - [x] Structure-aware generation, HTML: `generated_documents_build_the_html_model`
    (arbtest) builds random well-formed documents and compares the built
    tree with an HTML model. Found `HtmlElements` iterating breadth-first
    (fixed: depth-first), and Unicode-whitespace collapsing of NBSP/U+3000
    (fixed: ASCII whitespace only). CSS/FTL grammars remain open.
- [ ] **6. Mutation testing** (measures vector strength): `scripts/mutants.sh`
  (default: unit + html_ui + stateful, like CI; `--full` adds the property
  suites; `--file src/x.rs` narrows). Results in `target/mutants.out/` (not
  committed, overwritten per run — copy survivors here), reruns skip caught
  mutants (`--iterate`). The script parks the fuzz caches (cargo-mutants
  copies the tree, ignoring nested `.gitignore`s) and restores them on exit.
  Speed (measured): a mutant costs ~4–8 s build + ~0.5 s tests (was 45 s +
  9 s locally, 194 s + 39 s on CI), after a one-time ~150 s dependency build
  per job — a full run is minutes, not hours. Three settings, each measured:
  - Only the tests that run get built: target flags go in `--cargo-arg`.
    `--cargo-test-arg` reaches only the test run; cargo-mutants' build step
    (`cargo test --no-run`) then links every test binary and example (11
    Bevy links per mutant; 28 s → 3.6 s).
  - The `mutants` cargo profile (`Cargo.toml`): no debug info (it dominates
    Bevy link times) and dependencies at `opt-level = 3` (tests ~40%
    faster; Bevy's recommended dev setting, scoped to mutation runs).
  - Bevy linked dynamically (`--cargo-arg=--features=bevy/dynamic_linking`,
    the Bevy guide's biggest fast-compile win): each relink is ~1 s.
  With these, the build is 0.8 s per mutant in a warm tree. The Bevy guide's
  nightly options don't help (measured, same settings, rebuild per mutant):
  stable 1.03 s; nightly 2.03 s; + `-Zshare-generics` 2.08 s; + Cranelift
  for bevy_markup (LLVM for deps) 1.97 s; both 2.03 s. Share-generics only trims
  the one-time build (150 s → 128 s). `-j`/`--minimum-test-timeout` are set by
  the script: use `BEVY_MARKUP_MUTANTS_JOBS` / `CARGO_MUTANTS_MINIMUM_TEST_TIMEOUT`.
  No mold on CI (measured, not worth it): rustc ≥ 1.90 already links
  through its bundled `rust-lld` on x86_64 Linux (`-fuse-ld=lld` + its own
  `gcc-ld` dir), so `rui314/setup-mold` — which swaps `/usr/bin/ld` — never
  takes effect, and forcing mold via `RUSTFLAGS` measured the same as lld
  without debug info (4.6 s vs 4.2 s rebuild of all targets, 68 s vs 65 s
  cold). The main CI job is ~3–4 min with a warm cache (build 37 s, tests
  ~38 s).
  - [ ] Complete a `--full` run locally, or read the CI results (twice weekly)
    (`mutants.yml`: unit + html_ui + stateful, 4 shards; survivors in each
    shard's job summary and the `mutants-shard-N` artifacts). Line numbers
    below are from `e1b910c`.
  - [x] First optimized CI run (2026-10-04, run 37236608548, 4 shards of
    105 mutants: 28–49 min each). Per mutant: build median 4.7–8.0 s
    (was 194 s), tests 0.3–0.7 s (was 39 s) — all mutants of a shard take
    9–19 min. The rest is the one-time baseline build (18–30 min): a
    cache miss, because rust-cache's key includes an environment hash and
    the `env:` block had changed. Since then `cache.yml` builds the
    `mutants` cache once per dependency change and restores it daily
    (beating the 7-day eviction); the shards only restore it (see the CI
    notes below). Survivors: the 6 documented equivalents plus `l10n.rs`
    `localize` `delete !` (turning localization off left stale
    translations; the state machine only caught it by chance): now killed by
    `turning_localization_off_restores_own_content`.
  - [x] Triage the first CI run (2026-10-04, run 37223937948: 420 mutants;
    278 caught, 51 missed, 46 unviable across 8 shards of 25–44 min). All 51
    rerun against the full suite (+ properties, quickcheck,
    layout_properties): 9 were already caught there. New tests kill 36 more:
    - `cascade.rs` property mapping (border sides, flex-flow/-shrink/flex,
      align-content, max-height, margin sides, `display: flex`, `auto`/
      `none` sizes, generic families, weight 500/501 boundary,
      `border-image-source` longhand and `none`): table-driven unit tests
      (`layout_properties_map_to_bevy_values`, `border_sides_…`,
      `font_weight_boundary_is_above_500`,
      `generic_family_keywords_map_one_to_one`,
      `border_image_sources_none_and_longhands`).
    - `same_shape` (nested shape change, frame removed, merged runs):
      `restyles_that_change_shape_match_a_fresh_build`.
    - `apply_root_box` `had_image` guard: `root_box_keeps_the_apps_own_image`.
    - loader `extensions()` (css/html/htm/slice.ron):
      `loaders_are_found_by_extension` (untyped loads).
    - `HtmlTemplate::name`: `template_is_named_after_its_asset_path`;
      `error_chain`: `error_chain_includes_all_sources`; outline indentation
      and blank-text skipping: `outline_indents_levels_and_shows_translations`;
      `From<tera::Context>`: `template_context_from_tera_keeps_variables`.
    - `apply_nine_slices` reload arm and `!` guard:
      `style_reload_reapplies_to_unchanged_frames` (registered system, so
      change ticks persist).
    Equivalent (documented; 6 left, all behaviour-neutral):
    - `build.rs` `box_of` `delete !` on `if !fill`, `cascade.rs`
      `BorderImageWidth/Outset` arm deleted, `Image::None` arm deleted,
      `unsupported` → `None`: each only changes whether a `debug!` line is
      logged (the fallback arm returns the same value).
    - `style.rs` `ParserOptions { filename }`: only the file name in CSS
      parse-error messages.
    - `build.rs` `same_shape` top check `||` → `&&`: every shape change bevy_markup
      produces also changes the child/span structure checked next, so a
      wrong top-level answer is rejected one level down (redundant check).
    Gotcha: under `-j 6` with the full suite, two mutants hit the 60 s test
    timeout from load alone; rerun timeouts alone with
    `CARGO_MUTANTS_MINIMUM_TEST_TIMEOUT=300` (the script already passes the
    flag, which can't be repeated).
  - [x] Triage the survivors of the partial runs (`e1b910c`; reruns with
    unit + html_ui + stateful). Killed by new tests:
    - `BoxStyle::is_empty` `&&`→`||` (background-only block lost its
      background): `background_only_block_keeps_its_background`.
    - `sliced_image` / `apply_nine_slices` dropping `VisualBox::BorderBox`:
      the dump prints `visual-box=` for frames not over the border box;
      `nine_slice::tests::frames_get_the_sliced_image_over_the_border_box`.
    - stylesheet reload arm deleted, and `!=`→`==` in the (since removed) failed-latch
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
  - [x] Scheduled in CI: `.github/workflows/mutants.yml`, twice weekly (Sundays and Wednesdays; weekly until the repo went public),
    informational — it never fails on survivors, since the documented
    equivalents always survive.
  - Proptest regression seeds written while planting bugs by hand
    (`tests/layout_properties.proptest-regressions`,
    `proptest-regressions/*.txt`) are already covered: proptest replays them
    first on every run.
- [x] **7. Golden images, sparingly** (fixes the rest of weakness 3 and part of weakness 4: real font rasterization, wrapping and 9-slice drawing; CJK fallback is still untested, since only committed fonts are used). `tests/golden.rs` + `scripts/golden.sh` on Mesa lavapipe; three scenes (`text`, `frame`, `l10n`); see Testing. Brittle across drivers and font versions: a smoke check, not a spec.
  - [ ] A CJK/fallback scene would need a committed CJK font (or `system_fonts`, which isn't reproducible).

## Bug reports

Bugs found by the automated testing above — or by any other means — are
filed in `docs/agents/bugs/` as `bug_NNNN.md`, numbered sequentially in
discovery order, with an `INDEX.md` title/metadata page linking all reports.
File one for every real defect a fuzzer, property test, vector, or manual
investigation uncovers, *at minimum when the fix lands* (don't let the
analysis live only in commit messages or chat logs).

Per-bug file, whatever is known and useful:

- **Status** (open / fixed / upstream-unfixed) and **severity**; reachable
  from user data = high, even if only panics.
- **Component** and **symptoms** (the observable misbehavior, not the cause).
- **Discovery**: which tool/target found it (fuzz target, stateful op
  sequence, seed, minimizing commit), the repository commit at discovery, the
  agent/model, and the date.
- **Minimal reproduction** — a minimized input or op sequence, not the raw
  fuzzer artifact.
- **Root cause**, the **fix** (with commit), and the **regression test** that
  now guards it; upstream bugs note the affected/pinned versions and where
  the local patch (`[patch.crates-io]` + `vendor/`) is wired.

`INDEX.md` carries the shared metadata: date range, machine, OS/kernel,
toolchain and dependency versions, and the discovery agent. Keep it current
when new bugs are added; see the existing entries (bug_0001–bug_0005) as
templates.

Upstream defects — in dependencies and tools, whether or not they also have
a `bug_NNNN.md` — go in `docs/agents/bugs/UPSTREAM.md` until they're fixed
upstream: project and affected versions, reproduction, the local workaround
and where it lives, a PR sketch, and a status (unreported → reported → PR
open → fixed upstream → workaround removed). Add an entry whenever you
vendor/patch a dependency or work around a tool; when bumping a dependency,
check its entries and drop workarounds upstream has made unnecessary.

## Limits and next steps

Known limits (each skipped/ignored value is logged at `debug`):

- **Layout:** flex and grid layout, sizes, margins and `box-sizing` work on
  blocks, containers and the root (see the CSS subset); `position: fixed`/`sticky`, `order`, named grid
  lines or `grid-template-areas`, `place-*` shorthands, `gap` in `%`, or
  font-relative lengths (`em`/`rem`) for layout. `absolute` resolves against
  the parent (Bevy has no containing-block search). The `HtmlUi` node's default `Node` is a
  flex *row*: set `flex_direction: Column` on it or `html { flex-direction:
  column }` in CSS, or blocks sit side by side. The root rule has no
  `:hover`/`:focus` states (always `Pseudo::default()`).
- **Lists:** `ul`/`ol` are plain columns; `li` draws a fixed `• ` with a
  hard-coded 12px indent; `ol` isn't numbered; no `list-style`.
- **Selectors:** compound only (type/`*` + `.class` + `#id`, plus the
  interaction pseudo-classes `:hover`/`:active`); no combinators
  (`div p`, `>`), attribute selectors, other pseudo-classes or
  pseudo-elements.
- **Properties:** no `text-align`, `line-height`, `letter-spacing`,
  `text-decoration`, `opacity`, elliptical `border-radius`, border styles
  (every border is solid), `overflow`. Lengths: px/em/rem/% for `font-size`, absolute only
  for `padding`/`border-width`/`gap`; px/%/viewport units for sizes and margins.
- **border-image:** center always drawn, `-width`/`-outset` ignored, one
  repeat mode for all sides (Bevy `TextureSlicer` limits).
- **Inline:** no inline boxes — `background`, borders, frames and padding on
  inline elements are ignored (Bevy `TextSpan` has no box); inline elements
  have no entity, so `HtmlElements` can't find them (only blocks/containers).
- **Interactivity:** declarative only: `data-on-<trigger>`/`data-with` emit
  [`ElementSignal`] messages (`signals.rs`; click/press/release via picking
  observers, enter/leave via hover tracking that survives rebuilds — the
  deepest bound element wins, so buttons can nest). What signals *mean* is
  app code reading the queue (the demo's `controls::read_signals`). No
  built-in reactions, no forms/inputs. `:hover`/`:active` styling works
  (see the Selectors limit); focus and directional navigation are
  library-side (`focus.rs`), input bindings are the app's. No
  `:focus-within`, no Tab order (directional navigation only). App
  components on elements: `is="<name>"` + `data-*` runs the app's
  `define_html_element` system on every spawn (`custom_elements.rs`); no
  disconnected/attribute-changed callbacks, no autonomous custom tags
  (`<my-tag>` is walked through like any unknown tag).
- **Rebuilds:** content changes (template, context, locale) that alter the
  rendered HTML or translations rebuild the whole `HtmlUi` subtree (no
  diffing; an identical render is skipped); style changes restyle in place unless
  the node structure changes. A run merge (e.g. `b` restyled to its parent's
  style) changes the span count and falls back to a rebuild. Nested `HtmlUi`
  entities inside a rebuilding ancestor are despawned with it and skipped
  (bug_0016); re-nest them on `HtmlUiBuilt`.
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
3. ~~`:hover` / `:active` via `Interaction` or picking, re-styling without a
   full rebuild~~ (done: `PseudoState` from the picking hover map +
   pressed entities; a state change is a `Restyle` — in place; `:focus` /
   `:focus-visible` from `InputFocus` since, see `focus.rs`).
4. Lists done properly: `list-style-type`, `ol` numbering, CSS-driven indent.
5. Text properties: `text-align` (`Justify`), `line-height` (`LineHeight`).
6. Keyed reconciliation for content changes: keep entities for unchanged
   elements (key `NodeSpec`s by DOM path) so app-attached state survives
   template/locale changes too and large documents stay cheap. (Restyle in
   place for style-only changes is done.)
7. More test vectors alongside each of the above.

## Verification

- CI (`.github/workflows/`): `ci.yml` enforces the rules below on every push
  to `main` and every PR (`RUSTFLAGS`/`RUSTDOCFLAGS=-D warnings`, debug info
  off to fit the runner's disk), and that `tests/vectors/*/fluent.html` is
  what `scripts/fluent_oracle.sh` produces. `nightly.yml` runs the golden
  images on Ubuntu's lavapipe (references were recorded on Arch's Mesa 26.2;
  if Ubuntu's Mesa renders differently beyond the tolerance, the job uploads
  `golden-diffs`), checks `browser.json` against the runner's Chrome
  (ignoring the generator line; a failure means Chrome changed or a vector
  is stale), fuzzes each cargo-fuzz target for 15 min (`-rss_limit_mb=4096`) and builds the other
  fuzz drivers. Nightly failures are reports to triage, not merge blockers.
  Fuzz corpora persist across nights as GitHub caches
  (`fuzz-corpus-<target>-<run>`; restored from the newest, saved after
  every run, also after a crash) and are minimized with `cargo fuzz cmin`
  on Sundays. Before this (2026-10-05) every night started from an empty
  corpus: 60 s reached cov 3830 on `html` vs 5572 for the local corpus.
  nightly
  `coverage` job runs `scripts/coverage.py --html`: the per-layer table goes
  to the job summary, the reports to the `coverage` artifact (the fuzz-corpora layer
  replays the committed seeds plus the newest CI corpus caches). Each fuzz
  job also uploads its corpus as the `fuzz-corpus-<target>` artifact (90
  days) so CI's finds can be merged into `fuzz/seeds` (testing guide).
  First local measurement (2026-10-04): 94.3% of 1684 library lines
  covered by some layer. Its two gaps are closed: the template
  *render*-failure path (`template_render_failure_shows_the_error_and_recovers`
  in `html_ui`) and the CSS mapping arms nothing ran
  (`alignment_values_map_to_bevy_values`,
  `units_keywords_and_unsupported_values` in `cascade::tests`) — measured
  98.0% then. Latest measurement (2026-10-05, at `96f2cd3`): merged 88.1%
  of ~3112 lines — the total grew with the interaction/focus work and its
  new uncovered paths. The new `signals` layer (unit-style tests + signal
  properties) covers `src/signals.rs` at 95.1% alone, 99.0% merged (the
  rest is two defensive race guards). The rest of `uncovered.txt` is
  `#[derive]` lines, `debug!` fallbacks, defensive early returns and the
  outline-mode render failure.
  Mutants couldn't flag the mapping gaps (no arm of an exhaustive match can
  be deleted): read coverage and mutation results together.
  GitHub moves `ubuntu-latest` to Ubuntu 26 from 2026-10-19: that can change
  Mesa (golden images: lavapipe rendering vs the references) and the apt
  package names (Bevy/honggfuzz build dependencies). If the nightly breaks
  around then, suspect the image first (`golden-diffs` artifact, apt
  errors); pinning `runs-on: ubuntu-24.04` is the quick fallback.
  First CI runs (2026-10-04): CI and nightly golden, browser oracle and
  fuzz-driver builds green; the cargo-fuzz jobs needed
  `fuzz-libfuzzer.sh` to pass `--target` (the prebuilt cargo-fuzz defaults
  to musl, which ASan rejects).
  Shared build caches (2026-10-05): every job used to build its own
  dependencies; the 3 cargo-fuzz jobs rebuilt from scratch daily (a new
  nightly changes rust-cache's key: 11–19 min each for 60 s of fuzzing) and
  the mutants shards missed after a week. Now `cache.yml` builds the
  `mutants` and `fuzz` caches once, CI's `test` job the `dev` cache, and
  consumers only restore them; the fuzz jobs pin `BEVY_MARKUP_NIGHTLY`
  (`nightly-2026-09-09`, the nightly every fuzz driver was verified on
  locally; bump it in nightly.yml and cache.yml together). Verified locally
  in a fresh target dir: after the producer commands, cargo-mutants'
  baseline, `cargo test --test golden` (under CI's env) and `cargo fuzz
  build` → `run` compile no dependencies. Details and the producer/consumer
  table: `docs/agents/skills/testing.md`, CI. A consumer's `::warning::`
  "no exact … build cache" means the pair's env, toolchain or keys drifted.
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
              .observe(bevy::render::view::screenshot::save_to_disk("/tmp/bevy_markup_shot.png"));
      }
  });
  ```

- DOM outlines (`HtmlDebugOutline`) are logged at `debug`:
  `RUST_LOG=bevy_markup=debug timeout 15 cargo run --example demo`.
- Pointer interaction: write `bevy::window::WindowEvent::{CursorMoved,
  MouseWheel, MouseButtonInput}` messages from a throwaway system (move, then
  press and release in later frames), then check state. Real input sometimes
  reaches the window during runs; prefer setting resources directly when
  clicks aren't what's under test.
