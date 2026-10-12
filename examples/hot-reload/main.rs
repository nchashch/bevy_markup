//! Hot reload: Bevy's asset file watcher (`--features hot-reload`) restyles
//! the UI the moment a template or stylesheet changes on disk — no restart.
//! Pressing H (or the button) rewrites the stylesheet with the next accent
//! palette from inside the app; editing any file under
//! `examples/assets/hot-reload/` in an editor works the same way. The
//! library reacts to the asset events (`LoadedWithDependencies`/`Modified`)
//! on its own; the example reads none.
//!
//! `cargo run --example hot-reload --features hot-reload` — press
//! `H` / B to cycle the palette, click or activate the buttons, `L` / Y
//! switches the language.

use std::path::PathBuf;

use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/input.rs"]
mod input;

#[path = "../shared/harness.rs"]
mod harness;

use input::Hotkeys;

fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: hot-reload",
            ..Default::default()
        },
        |app| {
            app.add_systems(Startup, setup).add_systems(
                Update,
                (navigate, handle_signals, switch_language, cycle_palette),
            );
        },
    );
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

/// The stylesheet the palette cycling rewrites. An in-repo path: this is a
/// development example, and the asset root is `examples/assets`.
#[derive(Resource)]
struct PaletteFile(PathBuf);

/// The accent palettes, cycled by H / B: one stylesheet per palette,
/// otherwise identical (only the two accent colors change).
const PALETTES: usize = 3;

/// The stylesheet with palette `n`'s accent colors, in place of the two
/// accent declarations of the committed copy (palette 0).
fn palette_css(n: usize) -> String {
    let (accent, ring) = match n {
        1 => ("#34c8c8", "#f0b429"),     // Ocean.
        2 => ("#4dcc71", "#f0ea92"),     // Forest.
        _ => ("#f0b429", "#7fdbff"),     // Crimson: the committed copy.
    };
    format!(
        r#"/* Written by the example's palette cycling (and safe to edit by hand):
   the app rewrites this file with a different accent each press of H, and
   the file watcher restyles every user of the stylesheet without a
   restart. Palette {n} of {PALETTES}. */
html {{
  color: #e6e6e6;
  font-family: serif;
  font-size: 24px;
  /* 9-slice frame: image relative to this file, 16px insets, center drawn. */
  border-image: url("../shared/frame.png") 16 fill stretch;
  border-width: 16px;
  padding: 20px 32px;
}}

/* The accent the palette cycling rewrites. */
h1 {{
  color: {accent};
  font-size: 44px;
  font-weight: bold;
}}

.palette {{
  color: {accent};
  font-style: italic;
}}

kbd {{
  color: {ring};
  font-family: monospace;
}}

.action {{
  color: {accent};
  border-image: url("../shared/frame_transparent.png") 16 fill stretch;
  border-width: 12px;
  padding: 4px 16px;
}}

/* Keyboard/gamepad focus shows a ring; a mouse press hides it again. */
.action:focus-visible {{
  outline: 3px solid {ring};
  outline-offset: 2px;
}}

/* One full-width button per line: the same layout in every language. */
.actions {{
  gap: 8px;
}}

p.hint {{
  color: #9a9a9a;
  font-size: 0.75em;
  font-style: italic;
}}

.wallet {{
  border-image: url("../shared/frame_transparent.png") 16 fill stretch;
  border-width: 12px;
  padding: 12px 16px;
  background-color: #1a1a1a;
  gap: 8px;
}}
"#
    )
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(
        asset_server.load("hot-reload/style.css"),
    ));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("hot-reload/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.insert_resource(PaletteFile(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/assets/hot-reload/style.css"),
    ));

    commands.spawn((
        HtmlUi::new(asset_server.load("hot-reload/page.html")),
        TemplateContext::new()
            .with("player", "Ada")
            .with("coins", &0)
            .with("palette", &0),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Keyboard and gamepad navigation (arrows / D-pad, Enter / A), like
/// quickstart's.
fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let pad = |button| gamepads.iter().find(|(_, pad)| pad.just_pressed(button));
    for (key, button, direction) in [
        (KeyCode::ArrowUp, GamepadButton::DPadUp, CompassOctant::North),
        (
            KeyCode::ArrowDown,
            GamepadButton::DPadDown,
            CompassOctant::South,
        ),
        (
            KeyCode::ArrowLeft,
            GamepadButton::DPadLeft,
            CompassOctant::West,
        ),
        (
            KeyCode::ArrowRight,
            GamepadButton::DPadRight,
            CompassOctant::East,
        ),
    ] {
        if keys.just_pressed(key) || pad(button).is_some() {
            focus.navigate(direction);
        }
    }
    if keys.just_pressed(KeyCode::Enter) {
        focus.activate(ActivationInput::Key(KeyCode::Enter));
    } else if let Some((gamepad, _)) = pad(GamepadButton::South) {
        focus.activate(ActivationInput::GamepadButton {
            gamepad,
            button: GamepadButton::South,
        });
    }
}

/// Clicks (and activations) arrive as `ElementSignal` messages.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
    palettes: Query<&PaletteFile>,
) {
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        match signal.name.as_ref() {
            "add-coin" => {
                for mut context in &mut contexts {
                    let coins = context.get("coins").and_then(|v| v.as_i64()).unwrap_or(0);
                    // Mutating the context re-renders the template (and
                    // updates the UI in place).
                    context.insert("coins", &(coins + 1));
                }
            }
            "switch-language" => next_language(&languages, &mut active),
            "cycle" => {
                if let Ok(palette) = palettes.single() {
                    cycle(&mut contexts, &palette.0);
                }
            }
            _ => {}
        }
    }
}

/// H / B: rewrite the stylesheet with the next palette. The file watcher
/// fires, the asset reloads, and the library restyles the UI in place —
/// this system only writes the file and updates the context's palette
/// number (so the l10n'd palette name follows).
fn cycle_palette(
    hotkeys: Hotkeys,
    mut contexts: Query<&mut TemplateContext>,
    palettes: Query<&PaletteFile>,
) {
    if !hotkeys.just_pressed(KeyCode::KeyH, GamepadButton::East) {
        return;
    }
    if let Ok(palette) = palettes.single() {
        cycle(&mut contexts, &palette.0);
    }
}

fn cycle(contexts: &mut Query<&mut TemplateContext>, stylesheet: &PathBuf) {
    let current = contexts
        .iter()
        .find_map(|context| context.get("palette").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let next = (current as usize + 1) % PALETTES;
    if let Err(err) = std::fs::write(stylesheet, palette_css(next)) {
        error!("writing {}: {err}", stylesheet.display());
    }
    for mut context in contexts.iter_mut() {
        context.insert("palette", &(next as i64));
    }
}

fn switch_language(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    let pad = gamepads
        .iter()
        .any(|pad| pad.just_pressed(GamepadButton::North));
    if keys.just_pressed(KeyCode::KeyL) || pad {
        next_language(&languages, &mut active);
    }
}

fn next_language(languages: &Languages, active: &mut ActiveLocale) {
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
