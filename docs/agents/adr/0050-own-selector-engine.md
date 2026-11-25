# 50. Selectors are parsed and matched by bevy_markup; parcel_selectors is removed

| Field | Content |
|---|---|
| `ADR` | `0050` |
| `Title` | Selectors are parsed and matched by bevy_markup; parcel_selectors is removed |
| `Date` | 2026-11-25 06:58 +0400 |
| `Author` | Claude Opus 5.5 (Anthropic), via Claude Code |
| `Commit` | `f28ccd6 Propose ADR 0049: MiniJinja with fuel instead of Tera` + uncommitted: the new engine (branch `own-selector-matcher`) |
| `Status` | Accepted (2026-11-25) |
| `Related` | ADR 0028, 0029, 0034 (superseded), 0044, 0048; bug_0213, bug_0235; review_0005 I02, I03 |

## Context

ADR 0034 moved selector matching onto parcel_selectors, lightningcss's fork
of Servo's `selectors`; ADR 0048 removed lightningcss but kept
parcel_selectors behind bevy_markup's own `SelectorImpl` (`css/select.rs`)
and an `Element` implementation over the build walk's structures
(`cascade/selector.rs`). What it still cost (2026-11-25, `main` at
`f28ccd6`):

- **Panics kept out by hand.** Its matcher has no arm for `:nth-child(… of
  S)`, `:has()`, `&` nesting or `:nth-col()` and panics on them; only the
  allowlist kept them away (ADR 0034).
- **Wrong arithmetic.** `An+B` in checked `i32`: `:nth-child(n-2147483647)`
  matched nothing where CSS matches every element (review_0005 I02; two
  tests pinned the wrong answer).
