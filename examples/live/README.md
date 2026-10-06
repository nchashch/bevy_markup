# live

![The live example: a party list with spinning badges and health bars, one row focused](../screenshots/live.png)

Live game data: a party list rendered from game state every frame and
updated in place, so only what changed is touched.

```sh
cargo run --example live
```

## What you see

A "Party" panel: each member has a spinning badge, a name with HP, and a
health bar that drifts up and down (red when low). A line at the bottom left
counts party updates, rows spawned and frames.

| Input | Action |
|---|---|
| **Recruit** / N / gamepad X | A new member joins at the top |
| **Knock out** / K / gamepad B | Knock out the focused member (else the last); the row fades out, then leaves |
| **Language** / L / gamepad Y | English ↔ German |
| Arrows / D-pad / left stick, Enter / A | Move focus between the buttons and the rows, press |

## What it shows

- **Rendering from data every frame** — the app writes the members into the
  `TemplateContext` each frame without diffing anything itself. bevy_markup
  re-renders, skips the update when the HTML is identical, and otherwise
  reconciles it with the existing entities.
- **Keyed reconciliation** — each row has `id="unit-<name>"`, so a member
  added at the top or removed spawns or despawns one row and leaves the
  others — their entities, components and focus — alone. The spawn counter
  shows it: recruiting adds one spawn, not a whole list.
- **Inline styles and opacity** — health bars are `style="width: …%"`, and a
  knocked-out row fades with `style="opacity: …"` (group opacity: text, bar
  and badge together). The `low` class comes and goes on the same entity.
- **Custom elements** — `<div is="badge">` runs once per spawned row and
  starts its spin; a row that was respawned instead of updated would visibly
  jump back.
- **Focus survives updates** — focus a row and recruit: the focus stays on
  that member while a new row appears above it.

## Files

| File | Contents |
|---|---|
| `main.rs` | The party simulation, actions, the badge element |
| `../assets/live/party.html`, `stats.html` | Templates |
| `../assets/live/style.css` | The stylesheet |
| `../assets/live/locales/{en-US,de}/` | Fluent bundles |
| `../shared/input.rs` | Keyboard / gamepad focus navigation shared by the examples |
