# grid-names

![The camp board: a grid of rounded panels — the cat portrait spanning two rows, the stats, the gear list with counts at the right edge, and the journal — with the hint text above](../screenshots/grid-names.png)

Named grid lines and areas: the board's layout is named in the stylesheet,
and the panels place by those names.

```sh
cargo run --example grid-names
```

## What you see

A camp board grid: a portrait panel (the cat, spanning two rows), the
stats, the gear list, and the journal. Clicking a part checks it off (it
dims; the context re-renders in place).

| Input | Action |
|---|---|
| Click a panel or a supply row | Check it off |
| ↑ ↓ / D-pad / left stick | Move between the parts |
| Enter / gamepad A | Check the focused part off |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **`grid-template-areas`** names the board's regions (`"portrait stats" /
  "portrait gear" / "log log"`); the declaration implies the explicit
  tracks it spans (two columns, three rows, auto-sized — the template
  columns override the sizing). Each panel places by its area's name:
  `#portrait { grid-area: portrait }` — all four edges at once.
- **Named lines**: the supply list's columns are
  `[left] 1fr [middle] 60px [right]`; its rows place between the named
  lines (`grid-column: left / right`) and stretch across the panel.
- **In-place updates**: the checks live in the context (`picks` + the
  items' `checked` flags); flipping one re-renders the template and dims
  the part, keeping focus and hover.
- **Focus and input**: the panels and rows are focusable
  (`tabindex="0"` + `data-on-click`); the shared input plugin binds
  arrows / D-pad / left stick and Enter / A.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the check-off handling |
| `../assets/grid-names/board.html` | The template |
| `../assets/grid-names/style.css` | The stylesheet (the areas, the named lines, the panel chrome) |
| `../assets/grid-names/locales/{en-US,de}/` | Fluent bundles |
| `../assets/shared/cat.png` | The portrait |
