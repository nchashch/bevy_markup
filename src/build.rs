//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::cascade::HtmlStyles;
use crate::fonts::FontFamilies;
use crate::html::{HtmlDebugOutline, HtmlElement, HtmlUi, HtmlUiBuilt, RenderedHtml};
use crate::l10n::LocalizedText;
use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
use crate::template::decode_entities;

/// Values when no stylesheet rule applies (white suits the dark UIs Bevy
/// apps usually draw on; 16px is the CSS initial size).
const DEFAULT_COLOR: Color = Color::WHITE;
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Computed (inherited) text style.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    color: Color,
    /// Index into [`FontFamilies`]; `None` = Bevy's default font.
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
    /// `None` for anonymous loose text.
    element: Option<HtmlElement>,
    /// The block element's own computed style (bullet; root `Text` font).
    style: Style,
    background: Option<Color>,
    runs: Vec<Run>,
}

/// Computes styles from declared CSS + registered fonts.
struct Styler<'a> {
    styles: &'a HtmlStyles,
    fonts: &'a FontFamilies,
    /// Root font size, for `rem`.
    root_size: f32,
}

impl Styler<'_> {
    /// `tag`'s computed style: its declared values over `inherited`.
    fn style_of(&self, tag: &str, inherited: Style) -> Style {
        let declared = self.styles.get(tag);
        Style {
            color: declared.color.unwrap_or(inherited.color),
            family: declared
                .font_family
                .as_deref()
                .and_then(|list| self.fonts.resolve(list))
                .or(inherited.family),
            size: declared
                .font_size
                .map_or(inherited.size, |size| size.resolve(inherited.size, self.root_size)),
            bold: declared.bold.unwrap_or(inherited.bold),
            italic: declared.italic.unwrap_or(inherited.italic),
        }
    }

    fn text_font(&self, style: Style) -> TextFont {
        let font = style
            .family
            .and_then(|family| self.fonts.faces(family))
            .map(|faces| faces.face(style.bold, style.italic))
            .unwrap_or_default();
        TextFont::default()
            .with_font(font)
            .with_font_size(style.size)
    }
}

/// The `html` rule's values (the starting point even for fragments without
/// `<html>`), over the defaults.
fn root_style(styles: &HtmlStyles, fonts: &FontFamilies) -> Style {
    let defaults = Style {
        color: DEFAULT_COLOR,
        family: None,
        size: DEFAULT_FONT_SIZE,
        bold: false,
        italic: false,
    };
    Styler {
        styles,
        fonts,
        root_size: DEFAULT_FONT_SIZE,
    }
    .style_of("html", defaults)
}

pub(crate) fn build_html_ui(
    mut commands: Commands,
    mut sheet_events: MessageReader<AssetEvent<Stylesheet>>,
    sheets: Res<Assets<Stylesheet>>,
    default_sheet: Res<DefaultStylesheet>,
    fonts: Res<FontFamilies>,
    views: Query<
        (
            Entity,
            Ref<RenderedHtml>,
            Ref<LocalizedText>,
            Option<Ref<HtmlStylesheet>>,
            Option<Ref<HtmlDebugOutline>>,
        ),
        With<HtmlUi>,
    >,
) {
    let reloaded_sheets: HashSet<AssetId<Stylesheet>> = sheet_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    for (entity, rendered, localized, own_sheet, outline) in &views {
        let sheet = match &own_sheet {
            Some(own) => Some(&own.0),
            None => default_sheet.0.as_ref(),
        };
        let sheet_changed = match &own_sheet {
            Some(own) => own.is_changed(),
            None => default_sheet.is_changed(),
        } || sheet.is_some_and(|sheet| reloaded_sheets.contains(&sheet.id()));
        let dirty = rendered.is_changed()
            || localized.is_changed()
            || sheet_changed
            || fonts.is_changed()
            || outline.as_ref().is_some_and(|outline| outline.is_changed());
        if !dirty {
            continue;
        }

        let styles = match sheet {
            Some(sheet) => match sheets.get(sheet) {
                Some(css) => HtmlStyles::from_sheet(css.sheet()),
                // Still loading: its load event triggers the build.
                None => continue,
            },
            None => HtmlStyles::default(),
        };
        let root = root_style(&styles, &fonts);
        let styler = Styler {
            styles: &styles,
            fonts: &fonts,
            root_size: root.size,
        };

        let blocks = if outline.is_some() {
            let text = match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    let outline = document.outline(&localized);
                    debug!("DOM of {entity}:\n{outline}");
                    outline.trim_end().to_owned()
                }
                RenderedHtml::Failed(message) => format!("failed to render: {message}"),
            };
            let style = styler.style_of("pre", root);
            vec![Block {
                kind: BlockKind::Paragraph,
                element: None,
                style,
                background: None,
                runs: vec![Run { text, style }],
            }]
        } else {
            match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    collect_blocks(document.dom(), &localized.0, &styler, root)
                }
                RenderedHtml::Failed(message) => vec![Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    style: root,
                    background: None,
                    runs: vec![Run {
                        text: format!("failed to render: {message}"),
                        style: root,
                    }],
                }],
            }
        };

        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for block in blocks {
                    spawn_block(parent, &styler, block);
                }
            })
            .trigger(|entity| HtmlUiBuilt { entity });
    }
}

