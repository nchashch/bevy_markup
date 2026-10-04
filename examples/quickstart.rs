//! The whole API in one small app: an HTML template with Tera variables,
//! Fluent translations and a CSS stylesheet; a clickable element wired up via
//! `HtmlUiBuilt` + `HtmlElements`; runtime language switching.
//!
//! `cargo run --example quickstart` — click "add a coin", press Space to switch
//! between English and German.

use bevy::prelude::*;
use p23::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, HtmlUiPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, switch_language)
        .add_observer(wire_coin_button)
        .run();
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS font-family names → font files.
    fonts
        .insert(
            "Spectral",
            FontFaces::new(asset_server.load("fonts/Spectral-Regular.ttf"))
                .with_bold(asset_server.load("fonts/Spectral-Bold.ttf"))
                .with_italic(asset_server.load("fonts/Spectral-Italic.ttf")),
        )
        .insert(
            "Iosevka Slab Mono",
            FontFaces::new(asset_server.load("fonts/IosevkaSlabMono-Regular.ttf")),
        )
        .set_generic(GenericFamily::Monospace, "Iosevka Slab Mono");

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
        TemplateContext::new().with("player", "Ada").with("coins", &0),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Children are rebuilt on every change, so wire behaviour on each build.
fn wire_coin_button(built: On<HtmlUiBuilt>, elements: HtmlElements, mut commands: Commands) {
    let Some(button) = elements.by_id(built.entity, "add-coin") else {
        return;
    };
    let ui = built.entity;
    commands.entity(button).insert(Button).observe(
        move |_: On<Pointer<Click>>, mut contexts: Query<&mut TemplateContext>| {
            let Ok(mut context) = contexts.get_mut(ui) else {
                return;
            };
            let coins = context.get("coins").and_then(|v| v.as_i64()).unwrap_or(0);
            // Mutating the context re-renders the template.
            context.insert("coins", &(coins + 1));
        },
    );
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
