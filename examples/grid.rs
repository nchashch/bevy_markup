//! CSS grid layout: a page grid of panels whose track template switches at
//! runtime, a responsive `repeat(auto-fill, minmax(…))` slot grid with a
//! spanning item and dense packing, and small grids inside slots and the
//! stats table. All layout is in `grid/style.css`; the app only spawns one
//! `HtmlUi` and toggles context variables.
//!
//! Keyboard and gamepad (`examples/shared/input.rs`): the slots are
//! `tabindex="0"`, so arrows / D-pad / left stick walk the 2D grid —
//! `HtmlFocus` picks the nearest slot in that direction, whatever the
//! current column count or the 2×2 featured slot — and the details panel
//! shows the focused slot. Tabs and the Layout / Language buttons are
//! `data-on-click`: click them, or focus them and press Enter / A.
//!
//! `cargo run --example grid` — Space / X switches between the wide and the
//! narrow layout, L / Y the language (English/German); resize the window to
//! watch the slots reflow.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "shared/input.rs"]
mod input;
use input::{ExampleInputPlugin, Hotkeys};

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
            ExampleInputPlugin,
        ))
        .add_message::<SwitchLanguage>()
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (hotkeys, handle_signals, show_focused_slot, switch_language),
        )
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
            .with("active_tab", "items")
            .with("selected", ITEMS[0].0)
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

/// The Layout / Language buttons and the tabs (clicked, or activated with
/// Enter / A). The template puts `layout` into `.page`'s classes and marks
/// the `active_tab`.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
    mut switch: MessageWriter<SwitchLanguage>,
) {
    for signal in signals.read().filter(|s| s.trigger == SignalTrigger::Click) {
        for mut context in &mut contexts {
            match signal.name.as_ref() {
                "layout" => toggle_layout(&mut context),
                "tab" => {
                    if let Some(tab) = signal.data("tab") {
                        context.insert("active_tab", tab);
                    }
                }
                _ => {}
            }
        }
        if signal.name == "language" {
            switch.write(SwitchLanguage);
        }
    }
}

/// Space / X: layout; L / Y: language.
fn hotkeys(
    hotkeys: Hotkeys,
    mut contexts: Query<&mut TemplateContext>,
    mut switch: MessageWriter<SwitchLanguage>,
) {
    if hotkeys.just_pressed(KeyCode::Space, GamepadButton::West) {
        for mut context in &mut contexts {
            toggle_layout(&mut context);
        }
    }
    if hotkeys.just_pressed(KeyCode::KeyL, GamepadButton::North) {
        switch.write(SwitchLanguage);
    }
}

/// Swaps the page's track template.
fn toggle_layout(context: &mut TemplateContext) {
    let wide = context.get("layout").and_then(|v| v.as_str()) == Some("wide");
    context.insert("layout", if wide { "narrow" } else { "wide" });
}

/// The details panel follows focus: a focused `slot-<id>` becomes `selected`.
fn show_focused_slot(
    focus: Res<InputFocus>,
    elements: Query<&HtmlElement>,
    mut contexts: Query<&mut TemplateContext>,
) {
    let Some(id) = focus
        .get()
        .and_then(|entity| elements.get(entity).ok())
        .and_then(|element| element.id.as_deref()?.strip_prefix("slot-"))
    else {
        return;
    };
    for mut context in &mut contexts {
        if context.get("selected").and_then(|v| v.as_str()) != Some(id) {
            context.insert("selected", id);
        }
    }
}

/// A request for the next language.
#[derive(Message)]
struct SwitchLanguage;

/// Next language, once per request.
fn switch_language(
    mut requests: MessageReader<SwitchLanguage>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    if requests.read().count() == 0 {
        return;
    }
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
