# 8. Browser-style focus and directional navigation in the library; input stays the app's

| Field | Content |
|---|---|
| ADR | 0008 |
| Title | Browser-style focus and directional navigation in the library; input stays the app's |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — the decision was made in that omp session with the project owner, who chose browser-like answers to its design questions; committed by the owner (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `f81acce` (2026-10-05), with `35125b6` (bug_0022 fix) |
| Status | Accepted |
| Related | 0006, 0011; `bug_0019`, `bug_0022`; UPSTREAM U11; prototype_19 playtest 0025 |

## Context

prototype_19 targets the Steam Deck and needs gamepad- and keyboard-driven
menus. Its client implemented focus itself on top of bevy_markup (~260 lines
in its `ui/markup.rs`: `UiNav`, `UiNavModal`, edge events, clickability
checks, a ring drawn by hand). Any other game would have to write the same.

## Decision

`f81acce`, `src/focus.rs`, modelled on the browser:

- **Focusable** = elements with `data-on-click` (the `<button>`
  counterpart) or `tabindex >= 0`; `tabindex="-1"` opts out. The
  `autofocus` attribute takes initial focus.
- **Focus is Bevy's `InputFocus`**, shared with any other UI.
  `InputFocusVisible` implements the `:focus-visible` heuristic: navigation
  shows the ring, a pointer press hides it. CSS gained `:focus`,
  `:focus-visible` and `outline` (+ `outline-offset`), so the ring is
  styled in CSS.
- **Directional navigation** uses Bevy's `AutoDirectionalNavigation`
  between elements in scope. Navigating past the last element triggers a
  `FocusEdge` event, which the app can use to page a list.
- **Scope**: an `HtmlModal` root confines focus (the topmost wins);
  `HtmlNoFocus` roots never take it (UI on VR quads, driven by lasers).
- **Survives updates**: when the focused element is replaced, focus returns
  to the element with the same `id` in the same root.
- **Input is the app's**: bevy_markup reads no keys or buttons. The app
  calls `HtmlFocus::navigate` and `activate`; activation emits the same
  `ElementSignal` as a click.

## Alternatives considered

- **Keep focus in each app** — the prototype_19 state; duplicated per game.
- **Read input inside the library** — rejected: binding choices (which
  stick, repeat rates, an input context system like bevy_enhanced_input)
  are game policy. prototype_19 keeps hold-to-repeat in its own bindings.

## Consequences

- prototype_19's `ui/markup.rs` shrank from 592 to 329 lines; the selector
  pages on `FocusEdge` (playtest 0025).
- The `autofocus` attribute exposed a parser bug: a value-less attribute ate
  the next attribute's first character (bug_0019, UPSTREAM U11). Fixed by
  switching the parser dependency from `tl` 0.7.8 to the `astral-tl` 0.8.0
  package.
- In windowed apps `autofocus` never applied: Bevy focuses the window at
  startup and the focus repair left it there (bug_0022, found by the `menu`
  example; headless prototype_19 runs couldn't see it). Only UI nodes now
  keep focus from outside the library.
- Not done: Tab order (directional navigation only), `:focus-within`.
