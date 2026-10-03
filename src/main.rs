use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, P23SystemPlugins)).finish();
    app.run();
}

struct P23SystemPlugins;

impl Plugin for P23SystemPlugins {
    fn build(&self, app: &mut App) {}
}
