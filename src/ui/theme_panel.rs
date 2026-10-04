//! Theme selector: one toggle per CSS file in [`THEMES`]. Choosing one swaps
//! the stylesheet of every [`HtmlStylesheet`]; `HtmlUi` rebuilds.

use bevy::prelude::*;

use super::html_ui::HtmlStylesheet;
use super::selector::{Selector, spawn_selector_panel};
use crate::assets::css::CssStyleSheet;

/// (label, stylesheet path). The first is the default theme.
pub(super) const THEMES: &[(&str, &str)] = &[
    ("Crimson", "ui/themes/crimson.css"),
    ("Parchment", "ui/themes/parchment.css"),
    ("Terminal", "ui/themes/terminal.css"),
    ("Large print", "ui/themes/large_print.css"),
];

/// Every theme's stylesheet, loaded at startup so switching is immediate.
#[derive(Resource)]
pub(super) struct Themes(Vec<Handle<CssStyleSheet>>);

#[derive(Component)]
pub(super) struct ThemeSelector;

pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(Themes(
        THEMES
            .iter()
            .map(|(_, path)| asset_server.load(*path))
            .collect(),
    ));
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

pub(super) fn apply_theme_selection(
    selectors: Query<&Selector, (With<ThemeSelector>, Changed<Selector>)>,
    themes: Res<Themes>,
    mut stylesheets: Query<&mut HtmlStylesheet>,
) {
    for selector in &selectors {
        let sheet = &themes.0[selector.active];
        for mut stylesheet in &mut stylesheets {
            if stylesheet.0 != *sheet {
                stylesheet.0 = sheet.clone();
            }
        }
        info!("theme -> {}", THEMES[selector.active].0);
    }
}
