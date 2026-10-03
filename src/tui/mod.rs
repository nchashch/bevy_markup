use bevy::prelude::*;
use bevy_tui_texture::prelude::*;

mod panel;

pub use panel::TuiPanel;

use crate::consts::FONT_PATH;

const FONT_SIZE_PX: u32 = 48;

const PANEL_COLS: u16 = 60;
const PANEL_ROWS: u16 = 20;

pub struct TuiPlugin;

impl Plugin for TuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TerminalPlugin::default());
        app.add_systems(Startup, spawn_panel);
        app.add_systems(Update, panel::draw.in_set(TerminalSystemSet::Render));
    }
}

/// Spawns the panel as a bevy_ui node. The `TuiRequest` stays pending until the
/// font asset loads, then the plugin replaces it with a `Tui` component.
fn spawn_panel(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        TuiRequest::ui(
            PANEL_COLS,
            PANEL_ROWS,
            TuiFontSource::Asset {
                handle: asset_server.load(FONT_PATH),
                size_px: FONT_SIZE_PX,
            },
        ),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(16.0),
            top: Val::Px(16.0),
            ..default()
        },
        TuiPanel,
    ));
}
