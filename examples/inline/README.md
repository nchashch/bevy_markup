# inline

![The journal: a framed panel with a highlighted author name, an underlined day, struck-through done quests beside small seal images, an elliptical quest panel, and a "Field notes" ribbon pinned to the screen's top-right corner](../screenshots/inline.png)

The 0.5.1 inline and corner features in one small app: inline
`background-color` and `text-decoration`, an `<img>` flowing in the text,
elliptical `border-radius` corners, and a `position: fixed` ribbon.

```sh
cargo run --example inline
```

## What you see

A framed field journal: the byline's author name is highlighted (a painted
run), the day is underlined, three quests each carry a small seal image, and
a "Field notes" ribbon is pinned to the screen's top-right corner — over
everything, wherever the window goes.

| Input | Action |
|---|---|
| Click a quest | Toggle it done — its name is struck through |
| C / gamepad X | Complete the next open quest |
| ↑ ↓ / D-pad | Move between the quests and the button |
| Enter / gamepad A | Press the focused element |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Inline backgrounds** — `background-color` on an inline element paints
  behind its text run (Bevy 0.20's `TextBackgroundColor`): the author's
  name, like a marker pen. It fades with the element's `opacity` and survives
  line wraps.
- **Text decoration** — `text-decoration: underline` / `line-through` (+ a
  declared `text-decoration-color`): the day is underlined; toggling a quest
  to done strikes its name out and dims it — live, by re-rendering the
  template with the new class.
- **Inline images** — `<img src="…" width="20">` flows an image into the
  text like a word: the quest seals. `src` resolves relative to the template
  (like `{% include %}`), the aspect ratio is kept.
- **Elliptical corners** — `border-radius: 28px … / 14px …` (the CSS `h / v`
  syntax) on the quest panel; the ribbon rounds one corner elliptically into
  the screen edge.
- **`position: fixed`** — the ribbon is a child of the page, but fixed
  positions it against the viewport: the page's frame, margins and scroll
  don't move it, and nothing clips it.
- **Interaction** — `data-on-click` turns clicks into `ElementSignal`
  messages (the quest's name carries its index), focusable with a
  `:focus-visible` ring, keyboard and gamepad as in the quickstart.

Like the quickstart, the example keeps its navigation inline (no
`examples/shared/input.rs`) so it reads top to bottom; the only shared
module is the `dev-tools` harness.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the `HtmlUi`, input and signal handling |
| `../assets/inline/page.html` | The template |
| `../assets/inline/style.css` | The stylesheet (the features are commented) |
| `../assets/inline/locales/{en-US,de}/` | Fluent bundles |
| `../assets/ui/frame.png` | 9-slice frame of the page, and the seals' image |

With the `dev-tools` feature, the example also serves the localhost
BRP/MCP tool surface (`bevy_mcp_harness`), so an agent can playtest it —
screenshots (`game/screenshot` into `mcp_harness/screenshots/`), the laid-out
UI tree (`game/ui`; quests report `clickable`), mocked keyboard/gamepad/mouse
input (the click above came through it). Off by default; dev/QA only.
