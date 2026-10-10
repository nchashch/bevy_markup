# forms

![The forms example: a framed signup panel with two fields and a submit button showing the focus ring, next to the receipt panel shown after submitting](../screenshots/forms.png)

Form submission: a `<form data-on-submit>` collects its named `<input>`s
when its `<button type="submit">` is clicked — the form emits one `Submit`
signal whose payload is a JSON object of `{name: value}` (unnamed inputs
excluded). bevy_markup has no HTTP client, so the app decides what
submitting means: this one puts the payload in the context, and the
template swaps the form for a receipt panel.

```sh
cargo run --example forms --features text-input
```

(Without the feature the fields don't render; the form still submits, with
an empty payload.)

## What you see

A framed signup panel: a name and a role field, a submit button and a clear
button. Submitting replaces the form with a receipt showing the values, and
"edit again" brings the form back.

| Input | Action |
|---|---|
| Click a field | Focus it (press-to-focus) and type |
| ↑ ↓ / D-pad | Move between the fields and buttons |
| ← → (in a field) | Move the caret; the widget edits the text |
| Click / Enter / A on **Sign up** | Submit the form |
| Click / Enter / A on **Clear fields** | Empty both fields (a `type="button"` — it doesn't submit) |
| Click / Enter / A on **Edit again** | Back to the form |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Form submission** — clicking the submit button emits the form's
  `Submit` signal with `{"name": …, "role": …}` from the fields' live
  text; a `type="button"` never submits (but stays a clickable, focusable
  block).
- **Buttons are blocks** — `<button>` is a block element: boxes, 9-slice
  frames, `:hover` and `:focus-visible` all work, with or without their own
  `data-on-click`.
- **Tera structure** — `{% if submitted %}` swaps the form for the receipt;
  the receipt's paragraphs take the submitted values as Fluent args.
  Submitting from the app is one context insert.
- **The form is a container** — `<form>` lays out its children like a
  `div` (it's in the container set), so the fields stack in a column.
- **Input-aware navigation** — `examples/shared/input.rs` knows fields:
  while a field has the focus, ← → move its caret and only ↑ ↓ leave it.
- **Keyboard and gamepad** — arrows / D-pad / left stick move focus with
  hold-to-repeat; Enter / A activate, sending the same `ElementSignal` a
  click does.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the submit and clear handling |
| `../assets/forms/signup.html` | The template |
| `../assets/forms/style.css` | The stylesheet |
| `../assets/forms/locales/{en-US,de}/` | Fluent bundles |
| `../assets/shared/frame.png`, `frame_transparent.png` | 9-slice frame images |
