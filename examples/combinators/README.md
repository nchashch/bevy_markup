# combinators

![The selectors page: three group panels, the middle one selected — its direct paragraph blue and bold, its nested paragraph mint with a blue border — and the status line reading "Selected: middle"](../screenshots/combinators.png)

CSS combinators: descendant (a space) and child (`>`) chains reaching
different depths.

```sh
cargo run --example combinators
```

## What you see

Three group panels; one is selected at a time (click it, or focus it and
press Enter / A). Selecting highlights the paragraphs inside — and the two
highlighting rules reach different depths:

- `.selected p` (a descendant chain) hits **every** paragraph inside — the
  nested one, one level down, turns mint;
- `.selected > p` (a child chain) hits **only the group's direct
  children** — the direct paragraph turns blue and bold, the nested one
  doesn't;
- `.selected .nested` lights the nested block's border.

| Input | Action |
|---|---|
| Click a group | Select it |
| ↑ ↓ / D-pad / left stick | Move between the groups |
| Enter / gamepad A | Select the focused group |
| Space / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Combinator matching**: the selection is one class flip in the context;
  the re-render's fresh document re-runs the chains, and the difference
  between the two rules is visible immediately (the nested paragraph is
  mint but never blue — the child rule stops at one level).
- **Specificity**: the child rule and the descendant rule could tie —
  source order picks the winner, as in CSS.
- **In-place updates**: the groups keep their entities (focus and hover
  survive the re-render); only the class attribute changes.
- **Focus and input**: the groups are focusable (`tabindex="0"` +
  `data-on-click`); the shared input plugin binds arrows / D-pad / left
  stick and Enter / A.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the selection handling |
| `../assets/combinators/panel.html` | The template |
| `../assets/combinators/style.css` | The stylesheet (the two depth rules) |
| `../assets/combinators/locales/{en-US,de}/` | Fluent bundles |
