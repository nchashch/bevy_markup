use bevy::prelude::*;

mod consts;
mod system;
mod tui;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(crate::system::SystemPlugins)
        .run();
}
