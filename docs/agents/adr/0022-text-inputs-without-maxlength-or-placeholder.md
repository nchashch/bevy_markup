# 22. Text inputs carry no `maxlength` or `placeholder`

| Field | Content |
|---|---|
| ADR | 0022 |
| Title | Text inputs carry no `maxlength` or `placeholder` |
| Date | 2026-10-13 13:25 +0400 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp |
| Commit | `97e16bc` Merge fix/content, fix/interaction, fix/examples; post-merge fixes + uncommitted docs (CHANGELOG, bug reports) |
| Status | Accepted |
| Related | 0016 (supersedes the `maxlength` bullet of its Decision 1), bug_0038 (review finding R10), Bevy 0.20's `TextInput` widget |

## Context

ADR 0016's Decision 1 described the text-input mapping including
"`max_characters` from `maxlength`". The code never read `maxlength` (or
`placeholder`) — the whole-crate review of 2026-10-13 (finding R10,
`docs/agents/reviews/review_0001.md`) found the attributes were documented
in lib.rs, AGENTS.md and the `text_input_element_maps_to_the_widget`
comment but read nowhere. Bevy 0.20's `TextInput`/`EditableText` widget
supports neither a character limit nor placeholder rendering: there is no
API to map onto.

The fix commit for the kept-input update path (`bbd0959`, merged as part
of `fix/build`) retracts the claims instead of inventing a local
implementation.

## Decision

`<input>` supports only what the widget supports: `value` (the field's
text, app state after the first write) and the cascade's styling. The
`maxlength` and `placeholder` attributes are ignored (logged as skipped
values like any unsupported input) and the docs no longer claim them.
They come back only if Bevy's widget grows the features; a local
implementation would have to re-implement caret/selection truncation
around `EditableText` and a placeholder overlay, which is real widget
work, not attribute plumbing.

## Alternatives considered

- **Enforce `maxlength` in the library** (truncate or reject edits):
  fights the widget's editing state (selection, undo) and duplicates
  widget logic; rejected.
- **Render the `placeholder` as a styled span behind the field**: needs
  focus-aware z-ordering and per-frame sync the widget doesn't offer;
  rejected for the same reason.

## Consequences

- Templates using `maxlength`/`placeholder` silently lose them (debug
  log); nothing panics.
- ADR 0016 stays as written; this record supersedes the `maxlength`
  bullet of its Decision 1 and its `placeholder`-follow-up note.
- Recorded in bug_0038 with the kept-input update fix it shipped with.
