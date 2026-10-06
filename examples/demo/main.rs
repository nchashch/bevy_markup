//! Every feature at once: the whole demo app — panels, buttons, frames and
//! their layout — is one HTML document styled with CSS. The Rust side only
//! wires behaviour onto the HTML: clicks, scrolling, language and theme
//! selection, and the content documents spawned into the shell's slots.
//!
//! Keyboard and gamepad (`examples/shared/input.rs` in `ArrowMode::Linear`):
//! the option buttons are `data-on-click` (focusable), the scroll panels
//! `tabindex="0"`. Left / right (arrows, D-pad, left stick) step focus
//! through panels and buttons in document order; up / down scroll the
//! focused panel, or the middle column when a button there is focused
//! ([`shell::scroll_focused`]; also the right stick and PageUp / PageDown);
//! Enter / A press; L / Y and T / X step the language and the theme
//! ([`controls::hotkeys`]).
//!
//! `cargo run --example demo`

#[path = "../shared/input.rs"]
mod input;

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
            input::ExampleInputPlugin,
        ))
        // Left / right move focus, up / down scroll (`shell::scroll_focused`).
        .insert_resource(input::ArrowMode::Linear)
        .insert_resource(ClearColor(Color::srgb_u8(20, 24, 32)))
        .add_systems(Startup, (setup, controls::spawn, shell::spawn))
        .define_html_element("content-slot", shell::fill_content_slot)
        .define_html_element("scroll-column", shell::scroll_column)
        .define_html_element("scrollbar", shell::scrollbar)
        .init_resource::<shell::DraggedThumb>()
        .add_observer(shell::drag_thumb)
        .add_observer(shell::start_thumb_drag)
        .add_observer(shell::end_thumb_drag)
        .add_observer(shell::press_track)
        .add_systems(
            Update,
            (
                controls::read_signals,
                controls::hotkeys,
                shell::scroll_focused,
                shell::scroll_focus_into_view,
                shell::update_scrollbars,
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
