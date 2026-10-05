//! Every feature at once: HTML + Tera + Fluent + CSS panels, DOM outlines,
//! runtime language and theme switching, 9-slice frames, scrolling.
//!
//! `cargo run --example demo`

mod consts;
mod locale_panel;
mod panels;
mod scroll;
mod selector;
mod theme_panel;

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
        .insert_resource(ClearColor(Color::srgb_u8(20, 24, 32)))
        .add_systems(
            Startup,
            (
                setup,
                panels::spawn,
                locale_panel::spawn,
                theme_panel::spawn,
            ),
        )
        .add_systems(
            Update,
            (
                scroll::toggle_scrollbars,
                selector::style_selector_buttons,
                locale_panel::apply_locale_selection,
                theme_panel::apply_theme_selection,
            ),
        )

        .run();
}

fn setup(mut commands: Commands, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // System font families for the CSS generic keywords the themes use. One
    // source per family: bold and italic are picked by the system.
    fonts
        .insert("System Sans", FontFaces::new(consts::HEADER_FONT))
        .insert("System Serif", FontFaces::new(consts::BODY_FONT))
        .insert("System Mono", FontFaces::new(consts::MONO_FONT))
        .set_generic(GenericFamily::SansSerif, "System Sans")
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");
}
