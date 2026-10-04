use bevy::prelude::*;

mod dom_panel;
mod html_style;
mod html_ui;
mod locale_panel;
mod nine_slice;
mod panel;
mod scroll;

pub use nine_slice::NineSliceFrame;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(nine_slice::NineSlicePlugin);
        app.add_systems(Startup, (panel::spawn, dom_panel::spawn, html_ui::spawn));
        // Needs `Locales`, inserted by `L10nPlugin` during Startup.
        app.add_systems(PostStartup, locale_panel::spawn);
        app.add_systems(
            Update,
            (
                dom_panel::show_outline,
                scroll::toggle_scrollbars,
                html_ui::build_html_ui,
                locale_panel::style_locale_buttons,
            ),
        );
    }
}
