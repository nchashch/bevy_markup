# inputs

![The inputs example: a framed profile panel with two editable text fields, a live preview paragraph and two reset buttons, one field showing the focus ring](../screenshots/inputs.png)

Text input fields: `<input type="text">` renders Bevy's editable-text
widget behind the `text-input` cargo feature, and the value is app state —
this app reads the fields' live text into the template's context, so a
preview paragraph follows every keystroke.

```sh
cargo run --example inputs --features text-input
```

(Without the feature the fields don't render; the app warns at startup.)

## What you see

A framed profile panel: a name and a title field, a live preview line, and
two reset buttons.

| Input | Action |
|---|---|
| Click a field | Focus it (press-to-focus) and type |
| ↑ ↓ / D-pad | Move between the fields and buttons |
| ← → (in a field) | Move the caret; the widget edits the text |
| Click / Enter / A on a reset button | Write that field back to its initial value |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Text inputs** — `<input type="text">` is a block carrying the widget's
  `EditableText` + `TextInput`; its `value` and `name` attributes carry
  over, `autofocus` takes the initial focus, and the box, text and
  `:focus-visible` ring come from the CSS cascade.
- **The value is app state** — the app reads the fields'
  `EditableText` (looked up by id with `HtmlElements`) and writes them into
  the `TemplateContext`; the preview re-renders on every change. A
  re-render sets a field's text only when the template's `value` attribute
  changes, so typing survives it.
- **Writing fields** — the reset buttons put a fresh `EditableText` on the
  field entity (via their `data-on-click` signals), and the widget renders
  it.
- **Input-aware navigation** — `examples/shared/input.rs` knows fields:
  while a field has the focus, ← → move its caret (the widget reads them
  through `FocusedInput`) and only ↑ ↓ leave it.
- **Keyboard and gamepad** — arrows / D-pad / left stick move focus with
  hold-to-repeat; Enter / A activate, sending the same `ElementSignal` a
  click does.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the live preview and reset handling |
| `../assets/inputs/profile.html` | The template |
| `../assets/inputs/style.css` | The stylesheet |
| `../assets/inputs/locales/{en-US,de}/` | Fluent bundles |
| `../assets/ui/frame.png`, `frame_transparent.png` | 9-slice frame images |
