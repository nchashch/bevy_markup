# review_0010 — Code review 2026-11-27

| Field | Value |
|---|---|
| ID | review_0010 |
| Commit reviewed | `3b4253d40394bf16769cd3da49b563705730545f` |
| Reviewer | Gemini |
| Toolchain | rustc 1.99.1 (58cf8f9c3 2026-10-13), Linux 7.2.14-arch1-1 x86_64 |
| Scope | Independent repository review focusing on the post-`review_0009` architectural rewrites: own CSS parser (ADR 0048), own selector engine (ADR 0050), MiniJinja template engine, fuel budget and repo patch (ADRs 0049, 0052), SVG icon subset (ADR 0051), font-size budget, focus/modal visibility, and crate docs/metadata |
| Result | Nine confirmed findings: two high-severity untrusted-input bounds bypasses (stack-overflow abort in `calc()`, memory-bound bypass in template serialization), five medium-severity runtime/CSS/focus/Markdown defects, one grouped low/medium CSS parser & cascade regression set, and one low-severity stale docs/metadata finding |
| Changes | This report only; no implementation fixes |

## Coverage and method

Read `AGENTS.md`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, the ADR index, `.trail/learnings/`, and previous reviews (`review_0007`–`review_0009`) to establish invariants and avoid repeating resolved issues. Audited the four major subsystems rewritten since `review_0009` (`9811ccc` → `3b4253d`) and their interfaces with the rest of the crate:

- **CSS parser, values, grid, and cascade (ADR 0048, ADR 0050):** `src/css/{mod,sheet,select,property,value,color,grid}.rs`, `src/cascade/selector.rs`, and targeted paths in `src/cascade.rs` (`CalcExpr`, `nesting_over_limit`, `Substitution`, `apply_keyword`, `apply`, `track_sizing`, `parse_areas`) and `src/build.rs` (`resolve_calc`).
- **MiniJinja templates and fuel (ADR 0049, ADR 0052):** `src/template.rs`, `src/html.rs`, `src/md.rs`, `src/lib.rs`, and the pinned MiniJinja checkout (`minijinja` `b705d35`, `filters.rs`, `value/ops.rs`, `vm/state.rs`).
- **SVG icon subset and bounds (ADR 0051):** `src/svg.rs` and `src/svg_bounds.rs`.
- **Fonts, localization, focus, and UI glue:** `src/fonts.rs`, `src/ftl_check.rs`, `src/l10n.rs`, `src/focus.rs`, `src/signals.rs`, `src/canvas.rs`, `src/anchor.rs`, `src/tooltips.rs`, `src/rebuild.rs`, `src/style.rs`, `src/nine_slice.rs`, `src/custom_elements.rs`, and `src/fuzz.rs`.

Empirical probes ran in a temporary integration test (`tests/review10_probes.rs`, compiled with `--features fuzzing --offline --test-threads=1` and deleted afterward):

