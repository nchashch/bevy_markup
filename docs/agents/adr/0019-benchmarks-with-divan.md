# 19. Benchmarking with Divan

| Field | Content |
|---|---|
| ADR | 0019 |
| Title | Benchmarking with Divan |
| Date | 2026-10-12 06:20 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp — drafted for the project owner |
| Commit | `f541ee3` Add visibility, CSS variables and transitions; hot-reload and component examples + uncommitted ADRs 0017–0019 |
| Status | Accepted — reinstated by the owner's decision to run both harnesses in parallel (dev-time only, and their findings corroborate each other); implemented in the `pipeline-divan` bench |
| Related | 0017 (the measurement this serves), 0009 (the throwaway benchmark this formalizes); alternative proposed in [0018](./0018-benchmarks-with-criterion.md) |

## Context

Identical to 0018's (same measurement need from 0017, same 0009
throwaway-benchmark gap, same headless/deterministic/CI-cache harness
constraints) — read that section there; this ADR only differs in the
framework choice. Both ADRs are written so they can be compared and
exactly one accepted.

The Divan-specific fit: the pipeline's update paths are allocation-heavy
by construction — every build re-renders into fresh `String`s
(`HtmlDocument::source`), parses into a new `tl` tree, and builds fresh
`NodeSpec`/`Item` trees per element (`build.rs` `collect_items` →
`item_spec`); 0009's whole thesis is about *not* re-doing that work.
Allocation counts are the most direct signal of how much of that work a
change eliminates — 0017's fingerprints exist to avoid allocations
(cascade maps, `Vec<Property>`, spec trees) — and no timing-only harness
reports them.

## Decision (proposed)

Adopt [Divan](https://github.com/nvzqz/divan) 0.1.21 as the benchmarking
framework.

1. **Dependency**: `divan = "0.1.21"` in `[dev-dependencies]` (exact
   version pinned like the proptest family; divan's dependency tree is
   small — it advertises a lightweight design).
2. **Bench layout**: `benches/pipeline.rs` with `harness = false` and
   `fn main() { divan::Divan::default().main() }`, functions annotated
   `#[divan::bench]`. The same groups, documents and workloads as 0018's
   Decision 2–3 (same synthetic documents, same stage split, same three
   frame workloads) so the two ADRs describe directly comparable suites.
3. **Allocation counting**: `#[divan::bench(alloc_count = true, alloc_size = true)]`
   (divan installs its own global allocator when run through its entry
   point) — per benchmark: allocations, bytes allocated, and wall time
   side by side. For 0017 this is the headline metric: "fingerprints cut
   cascade allocations by X%" is a stronger, more stable claim than
   wall-time alone on a noisy runner.
4. **Determinism**: same approach as 0018's Decision 4 — schedule
   shuffling off, `Time::advance_by` inside iterations for fixed clock
   deltas.
5. **Baselines**: divan has no saved-baseline regression detection;
   numbers are per-run TSV-style output. CI: same shape as 0018's — a
   nightly bench job capturing output (as a job summary table or
   artifact), no merge gate; `cache.yml` builds the (small) dependency
   tree. Trend tracking would rely on the nightly job's history, not the
   tool.

## Alternatives considered

- **Criterion** (0018) — the direct alternative; weighed there. Divan's
  edge: fast iteration (no warm-up/sampling loop), first-class
  allocation metrics (the signal this crate's update-model work cares
  most about), a small dependency tree, simple output that pastes into
  ADRs. Its costs: young (0.1.x, one active maintainer, last release
  2025-04 vs criterion's 2026-02), no statistical change detection
  against a stored baseline (regressions are spotted by reading
  numbers, not flagged), fewer users/less battle-testing.
- **Criterion with a bolt-on allocation counter** (e.g.
  `alloc_counter`/custom global allocator) — keeps 0018's statistics but
  adds a second moving part and the allocation harness outside the
  timing loop's control; divan integrates it. Rejected *within this
  ADR* as the composite option (it is really 0018 plus more).
- **`#[bench]`/libtest and throwaway benchmarks** — same reasoning as
  0018's Alternatives; rejected there.
- **iai-style instruction counting** — deterministic but not wall-time;
  rejected there.

## Consequences

- **Gained**: allocation metrics (counts + bytes) alongside time for
  every pipeline stage and frame workload — the direct evidence 0017
  needs (fingerprints' whole point is not redoing allocating work); fast
  local iteration; a light dependency tree (CI cache cost near-zero).
- **Cost**: no automatic regression detection — trends must be tracked
  by comparing nightly outputs (a human or a script diffing job-summary
  tables) `[INFERENCE: how noisy those numbers are on a shared runner is
  unmeasured]`; a less battle-tested harness (0.1.x; breaking changes in
  the attribute API across 0.1 releases are possible); sampling is
  simpler than criterion's, so percentile/outlier story is weaker.
- **Open**: whether divan's global allocator interacts with Bevy's own
  allocator hooks (Bevy may install tracing/allocator instrumentation in
  some configurations — the benches use `MinimalPlugins`, but this needs
  checking at integration) `[INFERENCE]`; whether allocation numbers are
  stable enough across runs to diff in CI (allocator behavior is
  deterministic for a fixed input, so plausibly yes `[INFERENCE]`).
