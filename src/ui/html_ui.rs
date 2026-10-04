//! Renders an [`HtmlView`]'s DOM (after Tera and Fluent) as Bevy UI.
//!
//! Put [`HtmlUi`] next to an `HtmlView` on a UI node; its children are rebuilt
//! whenever the view re-renders or re-localizes. Supported, deliberately
//! minimal:
//!
//! - `h1`–`h6`: header font, sizes descending from `h1`
//! - `p`: body font
//! - `li`: body font with a bullet
//! - loose text directly inside a container: body font
//!
//! Any other element is walked through for its children; `head`, `script`, and
//! `style` are skipped. A block's text is its Fluent translation when it has a
//! `data-l10n-id` that resolved, otherwise its own text (inline tags such as
//! `<b>` contribute their text, unstyled). Whitespace collapses as in HTML.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::NineSliceFrame;
use super::dom_panel::{L10N_PATH, demo_context};
use crate::assets::html::{HtmlView, RenderedHtml, decode_entities};
use crate::assets::l10n::LocalizedText;
use crate::consts::{BODY_COLOR, BODY_FONT_PATH, FRAME_PATH, HEADER_COLOR, HEADER_FONT_PATH};

const BODY_SIZE: f32 = 20.0;

/// Renders this entity's [`HtmlView`] as its Bevy UI children.
#[derive(Component)]
#[require(Node)]
pub struct HtmlUi;

enum Block {
    Heading { level: u8, text: String },
    Paragraph(String),
    ListItem(String),
}

pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        HtmlUi,
        HtmlView {
            template: asset_server.load(L10N_PATH),
            context: demo_context(),
        },
        NineSliceFrame(asset_server.load(FRAME_PATH)),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(232.0),
            width: Val::Px(420.0),
            padding: UiRect::all(Val::Px(28.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..default()
        },
    ));
}

pub(super) fn build_html_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    views: Query<
        (Entity, &RenderedHtml, &LocalizedText),
        (
            With<HtmlUi>,
            Or<(Changed<RenderedHtml>, Changed<LocalizedText>)>,
        ),
    >,
) {
    for (entity, rendered, localized) in &views {
        let blocks = match rendered {
            RenderedHtml::Pending => continue,
            RenderedHtml::Ready(document) => collect_blocks(document.dom(), &localized.0),
            RenderedHtml::Failed(message) => {
                vec![Block::Paragraph(format!("failed to render: {message}"))]
            }
        };

        let header_font: Handle<Font> = asset_server.load(HEADER_FONT_PATH);
        let body_font = TextFont::default()
            .with_font(asset_server.load(BODY_FONT_PATH))
            .with_font_size(BODY_SIZE);

        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for block in blocks {
                    match block {
                        Block::Heading { level, text } => {
                            parent.spawn((
                                Text::new(text),
                                TextFont::default()
                                    .with_font(header_font.clone())
                                    .with_font_size(heading_size(level)),
                                TextColor(HEADER_COLOR),
                            ));
                        }
                        Block::Paragraph(text) => {
                            parent.spawn((
                                Text::new(text),
                                body_font.clone(),
                                TextColor(BODY_COLOR),
                            ));
                        }
                        Block::ListItem(text) => {
                            parent.spawn((
                                Text::new(format!("• {text}")),
                                body_font.clone(),
                                TextColor(BODY_COLOR),
                                Node {
                                    margin: UiRect::left(Val::Px(12.0)),
                                    ..default()
                                },
                            ));
                        }
                    }
                }
            });
    }
}

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 28.0,
        2 => 24.0,
        3 => 22.0,
        _ => BODY_SIZE,
    }
}

fn collect_blocks(
    dom: &tl::VDom,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
) -> Vec<Block> {
    let parser = dom.parser();
    let mut blocks = Vec::new();
    for handle in dom.children() {
        collect_node(parser, localized, *handle, &mut blocks);
    }
    blocks
}

fn collect_node(
    parser: &tl::Parser,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    handle: tl::NodeHandle,
    blocks: &mut Vec<Block>,
) {
    let Some(node) = handle.get(parser) else {
        return;
    };
    let tag = match node {
        tl::Node::Tag(tag) => tag,
        tl::Node::Raw(_) => {
            let text = collapse_whitespace(&inline_text(parser, handle));
            if !text.is_empty() {
                blocks.push(Block::Paragraph(text));
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let name = tag.name().as_utf8_str().to_ascii_lowercase();
    let block_text = || match localized.get(&handle) {
        Some(Ok(text)) => text.clone(),
        // Missing translation: fall back to the element's own content.
        _ => collapse_whitespace(&inline_text(parser, handle)),
    };
    match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => blocks.push(Block::Heading {
            level: name.as_bytes()[1] - b'0',
            text: block_text(),
        }),
        "p" => blocks.push(Block::Paragraph(block_text())),
        "li" => blocks.push(Block::ListItem(block_text())),
        "head" | "script" | "style" => {}
        _ => {
            for child in tag.children().top().iter() {
                collect_node(parser, localized, *child, blocks);
            }
        }
    }
}

/// Concatenated text of `handle` and its descendants, entities decoded.
fn inline_text(parser: &tl::Parser, handle: tl::NodeHandle) -> String {
    let mut out = String::new();
    push_inline_text(parser, handle, &mut out);
    out
}

fn push_inline_text(parser: &tl::Parser, handle: tl::NodeHandle, out: &mut String) {
    match handle.get(parser) {
        Some(tl::Node::Raw(text)) => out.push_str(&decode_entities(&text.as_utf8_str())),
        Some(tl::Node::Tag(tag)) => {
            for child in tag.children().top().iter() {
                push_inline_text(parser, *child, out);
            }
        }
        Some(tl::Node::Comment(_)) | None => {}
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
