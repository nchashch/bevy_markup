//! Framed panel with a title and a row of toggle buttons, exactly one active.
//!
//! [`spawn_selector_panel`] puts a [`Selector`] (plus a caller marker) on the
//! button row. Clicking a button sets `Selector::active`; callers react with a
//! system on `Changed<Selector>` filtered by their marker. It also changes
//! once when spawned, so that system applies the initial choice too.

use bevy::prelude::*;

use p23::prelude::NineSliceFrame;
use crate::consts::{BODY_COLOR, BODY_FONT_PATH, FRAME_PATH, HEADER_COLOR, HEADER_FONT_PATH};

const BUTTON_IDLE: Color = Color::srgb_u8(40, 40, 46);
const BUTTON_HOVER: Color = Color::srgb_u8(64, 64, 72);
const ACTIVE_TEXT: Color = Color::WHITE;

/// The active option's index. On the button row.
#[derive(Component, PartialEq)]
pub(super) struct Selector {
    pub active: usize,
}

/// A toggle button for option `index` of its parent [`Selector`].
#[derive(Component)]
pub(super) struct SelectorOption(usize);

/// Spawns the panel at `position` (a `Node` with placement fields set; the
/// panel styling is added here). `marker` goes on the [`Selector`] row.
pub(super) fn spawn_selector_panel<'a>(
    commands: &mut Commands,
    asset_server: &AssetServer,
    title: &str,
    options: impl IntoIterator<Item = &'a str>,
    active: usize,
    position: Node,
    marker: impl Bundle,
) {
    let label_font = TextFont::default()
        .with_font(asset_server.load(BODY_FONT_PATH))
        .with_font_size(18.0);

    commands
        .spawn((
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                width: Val::Px(420.0),
                padding: UiRect::all(Val::Px(28.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(12.0),
                ..position
            },
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(title),
                TextFont::default()
                    .with_font(asset_server.load(HEADER_FONT_PATH))
                    .with_font_size(24.0),
                TextColor(HEADER_COLOR),
            ));
            panel
                .spawn((
                    Selector { active },
                    marker,
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(8.0),
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    for (index, label) in options.into_iter().enumerate() {
                        row.spawn((
                            Button,
                            SelectorOption(index),
                            Node {
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(BUTTON_IDLE),
                            children![(
                                Text::new(label),
                                label_font.clone(),
                                TextColor(BODY_COLOR),
                                // Clicks on the label count as clicks on the button.
                                Pickable::IGNORE,
                            )],
                        ))
                        .observe(select_option);
                    }
                });
        });
}

fn select_option(
    click: On<Pointer<Click>>,
    options: Query<(&SelectorOption, &ChildOf)>,
    mut selectors: Query<&mut Selector>,
) {
    let Ok((SelectorOption(index), child_of)) = options.get(click.entity) else {
        return;
    };
    if let Ok(mut selector) = selectors.get_mut(child_of.parent()) {
        selector.set_if_neq(Selector { active: *index });
    }
}

/// Active option: red button, white label. Others: dark, lighter on hover.
pub(super) fn style_selector_buttons(
    selectors: Query<Ref<Selector>>,
    mut buttons: Query<(
        Ref<Interaction>,
        &SelectorOption,
        &ChildOf,
        &mut BackgroundColor,
        &Children,
    )>,
    mut labels: Query<&mut TextColor>,
) {
    for (interaction, SelectorOption(index), child_of, mut background, children) in &mut buttons {
        let Ok(selector) = selectors.get(child_of.parent()) else {
            continue;
        };
        if !selector.is_changed() && !interaction.is_changed() {
            continue;
        }
        let (bg, fg) = if selector.active == *index {
            (HEADER_COLOR, ACTIVE_TEXT)
        } else if *interaction != Interaction::None {
            (BUTTON_HOVER, BODY_COLOR)
        } else {
            (BUTTON_IDLE, BODY_COLOR)
        };
        background.0 = bg;
        let mut labels = labels.iter_many_mut(children);
        while let Some(mut label) = labels.fetch_next() {
            label.0 = fg;
        }
    }
}
