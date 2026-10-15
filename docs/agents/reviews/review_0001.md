# review_0001 — Code review 2026-10-13

| field | value |
|---|---|
| ID | review_0001 |
| Date | 2026-10-13 |
| Scope | whole crate at `b224619` (0.6.0 + the bounded-run commit) |
| Method | six parallel review slices + integrator checks |
| Findings | `R01`–`R80` (stable ids; [follow-up](review_0002.md) re-checks them) |
| Discovery agent | omp session, `anthropic/claude-opus-5-5:high` |

A full-crate code review at `b224619` (0.6.0 + the bounded-run commit).
Status: the findings were fixed by later sessions — see
[review_0002](review_0002.md) (the first fix session's audit),
[review_0003](review_0003.md) and [review_0004](review_0004.md), and the
bug reports they name. (When this review was written, a same-day fix
session had been cut off by a provider rate limit and held no changes.) Individual `bug_NNNN.md` entries will be
filed here as the fixes land; this file is the session's record. It contains
80 numbered findings; the IDs (`R01`…) are stable references.

## How the review was run

Six parallel review slices (build/reuse, cascade/fonts/styles, interaction,
templates/Markdown/Fluent/loaders, core wiring/rebuild, examples/scripts/CI),
each reading its files against AGENTS.md and the ADRs. Evidence levels:

- **[probe]** — reproduced in a throwaway scratch copy of the crate (since
  deleted); the scenario below is the probe's input.
- **[code]** — verified by direct reading of the code during triage.
- **[traced]** — followed the exact code path by reading; not executed.

Reviewers' scratch copies shared the repo's `target/` at first, so two
reports were re-verified under a renamed package; findings marked [probe]
below survived that. One reviewer's probe run and the integrator's reads
confirmed the cited code directly; anything that could not be re-checked is
marked [traced].

Discovery agent: model `anthropic/claude-opus-5-5:high`.
`cargo clippy --all-targets --features text-input` shows
only hygiene lints (28 redundant casts, 16 `clone()` on `Copy`, unit values
passed to functions); no clippy finding is a bug, so none is numbered.

## A. Test safety net

- **R01 · high [code]** The ADR 0021 safety net never tests reuse.
  `skeleton_reuse_matches_a_fresh_build` (tests/properties.rs:728-732)
  removes `Skeleton` from *both* UIs before every op, so it compares two
  fresh builds. Every reuse bug in section B got through it. Fixing the test
  (keep the skeleton on the `reused` side) and broadening its generator is
  the prerequisite for B.

## B. Skeleton reuse (ADR 0021)

- **R02 · high [probe/code]** Content vanishes when an unchanged subtree's
  root tag produced no node of its own. `collect_node` (build.rs:3386-3392)
  returns after `skeleton.reuse` even when `reuse` found nothing (the
  comment claims it "falls through"). `<div><p>{{ n }}</p><span>static
  span</span></div>`: changing `n` drops the span. Same for `<img>` in a
  container and for `<body>` when `<head>` carries the change.
- **R03 · high [probe/code]** Reused subtrees keep the previous
  stylesheet's styles. `skeleton_eligible` (build.rs:1027-1033) checks
  state/vars/fonts/images but never `own_changed`/`default_changed`, and a
  style change held back by a `Wait` is merged into a later `Build` by
  `pending.take().max(change)` (rebuild.rs:124), which looks content-only.
  Scenario (a): swap `DefaultStylesheet` a.css→b.css and change the context
  in the same frame — the static paragraph keeps the old color until
  something else forces a restyle. Scenario (b): replace a loaded sheet with
  a missing one while the context changes — the failing sheet resolves with
  no event and the retry build reuses. Contradicts ADR 0021 ("stylesheet
  changes already force full rebuilds") and the lib.rs docs.
- **R04 · high [probe]** Builds where the adaptive gate skipped the reuse
  walk leave the skeleton stale; the retry then serves cached specs whose
  entities were updated from newer content, and the skip guard
  (`DomNode` == paired handle, build.rs:2383-2391) checks position, not
  provenance, so the write is skipped. `<p>{{a}}</p><p>{{b}}</p><p>{{c}}</p>`
  with one all-dirty change, then single changes, shows `b=x` where a fresh
  build shows the new value.
- **R05 · high [probe]** The skeleton's document falls out of step with its
  specs when the render changes during a `Wait` frame (sheet loading) or an
  outline frame, because the document is re-parsed only when
  `rendered.is_changed()` this frame (build.rs:1253-1304). After `a,b` →
  theme switch to an unloaded sheet plus `b,a` → `a,c`, the UI shows
  `"b","c"`; a fresh build shows `"a","c"`. Outline-off reproduces it too.
- **R06 · high [probe]** Descendants of a skipped (reused) subtree keep
  `DomNode` handles from the frame they were spawned (the skip writes the
  root only); the state walk (build.rs:1104-1110) maps through handles, so
  after content changes `:hover`/`:active` on the reused subtree stops
  working (element stays unhovered) and a stale handle can collide with
  another element's and hover the wrong one.
- **R07 · medium [probe]** The usefulness gate counts a reused subtree as 1
  against every node inside it in `spec_count` (build.rs:1264 vs
  :3219-3226), so the flagship case — dynamic header, static Markdown body
  — is scored useless and the pairing is switched off for the next builds.

## C. Build/update paths

- **R08 · medium [code]** Text inputs read `type` from `dataset`
  (build.rs:3436-3441), i.e. `data-type`; any real `type="checkbox|
  password|submit|…"` becomes an editable text field. ADR 0016 says other
  types stay unsupported.
- **R09 · medium [traced]** A kept `<input>` is never updated by
  `update_spec` (build.rs:2442-2717 ignores `spec.text_input`): a re-render
  with a new `value` attribute doesn't change the field, restyles (theme
  swap, `:focus`/`:hover`) leave stale `TextFont`/`TextColor`, and `name`
  changes are ignored; `update_spec` also inserts `EmSize` where
  `spawn_spec` deliberately doesn't.
- **R10 · low [traced]** `maxlength` and `placeholder` are read nowhere
  (build.rs:3445-3459) but documented as carried over (lib.rs, AGENTS.md,
  the `text_input_element_maps_to_the_widget` comment).
- **R11 · low [traced]** The text-input branch rebuilds `boxed`
  (build.rs:3460) without the `resolve_names(grid)` call the normal path
  made (:3432), so a named-line/area placement on an `<input>` is lost.
- **R12 · high [probe]** Deep user content overflows the stack: ~150 nested
  `<div>`s (debug build, `{{ doc | safe }}`) abort in `collect_node`
  (~:3567); 1000 nested `<b>` in `push_runs`. `pair_node`, `item_spec`,
  `spawn_spec`, `update_children` and `remap_spec_handles` are equally
  unbounded, and every level clones the ancestor chain (:3405-3410,
  O(depth²)).
- **R13 · low [traced]** Dropping a root `calc()` leaves `CalcSized` in
  place (apply_root :2941-2946 only inserts), so a theme swap that removes
  `html { width: calc(…) }` keeps resolving the old calc; a top-level root
  (no `ChildOf`) is skipped by `resolve_calc` (:2726), so root calc never
  applies there at all.
- **R14 · low [traced]** A restyle that respawns spans (run merge/split
  under a new sheet) doesn't report the respawn (update_spec :2709-2714), so
  `HtmlUiRestyled` fires although link spans are new entities and app
  wiring on them is lost.
- **R15 · low [traced]** `item_spec` clones every container's whole child
  subtree (build.rs:1950, `children.clone()`) although only `.iter()` is
  used — O(n·depth) copying per build.
- **R16 · low [traced]** `update_children` tests `owned.contains(child)`
  linearly per current child (:2425) — O(n²) for wide parents (Markdown
  bodies) on every build that visits the parent.
- **R17 · low [probe]** Bevy's default `RemSize` is 20px, the CSS default
  16px, so every UI without an explicit root `font-size` queues a restyle on
  its first build and fires a spurious `HtmlUiRestyled` (build.rs:1225-1231,
  rebuild.rs:102-107); the doc comment ("`Val::Rem`s re-resolve on the next
  restyle") is wrong — Bevy re-resolves them itself.
- **R18 · medium [code]** The dump-driven equivalence properties
  (`restyle_matches_a_fresh_build`, `content_update_matches_a_fresh_build`)
  can't see `display`, `flex_direction`, width/height/min/max, grow/shrink,
  grid fields, `column_gap`, `Visibility`, `Focusable` or bound signal
  names — `tests/common/mod.rs:732-899` doesn't print them — so an
  `update_spec` leaving a stale `Display::None` or a stale binding passes.

## D. Cascade / CSS (src/cascade.rs, src/style.rs, src/fonts.rs)

- **R19 · high [probe]** `var()` substitution is exponential and can hang
  the app on one element (`substitute`, cascade.rs:1457-1506; the only guard
  is `depth > 32`, nothing is memoized).
  `p { --a: var(--a, 1px) var(--a, 1px); width: var(--a) }` recurses ~2³²
  times; a doubling reference chain `--vN: var(--vN-1) var(--vN-1)` doubles
  output per level (20 levels ≈ 1.55 s in debug, doubling per level). Also
  breaks the fuzz harness's never-hang contract.
- **R20 · medium [probe]** Rules whose compound has no type/class/id are
  dropped: lightningcss prints `*:hover` back as `:hover`, `Compound::parse`
  rejects it (:1065). `:focus-visible { … }` — the standard focus-ring rule
  — and `*:focus` never apply.
- **R21 · medium [probe]** `inherit`, `initial`, `unset` are silently
  ignored (the `_ => {}` arm, :1954), so the *lower*-priority rule's value
  shows through: `a { color: #4af } .nav a { color: inherit }` keeps nav
  links at `#4af`; `width: initial` keeps the earlier 200px.
- **R22 · medium [probe/traced]** `var()` names are not tracked
  transitively (`p { --a: var(--brand); color: var(--a) }` — `var_names`
  :834-868 returns only `--a`) nor from inline `style` attributes, so
  changing those template values never restyles (the vars hash
  build.rs:416-429 doesn't change, and ADR 0021 reuse would keep stale specs
  anyway).
- **R23 · medium [code]** `restore_from` on the root doesn't undo
  `position: fixed` (cascade.rs:665-674 covers only `Absolute | Relative`;
  comment stale) nor the `auto` tracks implied by `grid-template-areas`
  (:499-504), so after a theme swap drops them the root keeps the old
  positioning/tracks and `apply_root` saves that as the app's new baseline
  (build.rs:2829-2944).
- **R24 · low/medium [probe]** Negative named-line occurrences are off by
  one (`grid-column: -1 a` with `[a] 1fr [a] 1fr [a] 1fr [b]` resolves line
  2 instead of 3, cascade.rs:164), and an out-of-range negative clamps to
  the first occurrence instead of `auto`.
- **R25 · low/medium [probe]** A name at the end of a `repeat()` is never
  registered (`repeat(2, [a] 1fr [b]) [c]` → `b` unresolvable, :2225-2229
  `if j < tracks`), and an `auto-fill`/`auto-fit` repeat discards the names
  *around* it too (`[full-start] repeat(auto-fill, 100px) [full-end]`,
  :2209) although the docs only exclude names *inside* such repeats.
- **R26 · low [probe]** A `border-image` URL coming from `var()` (or an
  inline `style`) is never loaded: `image_urls` (:802-829) reads only typed
  `BorderImage` values, so the image dependency is missing and the frame is
  silently absent.
- **R27 · low [probe]** An unsupported value erases the earlier rule's
  value instead of being dropped (contradicting the `track_sizing` doc):
  `grid-template-columns: 100px 1fr` then `fit-content(2em) 1fr` → no
  columns (:1737 and the column/template/grid/auto-tracks branches);
  `row-gap: 8px` then `5%` → none (:1596-1601); `width: calc(50% - 10px)`
  then `fit-content` → none (`set_calc` :1313 removes the old calc before
  validating); same `= helper(value)` pattern for display/alignment/padding.
- **R28 · low [probe]** Grid names are not reset by later declarations:
  `grid-template-areas: "x y"` then `grid-template-areas: none` keeps the
  names and the implied 1×2 tracks (:1735-1825); a later nameless
  `grid-template-rows` keeps earlier row names.
- **R29 · low [traced]** The whole stylesheet is rescanned for `var()` names
  every frame for every UI (cascade.rs:834 from build.rs:422), allocating a
  `BTreeSet` + `Vec` each time even when idle; it should be computed once at
  load and stored on `Stylesheet`.
- **R30 · low [probe]** Descendant-combinator matching retries every
  ancestor combination (:973-1001): 40 nested `.a` ancestors under
  `.b .a ×7 p` ≈ 3.8M combinations, 618 ms for one element (debug), each
  distinct depth a cache miss.
- **R31 · low [probe]** `var()` in properties read as unknown/typed elsewhere
  — `pointer-events`, `outline-offset` (:1407-1414 substitutes only unparsed
  values; :1928-1953) — is treated as unsupported, though the docs say "any
  property".

## E. Markdown / Fluent / templates / loaders

- **R32 · high [probe]** The `markdown: true` Fluent cache misaligns text
  parts with markers (`markdown_cache` l10n.rs:436-462, `substitute`
  :375-404): a missing leading/trailing text adds no part, a multi-line
  pattern adds one part per line, so `parts[k]` ≠ marker `k`. `lead = { $n }
  items here` renders `\u{2}0\u{3} items here` (argument lost, control
  characters in the UI); `A { $x } B { $y }` loses `$y`; multi-line
  paragraphs (the normal Markdown shape) lose every argument.
- **R33 · high [probe]** String arguments in cached Markdown messages are
  escaped twice (`parse_args` already escapes; `substitute` escapes again,
  :402 + :493; the build decodes once, build.rs:3723): `O'Brien & Co`
  displays as `O&#39;Brien &amp; Co`.
- **R34 · high [probe]** Code spans and fenced/indented code in `.md`
  templates still run through Tera (md.rs:262-270 only HTML-escapes;
  `separate_statements` even inserts newlines inside fenced code, :56-80),
  contradicting ADR 0020: `` `{{ x }}` `` renders the variable (or fails the
  render); a literal `{% raw %}` in code fails the load; `let s = "{%"; //
  %}` in a fenced block splits a string literal.
- **R35 · high [probe]** `separate_statements` separates *every* `{% %}`,
  including inline ones and inside attribute values: `src="{% if hd
  %}hd{% endif %}.png"` becomes `src="\n\nhd\n\n.png"`; `id="row-{% if a
  %}x{% endif %}"` stops matching `by_id`/signals; a statement in a list
  item, heading or table cell splits the element. The committed
  `examples/assets/markdown/reader.md` survives only because classes are
  split on whitespace.
- **R36 · medium [probe/traced]** Markdown conversion is chosen by the root
  template's path (template.rs:312), not each included file's: an `.html`
  partial included from `.md` is Markdown-converted (indented HTML becomes a
  `<pre class="markdown">`), a `.md` partial included from `.html` is never
  converted. The loader docs claim per-file conversion.
- **R37 · medium [probe]** Cached Markdown messages escape term and message
  references as literal text (`{ -brand }` → `&lt;b&gt;…`), unlike the
  per-value path; ADR 0020 Decision 4 calls only *arguments* literal.
- **R38 · medium [probe]** A placeable in a Markdown link destination kills
  the link (`[docs]({ $url })` — control-character markers aren't allowed in
  destinations, l10n.rs:459): literal text, no `<a>`; the per-value path
  produces the link.
- **R39 · medium [probe]** A substitution failure errors instead of the
  documented per-value fallback (l10n.rs:308-312 turning `None` into `Err`),
  and duplicate message ids put the *later* pattern into the cache while the
  bundle keeps the earlier one (:465 `insert`).
- **R40 · medium [traced]** While the active bundle is loading or failed,
  `localize` returns early and keeps the old map (l10n.rs:264-266); the
  build then applies those stale handles to a freshly rendered document
  (build.rs:3044 doesn't re-check `data-l10n-id`), freezing args and
  misplacing translations across structural changes.
- **R41 · low [traced]** Messages without placeables skip the cache
  (l10n.rs:464 `&& index > 0`) and are Markdown-converted on every
  `localize` — every frame for a per-frame context — contradicting "converts
  once at load".
- **R42 · low [probe]** `separate_statements` skips an earlier `{#` when a
  later `{%` exists (:62 `find("{%").or_else(find("{#"))`), changing list vs
  paragraph behavior; a multi-line comment fails the per-line purity check
  (:227) and leaves an empty `<p class="markdown">` (visible at the top of
  the committed reader.md).
- **R43 · low [probe]** Argument values containing a later literal part or
  the marker characters disturb back-to-front substitution (l10n.rs:385
  first-`find`; :402 `replace`): `A { $x } B { $y } C` with
  `x = "1 B 2"` misassigns (same text here, wrong where markers sit in
  different formatting).
- **R44 · low [traced]** `resolve_references` follows includes inside
  `{# #}` comments, `{% raw %}` blocks and Markdown code samples
  (template.rs:207-240, :319): a commented-out include of a deleted file
  fails the whole template load (blank UI).
- **R45 · low [traced]** `localize` reads asset events with
  `events.read().any(…)` (l10n.rs:257), stopping at the first match; a
  second matching event (e.g. `Modified` then `LoadedWithDependencies` on a
  hot reload) is served next frame, causing an extra re-resolve and
  `HtmlUiBuilt`.
- **R46 · low [traced]** Template includes and `.ftl.ron` resources resolve
  through `read_asset_bytes`, which doesn't apply Bevy's `is_unapproved()`
  check that `AssetServer::load` enforces, so `../..` escapes the asset root
  (template.rs:221/319, l10n.rs:176/180) — matters for user-generated
  content.
- **R47 · low [traced]** `*.slice.ron` manifests aren't validated
  (nine_slice.rs:81, :106-116): `Tile(0.0)` divides by zero (infinite
  repeats in Bevy's slicer), negative insets and NaN/≤0 `max_corner_scale`
  are accepted and draw garbage with no load error.
- **R48 · low [code]** `src/fuzz.rs` never exercises what it claims: the
  `.md` path only Tera-renders the raw source (`HtmlTemplate::render` does
  no Markdown conversion), so `separate_statements`/`rewrite` etc. get no
  fuzz coverage; the cascade harness passes no ancestor chains (combinators
  unfuzzed); no `markdown: true` bundle is ever built. The doc comment's
  "the conversion and its parse must not panic" is false today.

## F. Signals / focus / anchors

- **R49 · high [probe/code]** Every primary click inside a `<form
  data-on-submit>` submits the form: `on_pointer` calls `emit_form_submit`
  for any clicked bound element with a parent (signals.rs:333), nothing
  reads the `FormSubmitButton` marker (only inserted/removed,
  build.rs:2201/2498-2500), and the comments at build.rs:2228/2466 claim
  "`on_pointer` checks the marker". The forms example's `type="button"`
  Clear button swaps in the receipt; a bound wrapper around the submit
  button emits Submit twice (uncovered + covered); link spans inside a form
  submit it.
- **R50 · high [probe/code]** `update_focus_states` uses a plain `insert`
  (focus.rs:268) — the bug_0021 class it missed: it and the exclusive
  `dispatch_signals` are only ordered against `Render`, not each other, so a
  signal handler that despawns the UI in a frame where focus changed panics
  (`insert<PseudoState> … Entity despawned`). `try_insert` is the fix.
- **R51 · medium [probe]** Enter / gamepad A on a submit button never
  submits: `activate_element` (focus.rs:389-410) only emits `Click`
  bindings and never runs the form-submit path, so keyboard/gamepad users
  can't submit (the forms README instructs exactly this).
- **R52 · medium [probe]** A tooltip on a link (`<a href data-tooltip>`) is
  respawned every frame and never shown: the hover hit is the span, which
  carries `HtmlElement`+tooltip data but no `ComputedNode`/`UiGlobalTransform`,
  so `place_anchored` treats it as gone and despawns (31 `Add<HtmlTooltip>`
  per 30 hovered frames, 0 alive). Constant spawn/build/despawn churn.
- **R53 · medium [probe]** The "first focusable element" fallback takes the
  lowest `Entity` (focus.rs:359), which sorts by generation/index — recycled
  order, not document order: `#a #b #c` (no autofocus) focuses `#c`. Same
  for modal open and focus-lost fallbacks.
- **R54 · medium [probe]** Touch pointers never emit `data-on-leave` and
  `hover_signals`' `entered` map grows by one entry per touch: Bevy despawns
  touch pointers on touch end and drops them from `HoverMap`, and the code
  `continue`s on a pointer missing from `now` (signals.rs:634-637) instead
  of emitting leaves and dropping the entry.
- **R55 · medium [traced]** `Above`/`Left` overlay placement isn't clamped
  to the viewport (anchor.rs:149/:162): an element at the top edge with
  `data-tooltip-placement="above"` gets `bottom = view_h + gap` — the whole
  tooltip off screen — although the docs promise clamping "by the overlay's
  size".
- **R56 · medium/low [traced]** `HtmlWorldAnchor` ignores `UiScale`
  (anchor.rs:308-309): window-logical px are written into `Val::Px`, which
  Bevy multiplies by scale × UiScale, so with `UiScale(2.0)` (the demo uses
  it) plates land at twice the position. `place_anchored` handles this
  correctly via `inverse_scale_factor`; the world anchor doesn't.
- **R57 · low [traced]** The world anchor's on-screen test compares against
  `0..logical_viewport_size` while `world_to_viewport` includes the
  viewport's origin (anchor.rs:288-293), so every visible point of an
  offset viewport (split screen) reads off-screen and the plate hides.
- **R58 · low [traced]** The Submit signal's `source` is hard-coded to
  `Pointer { Mouse, Primary, (0,0), 1 }` (signals.rs:386-391) regardless of
  the real pointer/touch/activation.
- **R59 · medium [probe]** `BevyMarkupPlugin` adds
  `DirectionalNavigationPlugin` itself (focus.rs:157-159), so an app that
  adds that plugin afterwards (Bevy's documented pattern) panics with
  "plugin was already added"; the auto-add is undocumented.
- **R60 · low [traced]** `update_focus_states` and `dispatch_signals` are
  unordered against each other (lib.rs:335, focus.rs:168): a handler moving
  focus restyles it a frame late.

## G. Examples

- **R61 · high [probe]** Five examples double-handle input: the launcher
  adds `ExampleInputPlugin` in all three modes (shared/harness.rs:89/143/166)
  while quickstart (:28), hot-reload (:36), inline (:31), markdown (:22) and
  links (:34) also register Enter/A and arrows. One press = two signals:
  Language toggles twice (no-op), a coin is added twice, the hot-reload
  palette skips one, an inline quest toggles twice, arrows move focus two
  steps. Introduced with the bounded-run work (2a6b2fd); contradicts
  AGENTS.md ("kept self-contained").
- **R62 · medium [probe/code]** The forms example's "Clear fields" looks up
  `by_id("name"/"role")` but the inputs only have `name=` attributes
  (signup.html:15/19; `HtmlElement.id` comes from the `id` attribute,
  build.rs:3649), so nothing is cleared. (Its *submitting* half is R49.)
- **R63 · medium [probe/traced]** Typing a space in a text field switches
  the language in `inputs` (:160) and `forms` (:134): the hotkey reads
  `just_pressed(Space)` with no focused-text-field guard (the shared
  `navigate` guards arrows; these hotkeys don't).
- **R64 · medium [probe/code]** grid-names' `handle_signals` takes
  `Res<Picks>`, clones, flips and writes the context but never the resource
  (grid-names/main.rs:97-119): cells can't be unchecked and checking one
  unchecks the others (every click starts from the initial all-false
  vector).
- **R65 · low [code]** Missing `:focus-visible` rings on focusable controls:
  inline `.quest` (only `.action` is styled) and the forms receipt's
  "Edit again" (`p.action`); breaks the every-control-has-a-ring convention.
- **R66 · low [traced]** User-typed text is spliced into hand-built
  `data-l10n-args` JSON (inputs preview, forms receipt): typing `"` or `\`
  makes the JSON invalid (l10n.rs:303 decodes entities before parsing), the
  message errors, and the untranslated English fallback shows.
- **R67 · low [code]** hot-reload's `palette_css(0)` differs from the
  committed style.css (three comment lines), so cycling back to "Crimson"
  leaves the tracked file modified; the starting palette is always 0 while
  the file keeps the last written one, so name and colors disagree on
  launch.
- **R68 · low [code]** The harness's bounded runs press their exit/keys
  inside `Update` (harness.rs:227), after Bevy cleared `just_pressed`, so
  whether the run exercises key branches depends on system order; a
  `KeyboardInput` message or PreUpdate placement is the fix.
- **R69 · low [code]** Linear arrow stepping filters `Without<HtmlNoFocus>`
  (shared/input.rs:214), but `HtmlNoFocus` lives on `HtmlUi` roots, not
  elements, so the filter is a no-op; `HtmlModal` isn't honored either
  (latent: only the demo uses linear mode).

## H. Scripts / CI

- **R70 · medium [code]** CI never compiles the `text-input`, `hot-reload`
  or `dev-tools` features (ci.yml:46-55 runs only plain check/build/test/
  fuzzing-test/doc), although AGENTS.md's Verification section claims
  `cargo check --lib --features text-input` is enforced. The text-input
  mapping arms and its gated tests, the hot-reload example and the
  dev-tools code are never compiled in CI.
- **R71 · medium [code]** The nightly coverage and divan-bench steps pipe
  into `tee` without `pipefail` (nightly.yml:209, :46; GitHub's default
  shell is `bash -e` without pipefail), so a failing coverage layer or a
  panicking bench leaves the job green.
- **R72 · low [code]** The oracle "references are current" checks use
  `git diff --exit-code`, which ignores untracked files: a new vector
  committed without its `browser.json`/`fluent.html` regenerated is
  silently never oracle-checked.
- **R73 · low [code]** `scripts/coverage.py` lists `benchmarks` *and* its
  two subsets, running the benches twice per run (again under `--html`),
  and the "only here" column is 0 for all three by construction.
- **R74 · low [code]** `scripts/fuzz-honggfuzz.sh` and
  `scripts/fuzz-test-fuzz.sh` treat a normal run as failure (`timeout`
  exits 124 when the run time elapses) and the timeout covers the build, so
  a cold run is killed mid-compile.
- **R75 · low [code]** `scripts/mutants.sh` doesn't park `fuzz/coverage`
  (ignored only by fuzz/.gitignore, which cargo-mutants doesn't read), and
  `trap restore EXIT INT TERM` returns into the script after INT/TERM
  instead of exiting.

## I. Stale docs (contradict the code)

- **R76 · low** style.rs:27-29 and AGENTS.md:501-502 say combinators are
  "skipped" — descendant/child combinators are implemented and
  oracle-tested (cascade.rs:918-990, tests/vectors/combinators/).
- **R77 · low** AGENTS.md:536-537 says named lines and
  `grid-template-areas` are skipped — they resolve (`GridNames`); same
  stale claim at cascade.rs:2175 and :2362-2363.
- **R78 · low** style.rs:128 says em/rem lengths are skipped (calc and
  sizes support them); style.rs:161 says `font-family` uses "the first name
  registered" (the whole chain composes into Bevy's fallback list, ADR
  0015).
- **R79 · low** The feature lists (lib.rs:184-187, AGENTS.md:58) omit
  `text-input`, `hot-reload`, `fuzzing` and `dev-tools`.
- **R80 · low** AGENTS.md's Layout section leaves out the `combinators` and
  `grid-names` examples; describes `inputs` as having a "Language button"
  (it has none); quickstart's "kept self-contained" navigation is
  contradicted by the launcher (R61).

## Triage notes

- R01 must be fixed first: it is the net under B, and B's five bugs are the
  ones most likely to show users stale or missing content in shipped UIs.
- Highest user-visible impact beyond B: R19 (hang from any stylesheet),
  R49/R51 (forms), R08 (text input types), R32-R35 (Markdown + Fluent, the
  documented ADR 0020 contract), R50 (panic), R61 (five examples).
- Of the perf findings, R29/R30/R15/R16 are per-frame costs on every UI;
  R12 is a crash-class bug first, perf second.
- Deliberate-design items surfaced by the review but *not* findings:
  `flex-shrink: 0` default (documented open question in AGENTS.md) and the
  `mutants` quirk with struct-field mutants.
