//! Renders an [`HtmlView`]'s DOM (after Tera and Fluent) as Bevy UI.
//!
//! Put [`HtmlUi`] next to an `HtmlView` on a UI node; its children are rebuilt
//! whenever the view re-renders or re-localizes. Supported, deliberately
//! minimal:
//!
//! - `h1`–`h6`: header font, sizes descending from `h1`
//! - `p`: body font
//! - `li`: body font with a bullet
//! - `pre`: monospace block on a dark background; whitespace and line breaks
//!   kept, no wrapping
//! - loose text directly inside a container: body font
//! - inline `b`/`strong` and `i`/`em`: Bold / Italic / BoldItalic face
//! - inline `code`/`kbd`/`samp`/`tt`: monospace family (bold/italic still apply)
//!
//! Any other element is walked through for its children; `head`, `script`, and
//! `style` are skipped. A block's text is its Fluent translation when it has a
//! `data-l10n-id` that resolved (plain, unstyled), otherwise its own content.
//! Outside `pre`, whitespace collapses as in HTML. Each block is one `Text`
//! with a `TextSpan` child per styled run.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use super::NineSliceFrame;
use super::dom_panel::{L10N_PATH, demo_context};
use crate::assets::html::{HtmlView, RenderedHtml, decode_entities};
use crate::assets::l10n::LocalizedText;
use crate::consts::{
    BODY_BOLD_FONT_PATH, BODY_BOLD_ITALIC_FONT_PATH, BODY_COLOR, BODY_FONT_PATH,
    BODY_ITALIC_FONT_PATH, FRAME_PATH, HEADER_BOLD_FONT_PATH, HEADER_BOLD_ITALIC_FONT_PATH,
    HEADER_COLOR, HEADER_FONT_PATH, HEADER_ITALIC_FONT_PATH, MONO_BOLD_FONT_PATH,
    MONO_BOLD_ITALIC_FONT_PATH, MONO_FONT_PATH, MONO_ITALIC_FONT_PATH,
};

const BODY_SIZE: f32 = 20.0;
const PRE_SIZE: f32 = 16.0;
const PRE_BACKGROUND: Color = Color::srgb_u8(28, 28, 34);

/// Renders this entity's [`HtmlView`] as its Bevy UI children.
#[derive(Component)]
#[require(Node)]
pub struct HtmlUi;

#[derive(Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    mono: bool,
}

/// A stretch of text in one style.
struct Run {
    text: String,
    style: Style,
}

enum BlockKind {
    Heading(u8),
    Paragraph,
    ListItem,
    Preformatted,
}

struct Block {
    kind: BlockKind,
    runs: Vec<Run>,
}

/// One typeface in its four faces.
struct Family {
    regular: Handle<Font>,
    bold: Handle<Font>,
    italic: Handle<Font>,
    bold_italic: Handle<Font>,
}

impl Family {
    fn load(
        asset_server: &AssetServer,
        regular: &'static str,
        bold: &'static str,
        italic: &'static str,
        bold_italic: &'static str,
    ) -> Self {
        Self {
            regular: asset_server.load(regular),
            bold: asset_server.load(bold),
            italic: asset_server.load(italic),
            bold_italic: asset_server.load(bold_italic),
        }
    }

    fn face(&self, style: Style) -> Handle<Font> {
        match (style.bold, style.italic) {
            (true, true) => self.bold_italic.clone(),
            (true, false) => self.bold.clone(),
            (false, true) => self.italic.clone(),
            (false, false) => self.regular.clone(),
        }
    }
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
            right: Val::Px(452.0),
            top: Val::Px(16.0),
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
    if views.is_empty() {
        return;
    }
    let header = Family::load(
        &asset_server,
        HEADER_FONT_PATH,
        HEADER_BOLD_FONT_PATH,
        HEADER_ITALIC_FONT_PATH,
        HEADER_BOLD_ITALIC_FONT_PATH,
    );
    let body = Family::load(
        &asset_server,
        BODY_FONT_PATH,
        BODY_BOLD_FONT_PATH,
        BODY_ITALIC_FONT_PATH,
        BODY_BOLD_ITALIC_FONT_PATH,
    );
    let mono = Family::load(
        &asset_server,
        MONO_FONT_PATH,
        MONO_BOLD_FONT_PATH,
        MONO_ITALIC_FONT_PATH,
        MONO_BOLD_ITALIC_FONT_PATH,
    );

