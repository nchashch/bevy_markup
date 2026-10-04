//! p23: HTML (Tera + Fluent + CSS) rendered as Bevy UI, with 9-slice framed
//! panels.
//!
//! Add [`P23Plugin`] after `DefaultPlugins`. The app provides the camera (any
//! UI camera, e.g. `Camera2d`); see `examples/demo.rs`.

use bevy::prelude::*;

mod assets;
mod consts;
mod ui;

/// Everything in the crate: asset loaders (`.css`, `.html`, Fluent locales)
/// and the UI panels (HTML rendering, debug outlines, language and theme
/// selectors).
pub struct P23Plugin;

impl Plugin for P23Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((assets::AssetsPlugin, ui::UiPlugin));
    }
}
