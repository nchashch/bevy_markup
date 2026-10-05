//! Theme selector: one toggle per CSS file in [`THEMES`]; the choice becomes
//! the [`DefaultStylesheet`], restyling every `HtmlUi` without its own sheet.

use bevy::prelude::*;
use bevy_markup::prelude::*;

use crate::selector::{Selector, spawn_selector_panel};

/// (label, stylesheet path). The first is the default theme.
const THEMES: &[(&str, &str)] = &[
    ("Crimson", "ui/themes/crimson.css"),
    ("Parchment", "ui/themes/parchment.css"),
    ("Terminal", "ui/themes/terminal.css"),
    ("Large print", "ui/themes/large_print.css"),
];

/// Every theme's stylesheet, preloaded so switching is immediate.
#[derive(Resource)]
pub struct Themes(Vec<Handle<Stylesheet>>);

#[derive(Component)]
pub struct ThemeSelector;

pub fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    let sheets: Vec<Handle<Stylesheet>> = THEMES
        .iter()
        .map(|(_, path)| asset_server.load(*path))
        .collect();
    commands.insert_resource(DefaultStylesheet::new(sheets[0].clone()));
    commands.insert_resource(Themes(sheets));
    spawn_selector_panel(
        &mut commands,
        &asset_server,
        "Theme",
        THEMES.iter().map(|(label, _)| *label),
        0,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(420.0),
            ..default()
        },
        ThemeSelector,
    );
}

pub fn apply_theme_selection(
    selectors: Query<&Selector, (With<ThemeSelector>, Changed<Selector>)>,
    themes: Res<Themes>,
    mut default_sheet: ResMut<DefaultStylesheet>,
) {
    for selector in &selectors {
        let sheet = &themes.0[selector.active];
        if default_sheet.0.as_ref() != Some(sheet) {
            default_sheet.0 = Some(sheet.clone());
            info!("theme -> {}", THEMES[selector.active].0);
        }
    }
}
