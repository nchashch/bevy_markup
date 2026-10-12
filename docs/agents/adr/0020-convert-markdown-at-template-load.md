# 20. Convert Markdown at template load, not per render

| Field | Content |
|---|---|
| ADR | 0020 |
| Title | Convert Markdown at template load, not per render |
| Date | 2026-10-12 08:23 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp — drafted for the project owner |
| Commit | `975266e` Remove debug print out line tanking performance for Markdown |
| Status | Accepted — Decisions 1–2 implemented (Tera-aware filter in `md.rs` `to_html_template`, the loader converts at load, `HtmlTemplate::render` is Tera-only); Decision 3 (Fluent values) deferred and now measured as the dominant remaining cost |
| Related | 0014 (the semantics this changes), 0018/0019 (the measurements), `md.rs` |

## Context

Markdown is the crate's slowest per-frame path, measured by the
criterion/divan suites (ADR 0018/0019; a 600-section document, one
`app.update()` per iteration, warm-up 0.5 s, measurement 1 s):

| Workload (600 elements) | Per frame |
|---|---|
| `frame/changing` (plain HTML, one value) | 1.93 ms |
| `frame/localized` (Fluent values) | 2.42 ms |
| `frame/markdown` (a `.md` template + `markdown: true` values) | 13.11 ms |

The gap decomposes into two conversion sites, both per frame:

1. **`.md` templates**: `template.rs` `HtmlTemplate::render` runs
   `md::to_html` over the whole Tera-rendered document on every render —
   the template's Markdown is static, only its Tera variables change, yet
   pulldown-cmark re-parses and re-emits all of it per frame.
2. **`markdown: true` Fluent values** (`l10n.rs` `resolve_all`): each
   formatted message converts per frame, ~11 µs apiece — ~6.8 ms at 600
   values.

(A third cost was a debug print removed in `975266e`, worth ~30% of the
markdown workload — the suites caught it.)

An input-keyed memo cache misses by construction in the dynamic case: the
converted input is the *rendered* Markdown, whose context values change
per frame. The only cache that hits is one taken before the values
arrive — converting the template's Markdown once, at load.

The blocker for doing that today is the interleaving: `.md` templates
carry Tera syntax (`{% for %}`, `{% if %}`, `{{ }}`), and converting
Markdown first wraps Tera-only lines in `<p>` tags and would escape what
they emit — a `{% for %}` inside a paragraph breaks across the loop body.
Markdown values interpolated through `{{ }}` are, per ADR 0014, themselves
parsed as Markdown (a value's `*` reads as emphasis).

## Decision (proposed)

Convert a `.md` template's Markdown **once, at asset load**, keeping Tera
syntax intact, so the per-frame path is Tera-only.

1. **A Tera-aware event filter in `md.rs`** (`to_html` gains a mode, or a
   sibling `to_html_template`): the pulldown event stream is filtered so
   that Tera syntax survives verbatim —
   - a block-level construct (paragraph/heading/list-item text) that
     consists solely of Tera tags becomes `Event::Html` (raw, no `<p>`
     wrapper, no escaping);
   - inline `{{ … }}`/`{% … %}` runs inside otherwise-plain text become
     inline `Event::Html` runs;
   - **code contexts are excluded**: text inside code blocks and code
     spans is Tera-literal (a code span containing `{{ x }}` must render
     as the literal characters), so the filter tracks code state and
     passes it through escaped;
   - the `.markdown`-class injection (`MARKER`) applies as today — the
     emitted HTML is byte-identical to a per-frame conversion of the same
     template expanded with constant values, minus the Tera text.
2. **The loader stores the converted HTML as the Tera template**
   (`template.rs` loader: `md::to_html_template` instead of the render-time
   branch); `HtmlTemplate::render` drops its markdown branch.
3. **Fluent `markdown: true` values stay per-frame** (phase 2, open): the
   message pattern could pre-convert per variant with placeables as
   literal text, but Fluent's selectors pick patterns per call and the
   win is ~11 µs/value — deferred until a measured need.
4. **Semantic changes, documented as breaking**:
   - a value interpolated through `{{ … }}` is no longer parsed as
     Markdown — `{{ name }}` inside emphasis keeps a literal `*`
     (ADR 0014's "their characters are still Markdown" note is removed);
     what the template itself writes as Markdown is still converted;
   - a Tera loop's body today is converted *per iteration with the
     iteration's values*; after this it is converted *once* with no
     values — the body's Markdown may not depend on loop variables
     (a `{% if %}` selecting between Markdown shapes works; a variable
     interpolating a Markdown *marker* character does not).
   The identical-render check is unaffected: it compares the Tera output
   (now HTML) as before.

## Alternatives considered

- **Memoize `md::to_html` by input** — misses per frame whenever a
  context value changes (the dynamic case the benchmark measures); only
  helps repeated identical values, which the crate cannot assume.
- **Keep per-frame conversion** — the current state (9.2 ms at 600): the
  fallback if the filter's semantics prove too subtle; the numbers above
  are the cost.
- **Cache per Fluent message pattern** (phase 2 above) — deferred, see
  Decision 3.
- **A faster Markdown engine** — pulldown is already streaming, no AST;
  the per-call cost is dominated by re-parsing static content, which is
  the problem, not the engine's speed.

## Consequences

- **Expected gain** `[INFERENCE]`: the markdown workload's per-frame cost
  approaches the localized one plus one Tera render — ~2.5 ms at 600
  (from 9.2 ms), since the whole-document conversion (~7 ms) leaves the
  per-frame path; the Fluent-value conversions remain.
- **Breaking semantics** (Decision 4) — a CHANGELOG **Changed** entry
  marking the interpolation change, and ADR 0014's "still Markdown" note
  updated (ADR 0014 itself stays as written; this ADR supersedes that
  aspect).
- **The filter's correctness surface**: code contexts (Decision 1),
  Tera tags split across pulldown events (soft breaks inside a tag), and
  the class-injection marker interacting with raw passthrough — each
  needs test vectors, checked against a browser or a hand-derived dump
  like the existing Markdown vectors.
- **Per-frame allocations** drop accordingly (divan's counts make the
  delta direct).

## Implementation note (2026-10-12, same day)

Decisions 1–2 are implemented and verified: the `rewrite` pass is a
pure function over the collected event list (the streaming variant was
attempted first and abandoned — paragraph stashing, marker lookahead and
catch-all flushes interleaved into unfixable orderings; the collect-then-
rewrite pass is 60 lines and exhaustively testable), with unit tests for
the pure-Tera, mixed, loop and code-context cases. The full suite passes,
including the markdown vector and the content lint over the `markdown`
example.

The benchmark after: `frame/markdown/600` ≈ 13.5 ms (from 13.1 ms) —
**no visible win in the value-heavy workload**, because Decision 3's
deferral was the wrong call: the 600 per-value `md::to_html` calls
(~11 µs each, ~9 ms) dominate, and the template-side conversion was
~1–2 ms. The template-side win is real (visible at 120: 2.84 → 2.71 ms;
the divan allocation counts drop accordingly) but masked.

**Decision 3 is therefore the next change**, with a design that works:
convert each message's Markdown pattern once at bundle load with unique
placeholder text in place of the placeables, cache the converted HTML per
(message id, selected variant), and substitute the formatted arguments
into the cached HTML per frame — the conversion is identical for any
argument values, so the cache hits every frame. The substitution inherits
Decision 4's semantics (arguments are literal text, not Markdown).
