# 49. Templates on MiniJinja with fuel instead of Tera and a static check

| Field | Content |
|---|---|
| `ADR` | `0049` |
| `Title` | Templates on MiniJinja with fuel instead of Tera and a static check |
| `Date` | 2026-11-25 05:21 +0400 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code |
| `Commit` | `a8d5260 Bump fluent to 0.17 (fluent-syntax 0.12)` |
| `Status` | Accepted (2026-11-25, after the trial on branch `minijinja`; see Results) |
| `Related` | ADR 0044, 0046, 0047, 0048; bug_0182, bug_0220, bug_0223, bug_0224, bug_0227, bug_0239, bug_0240, bug_0241; UPSTREAM U39, U46; U56, U57 (reserved: unfixed, filed privately) |

## Context

ADR 0044 made every input untrusted, templates included. Tera 2.4 bounds
nothing a render does: no limit on work or memory beyond `range` ≤ 10⁵ and
component recursion ≤ 20 (UPSTREAM U39), `unique` is quadratic (U46), and a
tag with a few thousand operators overflowed its parser's stack (bug_0182).
Its parser is private, so bevy_markup bounds templates statically, before
Tera ever renders them:

- `src/tera_check/`: Tera's lexer ported to `nom` (`lexer.rs`), its Pratt
  parser ported (`parser.rs`), and an abstract render (`analysis.rs`) that
  bounds written size, item counts, value sizes and magnitudes as linear
  functions of one app value (ADR 0044, 0046, 0047). **2,423 lines of logic
  and 567 of tests** (counted 2026-11-25: non-blank, non-comment lines,
  `#[cfg(test)]` items and `tests.rs` as tests), the largest single guard in
  the crate, about a tenth of its logic.
- `template::tera_tag_over_limit` (≤ 64 operators per tag, bug_0182) and
  `render_source`'s `BoundedOutput` (16 MiB of output, bug_0220).

The static check has been the main source of template bugs since 0044:
bug_0223 (its own recursion), bug_0224, bug_0227, bug_0239, bug_0240 and
bug_0241 were all cases the model under- or mis-charged. It must follow
Tera's grammar and semantics on every Tera bump, and it refuses templates
by shape rather than by what they actually cost: ADR 0046 refuses a loop
over app data nested in another unless it runs over the inner loop's
element (`NestedAppLoops`), even when the app's data is small.

MiniJinja (3.1.0, the current release, Armin Ronacher's Jinja2
implementation) offers what Tera lacks, built in: `Environment::set_fuel`.
Its `vm/fuel.rs` documents the model, and the measurements below match it:

- every instruction costs 1 fuel;
- every byte written to the output (captured output too: `set` blocks,
  macro bodies) and every byte of strings, and every list or map slot,
  produced by operators, literals, filters, tests, functions and methods,
  costs 1/64 fuel (`UNITS_PER_FUEL` = 64);
- every byte of string input to a filter, every item a built-in filter or
  operator visits (1/2 fuel), and every sort or dedup comparison (1 fuel);
- "allocations whose size is known ahead of time are checked before they
  are performed".

It also has `set_recursion_limit` (≤ 500, macro recursion and parser
nesting), `range` ≤ 100,000 items, and string or sequence repetition ≤ 10⁸.

## Decision

Evaluate replacing Tera with MiniJinja 3.1 with fuel, and, if the
open questions below come out as expected, switch:

1. **Bound renders dynamically.** Every render runs with a fuel budget;
   running out is a render failure (`RenderedHtml::Failed`, as a Tera
   error is today). The budget is app-configurable — a resource, like
   `HtmlFontLimits` — with a default calibrated from the measurements
   below. `BoundedOutput` stays.
