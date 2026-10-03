use bevy::prelude::*;

mod nine_slice;
mod panel;

pub use nine_slice::NineSliceFrame;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(nine_slice::NineSlicePlugin);
        app.add_systems(Startup, panel::spawn);
    }
}
