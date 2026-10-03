use bevy::prelude::*;

pub struct SystemPlugins;

impl Plugin for SystemPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::tui::TuiPlugin);
        app.add_plugins(crate::ui::UiPlugin);
        app.insert_resource(ClearColor(crate::consts::CLEAR_COLOR));
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}
