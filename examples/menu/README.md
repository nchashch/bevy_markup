# menu

A settings menu that works equally well with a mouse, a keyboard and a
gamepad: focus and navigation, a slider, a stepper, a modal confirm dialog,
tooltips and a reusable button component.

```sh
cargo run --example menu
```

## What you see

A "Settings" panel with:

- **Volume** — a slider. Drag or click the bar, step with the ◀ ▶ buttons,
  or focus the bar and use ← → / D-pad / left stick (hold to repeat).
- **Difficulty** — a stepper: ◀ [Easy / Normal / Hard] ▶. The middle is a
  focusable label, not a button; ← → change it while it's focused, stopping
  at the ends.
- **Reset to defaults** — opens a confirm dialog that keeps focus until you
  answer.
- A line showing which input produced the last click (left / right / middle
  mouse button, touch, keyboard, gamepad).

| Input | Action |
|---|---|
| Arrows / D-pad / left stick | Move focus; ← → change the focused setting |
| Enter / gamepad A | Press the focused button |
| Esc / gamepad B | Close the dialog |
| L / gamepad Y | English ↔ German |

Hover a control for its tooltip.

## What it shows

- **Focus and navigation** — `autofocus`, directional navigation through
  `HtmlFocus`, a `:focus-visible` ring that a mouse press hides again, and
  `HtmlModal` confining focus to the dialog.
- **Built-in tooltips** — elements with `data-tooltip="<Fluent key>"`; the
  app only inserts `HtmlTooltips` with a tooltip template.
- **Template components** — every button is one Tera 2 component,
  `ui.button` in `components.html`, which the menu and the dialog include
  (relative paths) and call with their content as the body:
  `{% <ui.button id="reset" signal="reset"> %}…{% </ui.button> %}`.
- **Signals routed to systems** — `app.on_html_click("reset", open_dialog)`;
  the dialog's two buttons share one handler and tell themselves apart by
  `data-answer` (`ElementSignal::data`). A plain `MessageReader` sees the
  same signals (the "last click" line), and `ElementSignal::source` says what
  produced each one.
- **Custom elements** — `<div is="icon" data-src="…">` inserts an image;
  `<div is="slider">` observes presses and drags on the bar.
- **In-place updates** — the settings are written into the template every
  frame; the menu updates in place, so focus, hover and drags survive.
- **Roots styled by CSS** — each root's placement, stacking (`z-index`),
  dimming backdrop and pickability are its `<html class>` rule.

## Files

| File | Contents |
|---|---|
| `main.rs` | Settings, signal handlers, the slider and icon elements |
| `../assets/menu/menu.html`, `dialog.html`, `tooltip.html` | Templates |
| `../assets/menu/components.html` | The `ui.button` component library |
| `../assets/menu/style.css` | The stylesheet |
| `../assets/menu/locales/{en-US,de}/` | Fluent bundles |
| `../shared/input.rs` | Keyboard / gamepad focus navigation shared by the examples (`data-setting` controls take ← → themselves) |
