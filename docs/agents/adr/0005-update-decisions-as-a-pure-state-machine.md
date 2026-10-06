# 5. When to rebuild or restyle: a pure state machine, with restyles in place

| Field | Content |
|---|---|
| ADR | 0005 |
| Title | When to rebuild or restyle: a pure state machine, with restyles in place |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `eda847f` (2026-10-04), with the build/restyle split it formalized |
| Status | Accepted (its "a content change rebuilds the subtree" part is superseded by 0009) |
| Related | 0004, 0009; `bug_0001`, `bug_0003`, `bug_0004`, `bug_0008`–`bug_0010`, `bug_0016`, `bug_0017` |

## Context

An `HtmlUi` depends on several assets that load and reload independently:
its template, the default and per-entity stylesheets (and their images),
the locale bundle and the font families. The early `build_html_ui` mixed
"what changed" with "what's loaded". The property and stateful tests (0004)
kept finding wedges and missed updates:

- a failed stylesheet left the UI permanently blank (bug_0001);
- removing the outline marker or a per-entity sheet never rebuilt (bug_0003,
  bug_0008);
- a failed sheet latched or ate later changes (bug_0004, bug_0009, bug_0010).

## Decision

`eda847f` "Factor out rebuilding logic into a state machine": `src/rebuild.rs`
is a pure function from one frame's observations (`Frame`: load phases of the
own and default sheets, document ready, which change signals fired) and the
entity's `RebuildState` to a `Decision`:

- `Build(Source)` — content changed: template output, translations, outline;
- `Restyle(Source)` — only styles changed: sheets, their images, fonts,
  interaction state;
- `Skip` or `Wait` — nothing changed, or a needed sheet is still loading.

`Source` says which stylesheet to use: own, default or unstyled. A failed
sheet resolves to a fallback, which is how a failure still produces an
update. It is tested exhaustively in `rebuild::tests` without an app.
Restyles apply the new styles onto the existing entities when the shape
matches, keeping app-attached components; otherwise they rebuild.

## Alternatives considered

- **Keep the decision inline in the ECS system**: that is what produced the
  bug cluster above; a pure function made every case unit-testable.

## Consequences

- No bug in the "UI wedged or missed an update after a load change" class
  was filed after `0d9d6cb`/`eda847f` (`docs/agents/bugs/INDEX.md`); their
  regression tests live in `rebuild::tests` and `tests/stateful.rs`.
- Nested `HtmlUi`s exposed two more interactions with the "rebuild replaces
  the subtree" rule (bug_0016: commands on despawned nested UIs; bug_0017:
  restyles treating nested UIs as shape changes). ADR 0009 later removed the
  root cause by updating in place and processing nested UIs first.
- The decision table still distinguishes Build from Restyle, but since 0009
  both are reconciled in place; the difference is only which event fires
  (`HtmlUiBuilt` vs `HtmlUiRestyled`).
