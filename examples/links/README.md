# links

![The links example: a framed field-notes panel with inline and standalone links, one lit as the keyboard-focused link, and a status line showing the last opened href](../screenshots/links.png)

`<a href>` links: an inline run that emits the `open` signal with the href
when clicked — bevy_markup has no HTTP client, so the app decides what
opening means. This one just records it.

```sh
cargo run --example links
```

## What you see

A framed field-notes panel: an intro paragraph with two inline links, three
standalone links of their own, a status line that shows the last opened
href, and the Language button.

| Input | Action |
|---|---|
| Click a link | "Open" it — the status line shows the href |
| ← → ↑ ↓ / D-pad / left stick | Cycle the keyboard focus through the links and the Language button (hold to repeat) |
| Q / E, LB RB | Cycle one link |
| Enter / gamepad A | Open the focused link |
| Click / Enter / A on **Language** | Switch the language (English ↔ German) |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Inline links** — `<a href="…">` runs keep the surrounding text's flow
  (two share the intro sentence). The href travels in the signal's payload
  (`{"href": …}`); a link without an href is plain styled text.
- **Markup from Fluent** — the intro's links come from the translation
  itself: Fluent values carry markup, and `<a>` inside a message works like
  in the template.
- **Inline styling** — a link is an inline run: only text properties reach
  it, so `a` / `a:hover` / `a:focus-visible` recolor it (no boxes, no
  outline on inline elements — like a browser).
- **Keyboard links** — link runs have no box to lay out, so they're not
  `Focusable` blocks; the app cycles them itself (`cycle_links`): it finds
  the link spans with `HtmlElements::by_tag`, sets Bevy's `InputFocus` on
  one, and `HtmlFocus::activate` sends the focused link's signal.
- **Signals** — click and activation arrive as one `open` `ElementSignal`;
  the app writes the href into the `TemplateContext` and the status line
  re-renders in place. The Language button is an ordinary
  `data-on-click` element routed in the same reader.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, link cycling and the `open` handler |
| `../assets/links/report.html` | The template |
| `../assets/links/style.css` | The stylesheet |
| `../assets/links/locales/{en-US,de}/` | Fluent bundles |
| `../assets/shared/frame.png`, `frame_transparent.png` | 9-slice frame images |