- **Dependencies.** 11 crates only it needed: parcel_selectors 0.28.3
  (5,770 lines), static-self and static-self-derive (which brings `syn` 1,
  37,520 lines, a second `syn` compiled beside Bevy's), phf 0.11 with its
  code generator and generator crates, rand 0.8, rand_core,
  precomputed-hash. It pinned `cssparser` to `=0.37.0`, its version.
- **Fit.** bevy_markup used a small part of it (the allowlist grammar), and
  every guard around it — the allowlist, the `X ~ subject` scan cache
  (review_0005 I03), the combinator-argument budget (bug_0235) — was ours
  already, written against its traits.

## Decision

1. **Parser (`css/select.rs`).** Selectors parse on cssparser's tokens into
   bevy_markup's own types: one flat `Vec<Simple>` per selector in match
   order, compounds separated by `Simple::Combinator` (parcel_selectors'
   layout: one allocation per selector), rule selectors shared behind
   `Arc` so the cascade keeps them without a copy. The parser accepts
   exactly the grammar parcel_selectors accepted, its parse states
   included (what may follow a pseudo-element, `::part()` or
   `::slotted()`; what argument lists inherit; forgiving `:is()`,
   `:where()`, `:has()` and `of S`, strict `:not()`; `*|`/`|` prefixes and
   their end-of-input reading). Everything outside the supported grammar
   parses to `Simple::Unsupported`; invalid syntax still fails the sheet.
2. **Matcher (`cascade/selector.rs`).** Right to left over the same `Node`
   view (subject, ancestor chain, shared sibling lists, stand-in siblings),
   with Servo's restart outcomes (`Outcome`), so a failed descendant chain
   stops instead of backtracking over every combination of ancestors.
   Nodes are read one field at a time. Positions come from `Position` where
   a node has one, from a sibling walk for stand-ins (as parcel_selectors
   computed them). `An+B` is exact in `i64`. The allowlist and its
   reactivity record (`SelectorReads`, `pseudo`, `later_sibling`,
   `combinator_arguments`) are unchanged, ported to the new types.
3. **Dependencies.** parcel_selectors and static-self are removed;
   `cssparser` is no longer pinned to an exact version.

## Evidence

Method: on the branch, `main`'s parcel_selectors code was kept as a
test-only oracle beside the new engine (removed before the merge).

- **Differential test.** Two proptest generators: text concatenated from
  ~90 pieces (valid, unsupported and invalid: namespaces, pseudo-elements,
  `::part()`, `::slotted()`, `:has()`, `of S`, `&`, stray tokens) and
  well-formed selectors from the supported grammar (nested
  `:is()`/`:where()`/`:not()`, every combinator), each against generated
  documents of up to 4 levels of 1–4 siblings (tags, ids, classes, a
  `data-k` value, interaction states, `:empty`/`:focus-within`), with and
  without threaded sibling lists. Compared: whether the list parses, per
  selector the allowlist verdict, specificity, `SelectorReads`, pseudo
  flag, combinator arguments, `matches` and `matches_ignoring_pseudo`.
  4,000,000 cases (release build): 749,365 lists parsed, 931,782 selectors
  admitted and compared, 138,810 matching; **0 disagreements**. The runs
  found two parser differences, fixed before the count: a type selector
  inside `:not()` after a pseudo-element (parcel refuses it: the argument
  inherits the state) and `&|` (parcel reads a prefix at the end of input
  as no type selector).
- **Tests.** The full suite passes; the two tests that pinned
  parcel_selectors' `i32` answer now assert CSS's
  (`an_b_extreme_offsets_match_without_overflow`,
  `nth_child_extreme_offsets_neither_panic_nor_mismatch`).
- **Robustness.** 661,191 libFuzzer runs of the `css` target (5 min), no
  crash; the replay harness of ADR 0048's safety check: 92 known crash
  inputs, 60,092 generated variants, 115,644 CI crash inputs and 222,260
  corpus inputs, 0 failures. The new code has no `unwrap`/`expect`/
  `unreachable!`/`panic!`.
- **Parse speed** (`fuzz::cascade`: parse + cascade over a few elements;
  release, `-align-all-functions=6 -align-all-nofallthru-blocks=5`,
  bevy_markup at one codegen unit; best of 5 rounds of best-of-5; a third
  build, `main` plus a no-op change, measured the noise at ±2.5%):

  | Workload | `main` | new |
  |---|---|---|
  | `selectors-1000-rules` | 5.208 ms | 3.763 ms (−27.8%) |
  | `examples-all` (every example sheet) | 1.037 ms | 0.962 ms (−7.2%) |
  | `theme-crimson` | 0.128 ms | 0.122 ms (−4.8%) |
  | `var-1000-rules` | 2.519 ms | 2.417 ms (−4.0%) |
  | `colors-1000-rules` | 2.170 ms | 2.100 ms (−3.3%) |
  | `plain-2000-rules` | 7.914 ms | 8.116 ms (+2.5%, at the noise bound) |
  | `calc-1000-rules`, `grid-500-rules` | | within noise |

  The first version was 6% slower on `plain-2000-rules`, its libc share
  twice `main`'s: a selector was a `Vec` of compound `Vec`s, deep-cloned by
  the cascade, and every simple selector test built a ~100-byte view of
  the element. The flat layout, the shared `Arc` and per-field reads fixed
  it.
- **Match speed** (criterion `frame/*`, full frames re-collecting the whole
  document; `main` measured twice, the second run differing by up to 2.6%):

  | Workload | 120 elements | 600 elements |
  |---|---|---|
  | `selectors` (ADR 0034's mixed sheet) | −6.6% | −7.5% |
  | `selectors-functional` (`:is()`/`:where()`/`:not()`) | −10.9% | −13.7% |
  | `selectors-siblings` (60-item lists, `+`/`~`, `:nth-*`) | −9.1% | −2.1% |
  | `selectors-deep` (24-deep chains of near-misses) | −1.5% | +0.2% |

  The last three workloads were added to `benches/` for this comparison.
- **Size.** 1,292 lines of logic (`css/select.rs` + `cascade/selector.rs`)
  replace 777 lines of wrapper plus parcel_selectors' 5,770 (and `syn` 1's
  37,520 at build time); scc complexity 243 against 97 + 669. Library
  dependency tree: 326 → 315 crates.

## Alternatives considered

- **Keep parcel_selectors.** Works, but carries the panics, the `i32`
  arithmetic, a pinned `cssparser` and eleven crates for the part of it the
  allowlist admits.
- **Servo's `selectors` (0.43).** Firefox's engine, maintained and fuzzed,
  but the same shape of dependency: a large generic API for a small
  subset, its own pinned `cssparser`, and phf at build time.
- **Own matcher, parcel's parser.** Keeps the dependency for the half that
  is easiest to write; the parser is where the accepted grammar must match
  exactly, and the differential test covers it as well as the matcher.

## Consequences

- Faster parsing of selector-heavy sheets and faster matching, no panic
  paths, `An+B` as CSS defines it, eleven fewer crates (`syn` 1 among them),
  `cssparser` free to follow its own releases.
- bevy_markup owns ~500 more lines and the obligation that its parser
  accepts what browsers accept. Guards: `tests/selector_model.rs` (matching
  against a reference model), the browser oracle's selector vectors,
  CSS fuzzing, and the cascade's allowlist tests. The differential test
  against parcel_selectors was removed with the dependency; the numbers
  above are its record.
- The cascade-side guards stay as they were: the allowlist (unsupported
  selectors still skip their rule), the `X ~ subject` scan cache, and
  `MAX_COMBINATOR_ARGUMENTS` (an `:is()` argument is still matched by a
  call of its own from every ancestor, so its cost is still depth²).
