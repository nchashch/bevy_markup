//! Localized Markdown documents: a reader with two pages of patch-notes
//! and tutorial prose, where every piece is Markdown (ADR 0014) — the
//! pages are `.md` templates included by a shell, and the prose itself is
//! Fluent values in Markdown (the `markdown: true` bundle), so switching
//! the language switches the documents. The tabs and the Language button
//! are raw HTML inside the Markdown page: real, clickable UI.
//!
//! `cargo run --example markdown`

use bevy::prelude::*;
use bevy_markup::prelude::*;

#[hotpath::main]
fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: markdown",
            ..Default::default()
        },
        |app| {
            app.add_systems(Startup, setup)
                .add_systems(Update, (hotkeys, handle_signals, switch_language));
        },
    );
}

#[path = "../shared/input.rs"]
mod input;

#[path = "../shared/harness.rs"]
mod harness;

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(
        asset_server.load("markdown/style.css"),
    ));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("markdown/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.spawn((
        HtmlUi::new(asset_server.load("markdown/reader.md")),
        TemplateContext::new().with("active_tab", "release"),
        Node {
            width: Val::Px(640.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            ..default()
        },
    ));
}

/// Page and language hotkeys: Q / E and the bumpers switch pages (the same
/// handler a tab click sends, one tab over), Space / Y and North switch
/// the language.
fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut signals: MessageWriter<ElementSignal>,
) {
    let pressed = |button| gamepads.iter().any(|(_, pad)| pad.just_pressed(button));
    let step = if keys.just_pressed(KeyCode::KeyQ) || pressed(GamepadButton::LeftTrigger) {
        Some("-1")
    } else if keys.just_pressed(KeyCode::KeyE) || pressed(GamepadButton::RightTrigger) {
        Some("+1")
    } else {
        None
    };
    if let Some(step) = step {
        signals.write(ElementSignal {
            name: "page".into(),
            trigger: SignalTrigger::Click,
            target: Entity::PLACEHOLDER,
            element: default(),
            payload: serde_json::json!({ "step": step }),
            source: SignalSource::Activation(ActivationInput::Key(KeyCode::KeyQ)),
        });
    }
}

/// Clicks (and activations) arrive as `ElementSignal` messages: a tab's
/// `data-tab` picks the page, `page` steps it from the hotkeys.
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
            "tab" => {
                if let Some(tab) = signal.data("tab") {
                    for mut context in &mut contexts {
                        context.insert("active_tab", tab);
                    }
                }
            }
            "page" => {
                let Some(step) = signal.payload.get("step").and_then(|s| s.as_str()) else {
                    continue;
                };
                for mut context in &mut contexts {
                    let tabs = ["release", "tutorial"];
                    let current = context
                        .get("active_tab")
                        .and_then(|v| v.as_str())
                        .and_then(|tab| tabs.iter().position(|t| *t == tab))
                        .unwrap_or(0);
                    let next = match step {
                        "+1" => (current + 1) % tabs.len(),
                        "-1" => (current + tabs.len() - 1) % tabs.len(),
                        _ => current,
                    };
                    context.insert("active_tab", tabs[next]);
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
