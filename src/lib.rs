//! p23: HTML (Tera + Fluent + CSS) rendered as Bevy UI, 9-slice framed panels,
//! and ratatui panels via bevy_tui_texture.
//!
//! Add [`P23Plugin`] after `DefaultPlugins`. The app provides the camera (any
//! UI camera, e.g. `Camera2d`); see `examples/demo.rs`.

use bevy::prelude::*;

mod assets;
mod consts;
mod tui;
mod ui;

/// Everything in the crate: asset loaders (`.css`, `.html`, Fluent locales),
/// the TUI panel, and the UI panels (HTML rendering, debug outlines, language
/// and theme selectors).
pub struct P23Plugin;

impl Plugin for P23Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((assets::AssetsPlugin, tui::TuiPlugin, ui::UiPlugin));
    }
}
