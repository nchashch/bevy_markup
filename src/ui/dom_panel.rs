use bevy::prelude::*;
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};
use serde::Serialize;

use super::NineSliceFrame;
use crate::assets::html::{HtmlView, RenderedHtml};
use crate::assets::l10n::LocalizedText;
use crate::consts::{FRAME_PATH, HEADER_COLOR, MONO_FONT_PATH};

/// Debug panels sit along the bottom edge; capping their height keeps the top
/// of the screen free for the other panels.
const PANEL_MAX_HEIGHT_VH: f32 = 45.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_MIN_THUMB: f32 = 24.0;
const SCROLLBAR_TRACK: Color = Color::srgb_u8(40, 40, 46);

const PLAIN_PATH: &str = "ui/content/test.html";
const TEMPLATE_PATH: &str = "ui/content/inventory.html";
pub(super) const L10N_PATH: &str = "ui/content/l10n.html";

/// Bevy UI panel showing the outline of its [`HtmlView`]'s rendered DOM.
#[derive(Component)]
pub struct DomPanel;

/// The panel's text node.
#[derive(Component)]
pub(super) struct DomPanelText;

#[derive(Serialize)]
struct Item {
    name: &'static str,
    count: u32,
}

pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Plain HTML: no Tera syntax, so it renders to itself.
    spawn_panel(
        &mut commands,
        &asset_server,
        PLAIN_PATH,
        tera::Context::new(),
        16.0,
    );

    let context = demo_context();
    spawn_panel(&mut commands, &asset_server, TEMPLATE_PATH, context.clone(), 452.0);

    // Tera fills `data-l10n-args`; Fluent resolves `data-l10n-id`.
    spawn_panel(&mut commands, &asset_server, L10N_PATH, context, 888.0);
}

/// Tera variables used by the demo templates.
pub(super) fn demo_context() -> tera::Context {
    let mut context = tera::Context::new();
    context.insert("player", "ada <the brave>");
    context.insert("hp", &7);
    context.insert("max_hp", &10);
    context.insert(
        "items",
        &[
            Item { name: "torch", count: 3 },
            Item { name: "rope", count: 1 },
            Item { name: "key", count: 0 },
        ],
    );
    context
}

fn spawn_panel(
    commands: &mut Commands,
    asset_server: &AssetServer,
    path: &'static str,
    context: tera::Context,
    right: f32,
) {
    let text_font = TextFont::default()
        .with_font(asset_server.load(MONO_FONT_PATH))
        .with_font_size(16.0);
    commands
        .spawn((
            DomPanel,
            HtmlView {
                template: asset_server.load(path),
                context,
            },
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(right),
                bottom: Val::Px(16.0),
                width: Val::Px(420.0),
                max_height: Val::Vh(PANEL_MAX_HEIGHT_VH),
                padding: UiRect::all(Val::Px(28.0)),
                column_gap: Val::Px(8.0),
                ..default()
            },
        ))
        .with_children(|panel| {
            // The viewport scrolls; the scrollbar sits beside it so it doesn't
            // scroll away with the content.
            let viewport = panel
                .spawn((
                    ScrollArea,
                    Node {
                        flex_grow: 1.0,
                        // Let the row's height cap the viewport instead of
                        // its content sizing it.
                        min_height: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    children![(
                        DomPanelText,
                        Text::new(format!("loading {path}…")),
                        text_font,
                        TextColor(Color::srgb_u8(225, 225, 225)),
                        // Keep full height so the viewport scrolls instead of
                        // squashing it.
                        Node {
                            flex_shrink: 0.0,
                            ..default()
                        },
                    )],
                ))
                .id();
            panel.spawn((
                Scrollbar::new(viewport, ControlOrientation::Vertical, SCROLLBAR_MIN_THUMB),
                Node {
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
        });
}

/// Hides a scrollbar while its viewport's content fits.
pub(super) fn toggle_scrollbars(
    mut scrollbars: Query<(&Scrollbar, &mut Visibility)>,
    viewports: Query<&ComputedNode>,
) {
    for (scrollbar, mut visibility) in &mut scrollbars {
        let Ok(viewport) = viewports.get(scrollbar.target) else {
            continue;
        };
        let overflows = viewport.content_size().y > viewport.size().y + 0.5;
        let wanted = if overflows {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
}

/// Rewrites the panel text whenever its view re-renders or re-localizes.
pub(super) fn show_outline(
    panels: Query<
        (Entity, &HtmlView, &RenderedHtml, &LocalizedText),
        (
            With<DomPanel>,
            Or<(Changed<RenderedHtml>, Changed<LocalizedText>)>,
        ),
    >,
    children: Query<&Children>,
    mut texts: Query<&mut Text, With<DomPanelText>>,
) {
    for (panel, view, rendered, localized) in &panels {
        let path = view.template.path().map(ToString::to_string).unwrap_or_default();
        let content = match rendered {
            RenderedHtml::Pending => continue,
            RenderedHtml::Ready(document) => {
                let outline = document.outline(localized);
                info!("DOM of {path}:\n{outline}");
                outline.trim_end().to_owned()
            }
            RenderedHtml::Failed(message) => format!("failed to render {path}:\n{message}"),
        };

        let mut texts = texts.iter_many_mut(children.iter_descendants(panel));
        while let Some(mut text) = texts.fetch_next() {
            content.clone_into(&mut text.0);
        }
    }
}
