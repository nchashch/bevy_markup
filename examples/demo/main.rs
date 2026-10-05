//! Every feature at once: the whole demo app — panels, buttons, frames and
//! their layout — is one HTML document styled with CSS. The Rust side only
//! wires behaviour onto the HTML: clicks, scrolling, language and theme
//! selection, and the content documents spawned into the shell's slots.
//!
//! `cargo run --example demo`

mod consts;
mod controls;
mod shell;

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
                // Wide enough for the shell's 420px panels (3 across the
                // bottom, two columns on top).
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: bevy::window::WindowResolution::new(1600, 900),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
        ))
        .insert_resource(ClearColor(Color::srgb_u8(20, 24, 32)))
        .add_systems(Startup, (setup, controls::spawn, shell::spawn))
        .add_observer(shell::wire_shell_build)
        .add_systems(
            Update,
            (
                controls::read_signals,
                controls::apply_locale_selection,
                controls::apply_theme_selection,
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
