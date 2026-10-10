//! The 0.5.1 features in one small app: an inline element's
//! `background-color` paints behind its text run and `text-decoration`
//! underlines or strikes it; `<img>` flows an image into the text; CSS
//! `border-radius: h / v` gives panels elliptical corners; `position: fixed`
//! pins the ribbon to the viewport, ignoring the framed page.
//!
//! Clicking a quest (keyboard: ↑ ↓ / D-pad + Enter / A) toggles it done —
//! its name is struck through. Space / Y switches English and German.
//!
//! `cargo run --example inline`

use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/harness.rs"]
mod harness;

fn main() {
    App::new()
        .add_plugins((
            // The example assets live beside the examples, not in ./assets.
            DefaultPlugins.set(AssetPlugin {
                file_path: "examples/assets".into(),
                ..default()
            }),
            BevyMarkupPlugin,
            harness::HarnessPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (navigate, complete_next, handle_signals, switch_language))
        .run();
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

/// The quests' done-flags: the source of truth lives in the app, the
/// context's `quests` mirrors it (each `context.insert` re-renders).
#[derive(Resource, Clone)]
struct Quests(Vec<bool>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(asset_server.load("inline/style.css")));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("inline/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    // The quests are just done-flags: their names come from Fluent
    // (`journal-quest-<index>`), so every locale translates them.
    let quests = Quests(vec![true, false, false]);
    commands.insert_resource(quests.clone());
    commands.spawn((
        HtmlUi::new(asset_server.load("inline/page.html")),
        TemplateContext::new()
            .with("player", "Ada")
            .with("day", &7)
            .with("quests", &quests.0),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Keyboard and gamepad navigation, as in the quickstart example.
fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let pad = |button| gamepads.iter().find(|(_, pad)| pad.just_pressed(button));
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

/// The C key / West button completes the next open quest — the same context
/// change a click sends.
fn complete_next(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut signals: MessageWriter<ElementSignal>,
) {
    let pad = gamepads
        .iter()
        .any(|pad| pad.just_pressed(GamepadButton::West));
    if keys.just_pressed(KeyCode::KeyC) || pad {
        signals.write(ElementSignal {
            name: "complete-next".into(),
            trigger: SignalTrigger::Click,
            target: Entity::PLACEHOLDER,
            element: default(),
            payload: default(),
            source: SignalSource::Activation(ActivationInput::Key(KeyCode::KeyC)),
        });
    }
}

/// Clicks (and activations) arrive as `ElementSignal` messages. A quest's
/// signal name carries its index (`toggle-quest-<index>`); toggling the
/// resource re-renders the template and the name loses its strikethrough.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
    mut quests: ResMut<Quests>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        match signal.name.as_ref() {
            name if name.starts_with("toggle-quest-") => {
                if let Ok(quest) = name["toggle-quest-".len()..].parse::<usize>()
                    && let Some(done) = quests.0.get_mut(quest)
                {
                    *done = !*done;
                    for mut context in &mut contexts {
                        context.insert("quests", &quests.0);
                    }
                }
            }
            "complete-next" => {
                let open = quests.0.iter().position(|done| !done);
                if let Some(quest) = open {
                    quests.0[quest] = true;
                    for mut context in &mut contexts {
                        context.insert("quests", &quests.0);
                    }
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
