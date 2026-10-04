//! Scrollable panel building blocks.
//!
//! Layout: a row node with a bounded height (e.g. `max_height`) holding
//! `[viewport, scrollbar]`. The viewport ([`viewport_node`] + `ScrollArea`)
//! scrolls its children; the scrollbar ([`spawn_scrollbar`]) is its sibling so
//! it doesn't scroll away with the content. [`toggle_scrollbars`] removes a
//! scrollbar from layout while its viewport's content fits, so the content gets
//! the full width.
//!
//! Children of a viewport need `flex_shrink: 0.0`, or the column squashes them
//! instead of overflowing.

use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, Scrollbar, ScrollbarThumb};

use crate::consts::HEADER_COLOR;

const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_MIN_THUMB: f32 = 24.0;
const SCROLLBAR_TRACK: Color = Color::srgb_u8(40, 40, 46);

/// Gap between a viewport and its scrollbar; set as the panel's `column_gap`.
pub(super) const SCROLLBAR_GAP: Val = Val::Px(8.0);

/// Node for a vertically scrolling viewport inside a row panel. Pair with
/// `bevy::ui_widgets::ScrollArea` for wheel/trackpad input.
pub(super) fn viewport_node() -> Node {
    Node {
        flex_grow: 1.0,
        // Let the panel's height cap the viewport instead of its content
        // sizing it.
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

/// Spawns a vertical scrollbar for `viewport`. Starts out of layout;
/// [`toggle_scrollbars`] shows it once the content overflows.
pub(super) fn spawn_scrollbar(parent: &mut ChildSpawnerCommands, viewport: Entity) {
    parent.spawn((
        Scrollbar::new(viewport, ControlOrientation::Vertical, SCROLLBAR_MIN_THUMB),
        Node {
            display: Display::None,
            width: Val::Px(SCROLLBAR_WIDTH),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(SCROLLBAR_WIDTH / 2.0)),
            ..default()
        },
        BackgroundColor(SCROLLBAR_TRACK),
        children![(
            ScrollbarThumb {
                border_radius: BorderRadius::all(Val::Px(SCROLLBAR_WIDTH / 2.0)),
                ..default()
            },
            BackgroundColor(HEADER_COLOR),
        )],
    ));
}

/// Puts a scrollbar in layout only while its viewport's content overflows.
/// Stable: showing the bar narrows the viewport, which can only make the
/// content taller, so it never flips back on its own.
pub(super) fn toggle_scrollbars(
    mut scrollbars: Query<(&Scrollbar, &mut Node)>,
    viewports: Query<&ComputedNode>,
) {
    for (scrollbar, mut node) in &mut scrollbars {
        let Ok(viewport) = viewports.get(scrollbar.target) else {
            continue;
        };
        let overflows = viewport.content_size().y > viewport.size().y + 0.5;
        let display = if overflows {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}
