# demo

Every feature at once. The whole app — panels, buttons, 9-slice frames,
scrollbars and their layout — is one HTML document styled with CSS. The Rust
side only wires behavior onto it: clicks, scrolling, language and theme
selection, and the content documents spawned into the shell's panels.

```sh
cargo run --example demo
```

## What you see

Five columns:

1. **Inventory** — a rendered document: Tera variables, Fluent messages with
   inline markup and plurals, a code block, lists, and a long expedition log
   (long enough to scroll even on a 4K screen).
2. **About, Language, Theme, Scrollbars** — four languages (English, Russian,
   German, Japanese), four themes (Crimson, Parchment, Terminal, Large
   print) and a scrollbar on/off toggle.
3. – 5. **DOM outlines** — the parsed DOM of three documents (a plain HTML
   page, a Tera template, and the localized inventory), shown as text.

Panels whose content doesn't fit scroll, with styled scrollbars that appear
only where something overflows.

| Input | Action |
|---|---|
| Click an option | Select it |
| Mouse wheel | Scroll the panel under the pointer |
| Drag a scrollbar thumb / click its track | Scroll / page |
| ← → / D-pad / left stick | Step focus through the panels and buttons |
| ↑ ↓ / D-pad / left stick, right stick, PageUp / PageDown | Scroll the focused panel (or the middle column when a button there is focused) |
| Enter / gamepad A | Press the focused option |
| L / gamepad Y, T / gamepad X | Next language, next theme |

## What it shows

- **One document as the app** — `ui/content/shell.html`; the themes' "Demo
  chrome" rules are its whole layout (columns, panels, scroll areas).
- **Theming** — switching `DefaultStylesheet` restyles every document,
  9-slice frames and scrollbar colors included.
- **Localization** — switching `ActiveLocale` re-translates everything,
  including CJK text and Russian plurals.
- **Nested documents** — `<div is="content-slot">` spawns a separate
  `HtmlUi` into each panel; shell updates keep them.
- **Scrolling** — viewports with `overflow-y: scroll` and Bevy's
  `ScrollArea`. The flex chain down to each viewport opts into shrinking
  (bevy_markup containers default to `flex-shrink: 0`).
- **Styled scrollbars** — `<div class="scrollbar" is="scrollbar">` with a
  `.thumb`; the theme styles both, and the app feeds the thumb's position and
  the bar's visibility to the template (`style`, `.hidden`, `.dragging`).
- **Keyboard and gamepad** — `ArrowMode::Linear`: left / right step through
  controls in document order, up / down scroll; focus moves scroll the
  focused option into view.

## Files

| File | Contents |
|---|---|
| `main.rs` | App setup and wiring |
| `shell.rs` | The shell document, content slots, scrolling, scrollbars |
| `controls.rs` | Language / theme / scrollbar selection and hotkeys |
| `consts.rs` | Fonts |
| `../assets/ui/content/shell.html` | The shell document |
| `../assets/ui/content/l10n.html`, `inventory.html`, `test.html` | The content documents |
| `../assets/ui/themes/*.css` | The four themes |
| `../assets/locales/{en-US,ru,de,ja}/` | Fluent bundles |
| `../shared/input.rs` | Keyboard / gamepad focus navigation shared by the examples |
