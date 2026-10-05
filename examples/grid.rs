//! CSS grid layout: a page grid of panels whose track template switches at
//! runtime, a responsive `repeat(auto-fill, minmax(…))` slot grid with a
//! spanning item and dense packing, and small grids inside slots and the
//! stats table. All layout is in `grid/style.css`; the app only spawns one
//! `HtmlUi` and toggles a context variable.
//!
//! `cargo run --example grid` — press Space to switch between the wide and
//! the narrow layout, L to switch the language (English/German); resize the
//! window to watch the slots reflow.

use bevy::prelude::*;
use bevy_markup::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            // The example assets live beside the examples, not in ./assets.
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: "examples/assets".into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "bevy_markup: CSS grid".into(),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
        ))
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_layout, switch_language))
        .run();
}

/// Inventory shown in the slots: (id, count, featured). Ids are data; the
/// names come from Fluent (`grid-item-<id>`).
const ITEMS: &[(&str, u32, bool)] = &[
    ("sword", 1, false),
    ("map", 1, true),
    ("shield", 1, false),
    ("potion", 5, false),
    ("herb", 12, false),
    ("lantern", 1, false),
    ("rope", 2, false),
    ("gem", 3, false),
    ("key", 1, false),
];

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::Serif))
        .insert("System Mono", FontFaces::new(FontSource::Monospace))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(asset_server.load("grid/style.css")));

    let languages: Vec<Handle<BundleAsset>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("grid/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    let items: Vec<serde_json::Value> = ITEMS
        .iter()
        .map(|&(id, count, featured)| serde_json::json!({ "id": id, "count": count, "featured": featured }))
        .collect();
    commands.spawn((
        HtmlUi::new(asset_server.load("grid/grid.html")),
        TemplateContext::new()
            .with("layout", "wide")
            .with("items", &items)
            .with("weight", "18.5 kg")
            .with("gold", &240),
        // The `HtmlUi` node is the app's: fill the window. `.page` grows to
        // fill it (`flex-grow: 1` in the stylesheet).
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            ..default()
        },
    ));
}

/// Space: swap the page's track template. The context change re-renders;
/// the template puts `layout` into `.page`'s classes.
fn toggle_layout(keys: Res<ButtonInput<KeyCode>>, mut contexts: Query<&mut TemplateContext>) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    for mut context in &mut contexts {
        let wide = context.get("layout").and_then(|v| v.as_str()) == Some("wide");
        context.insert("layout", if wide { "narrow" } else { "wide" });
    }
}

/// L: next language.
fn switch_language(
    keys: Res<ButtonInput<KeyCode>>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    if !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
