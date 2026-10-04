use bevy::prelude::*;
use bevy_tui_texture::prelude::*;

mod panel;

pub use panel::TuiPanel;

use crate::consts::{FRAME_PATH, MONO_FONT_PATH};
use crate::ui::NineSliceFrame;

const FONT_SIZE_PX: u32 = 64;

const PANEL_COLS: u16 = 60;
const PANEL_ROWS: u16 = 20;

pub struct TuiPlugin;

impl Plugin for TuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TerminalPlugin::default());
        app.add_systems(Startup, spawn_panel);
        app.add_systems(Update, panel::draw.in_set(TerminalSystemSet::Render));
        app.add_systems(Update, fit_panel_aspect);
    }
}

fn fit_panel_aspect(
    terms: Query<(&Tui, &ChildOf), (With<TuiPanel>, Added<Tui>)>,
    mut nodes: Query<&mut Node>,
) {
    for (tui, child_of) in &terms {
        let px = tui.size_px();
        // Frame (parent) gets the ratio; the terminal fills it.
        if let Ok(mut frame) = nodes.get_mut(child_of.parent()) {
            frame.aspect_ratio = Some(px.x as f32 / px.y as f32);
        }
    }
}

/// Spawns the panel: a 9-slice framed node holding the terminal node. The
/// terminal draws into its own `ImageNode`, so the frame lives on the parent.
/// The `TuiRequest` stays pending until the font asset loads, then the plugin
/// replaces it with a `Tui` component.
fn spawn_panel(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        // Full-window layout root so the frame's `Percent` height resolves
        // against the window. Invisible, so it must not take pointer input:
        // UI nodes are pickable and block lower nodes by default.
        Pickable::IGNORE,
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..default()
        },
        children![(
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(16.0),
                height: Val::Percent(80.),
                aspect_ratio: Some(1.0),
                padding: UiRect::all(Val::Px(20.0)),
                ..default()
            },
            children![(
                TuiRequest::ui(
                    PANEL_COLS,
                    PANEL_ROWS,
                    TuiFontSource::Asset {
                        handle: asset_server.load(MONO_FONT_PATH),
                        size_px: FONT_SIZE_PX,
                    },
                ),
                Node::default(),
                TuiPanel,
            )],
        )],
    ));
}
