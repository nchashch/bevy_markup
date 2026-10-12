# hot-reload

![The hot-reload example: a framed panel with the palette name, the Cycle palette / Add a coin / Language buttons, and a coin counter](../screenshots/hot-reload.png)

Bevy's asset file watcher plus bevy_markup: edit a template or stylesheet
on disk and the UI restyles without a restart. The app can also rewrite
its stylesheet itself, so the demo works without an editor.

```sh
cargo run --example hot-reload --features hot-reload
```

(The cargo feature enables Bevy's `file_watcher`; without it the example
isn't built.)

## What you see

A framed panel with the current palette's name, a coin counter and three
buttons, **Cycle palette**, **+ Add a coin** and **Language**.

| Input | Action |
|---|---|
| Click a button | Press it |
| ↑ ↓ / D-pad | Move between the buttons |
| Enter / gamepad A | Press the focused button |
| H / gamepad B | Rewrite `style.css` with the next accent palette |
| L / gamepad Y | Switch the language (English ↔ German) |

## What it shows

- **Hot reload** — pressing H (or activating **Cycle palette**) writes the
  next palette's stylesheet to `examples/assets/hot-reload/style.css`. The
  watcher fires, the asset reloads, and the library restyles the UI in
  place: colors change, entities and interaction state survive. Editing
  any file in the folder in an editor does the same.
- **Live context values** — the palette's *name* isn't in the CSS: it
  comes from the template context (`palette` 0–2), and the Fluent message
  selects the wording, so both the stylesheet and the l10n'd name follow
  the same state.
- **No example-side machinery** — the example reads no asset events;
  reacting to `LoadedWithDependencies`/`Modified` is the library's job
  (every template, stylesheet, bundle and image user already does).

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: fonts, stylesheet, locales, the `HtmlUi`, the palette writer, input and signal handling |
| `../assets/hot-reload/page.html` | The template |
| `../assets/hot-reload/style.css` | The stylesheet (rewritten by the palette cycling; the committed copy is palette 0) |
| `../assets/hot-reload/locales/{en-US,de}/` | Fluent bundles |
| `../assets/shared/frame.png`, `frame_transparent.png` | 9-slice frame images |