    for (entity, rendered, localized) in &views {
        let blocks = match rendered {
            RenderedHtml::Pending => continue,
            RenderedHtml::Ready(document) => collect_blocks(document.dom(), &localized.0),
            RenderedHtml::Failed(message) => vec![Block {
                kind: BlockKind::Paragraph,
                runs: vec![Run {
                    text: format!("failed to render: {message}"),
                    style: Style::default(),
                }],
            }],
        };

        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for block in blocks {
                    let (family, size, color) = match block.kind {
                        BlockKind::Heading(level) => (&header, heading_size(level), HEADER_COLOR),
                        BlockKind::Paragraph | BlockKind::ListItem => {
                            (&body, BODY_SIZE, BODY_COLOR)
                        }
                        BlockKind::Preformatted => (&mono, PRE_SIZE, BODY_COLOR),
                    };
                    let prefix = match block.kind {
                        BlockKind::ListItem => "• ",
                        _ => "",
                    };

                    let mut text = parent.spawn((
                        Text::new(prefix),
                        TextFont::default()
                            .with_font(family.regular.clone())
                            .with_font_size(size),
                        TextColor(color),
                    ));
                    match block.kind {
                        BlockKind::ListItem => {
                            text.insert(Node {
                                margin: UiRect::left(Val::Px(12.0)),
                                ..default()
                            });
                        }
                        BlockKind::Preformatted => {
                            text.insert((
                                TextLayout::no_wrap(),
                                BackgroundColor(PRE_BACKGROUND),
                                Node {
                                    padding: UiRect::all(Val::Px(8.0)),
                                    overflow: Overflow::clip_x(),
                                    ..default()
                                },
                            ));
                        }
                        BlockKind::Heading(_) | BlockKind::Paragraph => {}
                    }
                    text.with_children(|spans| {
                        for run in block.runs {
                            let face = if run.style.mono { &mono } else { family };
                            spans.spawn((
                                TextSpan::new(run.text),
                                TextFont::default()
                                    .with_font(face.face(run.style))
                                    .with_font_size(size),
                                TextColor(color),
                            ));
                        }
                    });
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
            let runs = inline_runs(parser, handle);
            if !runs.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Paragraph,
                    runs,
                });
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let name = tag.name().as_utf8_str().to_ascii_lowercase();
    let kind = match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => BlockKind::Heading(name.as_bytes()[1] - b'0'),
        "p" => BlockKind::Paragraph,
        "li" => BlockKind::ListItem,
        "pre" => BlockKind::Preformatted,
        "head" | "script" | "style" => return,
        _ => {
            for child in tag.children().top().iter() {
                collect_node(parser, localized, *child, blocks);
            }
            return;
        }
    };
    let preformatted = matches!(kind, BlockKind::Preformatted);
    let runs = match localized.get(&handle) {
        Some(Ok(text)) => {
            let run = Run {
                text: text.clone(),
                style: Style {
                    mono: preformatted,
                    ..Style::default()
                },
            };
            if preformatted {
                vec![run]
            } else {
                collapse_runs(vec![run])
            }
        }
        // Missing translation: fall back to the element's own content.
        _ if preformatted => preformatted_runs(parser, handle),
        _ => inline_runs(parser, handle),
    };
    blocks.push(Block { kind, runs });
}

/// Styled text runs of `handle` and its descendants, whitespace collapsed.
fn inline_runs(parser: &tl::Parser, handle: tl::NodeHandle) -> Vec<Run> {
    let mut runs = Vec::new();
    push_runs(parser, handle, Style::default(), &mut runs);
    collapse_runs(runs)
}

/// Styled runs of a `pre` element: monospace, whitespace kept. As in HTML, a
/// newline right after `<pre>` is dropped; so is trailing whitespace.
fn preformatted_runs(parser: &tl::Parser, handle: tl::NodeHandle) -> Vec<Run> {
    let mut runs = Vec::new();
    let style = Style {
        mono: true,
        ..Style::default()
    };
    push_runs(parser, handle, style, &mut runs);

    if let Some(first) = runs.first_mut() {
        let stripped = first
            .text
            .strip_prefix("\r\n")
            .or_else(|| first.text.strip_prefix('\n'));
        if let Some(stripped) = stripped {
            first.text = stripped.to_owned();
        }
    }
    if let Some(last) = runs.last_mut() {
        let trimmed = last.text.trim_end().len();
        last.text.truncate(trimmed);
    }
    runs.retain(|run| !run.text.is_empty());
    runs
}

fn push_runs(parser: &tl::Parser, handle: tl::NodeHandle, style: Style, runs: &mut Vec<Run>) {
    match handle.get(parser) {
        Some(tl::Node::Raw(text)) => {
            let text = decode_entities(&text.as_utf8_str());
            match runs.last_mut() {
                Some(last) if last.style == style => last.text.push_str(&text),
                _ => runs.push(Run { text, style }),
            }
        }
        Some(tl::Node::Tag(tag)) => {
            let style = match tag.name().as_utf8_str().to_ascii_lowercase().as_str() {
                "b" | "strong" => Style { bold: true, ..style },
                "i" | "em" => Style {
                    italic: true,
                    ..style
                },
                "code" | "kbd" | "samp" | "tt" => Style { mono: true, ..style },
                _ => style,
            };
            for child in tag.children().top().iter() {
                push_runs(parser, *child, style, runs);
            }
        }
        Some(tl::Node::Comment(_)) | None => {}
    }
}

/// HTML whitespace collapsing across run boundaries: whitespace sequences
/// become one space, leading/trailing space of the block is dropped, and
/// emptied runs are removed.
fn collapse_runs(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    // Start as if after a space so the block's leading whitespace is dropped.
    let mut after_space = true;
    for run in runs {
        let mut text = String::with_capacity(run.text.len());
        for c in run.text.chars() {
            if c.is_whitespace() {
                if !after_space {
                    text.push(' ');
                    after_space = true;
                }
            } else {
                text.push(c);
                after_space = false;
            }
        }
        if !text.is_empty() {
            out.push(Run {
                text,
                style: run.style,
            });
        }
    }
    if let Some(last) = out.last_mut() {
        let trimmed = last.text.trim_end().len();
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            out.pop();
        }
    }
    out
}
