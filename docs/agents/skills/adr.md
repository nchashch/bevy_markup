# Skill: writing Architecture Decision Records

ADRs live in `docs/agents/adr/` and record *why* a significant decision was
made: what was tried, what was rejected, and what it cost. `AGENTS.md`
describes the code's **current** state and is rewritten as the code changes;
an ADR is a **dated snapshot** and stays as written. Bugs go in
`docs/agents/bugs/` (see `AGENTS.md` "Bug reports"), not here. This format is
the one prototype_19 (bevy_markup's first user) uses for its ADRs.

## 1. When to write one

Write an ADR when a change:

- picks one approach over real alternatives (a dependency, a pipeline stage,
  an update model, how much of the browser to imitate);
- changes a contract users rely on (public types, events, what a template
  attribute means, when entities are kept or replaced);
- reverses or supersedes an earlier ADR;
- closes a span of work since the last ADR — a **catch-up ADR** (§6).

Don't write one for a bug fix with a single obvious cause and no
alternatives: file a bug report; the ADR's "Also since" can mention it.

## 2. File name and numbering

- `docs/agents/adr/NNNN-kebab-case-summary.md`, the next free 4-digit number.
  Numbers are never reused, even if a record is withdrawn.
- Add the record to the index table in `docs/agents/adr/README.md` **in the
  same change**, with the same title and status.

## 3. Header block (required, at the very top)

After the `# N. Title` heading, a two-column table:

| Field | Content |
|---|---|
| `ADR` | `NNNN` |
| `Title` | Same as the heading |
| `Date` | Writing time, ISO with UTC offset (`2026-10-06 06:28 +0400`) from `date '+%Y-%m-%d %H:%M %z'`, never guessed |
| `Author` | The **exact model** that wrote it plus the harness (`Claude Opus 5.5 (Anthropic), via omp`), or a human's name. Never "AI" or "agent" alone |
| `Commit` | `git log -1 --format='%h %s'` when written, plus `+ uncommitted <what>` if the tree differs |
| `Span` | Catch-up ADRs only: the commits covered (`59f8f32..421abbc`, or a list) |
| `Status` | `Proposed` / `Accepted` / `Superseded by NNNN` / `Rejected` |
| `Related` | ADR numbers, bug reports (`bug_0016`), UPSTREAM entries (`U11`), prototype_19 playtests or ADRs |

## 4. Body sections (in this order)

1. **Context** — the problem, forces and constraints, and what was already
   true. Facts, not intentions.
2. **Decision** — what was actually done, naming files, types and functions
   (`build.rs` `update_children`, `HtmlTooltips`). Numbered subsections when
   there are several decisions.
3. **Alternatives considered** — every real option weighed and why it lost;
   an option that was tried and reverted belongs here with the evidence.
4. **Consequences** — what it gained (measured where possible), what it
   cost, what is still open — concretely enough for someone else to pick up.
5. **Also since NNNN** (optional) — smaller changes in the span, one line each.

## 5. Evidence rules

- Every factual claim traceable: a commit hash, a file or symbol, a bug or
  UPSTREAM entry, a test, a measurement. Unobserved claims are marked
  `[INFERENCE]`.
- Commit hashes come from `git log`, never from prose.
- Describe what the code does, checked against the code; if `AGENTS.md`
  disagrees, trust the code and say the doc is stale.
- Measurements state their method as well as the number.

## 6. Catch-up and retrospective ADRs

1. `git log --reverse <last-ADR-commit>..HEAD` lists the span.
2. Group commits into **decisions**, not into commits.
3. Each decision gets its own Decision subsection, alternatives and
   consequences; housekeeping goes in "Also since".
4. Cite bug reports and test files; link, don't repeat them.
5. **Retrospective** records (written after the fact, like 0001–0012): the
   author of the record isn't the decision's author — say who decided (the
   commit author) and where the reasoning comes from (commit messages,
   `AGENTS.md` at that commit, a session transcript). Alternatives nobody
   recorded are `[INFERENCE]` or "not recorded", never invented.

## 7. Immutability

Once `Accepted`, an ADR's body is never edited to match later changes; a new
ADR supersedes it. Allowed afterwards: its `Status` row (`Superseded by
NNNN`) with the matching index row, and typo or link fixes.

## 8. Checklist

- [ ] Next free number; kebab-case file name
- [ ] Header filled with real `date`, `git log -1` and exact model name
- [ ] Context / Decision / Alternatives / Consequences present
- [ ] Every claim traceable; unobserved ones `[INFERENCE]`
- [ ] `docs/agents/adr/README.md` index row added
- [ ] Any superseded ADR's Status and index row updated, nothing else
