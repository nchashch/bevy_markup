use bevy::prelude::*;

mod assets;
mod consts;
mod system;
mod tui;
mod ui;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(crate::system::SystemPlugins)
        .run();
}
