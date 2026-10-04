//! Runs every p23 panel: `cargo run --example demo`.

use bevy::prelude::*;
use p23::P23Plugin;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, P23Plugin))
        .insert_resource(ClearColor(Color::srgb_u8(20, 24, 32)))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .run();
}
