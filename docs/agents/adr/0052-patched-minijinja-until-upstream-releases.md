# 52. MiniJinja is patched in this repository until upstream releases the fixes; bevy_markup keeps its own checks

| Field | Content |
|---|---|
| `ADR` | `0052` |
| `Title` | MiniJinja is patched in this repository until upstream releases the fixes; bevy_markup keeps its own checks |
| `Date` | 2026-11-26 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code |
| `Commit` | uncommitted at writing (after `102de67`) |
| `Status` | Accepted (2026-11-26) |
| `Related` | ADR 0044 (untrusted input), 0045 (checks over patched dependencies), 0049 (MiniJinja on fuel); UPSTREAM U56, U57, U59, U60, U62; bug_0253 |

## Context

ADR 0049 put templates on MiniJinja's fuel. Five MiniJinja 3.1.0 defects
let a template escape it: filters allocating before charging (U56), stack
overflow on long chains (U57), two `format` panics (U59, U60), and deep
walks of containers (U62: printing, `~`, `in`, `==` and compile-time
constant folding over a lazily repeated list, 10⁸ items for free). The
first four have local guards (filter wrappers, the 64-operator tag limit,
`checked_format`), as ADR 0045 prefers: checks in bevy_markup rather than a
patched dependency, because a patch protects only this repository.

U62 has no such hook. Printing and filters go through bevy_markup's
formatter and wrappers, which now count before they walk (bug_0253), but
constant folding runs while the template compiles and `~`, `in` and `==` run
inside MiniJinja's VM. The Nightly's 41 `html` findings were 34 folds of
`~`. The maintainer asked for public issues (2026-11-26); a release with the
fixes may take a while.

## Decision

1. All five fixes are on branches of a fork, one per issue and PR (from
   `main`), and together on `bevy_markup/3.1.0` (the 3.1.0 tag plus the
   five; MiniJinja's 439 tests pass on it).
2. bevy_markup's manifest and the four fuzz workspaces' (`fuzz`,
   `honggfuzz`, `fuzzcheck`, `test-fuzz`) patch `minijinja` to that branch
   (`[patch.crates-io]`, git). CI, the fuzzers, the tests and the examples
   run on it.
3. bevy_markup's own guards stay (the filter wrappers, `checked_format`,
   the tag limit, the formatter's item count): `[patch]` doesn't reach
   crates depending on bevy_markup, which get 3.1.0 from crates.io.
4. The patch goes with the MiniJinja release that has the fixes; the
   guards are reviewed then (ADR 0045's rule: drop what upstream covers).

## Alternatives considered

- **Local checks only.** Can't bound constant folding at load or `~`/
  `in`/`==` at render without MiniJinja's parser internals
  (`unstable_machinery`, an exact version pin) and still misses the
  render-time cases; the Nightly would keep reporting them.
- **A vendored copy** (`vendor/minijinja`). Same reach as the git patch,
  1.2 MB in the repository, and a copy to diff by hand on an update.
- **Publishing the fork under another crate name.** Would protect
  dependents now, at the cost of a fork to maintain and publish.

## Consequences

- The repository's own builds are protected; crates using bevy_markup are
  protected where bevy_markup has a hook and exposed to U62's folding and
  `~`/`in`/`==` until MiniJinja releases. The CHANGELOG says so.
- Building this repository fetches the fork (network, or a vendored Cargo
  cache offline).
- On a MiniJinja update: check U56–U62, drop the patch, keep or remove the
  guards per what the release fixes.