2. **Delete `src/tera_check/`** and its analysis. Keep whatever small
   load-time pre-check the unfixed upstream issue U57 still needs (a
   template that fails before rendering starts, so fuel can't see it), in
   the manner of `tera_tag_over_limit`.
3. **Close the budget's gaps.** Unfixed upstream issue U56 lets a render
   exceed its fuel budget's memory. Until upstream fixes it, bevy_markup
   overrides the affected built-in with one that checks sizes first
   (MiniJinja lets an environment replace a built-in filter).
4. **Migrate the template surface** (breaking; see Consequences): Tera 2
   components become macros called with `{% call %}`, `TemplateContext`
   wraps MiniJinja's context value instead of `tera::Context`, the `tera`
   re-export becomes a `minijinja` one, and the `json` filter is
   re-registered.

## Measurements

Method: a scratch binary on `minijinja` 3.1.0 (`fuel`, `json`, `serde`,
`loop_controls` features), release build unless noted, one process per
case under `prlimit --as=8 GiB` and a 120 s timeout; time and peak RSS
from `/usr/bin/time`. The context is a list of 1,000 integers (`items`).

| Template | Fuel | Result | Time | Peak RSS |
|---|---|---|---|---|
| `range(100000)` × `range(100000)` loops | 10⁶ / 10⁷ / 10⁸ | out of fuel | 0.02 / 0.12 / 1.16 s | 4 / 6 / 29 MiB |
| `items` × `items` × `items` loops | 10⁶ / 10⁷ / 10⁸ | out of fuel | 0.01 / 0.12 / 1.23 s | 4 / 6 / 29 MiB |
| macro fan-out `m(n-1)` twice, depth 40 | 10⁶ / 10⁷ / 10⁸ | out of fuel | 0.02 / 0.24 / 2.39 s | 4 MiB |
| `namespace` string doubling, 40 rounds | 10⁶ / 10⁷ / 10⁸ | out of fuel | 0.00 / 0.11 / 0.76 s | 44 / 644 / 5,124 MiB |
| 10 MB string through `upper` 1,000× | 10⁶ / 10⁷ / 10⁸ | out of fuel | ≤ 0.26 s | 32 MiB |
| `{{ items \| join(",") }}` (ordinary) | 5·10⁴ | 3,889 bytes | 0.00 s | — |

Fuel bounds time at roughly 10–25 ns per fuel and memory at roughly
fuel × 64 bytes (doubling: 644 MiB at 10⁷), except in one case (U56).

Stack, on a 2 MB thread (what Bevy's task pools and the test harness use),
debug and release: 499 nested `(`, `{% if %}` or `{% for %}` are refused
("template exceeds maximum recursion limits"), and macro recursion to
depth 10,000 stops at the recursion limit. One kind of input still
overflows the stack and aborts the process (U57); its details stay in the
private report until upstream fixes it.

Templates in use (`examples/assets`, `tests`, `src` doc examples,
2026-11-25): 17 component calls (`{% <ui.button … /> %}`) and one
component definition (`examples/assets/shared/components.html`), 54
`| json`, 4 `| title`, 2 `| length`, 1 `| upper`; no keyword-argument
filters, no `set_global`, no `{% filter %}`/`break`/`continue`; 21
includes and 4 `extends`, which `resolve_references` already rewrites to
resolved names.

## Alternatives considered

- **Keep Tera and the static check** (status quo). It works and refuses
  bad templates at load, before any data exists. But it costs ~2,400 lines
  that must mirror Tera's grammar and evaluation, has had six
  undercharging bugs, and refuses legitimate shapes by syntax (ADR 0046).
- **Runtime limits in Tera.** Tried in ADR 0044's session: a vendored
  Tera with an instruction budget, a value-size cap and byte accounting
  bounded everything at run time, but cost 13% of render time (~10%
  batched), and `[patch]` doesn't reach apps that depend on bevy_markup,
  so they would run unpatched Tera. Upstreaming the same accounting is the
  right fix for Tera users, but it isn't something bevy_markup can wait
  for. MiniJinja ships its accounting in the released crate, so the
  `[patch]` objection doesn't apply; the runtime cost does, and is open
  question 1.
- **MiniJinja without fuel, plus a static check.** Porting the analysis
  to another grammar keeps every cost of the status quo.
- **Run renders under a watchdog** (a thread with a deadline, an
  allocation cap). Time can't be bounded safely without cooperation from
  the engine, and memory can't be capped per thread in Rust's global
  allocator without a custom one.
- **Our own template engine.** Not pursued: unlike ADR 0048's CSS values,
  a template language with includes, inheritance, macros and an
  expression language is far more than bevy_markup should maintain, and
  users would get a dialect no other tool knows.

## Consequences

If adopted:

- **Gained.** About 2,400 lines of guard logic and 570 of tests go, and
  with them the whole class of "the model undercharged" bugs. Bounds hold
  by construction, per render, against real data instead of a model of it.
  Templates the static check refused by shape (nested app loops over
  small data) render. MiniJinja is actively developed, with fuel as a
  first-class feature.
- **Breaking for users.**
  - Tera 2 components (`{% component %}`, `{% <name … /> %}` calls,
    typed arguments with defaults, `body`) have no MiniJinja equivalent:
    they become macros (`{% macro %}`, `{% call %}` with `caller()`).
  - Tera-only syntax and filters differ (Tera's keyword-argument filters
    such as `default(value=…)`, `set_global`); every app template needs
    checking. The examples use none of them except components.
  - `TemplateContext` (`pub tera::Context`, Deref to it) and the `tera`
    re-export change type; apps that build a `tera::Context` must change.
- **Moved, not removed.** A template is no longer refused at load for
  what it *could* cost; it fails at render when the data makes it cost
  too much. Authors see the failure later (only with large data), and
  the fuel default has to fit legitimate large documents: 16 MiB of
  output alone is ~262,000 fuel.
- **Still needed.** The pre-check for U57 (decision 2), `BoundedOutput`,
  and an audit of the built-ins for gaps like U56 (decision 3). U56 and
  U57 are filed privately and reserved in `UPSTREAM.md`'s numbering; their
  reports go to MiniJinja's maintainer.
- **Elsewhere in the crate.** `build.rs` reads `tera::Value` for context
  `var()`s; `md.rs` keeps template syntax verbatim through the Markdown
  conversion and knows Tera's block names (`dedent_blocks`,
  `separate_statements`); the fuzz harness and `tests/content_lint.rs`
  build Tera environments. All need porting.

Open questions before acceptance (answered in Results below):

1. Port the examples' templates and the template test vectors, and
   measure: render time against Tera (the criterion and divan benches —
   ADR 0044 dropped runtime accounting in Tera at a 10–13% cost, so this
   is the number that decides), and the fuel the largest real template
   needs.
2. Audit MiniJinja's built-ins for gaps like U56, and follow U56 and U57
   upstream: report, fix, or override per case.
3. Choose the fuel default and how an app sets it (global resource, or
   per `HtmlUi`).
4. Decide the migration path for users: a release note with a Tera →
   MiniJinja table, or a deprecation period with both engines behind
   features (doubles the template surface for a release).

## Results (2026-11-25, branch `minijinja`)

The trial ported the crate, its examples, tests and benchmarks; the full
suite passes (654 tests). Method for every timing: criterion, release,
`main` (Tera) measured, then the branch against that baseline, then `main`
again (noise up to 2.6%).

1. **Render speed (open question 1).**

   | Workload (`rebuilding` page) | Tera | MiniJinja | |
   |---|---|---|---|
   | compiled once, render per iteration, 120 rows | 8.0 µs | 13.4 µs | +67% |
   | the same, 600 rows | 34.6 µs | 62.9 µs | +82% |
   | compile + render (`one_off`), 120 / 600 rows | 15.1 / 42.0 µs | 17.2 / 68.2 µs | +13% / +62% |
   | fuel accounting itself (fuel vs none) | | | ≈ 5% |
   | full frames, all 18 `frame/*` workloads | | | −4% to +4.7% |

   A profile puts half of a render in MiniJinja's per-iteration loop
   machinery (`eval_impl`, `next_loop_item`): a tight 600-row loop is its
   worst case. A render runs only when the context changes; the largest
   frame effect was `changing/600` (+4.7%, re-rendering 600 rows every
   frame). At 60 FPS the 600-row render is 0.38% of a frame; what it costs
   over Tera (28 µs) is 0.17%. The `speedups` feature is on (a faster escaper and
   number formatting: a few percent).
2. **Built-in audit (open question 2).** Every filter, function and
   operator that can produce more than it takes was measured against the
   default budget (peak RSS, release). Most charge before or while
   allocating and stay within ~1.1× of the budget's ~268 MB. Some build
   their result first and charge it afterwards (U56, filed privately and
   reserved); `new_environment` registers wrappers over those that charge
   the result's size up front, then call MiniJinja's own implementation.
   A counting-allocator regression test (kept with the private report
   until upstream fixes U56) holds every case under 1× of a 2¹⁸ budget and
   fails without the wrappers.
3. **Fuel default (open question 3).** `HtmlTemplateLimits { fuel }`, a
   resource, default 2²² (≈ 4.2 M), read when a template loads (the loaders
   get it through `TemplateFuel`, an atomic `sync_template_fuel` updates).
   Measured: the example templates use at most 2,605 fuel per render
   (`ui/content/l10n.html`); generated documents use 21–27 fuel per
   element and about one per 4 bytes of output (testbench `big-document`
   at 8,000 elements: 213,077 fuel, 836,742 bytes), so the default runs
   out near the 16 MiB output cap, which stays.
4. **Migration (open question 4).** One breaking release with a migration
   table in the CHANGELOG; no period with both engines. What the
   repository needed: the examples' component library became a macro
   library (`{% import … as ui %}`, `{% call ui.button(…) %}`), two
   `range(start=, end=)` calls, one `as_bool()`, and a handful of test
   templates (`range(end=)`, `default(value=)`, `set_global`).

Also decided in the trial:

- `{% import %}` and `{% from %}` resolve relative to the file and load
  like `include`/`extends`; a reference cycle fails the load (Tera refused
  cycles too; MiniJinja would recurse until its limit).
- `UndefinedBehavior::SemiStrict` (printing or iterating an undefined
  value fails; testing it in `{% if %}` is false), as Tera behaved, and
  the trailing newline is kept.
- The operator pre-check (`tera_tag_over_limit`, bug_0182) stays: it
  bounds what fuel can't see, the compile.
- Review 0009's template bombs now run out of fuel at render
  (`hostile_templates_run_out_of_fuel`); `unique` over a large range,
  quadratic in Tera (U46), renders.
- Removed: `src/tera_check/` (2,423 lines of logic, 567 of tests),
  `tera`, `nom`. Added: `minijinja` (3.1.0, features `fuel`,
  `loop_controls`, `json`, `serde`, `speedups`), `v_htmlescape`. Library
  tree: 315 → 313 crates.
- Fuzzing: 593,153 runs of the `html` target (5 min, over the Tera-era
  corpus), no crash.

