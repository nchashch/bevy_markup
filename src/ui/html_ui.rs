//! Renders an [`HtmlView`]'s DOM (after Tera and Fluent) as Bevy UI.
//!
//! Put [`HtmlUi`] next to an `HtmlView` on a UI node; its children are rebuilt
//! whenever the view re-renders, re-localizes, or its [`HtmlStylesheet`]
//! (re)loads.
//!
//! Tags only decide structure:
//! - blocks: `h1`–`h6`, `p`, `li` (with a bullet), `pre` (whitespace and line
//!   breaks kept, no wrapping), and loose text directly inside a container
//! - everything else is inline (contributes styled text) or, outside a
//!   block, a container that's walked through; `head`/`script`/`style` are
//!   skipped
//!
//! All styling — color, background, font family/size/weight/style — comes
//! from the stylesheet (see [`super::html_style`]), with inheritance. Without
//! a matching rule: white text, Bevy's default font, 16px.
//!
//! A block's text is its Fluent translation when it has a `data-l10n-id` that
//! resolved (one run in the block's style), otherwise its own content. Outside
//! `pre`, whitespace collapses as in HTML. Each block is one `Text` with a
//! `TextSpan` child per styled run.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;

use super::NineSliceFrame;
use super::dom_panel::{L10N_PATH, demo_context};
use super::html_style::HtmlStyles;
use super::scroll::{SCROLLBAR_GAP, spawn_scrollbar, viewport_node};
use crate::assets::css::CssStyleSheet;
use crate::assets::html::{HtmlView, RenderedHtml, decode_entities};
use crate::assets::l10n::LocalizedText;
use crate::consts::{FONT_FAMILIES, FRAME_PATH};

const STYLESHEET_PATH: &str = "ui/html.css";
/// Values when no stylesheet rule applies (CSS initial values, white on dark).
const DEFAULT_COLOR: Color = Color::WHITE;
const DEFAULT_FONT_SIZE: f32 = 16.0;
/// Top-anchored; capped so it stays clear of the debug panels along the bottom.
const PANEL_MAX_HEIGHT_VH: f32 = 50.0;

/// Renders this entity's [`HtmlView`] as its Bevy UI children.
#[derive(Component)]
#[require(Node)]
pub struct HtmlUi;

/// Stylesheet for an [`HtmlUi`]. While it is loading, the UI isn't built (so
/// it never flashes unstyled); it's rebuilt whenever the sheet (re)loads.
#[derive(Component)]
pub struct HtmlStylesheet(pub Handle<CssStyleSheet>);

/// Computed (inherited) text style.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    color: Color,
    /// Index into [`FONT_FAMILIES`]; `None` = Bevy's default font.
    family: Option<usize>,
    size: f32,
    bold: bool,
    italic: bool,
}

/// A stretch of text in one style.
struct Run {
    text: String,
    style: Style,
}

enum BlockKind {
    Heading,
    Paragraph,
    ListItem,
    Preformatted,
}

struct Block {
    kind: BlockKind,
    /// The block element's own computed style (bullet; root `Text` font).
    style: Style,
    background: Option<Color>,
    runs: Vec<Run>,
}

/// [`FONT_FAMILIES`] loaded as font handles, `[regular, bold, italic,
/// bold-italic]` per family.
struct Fonts(Vec<[Handle<Font>; 4]>);

impl Fonts {
    fn load(asset_server: &AssetServer) -> Self {
        Self(
            FONT_FAMILIES
                .iter()
                .map(|(_, paths)| paths.map(|path| asset_server.load(path)))
                .collect(),
        )
    }

    fn text_font(&self, style: Style) -> TextFont {
        let face = (style.bold as usize) + 2 * (style.italic as usize);
        let font = style
            .family
            .and_then(|family| self.0.get(family))
            .map(|faces| faces[face].clone())
            .unwrap_or_default();
        TextFont::default()
            .with_font(font)
            .with_font_size(style.size)
    }
}

pub(super) fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            NineSliceFrame(asset_server.load(FRAME_PATH)),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(452.0),
                top: Val::Px(16.0),
                width: Val::Px(420.0),
                max_height: Val::Vh(PANEL_MAX_HEIGHT_VH),
                padding: UiRect::all(Val::Px(28.0)),
                column_gap: SCROLLBAR_GAP,
                ..default()
            },
        ))
        .with_children(|panel| {
            // The viewport holds the rendered blocks (HtmlUi rebuilds its
            // children) and scrolls them.
            let viewport = panel
                .spawn((
                    HtmlUi,
                    HtmlView {
                        template: asset_server.load(L10N_PATH),
                        context: demo_context(),
                    },
                    HtmlStylesheet(asset_server.load(STYLESHEET_PATH)),
                    ScrollArea,
                    Node {
                        row_gap: Val::Px(10.0),
                        ..viewport_node()
                    },
                ))
                .id();
            spawn_scrollbar(panel, viewport);
        });
}

