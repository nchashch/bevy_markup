//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::cascade::{HtmlStyles, SliceValue};
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
    runs: Vec<Run>,
}

/// Resolved box properties of an element: `border-width`, `padding`,
/// `background-color`, `border-image`.
#[derive(Default)]
struct BoxStyle {
    border: [Option<f32>; 4],
    padding: [Option<f32>; 4],
    background: Option<Color>,
    image: Option<(Handle<Image>, TextureSlicer)>,
}

impl BoxStyle {
    fn is_empty(&self) -> bool {
        self.border.iter().all(Option::is_none)
            && self.padding.iter().all(Option::is_none)
            && self.background.is_none()
            && self.image.is_none()
    }

    fn sliced_image(&self) -> Option<ImageNode> {
        let (image, slicer) = self.image.as_ref()?;
        Some(ImageNode {
            visual_box: VisualBox::BorderBox,
            ..ImageNode::new(image.clone()).with_mode(NodeImageMode::Sliced(slicer.clone()))
        })
    }
}

/// `[top, right, bottom, left]` px values over `base`.
fn rect_over(base: UiRect, sides: [Option<f32>; 4]) -> UiRect {
    let side = |value: Option<f32>, base: Val| value.map_or(base, Val::Px);
    UiRect {
        top: side(sides[0], base.top),
        right: side(sides[1], base.right),
        bottom: side(sides[2], base.bottom),
        left: side(sides[3], base.left),
    }
}

/// What the `html` rule's box properties replaced on an `HtmlUi` node, so
/// they can be restored when a later stylesheet drops them.
#[derive(Component)]
pub(crate) struct CssRootBox {
    border: UiRect,
    padding: UiRect,
    image: bool,
}

/// Computes styles from declared CSS + registered fonts.
struct Styler<'a> {
    styles: &'a HtmlStyles<'a>,
    fonts: &'a FontFamilies,
    /// Root font size, for `rem`.
    root_size: f32,
    /// For `border-image` sources.
    sheet: Option<&'a Stylesheet>,
    images: &'a Assets<Image>,
}

impl Styler<'_> {
    /// `element`'s computed style: its declared values over `inherited`.
    fn style_of(&self, element: &HtmlElement, inherited: Style) -> Style {
        let declared = self.styles.get(element);
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

    /// `element`'s box properties. A `border-image` whose `%` slices need the
    /// image size is skipped until the image has loaded (its load rebuilds).
    fn box_of(&self, element: &HtmlElement) -> BoxStyle {
        let declared = self.styles.get(element);
        let image = declared.border_image.as_ref().and_then(|decl| {
            let url = decl.source.as_ref()?.as_ref()?;
            let handle = self.sheet?.image(url)?;
            let (offsets, fill) = decl
                .slice
                .unwrap_or(([SliceValue::Fraction(1.0); 4], false));
            if !fill {
                debug!("html css: border-image without `fill` still draws the center in Bevy");
            }
            let size = self.images.get(handle).map(Image::size_f32);
            // [top, right, bottom, left]; % is of the height for top/bottom.
            let mut px = [0.0; 4];
            for (index, offset) in offsets.iter().enumerate() {
                px[index] = match offset {
                    SliceValue::Px(px) => *px,
                    SliceValue::Fraction(fraction) => {
                        let size = size?;
                        fraction * if index % 2 == 0 { size.y } else { size.x }
                    }
                };
            }
            let mode = if decl.tile.unwrap_or(false) {
                SliceScaleMode::Tile { stretch_value: 1.0 }
            } else {
                SliceScaleMode::Stretch
            };
            Some((
                handle.clone(),
                TextureSlicer {
                    border: BorderRect {
                        min_inset: Vec2::new(px[3], px[0]),
                        max_inset: Vec2::new(px[1], px[2]),
                    },
                    center_scale_mode: mode,
                    sides_scale_mode: mode,
                    max_corner_scale: 1.0,
                },
            ))
        });
        BoxStyle {
            border: declared.border_width,
            padding: declared.padding,
            background: declared.background,
            image,
        }
    }
}