- **Stack-overflow probes (N01):** invoked `bevy_markup::fuzz::cascade` on a 2 MB worker thread (matching Bevy's IO task pool) inside a child process via `std::process::Command` so `SIGABRT` (exit 134) was observed cleanly without crashing the runner.
- **Template memory/fuel probes (N02):** invoked `HtmlTemplate::from_source` / `render_source` (via `bevy_markup::fuzz` and real `TestUi` / `AssetServer` loads) capped at safe allocation sizes (30–150 MB). OOM consequences for larger repetition counts are inferred from the measured linear scaling, not triggered by exhausting host RAM.
- **Runtime / UI / Markdown / CSS / focus probes (N03–N08):** exercised the real asset loaders, `BevyMarkupPlugin`, `HtmlTemplateLimits`, `HtmlFocus`, `HtmlModal`, and `tests/common::TestUi`.
- **Disproved hypotheses (not filed):** SVG `style="FILTER: blur(10px)"` was ignored by `usvg`/`simplecss` (case-sensitive property matching; returned `Ok("10x10 cost 572")` with no filter node), and tested large-radius SVG arc drift cases were rejected cleanly by `check_extent` (`TooFarOut`) in 80–310 µs.

This was not an exhaustive line-by-line audit of every file. No fuzz campaign, mutation run, browser differential test, GPU golden test, or windowed example run was executed, and third-party transitive dependencies (outside the patched MiniJinja fuel/serialization paths) were not audited.

## Validation

All commands ran offline against the installed dependency set with `CARGO_PROFILE_DEV_DEBUG=0` and `RUSTFLAGS='-D warnings'` (plus `RUSTDOCFLAGS='-D warnings'` for `cargo doc`).

| Command | Result |
|---|---|
| `cargo check --lib --offline` | Pass, minimal library features |
| `cargo check --lib --features text-input --offline` | Pass, warnings denied |
| `cargo check --lib --features svg --offline` | Pass, warnings denied |
| `cargo check --lib --features fuzzing --offline` | Pass, warnings denied |
| `cargo doc --no-deps --offline` | Pass, Rust and rustdoc warnings denied |
| `cargo test --lib --tests --offline` | Pass: 552 passed, 2 ignored (includes `examples/grid-names`) |
| `cargo test --lib --features fuzzing --offline` | Pass: 271 passed, 0 ignored (includes SVG and fuzz-harness unit tests) |
| `cargo test --features text-input --lib --test signals --offline` | Pass: 222 library tests + 34 signal tests passed |
| Temporary probe suite (`review10_probes.rs`, deleted after run) | Confirmed N01–N08 via direct execution |

The two ignored baseline tests were `tests/golden.rs` (GPU golden test) and the documented inline-image line-metrics case in `tests/html_ui.rs`. All baseline and feature tests passed despite all nine findings.

## Findings

Findings use stable IDs `N01`–`N09`. Each carries an evidence tag: `[probe]` (reproduced dynamically against the crate's real code) or `[code]` (verified by static inspection).

### N01 · high · Flat `calc()` operator chains bypass CSS nesting limits and abort with stack overflow `[probe]`

**Location:** `src/css/value.rs:461–511` (`parse_sum`, `parse_product`), `:576–626` (`evaluate`), `:76–88` (`bounded_expr`), `:712–729` (`expr_is_finite`), `src/cascade.rs:246–322` (`CalcExpr::collapse`, `resolve_raw`), and `src/cascade.rs:1849–1877` (`nesting_over_limit`).

The CSS load-time nesting pre-scan (`nesting_over_limit`) counts only bracket/paren/brace/function nesting depth, and `MAX_CALC_DEPTH` (32) in `src/css/value.rs:535` only increments on `(` or nested math functions. A flat operator chain with **zero** nested parentheses—such as `calc(1px + 1% + 1px + 1% + ...)` or `calc(1px * 1 * 1 * ...)`—builds a left-skewed recursive `Ast::Sum` or `Ast::Mul` tree of depth equal to the number of operators.

`evaluate` recurses once per AST node, and dropping `Ast` / `CalcExpr` also recurses along the left spine. In addition, for mixed-unit sums like `1px + 1% + ...`, `evaluate` calls both `simplify(CalcExpr::Sum(...))` (`CalcExpr::collapse()`) and `expr_is_finite(expr)` at **every** `Ast::Sum` step (`src/css/value.rs:597, 622`), walking the uncollapsible spine quadratically.

**Probe (`bevy_markup::fuzz::cascade`, debug build, 2 MB thread in a child process):**

- `p { width: calc(1px + 1% + ...); }` (alternating `1px` and `1%`, `n` additions):
  - `n = 500` (2,767 bytes): `ok` in 6.35 ms
  - `n = 1000` (5,517 bytes): `ok` in 14.80 ms
  - `n = 1400` (7,717 bytes): **`SIGABRT` (exit 134)** — `thread '<unnamed>' has overflowed its stack / fatal runtime error: stack overflow, aborting`
- `p { width: calc(1px * 1 * ...); }` (`n` multiplications):
  - `n = 1000` (4,019 bytes): `ok` in 1.65 ms
  - `n = 2000` (8,019 bytes): **`SIGABRT` (exit 134)** — stack overflow

**Impact:** an untrusted stylesheet under 8 KB aborts the host process on a 2 MB IO/worker thread (and a larger flat chain aborts even on an 8 MB main thread), violating ADR 0044's untrusted-CSS invariant. Even below the stack limit, mixed-unit chains incur $O(n^2)$ evaluation work per property.

**Fix direction:** enforce a per-expression node/term cap (e.g., 64–256 terms) in `parse_sum`/`parse_product` (or flatten `Ast::Sum`/`Ast::Mul` into `Vec`s with an explicit length cap), run `simplify`/`expr_is_finite` once at the top level or iteratively rather than re-walking the entire spine on every `+`, and add regression tests on a 2 MB thread for both `+` and `*` chains.

---

### N02 · high · Container-of-strings serialization bypasses `check_printable`, `MAX_RENDERED_BYTES`, and pre-allocation fuel `[probe]`

**Location:** `src/template.rs:227–229` (`new_environment`), `:293–330` (`fits_printed`), `:333–355` (`checked_string`, `checked_tojson`, `checked_pprint`), `:368–400` (`fueled_join`), `:443–460` (`fueled_escape`), `:465–479` (`checked_format`), `:755–763` (`json_filter`); patched MiniJinja `minijinja/src/value/ops.rs:942–976` (`deep_item_count`), `filters.rs:1915–1961, 2719–2727`, and `vm/state.rs:774–783` (`charge_serialized`).

There are three compounding gaps in template serialization bounds:

1. **String lengths inside containers are never counted before allocating.** Both `fits_printed` (`src/template.rs:293–330`) and MiniJinja's `deep_item_count` count only container items (`budget = MAX_RENDERED_BYTES / 3 ≈ 5.6M` items) and ignore the byte length of string elements inside a list or map. A 59-byte template creates one string `s` and lazily repeats `[s] * N` (costing $O(N)$ item fuel, **not** $N \times |s|$ byte fuel). Serializing `[s] * N` into a single `String` passes `check_printable`, allocates $N \times |s|$ bytes in one call **before** the VM charges fuel on the returned value, and piping into `| length` hides the allocation from `BoundedOutput`.
2. **`tojson`, `pprint`, `json`, `format`, `safe`, and `escape`/`e` lose or omit serialized-byte fuel.** `new_environment` (`src/template.rs:228–229`) overrides MiniJinja's internal `tojson_fueled` and `pprint_fueled` with `checked_tojson` / `checked_pprint`, which call the public unfueled `minijinja::filters::{tojson, pprint}` and drop MiniJinja's `charge_serialized` (which charges `(SERIALIZED_BYTE_COST - BYTE_COST)` per output byte). `json_filter` (`:755–763`) calls `serde_json::to_string` with no fuel charge of its own, `checked_format` (`:465–479`) does not budget argument serialized sizes (and multiple `%65535d` specifiers in one format string accumulate), and `fueled_escape` (`:443–460`) skips charging and `check_printable` whenever `value.as_str()` is `None` (a container passed to `| escape` / `| e` is formatted and HTML-escaped up to 6× unchecked `[code]`).
3. **`fueled_join` undercharges nested non-string items.** `src/template.rs:389` charges non-string elements a flat `ITEM_BYTES` (48 B) regardless of whether an element is a list/map containing large strings (the patched MiniJinja's `join` currently stops this via its own fuel charging, but downstream users building against upstream MiniJinja 3.1.0 without the repo patch rely on `fueled_join`).

**Probes (`HtmlTemplate` render with `TemplateContext::default()`, safe allocations ≤ 150 MB):**

- **Low fuel (`fuel = 10_000`, ~640 KB budget):** `{% set s = 'x' * 50000 %}{{ ([s] * 600) | <op> | length }}` — every path below allocated **~30 MB** (~47× the fuel budget) before failing fuel on return (whereas patched `join` stopped in 4.9 ms):
  - `| json`: failed out of fuel after **328.8 ms**
  - `| tojson`: failed out of fuel after **34.6 ms**
  - `| pprint`: failed out of fuel after **252.6 ms**
  - `| string`: failed out of fuel after **250.8 ms**
  - `~ ''`: failed out of fuel after **250.6 ms**
  - `'%s' | format([s] * 600)`: failed out of fuel after **251.7 ms**
  - `| safe`: failed out of fuel after **253.9 ms**
- **Default fuel (`1 << 22 = 4,194,304`):**
  - `{% set s = 'x' * 100000 %}{{ ([s] * 300) | <op> | length }}` (~30 MB intermediate string, nearly 2× `MAX_RENDERED_BYTES`):
    - `| json` → **`Ok("30000901")`** (329.6 ms)
    - `| tojson` → **`Ok("30001200")`** (32.3 ms)
    - `| pprint` → **`Ok("30002403")`** (255.5 ms)
    - `'%s' | format(...)` → **`Ok("30001200")`** (253.9 ms)
    - `| safe` → **`Ok("30001200")`** (252.4 ms)
    - (`| string` and `~ ''` failed fuel after allocating because they retain MiniJinja's `charge_serialized`.)
  - `{% set s = 'x' * 100000 %}{{ ([s] * 1500) | json | length }}`: allocated **~150 MB** in **1.66 s** before failing fuel on the returned `String`. Increasing `1500` to `100000` (still 100k items ≪ 5.6M) would attempt a ~10 GB single-call allocation and OOM-abort the process (not run).
  - `{{ ('%(a)65535d' * 500) | format(a=1) | length }}`: succeeded at default fuel with **`Ok("32767500")`** (32.7 MB).

**Impact:** an untrusted template under 100 bytes can allocate gigabytes in a single filter/operator call before fuel is charged, aborting the process via OOM even when `HtmlTemplateLimits::fuel` is explicitly lowered.

**Fix direction:**
- Make `fits_printed` / `check_printable` (and MiniJinja's `deep_item_count`) budget **estimated output bytes**—counting `text.len()` for strings (and accounting for JSON/HTML/Debug quoting expansion) plus per-item overhead—and cap that budget by **both** `MAX_RENDERED_BYTES` and remaining fuel (`state.fuel_levels()`), stopping early before allocating.
- Charge serialized bytes up front in `json_filter`, `checked_tojson`, `checked_pprint`, `checked_format` (including accumulated specifier widths and container arguments), `fueled_escape` (handle non-string values too), and `safe`, or route `tojson`/`pprint` through MiniJinja's fueled implementations after byte-budgeting.
- Estimate nested non-string items in `fueled_join` instead of charging flat `ITEM_BYTES`.

---

### N03 · medium · `HtmlTemplateLimits` is ignored for templates loaded in `Startup` `[probe]`

**Location:** `src/lib.rs:350–362` (`BevyMarkupPlugin::build`), `src/template.rs:544–602` (`HtmlTemplateLimits`, `TemplateFuel`, `sync_template_fuel`).

`BevyMarkupPlugin::build` creates `TemplateFuel::default()` (`1 << 22`) and registers `sync_template_fuel` only in `PreUpdate`. In Bevy's `MainScheduleOrder`, `Startup` runs **before** `PreUpdate` on frame 0. Consequently, an app following the documented pattern—`.insert_resource(HtmlTemplateLimits { fuel: ... })` during app setup and `asset_server.load(...)` in a `Startup` system—dispatches template loads while the loader's atomic `TemplateFuel` still holds the default `1 << 22`.

**Probe:** a headless Bevy app with `BevyMarkupPlugin`, `.insert_resource(HtmlTemplateLimits { fuel: 100 })`, and a `Startup` system loading `{% for i in range(500) %}x{% endfor %}` settled as **`RenderedHtml::Ready`** (500 bytes rendered) instead of failing out of fuel.

**Impact:** lowering `HtmlTemplateLimits::fuel` for untrusted templates fails silently on startup loads, and raising `HtmlTemplateLimits::fuel` for legitimate large templates also fails to take effect on startup loads.

**Fix direction:** sync `HtmlTemplateLimits` into `TemplateFuel` in `BevyMarkupPlugin::finish` (which runs after all app `.insert_resource(...)` calls) and also schedule `sync_template_fuel` in `PreStartup` (for apps that mutate `HtmlTemplateLimits` at startup before loading), while keeping the `PreUpdate` system for runtime changes.

---

### N04 · medium · `font-size` and `outline-offset` silently drop non-`px` absolute CSS units (`pt`, `in`, `cm`, `mm`, `pc`, `Q`) `[probe]`

**Location:** `src/css/property.rs:1435–1455` (`font_size`), `src/css/property.rs:474–480` (`outline-offset`); contrast with `src/css/property.rs:1221` (`letter_spacing`) and `src/css/value.rs:119` (`unit_val`).

In ADR 0048's CSS parser, `length_unit` converts `in`, `cm`, `mm`, `Q`, `pt`, and `pc` into CSS pixels tagged `Unit::Abs` (separate from `Unit::Px`). While `unit_val` maps `Unit::Px | Unit::Abs => Val::Px(value)`, `font_size` (`:1436`) and `outline-offset` (`:475`) only match `Lp::Dim(px, Unit::Px)` and drop `Unit::Abs` as unsupported. Inconsistently, `font-size: calc(24pt)` works because `evaluate` folds it via `unit_val` into `Val::Px(32.0)`.

**Probe (`TestUi`):**

- `#pt { font-size: 24pt; }` → rendered font size **`16px`** (dropped; expected `32px`)
- `#in { font-size: 0.5in; }` → rendered font size **`16px`** (dropped; expected `48px`)
- `#calc { font-size: calc(24pt); }` → rendered font size **`32px`**
- `#off { outline: 2px solid red; outline-offset: 6pt; }` → `outline=2,0,#ff0000` (offset dropped to `0px`; expected `8px`)

**Impact:** standard print/typographic CSS units (`pt`, `in`, `cm`, `mm`, `pc`, `Q`) work in `width`, `padding`, `letter-spacing`, and `calc()`, but silently fail on `font-size` and `outline-offset`.

**Fix direction:** match `Lp::Dim(px, Unit::Px | Unit::Abs)` in both `font_size` and `outline-offset` (and check any other `Unit::Px`-only matches across `src/css/property.rs`).

---

### N05 · medium · Negative `calc()` values bypass `non_negative_lp` and reach Bevy UI `Node` unclamped `[probe]`

**Location:** `src/css/property.rs:538–545` (`non_negative_lp`), `src/build.rs:4349–4353` (`resolve_calc`).

`non_negative_lp` checks `*v < 0.0` for `Lp::Dim` and `Lp::Percent`, but unconditionally returns `negative = false` for `Lp::Calc(_)`. Accepting `calc(-10px)` at parse time matches CSS Values 4, but the spec requires clamping negative `calc()` results to `0` at computed/used-value time for non-negative properties (`padding`, `border-width`, `gap`, `width`/`height`, `min-*`/`max-*`, `border-radius`, `font-size`). Neither `non_negative_lp` (for folded constant `CalcExpr::Val`) nor `resolve_calc` clamps the result.

**Probe (`TestUi`):**

```css
#box {
  padding: calc(-10px);
  border-width: calc(-5px);
  gap: calc(-8px);
  width: calc(-20px);
  border-radius: calc(-6px);
}
```

Resulting Bevy `Node` values: `padding = Px(-10.0)`, `border = Px(-5.0)`, `row_gap = Px(-8.0)`, `width = Px(-20.0)`, `border_radius = Px(-6.0)` (all negative and unclamped).

**Impact:** negative `calc()` values propagate into Bevy's Taffy layout and rounded-corner geometry, bypassing the invariant `non_negative_lp` enforces for plain literals.

**Fix direction:** clamp folded `CalcExpr::Val` values to `>= 0.0` (or mark non-negative properties so `resolve_calc` clamps layout-resolved `CalcExpr` results to `0.0.max(...)`), and add regression tests for both folded (`calc(-10px)`) and mixed-unit (`calc(10px - 100%)`) negative `calc()` expressions.

---

### N06 · medium · `HtmlUi` / `HtmlModal` nested under a `Display::None` app container still takes focus, traps modal scope, and activates hidden buttons `[probe]`

**Location:** `src/focus.rs:402–419` (`active_modal`), `src/focus.rs:534–552` (`displayed_none`).

`review_0009` M04 (`bug_0242`) fixed `display: none` inside an `HtmlUi` tree, but left a boundary gap when an `HtmlUi` (or `HtmlModal`) is mounted as a child of an app-owned Bevy UI container toggled with `Node { display: Display::None, .. }`:

- `displayed_none` (`src/focus.rs:547–549`) stops walking `parents.iter_ancestors(entity)` as soon as it reaches `if roots.contains(ancestor) { break; }`, ignoring `Display::None` on any ancestor above the `HtmlUi` root.
- `active_modal` (`src/focus.rs:408–413`) checks `visible_in_hierarchy` across the full `ChildOf` chain, but checks `node.display != Display::None` **only on the modal root entity itself**, not its ancestors.

**Probe (`TestUi::with_pointer`):** with a visible `HtmlUi` containing `#vis_btn` and a second `HtmlModal` `HtmlUi` containing `#hidden_btn` spawned as a child of an app container `Node { display: Display::None, .. }`:

- `InputFocus` focused `hidden_btn`
- `HtmlFocus::tab_order()` returned `[Some("hidden_btn")]`, locking `#vis_btn` out of keyboard/gamepad navigation
- `HtmlFocus::activate(ActivationInput::Synthetic)` emitted `["hidden_click"]`

**Impact:** apps that toggle panels or modal drawers by setting `Node::display = Display::None` on a wrapper entity end up with an invisible modal trapping focus and firing clicks on hidden controls.

**Fix direction:** remove the `if roots.contains(ancestor) { break; }` early exit from `displayed_none` so it walks the full `ChildOf` chain (just like `visible_in_hierarchy`), and use `displayed_none` in `active_modal` (and any root-eligibility check) instead of checking `nodes.get(entity)` on the root alone.

---

### N07 · medium · Regression (ADR 0049): Markdown block dedenting omits MiniJinja's `macro`, `call`, `with`, and `autoescape` tags `[probe]`

**Location:** `src/md.rs:210–228` (`block_tag`).

`block_tag` decides which `{% ... %}` tags open and close dedented template blocks in `.md` templates. After ADR 0049 replaced Tera with MiniJinja, `block_tag` still lists Tera's removed `"component"` and `"set_global"` tags, but omits MiniJinja's block tags: `"macro"`, `"call"`, `"with"`, and `"autoescape"` (and their `end*` closers).

Because `separate_statements` inserts blank lines around own-line `{% ... %}` tags, any 4-space indented body inside `{% macro %}`, `{% call %}`, `{% with %}`, or `{% autoescape %}` in a `.md` template is **not** dedented and is therefore parsed by `pulldown-cmark` as a CommonMark indented code block (`<pre class="markdown"><code>`), disabling Markdown formatting and suppressing template interpolation inside it.

**Probe (`TestUi`):** rendering a `.md` template containing:

```jinja
{% macro card(title) %}
    **{{ title }}**
{% endmacro %}
{% call box() %}
    Some **bold** text
{% endcall %}
```

produced:

```html
<pre class="markdown"><code>**{{ title }}**
</code></pre>
<div>
<pre class="markdown"><code>Some **bold** text
</code></pre>
</div>
```

instead of rendered `<strong>` elements with interpolated `title`.

**Impact:** idiomatic indented `{% macro %}`, `{% call %}`, `{% with %}`, and `{% autoescape %}` blocks in `.md` templates silently render as literal code blocks.

**Fix direction:** add `"macro"`, `"call"`, `"with"`, and `"autoescape"` (and their `end*` closers) to `block_tag`, remove Tera-only `"component"` / `"set_global"`, and add Markdown dedent + render tests for all four MiniJinja block tags.

---

### N08 · low/medium · CSS parser & cascade regressions after ADR 0048 (`background`, `grid`, `transition`) `[probe]`

Grouped smaller spec-conformance regressions introduced in the ADR 0048 CSS parser rewrite, each confirmed by `TestUi` / stylesheet probes:

1. **Multi-layer `background` discards the final layer's `background-color`:** `src/css/property.rs:1596–1605` reads `layers[0]` and overwrites `color` with transparent `(0, 0, 0, 0)` whenever `layers.len() > 1`, instead of reading `background-color` from the final layer (CSS Backgrounds 3 allows `<bg-color>` only on the final layer). Probe: `background: url('missing.png'), #ff0000;` resolved to `bg=#00000000`.
2. **`background: initial` / `background-image: initial` are no-ops:** `src/cascade.rs:2097–2261` (`apply_keyword`) resets `"background-color"` but omits `"background"`, `"background-image"`, `"background-repeat"`, and `"background-clip"`. Probe: `.red { background: #ff0000; } .clear { background: initial; }` retained `bg=#ff0000`.
3. **Non-rectangular `grid-template-areas` accepted:** `src/cascade.rs:2991–3031` (`parse_areas`) records each area name's min/max bounding box without checking that every cell inside that rectangle carries the same area name (CSS Grid 1 requires rejecting non-rectangular areas; `lightningcss` previously rejected them). Probe: `grid-template-areas: 'a b a';` was accepted and placed `#a` across `grid-col=1..4`, overlapping `#b`.
4. **`repeat(0, ...)` accepted in grid track lists:** `src/css/grid.rs:221–222` and `src/cascade.rs:2850` allow `0` (invalid per CSS Grid 1, which requires `<positive-integer> >= 1`) and pass `GridTrackRepetition::Count(0)` to Bevy. Probe: `grid-template-columns: repeat(0, 1fr);` produced `GridTrackRepetition::Count(0)` on `Node`.
5. **`transition` accepts empty comma-separated items:** `src/css/property.rs:1850–1899` never checks that an entry consumed at least one component value, so trailing/empty items parse as `all 0s`. Probe: `transition: color 1s, ;` was accepted as a valid declaration.

**Fix direction:**
- Take the final layer's color in `background` (and reject colors on non-final layers).
- Add `"background"`, `"background-image"`, `"background-repeat"`, and `"background-clip"` to `apply_keyword`.
- Verify every cell in `[r1..=r2] × [c1..=c2]` belongs to `name` in `parse_areas` (or validate at parse time in `src/css/grid.rs`).
- Reject `n == 0` in `track_repeat`.
- Require each `transition` comma item to consume at least one token (matching `background_layer`'s `if !any { return invalid(entry); }` pattern).

---

### N09 · low · Stale Tera and `svg-text`/filter metadata in `Cargo.toml`, `CHANGELOG.md`, and crate docs `[code]`

**Locations:**

- `Cargo.toml:6`: crates.io `description` still reads `"Markdown+HTML templates (Tera), CSS and Fluent localization as native Bevy UI"` after ADR 0049 replaced Tera with MiniJinja.
- `CHANGELOG.md:34, 54–57, 70–92`: under `## [Unreleased]` → `### Added`, still lists the removed `svg-text` feature, SVG `<text>`, and `feTurbulence`/`feConvolveMatrix`/`feMorphology`/`feGaussianBlur`/`feDropShadow`/pattern/mask/filter entries as added in the upcoming release, contradicting `### Changed` / `### Removed` in the same unreleased section (ADR 0051).
- `src/lib.rs:1, 6–7, 123, 316`, `src/l10n.rs:14`, `src/signals.rs:20`: user-facing rustdoc still names Tera, including the crate's top heading (`# bevy_markup — HTML + CSS + Fluent + Tera as Bevy UI`) and `HtmlUiSystems::Render`.

**Impact:** misleading crates.io description, crate-root docs, and upcoming release notes after the ADR 0049 and ADR 0051 breaking changes.

**Fix direction:** update `Cargo.toml:6` and the user-facing rustdoc comments from Tera to MiniJinja, and consolidate the `## [Unreleased]` `### Added` SVG entries in `CHANGELOG.md` so the unreleased section describes the actual icon subset being released.

---

## Recommended order

1. **N01** (`calc()` flat-chain stack overflow) and **N02** (container-of-strings template serialization / fuel bypass): both are high-severity untrusted-input bounds failures that can abort the process (`SIGABRT` / OOM) from small inputs.
2. **N03** (`HtmlTemplateLimits` ignored on `Startup` loads) and **N06** (`Display::None` app ancestor bypassed by focus/`HtmlModal`): silent security/runtime correctness bugs affecting normal app setups.
3. **N04** (`font-size`/`outline-offset` dropping `Unit::Abs`), **N05** (unclamped negative `calc()`), and **N07** (Markdown dedenting missing MiniJinja block tags): user-visible regressions from ADRs 0048 and 0049.
4. **N08** (`background`/`grid`/`transition` CSS regressions) and **N09** (stale Tera/`svg-text` docs, manifest, and changelog).

All nine findings remain open in this report; no fixes or separate bug reports were filed during this review.

---

## Addendum — independent verification (Claude Opus 5.5, 2026-11-28)

*Everything above this line is the original review by Gemini, unchanged.
This section was written afterwards by a different reviewer, who
re-checked each finding from scratch: first by reading the code at the
cited locations, then with probes of its own (not the original
`review10_probes.rs`, which was deleted and not seen).*

| Field | Value |
|---|---|
| Verifier | Claude Opus 5.5 (`claude-opus-5-5`), Claude Code |
| Date | 2026-11-28 |
| Commit verified | `ec1e43c7832903ab2503ddac4b7471206f9fc06d` (the commits after `3b4253d` — bug_0256's `tojson` indent accounting, the MiniJinja fork bump to `db625d2e`, `cargo update` — touch none of the cited code paths except `checked_tojson`'s indent parsing) |
| Toolchain | rustc 1.99.1 (58cf8f9c3 2026-10-13), Linux 7.2.14-arch1-1 x86_64, debug build, `CARGO_PROFILE_DEV_DEBUG=0` |
| Result | All nine findings corroborated. Seven reproduced by independent probes. N08.5 and N09 confirmed by reading the code. Two findings turned out wider than reported (N01, N02 item 3), and one claim is weaker than stated (N01's quadratic cost). |

### Method

- **Code reading:** every cited location was read at the verified commit.
  The descriptions match the code: `parse_sum`/`parse_product` build
  left-skewed trees whose depth is the operator count, and `evaluate` and
  `expr_is_finite` recurse over them. `fits_printed` counts items, never
  string bytes. `json_filter`, `checked_tojson` and `checked_pprint`
  charge no fuel for their output. `sync_template_fuel` runs only in
  `PreUpdate`. `font_size` and `outline-offset` match `Unit::Px` only.
  `non_negative_lp` lets every `Lp::Calc` through. `displayed_none` stops
  at the `HtmlUi` root, and `active_modal` checks `display` on the root
  alone. `block_tag` lists Tera's keywords. `background` takes layer 0's
  color. The transition entry loop accepts an item that consumes nothing.
- **Probes:** one temporary integration test (`tests/review10_recheck.rs`,
  built with `--features fuzzing`, run with `--test-threads=1`, deleted
  afterwards). A counting `#[global_allocator]` recorded each template
  probe's peak heap growth. Stack cases re-executed the test binary as a
  child process, which ran `bevy_markup::fuzz::cascade` on a 2 MB thread,
  so an abort showed up as a signal exit.

### Per-finding results

**N01 — confirmed, wider than reported.** Each stylesheet below is
`p { width: calc(<chain>); }`. Child-process results on a 2 MB thread:

| Chain | Terms | Bytes | Result |
|---|---|---|---|
| `1px + 1% + …` | 250 | 1,398 | ok, 4.5 ms |
| `1px + 1% + …` | 500 | 2,773 | ok, 8.8 ms |
| `1px + 1% + …` | 1,000 | 5,523 | ok, 30.0 ms |
| `1px + 1% + …` | 1,400 | 7,723 | **stack overflow (abort)** |
| `1px * 1 * …` | 1,000 | 4,023 | ok, 1.7 ms |
| `1px * 1 * …` | 2,000 | 8,023 | **stack overflow (abort)** |
| `1px + 1px + …` (one unit) | 3,000 | 18,023 | **stack overflow (abort)** |

- **Same-unit chains:** these also overflow. Folding into one value
  doesn't help, because parsing and evaluating recurse before anything
  folds. The fix needs the term cap or a flat representation, not only
  the single-collapse change.
- **The quadratic claim:** only weakly supported. Time grows somewhat
  faster than linearly (×2.0, then ×3.4 per doubling), and is 30 ms at
  1,000 terms. The stack overflow is the defect that matters.

**N02 — confirmed, high.** Templates were rendered with
`template::new_environment(fuel)`, the environment every loader uses:

| Fuel | Template (`s = 'x' * 100000` unless noted) | Result | Peak heap | Time |
|---|---|---|---|---|
| default | `([s] * 300) \| join \| length` | `Ok(30000000)` | 81.3 MB | 6.3 ms |
| default | `([s] * 300) \| json \| length` | `Ok(30000901)` | 81.3 MB | 340.6 ms |
| default | `([s] * 300) \| tojson \| length` | `Ok(30001200)` | 81.3 MB | 33.6 ms |
| default | `([s] * 300) \| pprint \| length` | `Ok(30002403)` | 81.3 MB | 260.3 ms |
| default | `([s] * 300) \| string \| length` | out of fuel | 51.3 MB | 255.3 ms |
| default | `(([s] * 300) ~ '') \| length` | out of fuel | 81.3 MB | 256.8 ms |
| default | `'%s' \| format([s] * 300) \| length` | `Ok(30001200)` | 81.3 MB | 255.2 ms |
| default | `([s] * 300) \| safe \| length` | `Ok(30001200)` | 81.3 MB | 268.8 ms |
| default | `([s] * 300) \| e \| length` | `Ok(30004200)` | 102.5 MB | 337.8 ms |
| default | `('%(a)65535d' * 500) \| format(a=1) \| length` | `Ok(32767500)` | 66.3 MB | 4.0 ms |
| default | `([s] * 1200) \| json \| length` | `Ok(120003601)` | 324.9 MB | 1.4 s |
| default | `([s] * 3000) \| join \| length` | out of fuel | **0.2 MB** | 0.75 ms |
| default | `([s] * 3000) \| json \| length` | out of fuel | **709.7 MB** | 3.3 s |
| default | `([s] * 3000) \| tojson \| length` | out of fuel | **709.7 MB** | 335.6 ms |
| 10,000 | `([s'] * 600) \| join \| length`, `s' = 'x' * 50000` | out of fuel | 0.1 MB | 0.2 ms |
| 10,000 | `([s'] * 600) \| json \| length` | out of fuel | 81.3 MB | 328.7 ms |
| 10,000 | `([s'] * 600) \| tojson \| length` | out of fuel | 81.3 MB | 33.6 ms |
| 10,000 | `([s'] * 600) \| string \| length` | out of fuel | 51.3 MB | 254.0 ms |

What these show:

- **Why 30 MB doesn't prove it:** at default fuel, a 30 MB result is
  within the fuel's byte budget (2²² units × 64 bytes ≈ 268 MB), so it
  says nothing on its own. The decisive rows are the 3,000-item ones.
  The up-front-charging `join` refuses before allocating, while `json`
  and `tojson` build the whole 300 MB string (peaking at 710 MB, about
  2.4× the output) and only then fail fuel on the returned value.
- **What stays out of the budget:** memory scales with item count times
  string length, and only the item count is budgeted (`MAX_RENDERED_BYTES
  / 3` items). The template's own cost stays small: `s` costs
  `len / 64` fuel, and the repeat costs one unit per item. Out-of-memory
  from a short template follows; it wasn't triggered.
- **`string` and `~`:** these fail fuel, but only after allocating.
  MiniJinja's `charge_serialized` charges what was already serialized.

**N02 item 3 — confirmed, and the review understated it.** The review says
the patched MiniJinja's `join` stops nested items. It doesn't stop them
before allocating:

- `[[s] * 300] | join | length` at fuel 10,000 (a 640 KB byte budget)
  allocated 51.3 MB, then failed out of fuel.
- `fueled_join` charges the nested list a flat `ITEM_BYTES`. The patched
  built-in then stringifies the inner list without charging for it first.

**N03 — confirmed.**

- **Probe:** `TestUi` with `HtmlTemplateLimits { fuel: 100 }`, inserted
  before any frame, and a page of `{% for i in range(500) %}x{% endfor %}`.
- **Load before the first frame:** `RenderedHtml::Ready` (500 bytes).
  This is a `Startup` system's position, since `Startup` runs before
  `PreUpdate` on frame 0.
- **Same load after one frame:** `Failed("engine ran out of fuel")`.

**N04 — confirmed exactly.**

- `font-size: 24pt` and `0.5in` render at 16 px (dropped).
- `calc(24pt)` and `32px` render at 32 px.
- `outline: 2px solid red; outline-offset: 6pt` dumps as
  `outline=2,0,#ff0000` (the offset is dropped).

**N05 — confirmed exactly.**

- The stylesheet: `#box { padding: calc(-10px); border-width: calc(-5px);
  gap: calc(-8px); width: calc(-20px); border-radius: calc(-6px); }`.
- The `Node` it gives: `padding` `Px(-10.0)`, `border` `Px(-5.0)`,
  `row_gap` `Px(-8.0)`, `width` `Px(-20.0)`, corner radius `Px(-6.0)`
  on both axes.
- Control: a plain `padding: -10px` is dropped, as it should be.

**N06 — confirmed exactly.**

- **Setup:** a visible `HtmlUi` with
  `<button id="vis_btn" data-on-click="vis">`, and an `HtmlModal` `HtmlUi`
  with `<button id="hidden_btn" data-on-click="hidden" autofocus>`. The
  modal is spawned as a child of an app entity with
  `Node { display: Display::None, .. }`.
- **Focus:** `InputFocus` goes to `hidden_btn`.
- **Tab order:** `HtmlFocus::tab_order()` returns only `[hidden_btn]`.
- **Activation:** `activate(ActivationInput::Synthetic)` emits the
  `hidden` signal.

**N07 — confirmed.**

- **Probe:** a `.md` page with an indented `{% macro %}` body, an
  indented `{% with %}` body and, as a control, an indented `{% if %}`
  body.
- **Result:** the `macro` and `with` bodies render as
  `<pre class="markdown"><code>…</code></pre>`, with the macro's
  `{{ title }}` left literal. The `if` body renders as a paragraph with
  `<strong>`.
- **Not probed:** `call` and `autoescape`. They are absent from the same
  keyword match in `block_tag`, so they take the same path.

**N08 — confirmed, low.**

1. **Multi-layer `background`:**
   `#a { background: url('frame.png'), #ff0000; }` dumps
   `bg=#00000000 bg-image=frame.png`. CSS gives the final layer's
   `#ff0000`. (`frame.png` replaced the review's `missing.png`: a sheet
   whose `url()` image is missing never settles in `TestUi`, as AGENTS.md
   notes.)
2. **`initial` on `background`:**
   `.red { background: #ff0000 } .clear { background: initial }` keeps
   `bg=#ff0000`. Control: `background-color: initial` clears it.
3. **Non-rectangular areas:** `grid-template-areas: 'a b a'` is accepted
   with three auto columns. `#a` gets `GridPlacement { start: 1, end: 4 }`
   and overlaps `#b`'s `2..3`.
4. **`repeat(0, …)`:** `grid-template-columns: repeat(0, 1fr)` reaches
   `Node` as `RepeatedGridTrack { repetition: Count(0), … }`.
5. **Empty `transition` items (code reading only):** the probe compared
   cascade output for `transition: color 1s, ;` and `transition: color 1s;`
   and was inconclusive (that output doesn't show transitions). In the
   code, the per-entry loop in `transition` (`src/css/property.rs:1849`)
   breaks at once on an empty item and returns `("all", 0.0, 0.0, Ease)`.
   `parse_comma_separated` doesn't reject empty items, so the declaration
   is accepted with an extra `all 0s` entry.

**N09 — confirmed by code.**

- `Cargo.toml:6` still says `"Markdown+HTML templates (Tera), …"`.
- Tera appears in `src/lib.rs:1, 6, 7, 123, 316`, `src/l10n.rs:14, 1062`
  and `src/signals.rs:20`.
- CHANGELOG's `[Unreleased]` "Added" section still describes `svg-text`
  and SVG `<text>` (lines 34–37 and 54–55) and the filter primitives
  (from line 70). The same section removes `svg-text` at line 156.

### Assessment

- **Severities:** the original review's severities and recommended order
  stand. N01 and N02 each abort the process from a small untrusted input
  (ADR 0044), N02 at the default fuel.
- **Corrections for the fixes:**
  - N01's fix must cover same-unit chains: cap the terms, or flatten the
    tree.
  - N02's fix must charge or estimate bytes *before* serializing in
    `json`, `tojson`, `pprint`, `format`, `safe` and `escape`. It must
    also cover nested values in `join`, and the post-hoc charges in
    `string` and `~` (those two live in MiniJinja and belong in
    `UPSTREAM.md`).
- **Status:** none of the nine findings is filed in `docs/agents/bugs/` or
  fixed as of `ec1e43c`.
