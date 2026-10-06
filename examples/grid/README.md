# grid

![The grid example on its Gear screen: a paper doll of equipment slots around a framed figure, with the tabs, the details panel and the status bar around it](../screenshots/grid.png)

CSS grid layout, three ways. The page itself is a grid of panels, and its
tabs switch the main panel between three screens, each built on a different
grid technique. All of the layout is in the stylesheet; the app only spawns
one `HtmlUi` and sets context variables.

```sh
cargo run --example grid
```

## What you see

| Tab | Screen | Grid technique |
|---|---|---|
| Items | An inventory of slots | `repeat(auto-fill, minmax(120px, 1fr))`: as many columns as fit; a 2×2 featured slot (`span 2`) and `grid-auto-flow: row dense` back-filling the holes it leaves. Resize the window to watch it reflow. |
| Gear | A "paper doll": equipment slots around a character figure | A fixed 3×4 grid. Every slot is placed by line numbers (`grid-column: 3; grid-row: 2`), the figure spans two rows (`grid-row: 2 / 4`), and the cells nobody is placed in stay empty. |
| Quests | A quest board: Active / Done / Failed | Three equal `minmax(0, 1fr)` columns, each a nested grid of cards, under a main quest spanning all of them (`grid-column: 1 / -1`). |

Around them: the page grid (banner and status bar span every column), whose
track template switches between a wide and a narrow layout; the tab list
(`repeat(auto-fit, …)`: a column in the wide sidebar, a row when narrow); and
the stats table in the details panel (`auto 1fr`). The details panel shows
the focused cell of the current screen.

| Input | Action |
|---|---|
| Click / Enter / gamepad A | Press a tab or button |
| Arrows / D-pad / left stick | Move through the current grid (to the nearest cell in that direction, whatever the column count, spans or empty cells) |
| Q / E, gamepad LB / RB | Previous / next tab |
| Space / gamepad X, or **Layout** | Wide ↔ narrow page layout |
| L / gamepad Y, or **Language** | English ↔ German |

## What it shows

- **CSS grid** onto Bevy's grid (taffy): track lists, `repeat()` with
  `auto-fill` / `auto-fit`, `minmax()`, `fr`, line and `span` placement,
  dense packing. Named lines and `grid-template-areas` aren't supported by
  Bevy, which is why the paper doll places by number.
- **Screens from one template** — `{% if active_tab == "gear" %}` picks the
  screen; switching tabs updates the UI in place.
- **Focus in 2D** — every cell is `tabindex="0"`; directional navigation
  follows the layout on every screen.
- **Per-feature data attributes** — the tabs carry `data-tab`, read with
  `ElementSignal::data("tab")`.

## Files

| File | Contents |
|---|---|
| `main.rs` | Items, gear and quests data, tab / layout / language handling, the focused-cell details |
| `../assets/grid/grid.html` | The page and its three screens |
| `../assets/grid/style.css` | All of the layout |
| `../assets/grid/locales/{en-US,de}/` | Fluent bundles |
| `../shared/input.rs` | Keyboard / gamepad focus navigation shared by the examples |
