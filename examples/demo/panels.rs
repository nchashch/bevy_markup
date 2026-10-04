//! The demo's panels: a plain Bevy UI panel, the rendered HTML panel, and
//! three debug panels showing DOM outlines (`HtmlDebugOutline`).

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use p23::prelude::*;
use serde::Serialize;

use crate::consts::{BODY_COLOR, BODY_FONT_PATH, FRAME_PATH, HEADER_COLOR, HEADER_FONT_PATH};
use crate::scroll::{SCROLLBAR_GAP, spawn_scrollbar, viewport_node};

const PLAIN_PATH: &str = "ui/content/test.html";
const TEMPLATE_PATH: &str = "ui/content/inventory.html";
const L10N_PATH: &str = "ui/content/l10n.html";

#[derive(Serialize)]
struct Item {
    name: &'static str,
    count: u32,
}

/// Tera variables used by the demo templates.
fn demo_context() -> TemplateContext {
    TemplateContext::new()
        .with("player", "ada <the brave>")
        .with("hp", &7)
        .with("max_hp", &10)
        .with(
            "items",
            &[
                Item { name: "torch", count: 3 },
                Item { name: "rope", count: 1 },
                Item { name: "key", count: 0 },
            ],
        )
}

pub fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    spawn_plain_panel(&mut commands, &asset_server);

    // The rendered HTML (Tera + Fluent + CSS), top-anchored and scrollable.
    spawn_scroll_panel(
        &mut commands,
        &asset_server,
        Node {
            right: Val::Px(452.0),
            top: Val::Px(16.0),
            max_height: Val::Vh(50.0),
            ..default()
        },
        (
            HtmlUi::new(asset_server.load(L10N_PATH)),
            demo_context(),
        ),
        Val::Px(10.0),
    );

    // DOM outlines along the bottom: plain HTML, Tera, Tera + Fluent.
    for (path, right) in [(PLAIN_PATH, 16.0), (TEMPLATE_PATH, 452.0), (L10N_PATH, 888.0)] {
        spawn_scroll_panel(
            &mut commands,
            &asset_server,
            Node {
                right: Val::Px(right),
                bottom: Val::Px(16.0),
                max_height: Val::Vh(45.0),
                ..default()
            },
            (
                HtmlUi::new(asset_server.load(path)),
                demo_context(),
                HtmlDebugOutline,
            ),
            Val::Px(0.0),
        );
    }
}

/// A framed, absolutely positioned panel (`placement` sets the position and
/// height cap) whose scrolling viewport carries `content` (an `HtmlUi`).
fn spawn_scroll_panel(
    commands: &mut Commands,
    asset_server: &AssetServer,
    placement: Node,
    content: impl Bundle,
    row_gap: Val,
) {
    commands
        .spawn((
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(420.0),
                padding: UiRect::all(Val::Px(28.0)),
                column_gap: SCROLLBAR_GAP,
                ..placement
            },
        ))
        .with_children(|panel| {
            let viewport = panel
                .spawn((
                    content,
                    ScrollArea,
                    Node {
                        row_gap,
                        ..viewport_node()
                    },
                ))
                .id();
            spawn_scrollbar(panel, viewport);
        });
}

/// Plain Bevy UI for comparison.
fn spawn_plain_panel(commands: &mut Commands, asset_server: &AssetServer) {
    commands.spawn((
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
                TextFont::default()
                    .with_font(asset_server.load(HEADER_FONT_PATH))
                    .with_font_size(28.0),
                TextColor(HEADER_COLOR),
            ),
            (
                Text::new(
                    "This panel is plain Bevy UI. Its background is ui/frame.png, \
                     9-sliced so the corners keep their size as the panel grows.",
                ),
                TextFont::default()
                    .with_font(asset_server.load(BODY_FONT_PATH))
                    .with_font_size(20.0),
                TextColor(BODY_COLOR),
            ),
        ],
    ));
}