/// The `html` rule's values (the starting point even for fragments without
/// `<html>`), over the defaults.
fn root_style(styles: &HtmlStyles, fonts: &FontFamilies, images: &Assets<Image>) -> Style {
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
        sheet: None,
        images,
    }
    .style_of(&element_tag("html"), defaults)
}

pub(crate) fn build_html_ui(
    mut commands: Commands,
    mut sheet_events: MessageReader<AssetEvent<Stylesheet>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    sheets: Res<Assets<Stylesheet>>,
    images: Res<Assets<Image>>,
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
    mut roots: Query<(&mut Node, Option<&CssRootBox>), With<HtmlUi>>,
) {
    let reloaded_sheets: HashSet<AssetId<Stylesheet>> = sheet_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    let loaded_images: HashSet<AssetId<Image>> = image_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    for (entity, rendered, localized, own_sheet, outline) in &views {
        let sheet_handle = match &own_sheet {
            Some(own) => Some(&own.0),
            None => default_sheet.0.as_ref(),
        };
        let css = match sheet_handle {
            Some(handle) => match sheets.get(handle) {
                Some(css) => Some(css),
                // Still loading: its load event triggers the build.
                None => continue,
            },
            None => None,
        };
        let sheet_changed = match &own_sheet {
            Some(own) => own.is_changed(),
            None => default_sheet.is_changed(),
        } || sheet_handle.is_some_and(|sheet| reloaded_sheets.contains(&sheet.id()))
            || css.is_some_and(|css| {
                css.images()
                    .iter()
                    .any(|image| loaded_images.contains(&image.id()))
            });
        let dirty = rendered.is_changed()
            || localized.is_changed()
            || sheet_changed
            || fonts.is_changed()
            || outline.as_ref().is_some_and(|outline| outline.is_changed());
        if !dirty {
            continue;
        }

        let styles = css
            .map(|css| HtmlStyles::from_sheet(css.sheet()))
            .unwrap_or_default();
        let root = root_style(&styles, &fonts, &images);
        let styler = Styler {
            styles: &styles,
            fonts: &fonts,
            root_size: root.size,
            sheet: css,
            images: &images,
        };

        if let Ok((mut node, state)) = roots.get_mut(entity) {
            let root_box = styler.box_of(&element_tag("html"));
            apply_root_box(&mut commands, entity, &mut node, state, root_box);
        }

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
            let style = styler.style_of(&element_tag("pre"), root);
            vec![Block {
                kind: BlockKind::Paragraph,
                element: None,
                style,
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
    let boxed = block
        .element
        .as_ref()
        .map(|element| styler.box_of(element))
        .unwrap_or_default();
    let prefix = match block.kind {
        BlockKind::ListItem => "• ",
        _ => "",
    };
    let indent = match block.kind {
        BlockKind::ListItem => UiRect::left(Val::Px(12.0)),
        _ => UiRect::DEFAULT,
    };
    // Blocks keep their height (`flex_shrink: 0`) so a scrolling parent
    // overflows instead of squashing them.
    let text_node = match block.kind {
        BlockKind::Preformatted => Node {
            flex_shrink: 0.0,
            padding: UiRect::all(Val::Px(8.0)),
            overflow: Overflow::clip_x(),
            ..default()
        },
        BlockKind::Heading | BlockKind::Paragraph | BlockKind::ListItem => Node {
            flex_shrink: 0.0,
            ..default()
        },
    };
    let text = (
        Text::new(prefix),
        styler.text_font(block.style),
        TextColor(block.style.color),
    );
    let spawn_spans = |spans: &mut ChildSpawnerCommands| {
        for run in block.runs {
            spans.spawn((
                TextSpan::new(run.text),
                styler.text_font(run.style),
                TextColor(run.style.color),
            ));
        }
    };
    let no_wrap = matches!(block.kind, BlockKind::Preformatted);

    if boxed.is_empty() {
        let mut entity = parent.spawn((text, Node { margin: indent, ..text_node }));
        if let Some(element) = block.element {
            entity.insert(element);
        }
        if no_wrap {
            entity.insert(TextLayout::no_wrap());
        }
        entity.with_children(spawn_spans);
        return;
    }

    // Box properties go on a wrapper node: a node can't be both `Text` and
    // `ImageNode` (both size it from content).
    let mut wrapper = parent.spawn(Node {
        flex_shrink: 0.0,
        margin: indent,
        border: rect_over(UiRect::DEFAULT, boxed.border),
        padding: rect_over(UiRect::DEFAULT, boxed.padding),
        ..default()
    });
    if let Some(element) = block.element {
        wrapper.insert(element);
    }
    if let Some(background) = boxed.background {
        wrapper.insert(BackgroundColor(background));
    }
    if let Some(image) = boxed.sliced_image() {
        wrapper.insert(image);
    }
    wrapper.with_children(|wrapper| {
        let mut entity = wrapper.spawn((text, text_node));
        if no_wrap {
            entity.insert(TextLayout::no_wrap());
        }
        entity.with_children(spawn_spans);
    });
}

/// Applies the `html` rule's `border-image`, `border-width` and `padding` to
/// the `HtmlUi` node itself, remembering what they replaced (in
/// [`CssRootBox`]) so a stylesheet without them restores the node.
/// `background-color` stays block-only (the node's own `BackgroundColor`
/// belongs to the app).
fn apply_root_box(
    commands: &mut Commands,
    entity: Entity,
    node: &mut Node,
    state: Option<&CssRootBox>,
    boxed: BoxStyle,
) {
    let (base_border, base_padding) = state.map_or((node.border, node.padding), |state| {
        (state.border, state.padding)
    });
    let image = boxed.sliced_image();
    let applies = image.is_some()
        || boxed.border.iter().any(Option::is_some)
        || boxed.padding.iter().any(Option::is_some);
    let had_image = state.is_some_and(|state| state.image);

    if !applies {
        if state.is_some() {
            node.border = base_border;
            node.padding = base_padding;
            let mut entity = commands.entity(entity);
            entity.remove::<CssRootBox>();
            if had_image {
                entity.remove::<ImageNode>();
            }
        }
        return;
    }

    let border = rect_over(base_border, boxed.border);
    let padding = rect_over(base_padding, boxed.padding);
    if node.border != border {
        node.border = border;
    }
    if node.padding != padding {
        node.padding = padding;
    }
    let mut entity = commands.entity(entity);
    entity.insert(CssRootBox {
        border: base_border,
        padding: base_padding,
        image: image.is_some(),
    });
    match image {
        Some(image) => {
            entity.insert(image);
        }
        None if had_image => {
            entity.remove::<ImageNode>();
        }
        None => {}
    }
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
                    runs,
                });
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let element = element_of(tag);
    let style = ctx.styler.style_of(&element, inherited);
    let kind = match element.tag.as_str() {
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
    blocks.push(Block {
        kind,
        element: Some(element),
        style,
        runs,
    });
}

/// Tag name (lowercase), `id` and `class` of an element.
fn element_of(tag: &tl::HTMLTag) -> HtmlElement {
    let attribute = |key: &str| {
        tag.attributes()
            .get(key)
            .flatten()
            .map(|value| decode_entities(&value.as_utf8_str()))
    };
    HtmlElement {
        tag: tag.name().as_utf8_str().to_ascii_lowercase(),
        id: attribute("id"),
        classes: attribute("class")
            .map(|classes| classes.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
    }
}

/// An element known only by its tag (`html` root, `pre` for outlines).
fn element_tag(tag: &str) -> HtmlElement {
    HtmlElement {
        tag: tag.to_owned(),
        ..default()
    }
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
            let style = ctx.styler.style_of(&element_of(tag), style);
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
