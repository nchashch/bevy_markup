use bevy::prelude::*;

use super::NineSliceFrame;
use crate::consts::{BODY_COLOR, BODY_FONT_PATH, FRAME_PATH, HEADER_COLOR, HEADER_FONT_PATH};

/// Marker for the 9-slice framed Bevy UI panel.
#[derive(Component)]
pub struct UiPanel;

pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    let header_font = TextFont::default()
        .with_font(asset_server.load(HEADER_FONT_PATH))
        .with_font_size(28.0);
    let body_font = TextFont::default()
        .with_font(asset_server.load(BODY_FONT_PATH))
        .with_font_size(20.0);

    commands.spawn((
        UiPanel,
        NineSliceFrame(asset_server.load(FRAME_PATH)),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(16.0),
            width: Val::Px(420.0),
            padding: UiRect::all(Val::Px(28.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..default()
        },
        children![
            (
                Text::new("Bevy UI"),
                header_font,
                TextColor(HEADER_COLOR),
            ),
            (
                Text::new(
                    "This panel is plain Bevy UI. Its background is ui/frame.png, \
                     9-sliced so the corners keep their size as the panel grows.",
                ),
                body_font,
                TextColor(BODY_COLOR),
            ),
        ],
    ));
}
