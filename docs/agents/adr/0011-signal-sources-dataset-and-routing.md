# 11. Signals: what produced them, browser button semantics, per-feature `data-*`, routing by name

| Field | Content |
|---|---|
| ADR | 0011 |
| Title | Signals: what produced them, browser button semantics, per-feature `data-*`, routing by name |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — designed in that omp session with the project owner, who asked for the source and required that it always be supplied; committed by the owner (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `4cef523`, `d2c9b63` (2026-10-06) |
| Status | Accepted (extends 0006) |
| Related | 0006, 0008, 0010; `bug_0024`; prototype_19 playtests 0031, 0032, 0035 |

## Context

`ElementSignal` (0006) carried a name, a trigger, the element, the
`data-with` payload and an optional position. That left four gaps:

- **Unknown source**: a handler couldn't tell a left click from a right
  click, a touch, a VR laser or a gamepad activation.
- **Button semantics** (bug_0024): the click observer ignored the pointer
  button, so a right click activated buttons.
- **Shared payloads**: one `data-with` object served every feature on an
  element; prototype_19's Options button carried
  `{"selector": …, "tooltip": …, "tooltip_above": true}`.
- **Boilerplate**: every surface had a `MessageReader<ElementSignal>` with a
  `match` on string names (five such systems in prototype_19).

## Decision

1. **Every signal names its source** (`4cef523`): `position` was replaced by
   `source: SignalSource`:
   - `Pointer { pointer: PointerId, button, position, count }` — mouse,
     touch, or custom pointers such as VR lasers and mocks;
   - `Hover { pointer }`;
   - `Activation(ActivationInput)` — `Key`, `GamepadButton`, `Synthetic`
     (harnesses, tests, replays), `Other`.

   `HtmlFocus::activate(input)` and `ActivateElement` take the input: the
   source is never absent, at the owner's request.
2. **Browser button semantics** (`4cef523`): `data-on-click` is the primary
   button or activation; `data-on-auxclick` is any other button;
   press and release fire for every button. Fixes bug_0024.
3. **The element's `data-*`** (`d2c9b63`): `HtmlElement.dataset` (prefix
   stripped, like a browser's `element.dataset`), read with
   `ElementSignal::data(key)`. One attribute per feature; `data-with` stays
   for structured payloads (row indices).
4. **Routing by name** (`d2c9b63`): `app.on_html_click(name, system)` and
   `on_html_signal(name, system)` register systems that take
   `In<ElementSignal>`. They run in an exclusive dispatcher in `PostUpdate`,
   before `Render`, so changes show the same frame; the message is still
   sent for generic readers.

## Alternatives considered

- **Optional source with an `activate_with(…)` variant** — proposed first;
  the owner pointed out that every activation has some source (even a mock),
  so it is required.
- **Keep `position` beside `source`** — two places to read the same thing;
  position lives only in the pointer variant.
- **Observers per element for routing** — the dispatcher keeps one cursor
  and lets messages, routed handlers and other readers coexist.

## Consequences

- prototype_19 reports the confirming input (Enter or the gamepad's South)
  from its `UiConfirm` binding; right clicks now do nothing in its UI
  (playtest 0031). Its templates read `data-tooltip` / `data-selector`
  (playtest 0032), and its main-menu, lobby and pause buttons are
  `on_html_click` handlers (playtest 0035).
- Breaking changes for users: `ElementSignal.position`, `activate()` without
  an argument, and `ActivateElement { entity }` are gone.
- The `menu` example shows which input produced the last click.
