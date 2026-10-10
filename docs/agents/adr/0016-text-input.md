# 16. Text input as `<input type="text">` via Bevy 0.20's `TextInput`

| Field | Content |
|---|---|
| ADR | 0016 |
| Title | Text input as `<input type="text">` via Bevy 0.20's `TextInput` |
| Date | 2026-10-10 14:13 +0400 |
| Author | GLM-5.3-Flash (Z.ai), via omp, at the project owner's request |
| Commit | `1bbab7d` Add support for em and rem + uncommitted font fallback lists (ADR 0015) |
| Status | Accepted |
| Related | 0002 (dependency discipline), 0008 (focus in the library); the Interactivity limits in AGENTS.md and README |

## Context

The crate renders documents declaratively — no form controls. The
Interactivity limits say so: "no text input and other form controls". Bevy
0.20 makes a real text field buildable: `EditableText` (bevy_text) holds the
editor state (a `parley::PlainEditor`: text, cursor, selection,
`max_characters`, pending edits applied by bevy_text's
`EditableTextSystems`), and the new `TextInput` component (bevy_ui_widgets)
supplies the widget behavior — keyboard editing via `FocusedInput` observers
(keyed to Bevy's `InputFocus`), pointer drag/press, select-all, autoscroll,
read-only modes.

bevy_markup already owns half of this: `InputFocus` (its `HtmlFocus`
navigation, press-to-focus, modal confinement, `:focus`/`:focus-visible`
restyles) and the cascade's `TextFont`/`TextColor`. bevy 0.20's
`bevy_ui_widgets` is **not** part of the crate's minimal `ui_api` feature —
it needs an opt-in.

## Decision

1. **`<input type="text">` becomes a block element with an editable text
   field.** Behind a new `text-input` cargo feature
   (`bevy/bevy_ui_widgets`), `collect_node` maps the tag to a `NodeSpec`
   that spawns the element's Node plus:
   - `EditableText` (initial text from the `value` attribute,
     `max_characters` from `maxlength`),
   - `TextInput` (the widget's observers and editing behavior),
   - the cascade's `TextFont`/`TextColor` (so font family, size, weight,
     color style the field's text),
   - `Focusable` (click/press-to-focus, `autofocus`, `HtmlFocus`
     navigation, `HtmlModal` confinement — all existing behavior).
2. **CSS styles the box like any block.** `background-color`, `border-*`,
     `border-radius`, `padding`, sizes and `box-sizing` apply to the
     input's node; the text styles through the cascade. No new CSS
     properties.
3. **The value is app state, template-driven on write.** The initial value
     comes from the `value` attribute; a re-render sets the text only when
     that attribute's value *changes* (so template updates don't clobber
     what the user typed). The app reads the current text from the
     entity's `EditableText` (bevy_text's editing API) — one query, no new
     signal surface. `data-on-*` hooks on the element work unchanged
     (click/press/…).
4. **`TextInputPlugin` is added by the crate's plugin** when the feature is
     on (idempotent, `is_plugin_added`-guarded), so apps don't need to know
     the widget plugin exists; `bevy_text`'s editable-text systems come
     with it.
5. **The input is a block; `placeholder` and live events are out of scope**
     for this pass (recorded as limits): placeholder rendering and
     input/change events need new signal triggers and text-composition
     plumbing — follow-ups, not half-measures here.

## Alternatives considered

- **Always-on** (no cargo feature): pulls `bevy_ui_widgets` into every
  user's build for a feature most documents never touch — against 0002's
  dependency discipline (the crate's minimal-features rule).
- **A custom text editor** on the crate's own text model: re-implements
  cursor/selection/IME/clipboard — exactly what 0.20's `PlainEditor` and
  `TextInput` now provide; rejected.
- **`is="text-input"` custom element** (app-defined wiring, like the
  examples' sliders): pushes the widget integration into every app; the
  HTML element is the crate's job — `<input>` is standard.

## Consequences

- **The Interactivity limit partially retires**: text input works; other
  form controls (checkbox, select) remain future work.
- **New cargo feature `text-input`** → `bevy/bevy_ui_widgets`; the feature
  gates the `<input>` mapping, the `TextInputPlugin` add, and docs.
  Without it, `<input>` stays an unknown tag (walked through, as today).
- **Focus interplay is inherited, not new**: clicking the field focuses it
  (press-to-focus), the caret follows Bevy's `InputFocus`, and
  `HtmlModal`/`HtmlNoFocus` behave as for any focusable. The widget's
  keyboard observers only act on the focused field.
- **Tests**: dump vectors for the element (components, initial value,
  maxlength, CSS box), a focus test (click → focus → type → the value
  changes), and a content_lint pass stay out (the element renders text
  through bevy_text's editable pipeline — covered by the dump vectors).
- **Docs**: `text-input` joins the cargo-feature list in README/AGENTS;
  the Interactivity limits gain "text input behind `text-input`".
