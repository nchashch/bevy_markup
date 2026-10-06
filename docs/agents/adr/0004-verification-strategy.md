# 4. Verify against reference implementations, then attack with every kind of generated input

| Field | Content |
|---|---|
| ADR | 0004 |
| Title | Verify against reference implementations, then attack with every kind of generated input |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii), with agent sessions doing much of the work (discovery agents per bug report) |
| Commit | `d2c9b63` Implement more features |
| Span | `30570ae..eb0cd5a` (2026-10-04 – 05), extended by `4164c2d`, `54f4890`, `dd15087`, `96f2cd3`, `bf8b23f`, `9721039`, `9d84990` |
| Status | Accepted |
| Related | 0001, 0005; `bug_0001`–`bug_0025`; UPSTREAM U1–U12; `docs/agents/skills/testing.md` |

## Context

bevy_markup re-implements behaviour defined elsewhere: CSS cascade and
layout (browsers), Fluent's DOM overlays (`@fluent/dom`), HTML parsing. Its
correctness question is "does it do what a browser would?" Hand-written
expectations alone encode the author's understanding, not the standard. It
also sits behind four parsers fed by content files (0001).

## Decision

Layers, each added as its own commit, all documented in `AGENTS.md`
"Testing" and `docs/agents/skills/testing.md`:

1. **Headless test vectors** (`30570ae`): template + CSS + Fluent in → a
   text dump of the Bevy world out (`tests/html_ui.rs`, `TestUi`); expected
   dumps derived by hand from the standards, not pasted from output.
2. **Oracles** — the same inputs through the reference implementation:
   Chromium for the cascade (`969556c`, `scripts/browser_oracle.py`) and for
   layout rects (`e1b910c`); `@fluent/dom` in jsdom for localization
   (`94a4fb9`, `scripts/fluent_oracle.sh`).
3. **Generated input**:
   - property tests: proptest (`c59357f`, `a15f775` test-strategy), quickcheck
     (`ccfe874`), arbtest (`9e87b67`);
   - stateful model checking with proptest-stateful (`06b86ae`), extended to
     pointer input (`9721039`);
   - signal properties over the real picking stack (`bf8b23f`).
4. **Fuzzing** with four engines over the same harness targets
   (`html`, `css`, `ftl`): cargo-fuzz (`017cfac`), honggfuzz (`042782f`),
   fuzzcheck (`ba6db6e`), test-fuzz (`d202922`); corpora persisted and
   carried between CI runs (`b023b9e`, `619e8b7`, `4f2f764`).
5. **Mutation testing** (`311c808`, `4164c2d`, `d3f6562`; cargo-mutants,
   `scripts/mutants.sh`) to find tests that don't fail when code breaks.
6. **Coverage per layer** (`7bcc546`, `eb0cd5a`, `scripts/coverage.py`).
7. **Bug and upstream ledgers** (`9162e6c`, `c8acddf`): every real defect
   gets `docs/agents/bugs/bug_NNNN.md` plus a regression test; defects in
   dependencies and tools go to `UPSTREAM.md`.
8. **CI** (`147a04e` and follow-ups): `ci.yml`, `nightly.yml` (fuzzing,
   coverage), `mutants.yml`, `cache.yml`.

A convention that runs through this session's work too: a new test must
fail when its feature is broken, checked by deliberately breaking the
code ("ablation") before trusting it.

## Alternatives considered

- **Hand-written expectations only** — the oracles' first runs found bugs the
  hand-written vectors encoded wrongly (`969556c` AGENTS.md: unregistered
  `font-family` handling; `0d9d6cb` records that the layout oracle found
  Bevy's border-box default, so bevy_markup switched to content-box).
- **One fuzzer** — several were kept, each finding different things; their
  own tooling bugs are UPSTREAM U3–U8. [INFERENCE: the cost of four engines
  was judged worth the coverage; not stated.]

## Consequences

- 25 bug reports so far (`docs/agents/bugs/INDEX.md`), most found by these
  layers: fuzzers (bug_0002, bug_0005), stateful tests (bug_0003, bug_0004,
  bug_0010), the Fluent oracle (bug_0006, bug_0007), mutation triage
  (bug_0014), signal tests (bug_0020).
- 12 upstream issues recorded (UPSTREAM U1–U12); the two `fluent-syntax`
  crashes are patched in a fork (ADR 0002), the `tl` one avoided by
  switching to `astral-tl` (ADR 0008).
- Coverage was 98.0% merged after the first gap-closing pass, then 88.1% at
  `96f2cd3` as interaction/focus code grew (`AGENTS.md` Testing TODO;
  method: `scripts/coverage.py`, cargo-llvm-cov).
- The suite has a known flake: bug_0025 (`ui_state_machine_matches_model`,
  ~1 in 10 runs, present at `9d84990`), still open.
- Integration bugs that need a windowed app or a real game still escaped
  these layers (bug_0019, bug_0021, bug_0022, bug_0023 were found from
  prototype_19 or the examples), which is why the examples gained in-app
  smoke drivers in this session (not committed; recorded in ADR 0012).
