# quickstart

The whole bevy_markup API in one small app: an HTML template, a CSS
stylesheet and Fluent translations become a native Bevy UI, with two buttons
that work with the mouse, the keyboard and a gamepad.

```sh
cargo run --example quickstart
```

## What you see

A framed panel that greets the player, counts their coins and has two
buttons, **+ Add a coin** and **Language**.

| Input | Action |
|---|---|
| Click a button | Press it |
| ↑ ↓ / D-pad | Move between the buttons |
| Enter / gamepad A | Press the focused button |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Tera templates** — `{{ player }}` and `{{ coins }}` come from the
  entity's `TemplateContext`; changing a value re-renders the template and
  updates the UI in place.
- **Fluent localization** — every visible string is a `data-l10n-id`
  message; translations carry inline markup (`<b>`, `<em>`, `<kbd>`) and
  plurals ("no coins" / "one coin" / "3 coins").
- **CSS** — fonts, colors and 9-slice `border-image` frames come from the
  stylesheet; the buttons stack in a column and get a `:focus-visible` ring.
- **Interaction** — `data-on-click` turns clicks into `ElementSignal`
  messages; elements with `data-on-click` are focusable, and `autofocus`
  picks the first.
- **Keyboard and gamepad** — a short `navigate` system binds arrows /
  D-pad to `HtmlFocus::navigate` and Enter / A to `HtmlFocus::activate`,
  which sends the same signal a click does. bevy_markup owns focus but reads
  no input; the app chooses its bindings.

This example is deliberately self-contained (unlike the others, it doesn't
use `examples/shared/input.rs`), so it can be read top to bottom.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the `HtmlUi`, input and signal handling |
| `../assets/quickstart/hello.html` | The template |
| `../assets/quickstart/style.css` | The stylesheet |
| `../assets/quickstart/locales/{en-US,de}/` | Fluent bundles |
| `../assets/ui/frame.png`, `frame_transparent.png` | 9-slice frame images |

The examples share one asset root (`examples/assets/`, set with
`AssetPlugin::file_path`), so each example's templates live in its own
folder there.
