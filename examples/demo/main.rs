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
        .add_plugins((DefaultPlugins, BevyMarkupPlugin))
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

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    for (name, [regular, bold, italic, bold_italic]) in consts::FONT_FAMILIES {
        fonts.insert(
            *name,
            FontFaces::new(asset_server.load(*regular))
                .with_bold(asset_server.load(*bold))
                .with_italic(asset_server.load(*italic))
                .with_bold_italic(asset_server.load(*bold_italic)),
        );
    }
    fonts
        .set_generic(GenericFamily::Serif, "Spectral")
        .set_generic(GenericFamily::Monospace, "Iosevka Slab Mono");
}
