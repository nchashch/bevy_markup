# calc

![A fluid column: a framed sidebar panel (Sidebar 160px), a content column with a gold headline indented 8px + 1em, a monospace formula line reading `width: calc(100% − 160px − 12px)`, three numbered cards each labeled `calc(100% / 3 − 8px)`, and three − / + stepper rows below](../screenshots/calc.png)

CSS `calc()` in layout properties: the content column, the cards and the
headline indent are all `calc()` over mixed units, and the steppers change
the numbers the expressions are built from.

```sh
cargo run --example calc
```

## What you see

A sidebar (a plain templated width) and a content column that gives the
sidebar and the layout gap back: `width: calc(100% - {{ sidebar }}px - 12px)`.
Inside it, a headline indented by `calc({{ indent }}px + 1em)` (`em` against
the headline's own font size), a monospace line showing the column's live
formula, and a card row where each card takes
`width: calc(100% / {{ cards }} - 8px)` — the column divided by the card
count. The three steppers change `sidebar`, `cards` and `indent`; every
expression re-resolves in place (mixed-unit expressions resolve per frame:
`%` against the containing block's content box per axis, `em`/`rem` against
the font sizes, viewport units against the UI root's size).

| Input | Action |
|---|---|
| Click − / + on a row | Step that setting |
| Focus a value (click it), then ← → / D-pad | Step it |
| L / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Mixed-unit `calc()`**: the content width and the card widths mix `%`
  with templated px and division — they resolve in the layout pass each
  frame, so a stepper click reflows the column live.
- **Templated values through inline styles**: the context numbers are
  baked into `style` attributes (`style="width: calc(100% - {{ … }}px -
  12px)"`); the stylesheet keeps the fixed parts. A re-render updates the
  UI in place.
- **`em` inside `calc()`**: the headline's `margin-left` mixes its own
  font-relative unit with a stepped px value.
- **Focus and input**: the − / + buttons and the value boxes are
  focusable (`tabindex="0"` + `data-on-click`); a focused value with
  `data-setting` turns ← → into steps (the shared input plugin's
  `SettingStep`), which is also how the D-pad / left stick drive it.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the steppers and their ranges |
| `../assets/calc/page.html` | The template (the layout, the live formulas, the steppers) |
| `../assets/calc/style.css` | The stylesheet (the calc() rules, the frames, the focus ring) |
| `../assets/calc/locales/{en-US,de}/` | Fluent bundles |
