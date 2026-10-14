//! CSS combinators: descendant (a space) and child (`>`) chains. Clicking
//! a group selects it — the app flips the group's `selected` class in the
//! context, the template re-renders, and two rules reach different
//! depths: `.selected p` highlights every paragraph inside (the nested
//! one too), `.selected > p` only the group's direct children.
//!
//! `cargo run --example combinators` — click the groups, or ↑ ↓ / D-pad +
//! Enter / A to select them. Space / Y switches English/German.

use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/input.rs"]
mod input;

#[path = "../shared/harness.rs"]
mod harness;

#[hotpath::main]
fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: CSS combinators",
            ..Default::default()
        },
        |app| {
            app.add_systems(Startup, setup)
                .add_systems(Update, (handle_signals, switch_language));
        },
    );
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

/// The groups' names, in document order (the template labels localize).
const GROUPS: [&str; 3] = ["north", "middle", "south"];

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(
        asset_server.load("combinators/style.css"),
    ));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("combinators/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.spawn((
        HtmlUi::new(asset_server.load("combinators/panel.html")),
        selected_context(0),
        Node {
            width: Val::Px(640.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// The template's context for the selected group: each group's `selected`
/// flag and the selected group's name for the status line.
fn selected_context(selected: usize) -> TemplateContext {
    let groups: Vec<_> = GROUPS
        .iter()
        .enumerate()
        .map(|(index, label)| {
            serde_json::json!({
                "label": label,
                "selected": index == selected,
            })
        })
        .collect();
    TemplateContext::new()
        .with("groups", &groups)
        .with("selected_label", GROUPS[selected])
}

/// Clicks (and activations) select the clicked group: the context flips
/// the flags and the template re-renders in place.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
) {
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        let Some(selected) = signal
            .name
            .as_ref()
            .strip_prefix("select-")
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        let selected = selected.min(GROUPS.len() - 1);
        for mut context in &mut contexts {
            *context = selected_context(selected);
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
