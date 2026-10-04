use bevy::prelude::*;

mod dom_panel;
mod html_ui;
mod nine_slice;
mod panel;

pub use nine_slice::NineSliceFrame;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(nine_slice::NineSlicePlugin);
        app.add_systems(Startup, (panel::spawn, dom_panel::spawn, html_ui::spawn));
        app.add_systems(
            Update,
            (
                dom_panel::show_outline,
                dom_panel::toggle_scrollbars,
                html_ui::build_html_ui,
            ),
        );
    }
}
