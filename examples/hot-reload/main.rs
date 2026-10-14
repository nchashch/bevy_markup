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

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/input.rs"]
mod input;

#[path = "../shared/harness.rs"]
mod harness;

use input::Hotkeys;

#[hotpath::main]
fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: hot-reload",
            ..Default::default()
        },
        |app| {
            app.add_systems(Startup, setup)
                .add_systems(Update, (handle_signals, switch_language, cycle_palette));
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
/// otherwise identical (only the two accent colors and the comment naming
/// the palette change).
const PALETTES: usize = 3;

/// The stylesheet with palette `n`'s accent colors. Palette 0 is
/// byte-identical to the committed copy, so cycling back to it leaves the
/// tracked file unmodified.
fn palette_css(n: usize) -> String {
    let (name, accent, ring) = match n {
        1 => ("ocean", "#34c8c8", "#f0b429"),
        2 => ("forest", "#4dcc71", "#f0ea92"),
        _ => ("crimson", "#f0b429", "#7fdbff"),
    };
    format!(
        r#"/* Written by the example's palette cycling (and safe to edit by hand):
   the app rewrites this file with a different accent each press of H, and
   the file watcher restyles every user of the stylesheet without a
   restart. The committed copy is palette 0. */
html {{
  color: #e6e6e6;
  font-family: serif;
  font-size: 24px;
  /* 9-slice frame: image relative to this file, 16px insets, center drawn. */
  border-image: url("../shared/frame.png") 16 fill stretch;
  border-width: 16px;
  padding: 20px 32px;
}}

/* The accent the palette cycling rewrites (palette {n}: {name}). */
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

    let stylesheet =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/assets/hot-reload/style.css");
    // The file keeps the last palette a run wrote: start from it, so the
    // palette name matches the colors.
    let palette = palette_on_disk(&stylesheet);
    commands.insert_resource(PaletteFile(stylesheet));

    commands.spawn((
        HtmlUi::new(asset_server.load("hot-reload/page.html")),
        TemplateContext::new()
            .with("player", "Ada")
            .with("coins", &0)
            .with("palette", &(palette as i64)),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Clicks (and activations) arrive as `ElementSignal` messages.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
    palettes: Query<&PaletteFile>,
    exit: Option<Res<crate::harness::ExitAfter>>,
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
                    cycle(&mut contexts, &palette.0, exit.is_some());
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
    // A bounded (`--exit-after`) run leaves the tracked file alone: the
    // harness injects keys, so the cycle would rewrite the committed
    // stylesheet and dirty the working tree.
    exit: Option<Res<crate::harness::ExitAfter>>,
) {
    if !hotkeys.just_pressed(KeyCode::KeyH, GamepadButton::East) {
        return;
    }
    if let Ok(palette) = palettes.single() {
        cycle(&mut contexts, &palette.0, exit.is_some());
    }
}

fn cycle(
    contexts: &mut Query<&mut TemplateContext>,
    stylesheet: &PathBuf,
    bounded: bool,
) {
    let current = contexts
        .iter()
        .find_map(|context| context.get("palette").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let next = (current as usize + 1) % PALETTES;
    if !bounded {
        if let Err(err) = std::fs::write(stylesheet, palette_css(next)) {
            error!("writing {}: {err}", stylesheet.display());
        }
    }
    for mut context in contexts.iter_mut() {
        context.insert("palette", &(next as i64));
    }
}

/// The palette number in the stylesheet's accent comment (`palette_css`'s
/// `(palette N: name)`); 0 when the file is missing or the comment was
/// edited away.
fn palette_on_disk(stylesheet: &Path) -> usize {
    std::fs::read_to_string(stylesheet)
        .ok()
        .and_then(|css| {
            let (_, rest) = css.split_once("(palette ")?;
            rest.split_once(':')?.0.parse().ok()
        })
        .filter(|&n| n < PALETTES)
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Cycling back to palette 0 restores the committed stylesheet
    /// byte for byte (no stray diff in the working tree), and every palette
    /// reads back as itself.
    #[test]
    fn palettes_round_trip_through_the_file() {
        let committed = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/assets/hot-reload/style.css"),
        )
        .unwrap();
        assert_eq!(
            palette_css(0),
            committed,
            "palette 0 must be the committed style.css (if a run left another \
             palette in the working tree, cycle back to palette 0 first)"
        );
        let dir = std::env::temp_dir().join(format!("hot-reload-palette-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..PALETTES {
            let file = dir.join("style.css");
            std::fs::write(&file, palette_css(n)).unwrap();
            assert_eq!(palette_on_disk(&file), n);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
