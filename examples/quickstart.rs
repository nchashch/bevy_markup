//! The whole API in one small app: an HTML template with Tera variables,
//! Fluent translations and a CSS stylesheet; a clickable element
//! (`data-on-click`, read as `ElementSignal` messages); runtime language
//! switching.
//!
//! `cargo run --example quickstart` — click "add a coin", press Space to switch
//! between English and German.

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
        .add_systems(Update, (add_coin, switch_language))
        .run();
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

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

    let languages: Vec<Handle<BundleAsset>> = ["en-US", "de"]
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

/// `data-on-click="add-coin"` arrives as an `ElementSignal` message. Wiring
/// it in the template (rather than attaching an observer after a build)
/// keeps working when updates keep the element: the binding is part of it.
fn add_coin(mut signals: MessageReader<ElementSignal>, mut contexts: Query<&mut TemplateContext>) {
    for signal in signals.read() {
        if signal.name != "add-coin" || signal.trigger != SignalTrigger::Click {
            continue;
        }
        for mut context in &mut contexts {
            let coins = context.get("coins").and_then(|v| v.as_i64()).unwrap_or(0);
            // Mutating the context re-renders the template (and updates the
            // UI in place).
            context.insert("coins", &(coins + 1));
        }
    }
}

fn switch_language(
    keys: Res<ButtonInput<KeyCode>>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
