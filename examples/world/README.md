# world

Nameplates over 3D units: HTML + CSS UI anchored to points in the world.

```sh
cargo run --example world
```

## What you see

Six units walk in circles under an orbiting camera, each with a nameplate
above it: a level badge, the name, a localized title, a health bar and a
status line. Allies, hostiles and neutrals each get their own frame. A HUD
in the top left counts the plates on screen and has buttons for every
action.

| Input | Action |
|---|---|
| **Hit nearest** / K / gamepad B | The unit nearest the camera loses health; at zero it — and its plate — disappear |
| **Everyone back** / R / gamepad X | Respawn all units |
| **Plates on/off** / Space / gamepad Select | Toggle the plates |
| **Language** / L / gamepad Y | English ↔ German (the plates re-translate too) |
| ← → / D-pad / left stick, Enter / A | Choose a HUD button, press it |

## What it shows

- **`HtmlWorldAnchor`** — each plate is its own `HtmlUi` root anchored to a
  unit plus an offset: bevy_markup projects that point through the camera
  every frame and keeps the plate's bottom center on it. The anchor hides the
  plate while its point is behind the camera, off screen, or its unit is
  invisible (one unit blinks), and despawns it with its unit.
- **`HtmlWorldAnchorView`** — the measured distance fades plates out
  (`style="opacity: …"`) and orders them (nearer on top, via an inline
  `z-index`); the HUD counts the plates whose point is on screen.
- **Hiding with CSS** — the anchor owns `Visibility`, so the app hides plates
  for its own reasons with a `hidden` class (`display: none`).
- **Plates are real HTML + CSS** — a faction class picks a 9-slice
  `border-image` frame (hostile), a rounded bordered panel (ally) or an
  outlined one (neutral); a monospace level badge, a bold sans-serif name,
  an italic serif title and a three-color health bar.
- **Fluent in the plates** — the level, title and status lines are messages
  with markup (`<b>`, `<i>`, `<span class="{ $faction }">`) and plurals.
- **A HUD that doesn't block the scene** — its root is
  `pointer-events: none`; the button row takes the pointer back with
  `pointer-events: auto`.

## Files

| File | Contents |
|---|---|
| `main.rs` | The scene, units, actions, plate and HUD contexts |
| `../assets/world/plate.html`, `hud.html` | Templates |
| `../assets/world/style.css` | The stylesheet |
| `../assets/world/locales/{en-US,de}/` | Fluent bundles |
| `../assets/ui/frame.png` | The hostile plates' 9-slice frame |
| `../shared/input.rs` | Keyboard / gamepad focus navigation shared by the examples |