pub(super) fn build_html_ui(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut sheet_events: MessageReader<AssetEvent<CssStyleSheet>>,
    sheets: Res<Assets<CssStyleSheet>>,
    views: Query<
        (
            Entity,
            Ref<RenderedHtml>,
            Ref<LocalizedText>,
            Option<&HtmlStylesheet>,
        ),
        With<HtmlUi>,
    >,
) {
    let reloaded_sheets: HashSet<AssetId<CssStyleSheet>> = sheet_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    let mut fonts = None;
    for (entity, rendered, localized, stylesheet) in &views {
        let sheet_reloaded =
            stylesheet.is_some_and(|sheet| reloaded_sheets.contains(&sheet.0.id()));
        if !rendered.is_changed() && !localized.is_changed() && !sheet_reloaded {
            continue;
        }
        let styles = match stylesheet {
            Some(sheet) => match sheets.get(&sheet.0) {
                Some(css) => HtmlStyles::from_sheet(css.sheet()),
                // Still loading: its load event triggers the build.
                None => continue,
            },
            None => HtmlStyles::default(),
        };
        let root = root_style(&styles);

        let blocks = match &*rendered {
            RenderedHtml::Pending => continue,
            RenderedHtml::Ready(document) => {
                collect_blocks(document.dom(), &localized.0, &styles, root)
            }
            RenderedHtml::Failed(message) => vec![Block {
                kind: BlockKind::Paragraph,
                style: root,
                background: None,
                runs: vec![Run {
                    text: format!("failed to render: {message}"),
                    style: root,
                }],
            }],
        };

        let fonts = fonts.get_or_insert_with(|| Fonts::load(&asset_server));
        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for block in blocks {
                    let prefix = match block.kind {
                        BlockKind::ListItem => "• ",
                        _ => "",
                    };

                    // Blocks keep their height (`flex_shrink: 0`) so a
                    // scrolling parent overflows instead of squashing them.
                    let node = match block.kind {
                        BlockKind::ListItem => Node {
                            flex_shrink: 0.0,
                            margin: UiRect::left(Val::Px(12.0)),
                            ..default()
                        },
                        BlockKind::Preformatted => Node {
                            flex_shrink: 0.0,
                            padding: UiRect::all(Val::Px(8.0)),
                            overflow: Overflow::clip_x(),
                            ..default()
                        },
                        BlockKind::Heading | BlockKind::Paragraph => Node {
                            flex_shrink: 0.0,
                            ..default()
                        },
                    };
                    let mut text = parent.spawn((
                        Text::new(prefix),
                        fonts.text_font(block.style),
                        TextColor(block.style.color),
                        node,
                    ));
                    if matches!(block.kind, BlockKind::Preformatted) {
                        text.insert(TextLayout::no_wrap());
                    }
                    if let Some(background) = block.background {
                        text.insert(BackgroundColor(background));
                    }
                    text.with_children(|spans| {
                        for run in block.runs {
                            spans.spawn((
                                TextSpan::new(run.text),
                                fonts.text_font(run.style),
                                TextColor(run.style.color),
                            ));
                        }
                    });
                }
            });
    }
}

/// The `html` rule's values (the starting point even for fragments without
/// `<html>`), over the defaults.
fn root_style(styles: &HtmlStyles) -> Style {
    let declared = styles.get("html");
    Style {
        color: declared.color.unwrap_or(DEFAULT_COLOR),
        family: declared.font_family,
        size: declared
            .font_size
            .map_or(DEFAULT_FONT_SIZE, |size| {
                size.resolve(DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            }),
        bold: declared.bold.unwrap_or(false),
        italic: declared.italic.unwrap_or(false),
    }
}

/// Walk context shared by every node.
struct Ctx<'a, 'p, 'buf> {
    parser: &'p tl::Parser<'buf>,
    localized: &'a HashMap<tl::NodeHandle, Result<String, String>>,
    styles: &'a HtmlStyles,
    /// Root font size, for `rem`.
    root_size: f32,
}

