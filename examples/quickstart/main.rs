//! The whole API in one small app: an HTML template with Tera variables,
//! Fluent translations and a CSS stylesheet; clickable elements
//! (`data-on-click`, read as `ElementSignal` messages) that the keyboard and
//! a gamepad reach too (`HtmlFocus`); runtime language switching.
//!
//! `cargo run --example quickstart` — click "add a coin" or "Language", or
//! move between them with ↑ ↓ / D-pad and press Enter / A. Space / Y also
//! switches between English and German.

use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy_markup::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            // The example assets live beside the examples, not in ./assets.
            DefaultPlugins.set(AssetPlugin {
                file_path: "examples/assets".into(),
                ..default()
            }),
            BevyMarkupPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (navigate, handle_signals, switch_language))
        .run();
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    // One source per family: the system picks bold and italic faces.
    fonts
        .insert("System Serif", FontFaces::new(FontSource::Serif))
        .insert("System Mono", FontFaces::new(FontSource::Monospace))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(
        asset_server.load("quickstart/style.css"),
    ));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("quickstart/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.spawn((
        HtmlUi::new(asset_server.load("quickstart/hello.html")),
        TemplateContext::new()
            .with("player", "Ada")
            .with("coins", &0),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Keyboard and gamepad: bevy_markup owns focus (`data-on-click` elements
/// are focusable, `autofocus` picks the first) but reads no input — the app
/// binds it. Navigating shows the `:focus-visible` ring; activating sends the
/// same `ElementSignal` a click does.
fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let pad = |button| gamepads.iter().find(|(_, pad)| pad.just_pressed(button));
    // All four directions: `navigate` picks the nearest focusable element
    // that way on screen, so the keys follow whatever the layout is.
    for (key, button, direction) in [
        (
            KeyCode::ArrowUp,
            GamepadButton::DPadUp,
            CompassOctant::North,
        ),
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

/// Clicks (and activations) arrive as `ElementSignal` messages. Wiring
/// them in the template (rather than attaching observers after a build)
/// keeps working when updates keep the elements: the binding is part of them.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
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
            _ => {}
        }
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
    if keys.just_pressed(KeyCode::Space) || pad {
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
