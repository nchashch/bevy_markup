# 18. Benchmarking with Criterion

| Field | Content |
|---|---|
| ADR | 0018 |
| Title | Benchmarking with Criterion |
| Date | 2026-10-12 06:20 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp — drafted for the project owner |
| Commit | `f541ee3` Add visibility, CSS variables and transitions; hot-reload and component examples + uncommitted ADRs 0017–0019 |
| Status | Proposed |
| Related | 0017 (the measurement this serves), 0009 (the throwaway benchmark this formalizes); alternative proposed in [0019](./0019-divan-benchmarks.md) |

## Context

0017's design has an unverified core claim: skipping unchanged sub-trees
via fingerprints beats re-cascading them. 0009 validated its own change
with a throwaway release benchmark (1200 frames, one templated value
changing per frame; 9.85 → 4.06 ms/frame at 600 elements) whose method
and results live only in that ADR's prose — it cannot be re-run, and
nothing since (the transitions system, `resolve_calc`, the `vars_changed`
signal) has ever been measured. The repo has no `benches/` directory and
no benchmark dependency (`Cargo.toml` has no `[[bench]]` sections; the
only timing evidence anywhere is 0009's throwaway).

What needs measuring, repeatedly and comparably:

- the four pipeline stages separately (Tera render, `tl` parse,
  cascade + spec, `update_children` diff), so 0017's skip target is
  visible per stage;
- whole-pipeline frame cost at the 0009 document sizes (120, 600
  elements) for idle, one-value-changing, and full-rebuild workloads;
- the newer per-frame paths: `resolve_calc`, `advance_transitions`,
  the `vars_changed` hash.

The harness constraints: benches must run headless (the crate's tests
already build a minimal Bevy app without a window — `tests/common`
`TestUi`; `bevy_markup::fuzz`, feature `fuzzing`, exposes internal glue
like `Styler`/`cascade` without any app), must be deterministic (the test
harness shuffles schedules — a bench wants the opposite: pinned order,
fixed time), and must compile in CI's shared caches (`cache.yml` builds
the `dev` dependency cache; a new dependency tree lands there too).

## Decision (proposed)

Adopt [Criterion](https://github.com/bheisler/criterion.rs) 0.8 as the
benchmarking framework.

1. **Dependency**: `criterion = { version = "0.8.2", features = ["html_reports"], default-features = false }`
   in `[dev-dependencies]` (exact version pinned like the proptest
   family; `html_reports` for the browsable report, `default-features =
   false` to drop `rayon`/`plotters`-related extras if the default set
   pulls them `[INFERENCE: to check at integration]`).
2. **Bench layout**: `benches/pipeline.rs` with `harness = false` in a
   `[[bench]]` section, `criterion_group!`/`criterion_main!`. Setup
   builds a headless app once per benchmark group (the `TestUi`
   composition: `MinimalPlugins` + `AssetPlugin` + a fixed-size camera +
   `BevyMarkupPlugin`, no window), using the same synthetic documents as
   0009's method (120 and 600 block elements; a small and a large
   template).
3. **Benchmarks**:
   - per-stage groups: `render` (Tera one-off — `HtmlTemplate` has no
     public constructor, and the crate's `.html` render is a pass-through
     over it) and `parse` (`HtmlDocument::parse`, the clone excluded with
     `iter_batched`), each `BenchmarkId`-parameterized by document size;
   - frame groups: one `app.update()` per iteration over the workloads
     idle / changing (one templated value *and one templated color with a
     transition attached*, so the advance system's active path runs) /
     rebuilding, plus — at 600 — `localized` (every paragraph a
     `data-l10n-id` message with a changing arg: the Fluent stage) and
     `markdown` (a `.md` template with `markdown: true` Fluent values:
     the pulldown-cmark stage);
   - the kitchen-sink markup and stylesheet (registered fake font faces
     served from the embedded font, a grid, a flex row, `var()` +
     `calc()`, positioning, a `border-image` over the `frame.png`
     fixture) exist to run the cascade's mapping arms per re-collect —
     the benches doubled as a coverage layer (`scripts/coverage.py`'s
     `benchmarks` layer: 40% of library lines, from 27% with a minimal
     CSS).
   - `resolve_calc` and the `vars_changed` hash run inside the `changing`
     workload's frames rather than as separate small benches (the real
     path, not a synthetic one).
4. **Determinism**: schedule shuffling off (the bench app does not add
   Bevy's `debug` shuffle features); `Time` advanced manually
   (`advance_by`) inside iterations so transition/`resolve_calc` benches
   see fixed deltas instead of wall-clock noise.
5. **Baselines**: `target/criterion/` is gitignored (criterion saves
   per-bench baselines and flags regressions on the next run); CI does
   **not** gate on benches (a runner's timing noise would flake) — the
   nightly workflow gets an optional bench job that uploads the
   `html_reports` artifact for eyeballing trends, and the shared-cache
   workflow (`cache.yml`) starts building the bench dependency tree so
   local runs don't pay a cold Bevy+criterion build each time.

## Alternatives considered

- **Divan** (0019) — the direct alternative; weighed there. Criterion's
  edge: statistical rigor (outlier detection, change detection against a
  saved baseline) and maturity; its costs: slower iteration (warm-up +
  long sampling), a heavier dependency tree, and no built-in allocation
  counting.
- **`#[bench]`/libtest benches**: unstable-only (`nightly`), no
  statistics beyond the harness's, already needing a pinned nightly for
  fuzz — but benchmarking shouldn't require nightly for everyday local
  runs. Rejected.
- **Keep throwaway benchmarks**: what 0009 did; not repeatable, not
  comparable across changes, results only live in ADR prose. This ADR
  exists to replace that.
- **bencher / iai**: instruction-counting (`iai`) is deterministic and
  CI-friendly but measures a different thing (instruction counts, not
  wall time) and would not answer "how many ms/frame at 600 elements".

## Consequences

- **Gained**: repeatable, comparable numbers for 0017 (before/after per
  stage, same method for all future update-model work); regression
  detection against saved baselines for free on local runs; the 0009
  method becomes code instead of prose.
- **Cost**: `criterion` 0.8's dependency tree and its compile time on
  every cold `cargo test`-adjacent build (dev-dependencies are not built
  by plain `cargo build`, but `cargo bench` and CI cache jobs pay it)
  `[INFERENCE: magnitude to measure at integration — historically a few
  dozen extra crates]`; slower benchmark sessions (criterion's sampling
  and warm-up run seconds per benchmark — a 12-benchmark suite is minutes
  `[INFERENCE]`); a `benches/` harness to maintain alongside the test
  harness.
- **Open**: whether the per-stage groups can run off the `fuzzing`
  harness unchanged or need their own glue (the harness is
  `#[doc(hidden)]` and feature-gated — benches enabling `fuzzing` is
  acceptable for a dev-only target, consistent with the fuzz drivers);
  whether the nightly bench job runs on every night or only when the
  update model changes (0017's landing, update-model ADRs).