impl Ctx<'_, '_, '_> {
    /// `tag`'s computed style: its declared values over `inherited`.
    fn style_of(&self, tag: &str, inherited: Style) -> Style {
        let declared = self.styles.get(tag);
        Style {
            color: declared.color.unwrap_or(inherited.color),
            family: declared.font_family.or(inherited.family),
            size: declared
                .font_size
                .map_or(inherited.size, |size| size.resolve(inherited.size, self.root_size)),
            bold: declared.bold.unwrap_or(inherited.bold),
            italic: declared.italic.unwrap_or(inherited.italic),
        }
    }
}

fn collect_blocks(
    dom: &tl::VDom,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    styles: &HtmlStyles,
    root: Style,
) -> Vec<Block> {
    let ctx = Ctx {
        parser: dom.parser(),
        localized,
        styles,
        root_size: root.size,
    };
    let mut blocks = Vec::new();
    for handle in dom.children() {
        collect_node(&ctx, *handle, root, &mut blocks);
    }
    blocks
}

fn collect_node(ctx: &Ctx, handle: tl::NodeHandle, inherited: Style, blocks: &mut Vec<Block>) {
    let Some(node) = handle.get(ctx.parser) else {
        return;
    };
    let tag = match node {
        tl::Node::Tag(tag) => tag,
        tl::Node::Raw(_) => {
            // Anonymous block: inherits the container's style.
            let runs = inline_runs(ctx, handle, inherited);
            if !runs.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Paragraph,
                    style: inherited,
                    background: None,
                    runs,
                });
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let name = tag.name().as_utf8_str().to_ascii_lowercase();
    let style = ctx.style_of(&name, inherited);
    let kind = match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => BlockKind::Heading,
        "p" => BlockKind::Paragraph,
        "li" => BlockKind::ListItem,
        "pre" => BlockKind::Preformatted,
        "head" | "script" | "style" => return,
        _ => {
            for child in tag.children().top().iter() {
                collect_node(ctx, *child, style, blocks);
            }
            return;
        }
    };
    let preformatted = matches!(kind, BlockKind::Preformatted);
    let runs = match ctx.localized.get(&handle) {
        Some(Ok(text)) => {
            let run = Run {
                text: text.clone(),
                style,
            };
            if preformatted {
                vec![run]
            } else {
                collapse_runs(vec![run])
            }
        }
        // Missing translation: fall back to the element's own content.
        _ if preformatted => preformatted_runs(ctx, handle, style),
        _ => inline_runs(ctx, handle, style),
    };
    blocks.push(Block {
        kind,
        style,
        background: ctx.styles.get(&name).background,
        runs,
    });
}

/// Styled text runs of `handle`'s content, whitespace collapsed.
fn inline_runs(ctx: &Ctx, handle: tl::NodeHandle, style: Style) -> Vec<Run> {
    let mut runs = Vec::new();
    push_children_runs(ctx, handle, style, &mut runs);
    collapse_runs(runs)
}

/// Styled runs of a `pre` element's content, whitespace kept. As in HTML, a
/// newline right after `<pre>` is dropped; so is trailing whitespace.
fn preformatted_runs(ctx: &Ctx, handle: tl::NodeHandle, style: Style) -> Vec<Run> {
    let mut runs = Vec::new();
    push_children_runs(ctx, handle, style, &mut runs);

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

/// Runs for `handle`'s content: its children for an element (whose own style
/// is already in `style`), the text itself for a text node.
fn push_children_runs(ctx: &Ctx, handle: tl::NodeHandle, style: Style, runs: &mut Vec<Run>) {
    match handle.get(ctx.parser) {
        Some(tl::Node::Tag(tag)) => {
            for child in tag.children().top().iter() {
                push_runs(ctx, *child, style, runs);
            }
        }
        Some(tl::Node::Raw(_)) => push_runs(ctx, handle, style, runs),
        Some(tl::Node::Comment(_)) | None => {}
    }
}

fn push_runs(ctx: &Ctx, handle: tl::NodeHandle, style: Style, runs: &mut Vec<Run>) {
    match handle.get(ctx.parser) {
        Some(tl::Node::Raw(text)) => {
            let text = decode_entities(&text.as_utf8_str());
            match runs.last_mut() {
                Some(last) if last.style == style => last.text.push_str(&text),
                _ => runs.push(Run { text, style }),
            }
        }
        Some(tl::Node::Tag(tag)) => {
            let name = tag.name().as_utf8_str().to_ascii_lowercase();
            let style = ctx.style_of(&name, style);
            for child in tag.children().top().iter() {
                push_runs(ctx, *child, style, runs);
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