fn spawn_block(parent: &mut ChildSpawnerCommands, styler: &Styler, block: Block) {
    let prefix = match block.kind {
        BlockKind::ListItem => "• ",
        _ => "",
    };
    // Blocks keep their height (`flex_shrink: 0`) so a scrolling parent
    // overflows instead of squashing them.
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
        styler.text_font(block.style),
        TextColor(block.style.color),
        node,
    ));
    if let Some(element) = block.element {
        text.insert(element);
    }
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
                styler.text_font(run.style),
                TextColor(run.style.color),
            ));
        }
    });
}

/// Walk context shared by every node.
struct Ctx<'a, 'p, 'buf> {
    parser: &'p tl::Parser<'buf>,
    localized: &'a HashMap<tl::NodeHandle, Result<String, String>>,
    styler: &'a Styler<'a>,
}

fn collect_blocks(
    dom: &tl::VDom,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    styler: &Styler,
    root: Style,
) -> Vec<Block> {
    let ctx = Ctx {
        parser: dom.parser(),
        localized,
        styler,
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
            let mut runs = Vec::new();
            push_children_runs(ctx, handle, inherited, &mut runs);
            let runs = collapse_runs(runs);
            if !runs.is_empty() {
                blocks.push(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
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
    let style = ctx.styler.style_of(&name, inherited);
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
        Some(Ok(translation)) => translation_runs(ctx, translation, style, preformatted),
        // Missing translation: fall back to the element's own content.
        _ => {
            let mut runs = Vec::new();
            push_children_runs(ctx, handle, style, &mut runs);
            finish_runs(runs, preformatted)
        }
    };
    let attribute = |key: &str| {
        tag.attributes()
            .get(key)
            .flatten()
            .map(|value| decode_entities(&value.as_utf8_str()))
    };
    let element = HtmlElement {
        id: attribute("id"),
        classes: attribute("class")
            .map(|classes| classes.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        tag: name.clone(),
    };
    blocks.push(Block {
        kind,
        element: Some(element),
        style,
        background: ctx.styler.styles.get(&name).background,
        runs,
    });
}

/// Styled runs of a translation. Translations are markup (fluent-dom style
/// overlays): inline elements in them are styled by the stylesheet like
/// elements in the document; entities are decoded.
fn translation_runs(ctx: &Ctx, translation: &str, style: Style, preformatted: bool) -> Vec<Run> {
    let Ok(fragment) = tl::parse(translation, tl::ParserOptions::default()) else {
        return finish_runs(
            vec![Run {
                text: decode_entities(translation),
                style,
            }],
            preformatted,
        );
    };
    let fragment_ctx = Ctx {
        parser: fragment.parser(),
        localized: ctx.localized,
        styler: ctx.styler,
    };
    let mut runs = Vec::new();
    for child in fragment.children() {
        push_runs(&fragment_ctx, *child, style, &mut runs);
    }
    finish_runs(runs, preformatted)
}

/// Whitespace handling for a block's runs: collapsed as in HTML, or for `pre`
/// kept, minus a newline right after the start tag and trailing whitespace.
fn finish_runs(mut runs: Vec<Run>, preformatted: bool) -> Vec<Run> {
    if !preformatted {
        return collapse_runs(runs);
    }
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
            let style = ctx.styler.style_of(&name, style);
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
