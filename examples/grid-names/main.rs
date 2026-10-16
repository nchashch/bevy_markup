//! Named grid lines and areas: the board is a CSS grid whose *areas* are
//! named in the stylesheet (`grid-template-areas`; each panel places by
//! its area's name, one spanning two rows), and the supply list names its
//! own *column lines* (`[left] … [middle] … [right]`); the rows place
//! between the named lines. Clicking a part checks it off (the context
//! re-renders in place).
//!
//! `cargo run --example grid-names` — click the panels and the supply
//! rows, or ↑ ↓ / D-pad + Enter / A. Space / Y switches English/German.

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
            title: "bevy_markup: named grid lines and areas",
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

/// What's checked off: the clicked part's id, so the template can mark it.
#[derive(Resource, Clone)]
struct Picks(Vec<bool>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(
        asset_server.load("grid-names/style.css"),
    ));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("grid-names/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    // The pick marks: one per board part (portrait, stats, gear, log) and
    // one per supply row.
    let picks = Picks(vec![false; 7]);
    commands.insert_resource(picks.clone());

    commands.spawn((
        HtmlUi::new(asset_server.load("grid-names/board.html")),
        TemplateContext::new()
            .with("player", "Ada")
            .with("hp", &7)
            .with("max_hp", &10)
            .with("coins", &12)
            .with("picks", &picks.0)
            .with(
                "items",
                &serde_json::json!([
                    { "name": "torch", "count": 3, "checked": false },
                    { "name": "rope", "count": 1, "checked": false },
                    { "name": "key", "count": 0, "checked": false },
                ]),
            ),
        Node {
            width: Val::Px(640.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Clicks (and activations) check a board part off: the context's `picks`
/// flips the flag and the template re-renders (the checked part dims).
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut picks: ResMut<Picks>,
    mut contexts: Query<&mut TemplateContext>,
) {
    let picked = |name: &str| -> Option<usize> {
        let index = name.strip_prefix("pick-")?;
        match index {
            "portrait" => Some(0),
            "stats" => Some(1),
            "gear" => Some(2),
            "log" => Some(3),
            other => other
                .strip_prefix("item-")?
                .parse::<usize>()
                .ok()
                .map(|i| i + 4),
        }
    };
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        if let Some(index) = picked(signal.name.as_ref()) {
            let index = index.min(picks.0.len() - 1);
            // Flip in place and write back: the previous code cloned into
            // the context only, so every click restarted from the initial
            // flags (cells couldn't be unchecked, checking one unchecked
            // the others).
            picks.0[index] = !picks.0[index];
            let flags = picks.0.clone();
            for mut context in &mut contexts {
                context.insert("picks", &flags);
            }
            // The supply rows' checked flags live in the items (the JSON
            // the context carries): flip `checked` on the row's item.
            if index >= 4 {
                let item = index - 4;
                for mut context in &mut contexts {
                    let mut items = context
                        .get("items")
                        .and_then(|v| serde_json::to_value(v).ok())
                        .unwrap_or_else(|| serde_json::json!([]));
                    if let Some(entry) = items.get_mut(item) {
                        entry["checked"] =
                            serde_json::json!(!entry["checked"].as_bool().unwrap_or(false));
                    }
                    context.insert("items", &items);
                }
            }
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

/// The picking state machine: a click flips the part's flag on the
/// resource (R64: writing only the context made every click restart from
/// the initial flags — cells couldn't be unchecked, checking one
/// unchecked the others).
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn clicks_flip_and_persist_on_the_resource() {
        let mut app = App::new();
        app.add_message::<ElementSignal>();
        app.insert_resource(Picks(vec![false; 4]));
        let ui = app
            .world_mut()
            .spawn(TemplateContext::new().with("picks", &vec![false; 4]))
            .id();
        app.add_systems(Update, handle_signals);

        let click = |app: &mut App, name: &'static str| {
            app.world_mut()
                .resource_mut::<Messages<ElementSignal>>()
                .write(ElementSignal {
                    trigger: SignalTrigger::Click,
                    name: name.into(),
                    element: HtmlElement::default(),
                    target: ui,
                    payload: Default::default(),
                    source: SignalSource::Activation(ActivationInput::Synthetic),
                });
            app.update();
        };

        click(&mut app, "pick-portrait");
        let picks = app.world().resource::<Picks>().clone();
        assert_eq!(picks.0, [true, false, false, false], "checked");
        let context = app.world().get::<TemplateContext>(ui).unwrap();
        assert_eq!(
            context.get("picks").unwrap(),
            &tera::Value::from(vec![true, false, false, false])
        );

        click(&mut app, "pick-portrait");
        let picks = app.world().resource::<Picks>().clone();
        assert_eq!(picks.0, [false, false, false, false], "unchecked");
        let context = app.world().get::<TemplateContext>(ui).unwrap();
        assert_eq!(
            context.get("picks").unwrap(),
            &tera::Value::from(vec![false, false, false, false])
        );

        click(&mut app, "pick-stats");
        let picks = app.world().resource::<Picks>().clone();
        assert_eq!(
            picks.0,
            [false, true, false, false],
            "checking another keeps the first"
        );
    }
}
