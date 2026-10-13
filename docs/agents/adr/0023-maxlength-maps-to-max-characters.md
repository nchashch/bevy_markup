# 23. `maxlength` maps to the widget's `max_characters`

| Field | Content |
|---|---|
| ADR | 0023 |
| Title | `maxlength` maps to the widget's `max_characters`; `placeholder` stays unsupported |
| Date | 2026-10-13 |
| Author | GLM (opencode-zen/glm-5.3-flash), via omp |
| Commit | — (this change) |
| Status | Accepted (supersedes ADR 0022's `maxlength` half) |
| Related | 0016, 0022 (superseded in part), bug_0038 (R10), bevy_text 0.20's `EditableText::max_characters` |

## Context

ADR 0022 retracted the `maxlength`/`placeholder` claims on the grounds
that Bevy 0.20's widget "supports neither a character limit nor
placeholder rendering: there is no API to map onto". The character-limit
half is false: `bevy_text` 0.20's `EditableText` has
`max_characters: Option<usize>` — "Edits which would cause the length to
exceed the maximum are ignored" — which is exactly ADR 0016's Decision 1
mapping ("`max_characters` from `maxlength`"). ADR 0022's Context,
Consequences ("debug log"), and the CHANGELOG line built on it are wrong;
nothing logs the attribute either.

## Decision

`<input maxlength="N">` parses N and sets the field's
`EditableText::max_characters` — at spawn, and on a restyle/update when
the attribute changed (the typed text is app state and is not touched;
the widget ignores past-the-limit *edits* by itself). A missing or
non-numeric `maxlength` leaves the limit off. The `placeholder`
attribute stays unsupported (ADR 0022 stands for it): the widget has no
placeholder rendering, and faking one is real widget work.

## Consequences

- `maxlength` works as HTML authors expect, to the extent the widget
  enforces it: edits past the limit are ignored, but an app writing a
  longer text directly (`set_text`) is not stopped (the widget documents
  this).
- ADR 0022 remains the record of the retraction that was believed
  correct at the time; this record supersedes its `maxlength` half only.
- ADR 0016's Decision 1 mapping is in force again.
