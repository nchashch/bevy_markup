# 6. Declarative interaction: `data-on-*` signals as messages, `:hover`/`:active` as restyles

| Field | Content |
|---|---|
| ADR | 0006 |
| Title | Declarative interaction: `data-on-*` signals as messages, `:hover`/`:active` as restyles |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `8952749`, `d079b08` (2026-10-05) |
| Status | Accepted (extended by 0011) |
| Related | 0005, 0008, 0011; `bug_0020`; UPSTREAM U9, U12 |

## Context

Until `8952749` an app made a template element interactive by finding it after
each build (`HtmlUiBuilt` + `HtmlElements::by_id`) and attaching observers in
Rust. Behaviour therefore lived far from the markup, and every rebuild
required re-wiring. Hover styling needed CSS pseudo-classes, which the
cascade skipped.

## Decision

1. **Signals** (`8952749`, `src/signals.rs`): an element declares hooks with
   `data-on-click` / `-press` / `-release` / `-enter` / `-leave`, each naming
   an app-side signal, plus a `data-with` JSON payload rendered by Tera. The
   library turns pointer interactions into one buffered `ElementSignal`
   message. The deepest bound element under the pointer wins, so buttons can
   nest. Enter and leave come from hover tracking that survives rebuilds: a
   leave is emitted from a snapshot taken at enter. What a signal means is
   app code reading the messages.
2. **Pseudo-classes** (`d079b08`): `:hover` and `:active` match from a
   `PseudoState` component maintained from the picking hover map and pressed
   entities; a change restyles in place (0005), not rebuilds.

## Alternatives considered

- **App-attached observers after each build** — the previous pattern; kept
  possible (`HtmlElements`) but no longer needed for ordinary buttons.
- **Inline script handlers** (`onclick="…"`): not recorded [INFERENCE: there
  is no scripting runtime, and names-as-data keeps behaviour in Rust].

## Consequences

- Templates carry their own interactivity; this is what made focus and
  keyboard/gamepad activation (0008) possible: activation emits the same
  `ElementSignal` as a click.
- Picking edge cases came with it: text hits resolve `Pickable` on the span
  entity, so `pointer-events: none` had to reach spans (bug_0020, UPSTREAM
  U12); Bevy's UI picking systems need picking resources (U9).
- 0011 later added what produced a signal (`SignalSource`), primary-only
  clicks with `data-on-auxclick`, per-feature `data-*` reads and routing by
  name.
