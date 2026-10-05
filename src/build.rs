//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use std::cell::RefCell;

use bevy::asset::{AssetEvent, LoadState};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::cascade::{HtmlStyles, LayoutDecl, Pseudo, SliceValue};
use crate::fonts::FontFamilies;
use crate::html::{
    HtmlDebugOutline, HtmlElement, HtmlUi, HtmlUiBuilt, HtmlUiRestyled, RenderedHtml,
};
use crate::l10n::LocalizedText;
use crate::rebuild::{Decision, Frame, Phase, RebuildState, Source};
use crate::signals::{self, ElementSignals, PseudoState, SignalBinding};
use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
use crate::template::decode_entities;

/// Values when no stylesheet rule applies (white suits the dark UIs Bevy
/// apps usually draw on; 16px is the CSS initial size).
const DEFAULT_COLOR: Color = Color::WHITE;
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Computed (inherited) text style.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Style {
    pub(crate) color: Color,
    /// Index into [`FontFamilies`]; `None` = Bevy's default font.
    pub(crate) family: Option<usize>,
    pub(crate) size: f32,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
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
    /// The element's `data-on-*` hooks.
    signals: Vec<SignalBinding>,
    /// The DOM node, for `:hover`/`:active` restyles.
    handle: Option<tl::NodeHandle>,
    /// The block element's own computed style (bullet; root `Text` font).
    style: Style,
    /// Box properties, computed with the element's interaction state.
    boxed: BoxStyle,
    runs: Vec<Run>,
}

/// Elements that become column nodes holding their children's nodes (so CSS
/// box properties and `HtmlElements` reach them).
const CONTAINERS: &[&str] = &[
    "div",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "ul",
    "ol",
    "blockquote",
    "figure",
    "form",
];

/// A node to spawn: a text block, or a container of further items.
enum Item {
    Block(Block),
    Container {
        element: HtmlElement,
        handle: Option<tl::NodeHandle>,
        signals: Vec<SignalBinding>,
        /// Box properties, computed with the element's interaction state.
        boxed: BoxStyle,
        children: Vec<Item>,
    },
}

/// Resolved box properties of an element: `border-width`, `padding`,
/// `background-color`, `border-image`, plus layout (flex, sizes, margins).
#[derive(Default)]
pub(crate) struct BoxStyle {
    pub(crate) border: [Option<f32>; 4],
    pub(crate) padding: [Option<f32>; 4],
    pub(crate) background: Option<Color>,
    pub(crate) image: Option<(Handle<Image>, TextureSlicer)>,
    /// `row-gap` for containers (not part of `is_empty`: blocks ignore it).
    pub(crate) row_gap: Option<f32>,
    /// Not part of `is_empty`: a `Text` node takes these itself.
    pub(crate) layout: LayoutDecl,
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
pub(crate) struct Styler<'a> {
    pub(crate) styles: &'a HtmlStyles<'a>,
    pub(crate) fonts: &'a FontFamilies,
    /// Root font size, for `rem`.
    pub(crate) root_size: f32,
    /// For `border-image` sources.
    pub(crate) sheet: Option<&'a Stylesheet>,
    pub(crate) images: &'a Assets<Image>,
}

impl Styler<'_> {
    /// `element`'s computed style: its declared values over `inherited`.
    /// `pseudo` is the element's interaction state (`:hover`/`:active`).
    pub(crate) fn style_of(
        &self,
        element: &HtmlElement,
        inherited: Style,
        pseudo: Pseudo,
    ) -> Style {
        let declared = self.styles.get(element, pseudo);
        Style {
            color: declared.color.unwrap_or(inherited.color),
            // A declared list with no registered family falls back to the
            // default font (as in browsers), not to the inherited family.
            family: match declared.font_family.as_deref() {
                Some(list) => self.fonts.resolve(list),
                None => inherited.family,
            },
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
        let mut text_font = TextFont::default().with_font_size(style.size);
        // A file is one face (its weight and style are baked in); a system
        // family is many, and the system picks by the requested weight/style.
        if !matches!(font, FontSource::Handle(_)) {
            if style.bold {
                text_font.weight = FontWeight::BOLD;
            }
            if style.italic {
                text_font.style = FontStyle::Italic;
            }
        }
        text_font.font = font;
        text_font
    }

    /// `element`'s box properties. A `border-image` whose `%` slices need the
    /// image size is skipped until the image has loaded (its load rebuilds).
    pub(crate) fn box_of(&self, element: &HtmlElement, pseudo: Pseudo) -> BoxStyle {
        let declared = self.styles.get(element, pseudo);
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
            row_gap: declared.row_gap,
            layout: declared.layout.clone(),
        }
    }
}

/// The `html` rule's values (the starting point even for fragments without
/// `<html>`), over the defaults.
pub(crate) fn root_style(styles: &HtmlStyles, fonts: &FontFamilies, images: &Assets<Image>) -> Style {
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
    .style_of(&element_tag("html"), defaults, Pseudo::default())
}

pub(crate) fn build_html_ui(
    mut commands: Commands,
    mut sheet_events: MessageReader<AssetEvent<Stylesheet>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    sheets: Res<Assets<Stylesheet>>,
    server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    default_sheet: Res<DefaultStylesheet>,
    fonts: Res<FontFamilies>,
    mut views: Query<
        (
            Entity,
            Ref<RenderedHtml>,
            Ref<LocalizedText>,
            Option<Ref<HtmlStylesheet>>,
            Option<Ref<HtmlDebugOutline>>,
            &mut RebuildState,
        ),
        With<HtmlUi>,
    >,
    mut roots: Query<(&mut Node, Option<&CssRootBox>), With<HtmlUi>>,
    parents: Query<&ChildOf>,
    tree: Tree,
    dom_nodes: Query<(&DomNode, Option<&PseudoState>)>,
    pseudo_changed: Query<Entity, Changed<PseudoState>>,
    mut removed_outlines: RemovedComponents<HtmlDebugOutline>,
    mut removed_sheets: RemovedComponents<HtmlStylesheet>,
) {
    // Removals aren't `Ref` changes, so they're read separately.
    let removed_outlines: HashSet<Entity> = removed_outlines.read().collect();
    let removed_sheets: HashSet<Entity> = removed_sheets.read().collect();
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
    let phase = |handle: &Handle<Stylesheet>| match sheets.get(handle) {
        Some(_) => Phase::Ready,
        None if matches!(server.load_state(handle.id()), LoadState::Failed(_)) => Phase::Failed,
        None => Phase::Loading,
    };
    // A sheet reloaded, or one of its `border-image` images (re)loaded.
    let refreshed = |handle: &Handle<Stylesheet>| {
        reloaded_sheets.contains(&handle.id())
            || sheets.get(handle).is_some_and(|css| {
                css.images()
                    .iter()
                    .any(|image| loaded_images.contains(&image.id()))
            })
    };
    let default_changed =
        default_sheet.is_changed() || default_sheet.0.as_ref().is_some_and(refreshed);

    // UIs with an element whose `:hover`/`:active` state changed: the state
    // change restyles that UI (the owning `HtmlUi` is the walk's first hit).
    let mut state_restyle: HashSet<Entity> = HashSet::new();
    for entity in pseudo_changed.iter() {
        let mut current = Some(entity);
        while let Some(node) = current {
            if views.contains(node) {
                state_restyle.insert(node);
                break;
            }
            current = parents.get(node).ok().map(|parent| parent.0);
        }
    }

    // First pass: the decision per UI. Nested UIs need the whole set before
    // anything acts (see the suppression below).
    let mut decisions: Vec<(Entity, Decision)> = Vec::new();
    for (entity, rendered, localized, own_sheet, outline, mut rebuild) in &mut views {
        let frame = Frame {
            own: own_sheet.as_ref().map(|own| phase(&own.0)),
            default: default_sheet.0.as_ref().map(phase),
            document_ready: !matches!(*rendered, RenderedHtml::Pending),
            own_changed: removed_sheets.contains(&entity)
                || own_sheet.as_ref().is_some_and(|own| own.is_changed() || refreshed(&own.0)),
            default_changed,
            fonts_changed: fonts.is_changed(),
            state_changed: state_restyle.contains(&entity),
            content_changed: rendered.is_changed()
                || localized.is_changed()
                || removed_outlines.contains(&entity)
                || outline.as_ref().is_some_and(|outline| outline.is_changed()),
        };
        decisions.push((entity, rebuild.decide(frame)));
    }
    // Every UI rebuilding this frame: its `despawn_related` replaces the
    // whole subtree, nested `HtmlUi`s included.
    let rebuilding: HashSet<Entity> = decisions
        .iter()
        .filter(|(_, decision)| matches!(decision, Decision::Build(_)))
        .map(|(entity, _)| *entity)
        .collect();

    for (entity, decision) in decisions {
        let (source, restyle) = match decision {
            Decision::Wait | Decision::Skip => continue,
            Decision::Build(source) => (source, false),
            Decision::Restyle(source) => (source, true),
        };
        // An ancestor rebuilding this frame despawns this UI with its
        // subtree (children are replaced wholesale), so acting here — build
        // or restyle — would queue commands on despawned entities. Whatever
        // this UI decided is moot; an app that nests UIs is expected to
        // re-nest them on `HtmlUiBuilt`.
        let mut ancestor = Some(entity);
        let suppressed = std::iter::from_fn(|| {
            ancestor = parents.get(ancestor?).ok().map(|parent| parent.0);
            ancestor
        })
        .any(|parent| rebuilding.contains(&parent));
        if suppressed {
            debug!("html ui: {entity} is nested under a rebuilding UI; skipped");
            continue;
        }
        let Ok((_, rendered, localized, own_sheet, outline, _rebuild)) = views.get(entity) else {
            continue;
        };
        let css = match source {
            Source::Own => own_sheet.as_ref().and_then(|own| sheets.get(&own.0)),
            Source::Default => default_sheet.0.as_ref().and_then(|d| sheets.get(d)),
            Source::Unstyled => None,
        };

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
            let root_box = styler.box_of(&element_tag("html"), Pseudo::default());
            apply_root_box(&mut commands, entity, &mut node, state, root_box);
        }

        // The elements' interaction state by DOM node, so the collected
        // items style for `:hover`/`:active`. Nested `HtmlUi`s have their
        // own documents — their handles index those, not this one — so their
        // subtrees are skipped (the starting entity is the UI root itself).
        let mut states = HashMap::new();
        let mut stack = vec![entity];
        while let Some(current) = stack.pop() {
            if let Ok((handle, state)) = dom_nodes.get(current) {
                states.insert(
                    **handle,
                    state.copied().map_or(Pseudo::default(), |state| Pseudo {
                        hover: state.hovered,
                        active: state.active,
                    }),
                );
            }
            let nested_root = current != entity
                && tree.get(current).is_ok_and(|(.., nested_ui)| nested_ui);
            if !nested_root
                && let Ok((children, ..)) = tree.get(current)
            {
                stack.extend(children.iter().flat_map(|children| children.iter()));
            }
        }

        let items = if outline.is_some() {
            let text = match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    let outline = document.outline(&localized);
                    debug!("DOM of {entity}:\n{outline}");
                    outline.trim_end().to_owned()
                }
                RenderedHtml::Failed(message) => format!("failed to render: {message}"),
            };
            let style = styler.style_of(&element_tag("pre"), root, Pseudo::default());
            vec![Item::Block(Block {
                kind: BlockKind::Paragraph,
                element: None,
                handle: None,
                signals: Vec::new(),
                style,
                boxed: BoxStyle::default(),
                runs: vec![Run { text, style }],
            })]
        } else {
            match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    collect_items(
                        document.dom(),
                        &localized.0,
                        Some(&states),
                        &styler,
                        root,
                    )
                }
                RenderedHtml::Failed(message) => vec![Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    handle: None,
                    signals: Vec::new(),
                    style: root,
                    boxed: BoxStyle::default(),
                    runs: vec![Run {
                        text: format!("failed to render: {message}"),
                        style: root,
                    }],
                })],
            }
        };

        // Nested containers space their children like the root does.
        let default_gap = roots.get(entity).map_or(Val::Auto, |(node, _)| node.row_gap);
        let specs: Vec<NodeSpec> = items
            .into_iter()
            .map(|item| item_spec(&styler, item, default_gap))
            .collect();
        let children: Vec<Entity> = tree
            .get(entity)
            .ok()
            .and_then(|(children, ..)| children)
            .map_or_else(Vec::new, |children| children.to_vec());
        // Restyle in place when the existing children have the spec's shape;
        // otherwise (e.g. CSS added box properties, which need a wrapper)
        // rebuild.
        if restyle
            && children.len() == specs.len()
            && children.iter().zip(&specs).all(|(child, spec)| same_shape(spec, *child, &tree))
        {
            for (child, spec) in children.into_iter().zip(specs) {
                apply_spec(&mut commands, child, spec, &tree);
            }
            commands.trigger(HtmlUiRestyled { entity });
        } else {
            commands
                .entity(entity)
                .despawn_related::<Children>()
                .with_children(|parent| {
                    for spec in specs {
                        spawn_spec(parent, spec);
                    }
                })
                .trigger(|entity| HtmlUiBuilt { entity });
        }
    }
}

/// What one UI node should be, computed from the item tree, then spawned
/// ([`spawn_spec`]) or applied onto an existing entity of the same shape
/// ([`apply_spec`]): one description for both builds and restyles.
struct NodeSpec {
    node: Node,
    element: Option<HtmlElement>,
    /// The element's `data-on-*` hooks.
    signals: Vec<SignalBinding>,
    /// The DOM node, for `:hover`/`:active` restyles.
    handle: Option<tl::NodeHandle>,
    background: Option<Color>,
    image: Option<ImageNode>,
    /// `Text` nodes hold spans, never child nodes.
    text: Option<TextSpec>,
    children: Vec<NodeSpec>,
}

struct TextSpec {
    /// Before the spans, in the block's own style (the `li` bullet).
    prefix: &'static str,
    font: TextFont,
    color: Color,
    no_wrap: bool,
    spans: Vec<(String, TextFont, Color)>,
}

impl NodeSpec {
    fn new(node: Node) -> Self {
        Self {
            node,
            element: None,
            signals: Vec::new(),
            handle: None,
            background: None,
            image: None,
            text: None,
            children: Vec::new(),
        }
    }
}

/// The existing children's structure, for [`same_shape`] and [`apply_spec`].
type Tree<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Children>,
        Has<Text>,
        Has<TextSpan>,
        Has<HtmlElement>,
        Has<ImageNode>,
        Has<HtmlUi>,
    ),
>;

fn item_spec(styler: &Styler, item: Item, default_gap: Val) -> NodeSpec {
    let (element, handle, signals, boxed, children) = match item {
        Item::Block(block) => return block_spec(styler, block),
        Item::Container {
            element,
            handle,
            signals,
            boxed,
            children,
        } => (element, handle, signals, boxed, children),
    };
    let mut node = Node {
        flex_direction: FlexDirection::Column,
        flex_shrink: 0.0,
        // CSS sizes the content box; Bevy defaults to the border box.
        box_sizing: BoxSizing::ContentBox,
        row_gap: boxed.row_gap.map_or(default_gap, Val::Px),
        border: rect_over(UiRect::DEFAULT, boxed.border),
        padding: rect_over(UiRect::DEFAULT, boxed.padding),
        ..default()
    };
    boxed.layout.apply_to(&mut node);
    NodeSpec {
        element: Some(element),
        signals,
        handle,
        background: boxed.background,
        image: boxed.sliced_image(),
        children: children
            .into_iter()
            .map(|child| item_spec(styler, child, default_gap))
            .collect(),
        ..NodeSpec::new(node)
    }
}

fn block_spec(styler: &Styler, block: Block) -> NodeSpec {
    let boxed = block.boxed;
    let indent = match block.kind {
        BlockKind::ListItem => UiRect::left(Val::Px(12.0)),
        _ => UiRect::DEFAULT,
    };
    // `pre`'s default padding; CSS padding replaces it per side.
    let padding = rect_over(
        match block.kind {
            BlockKind::Preformatted => UiRect::all(Val::Px(8.0)),
            _ => UiRect::DEFAULT,
        },
        boxed.padding,
    );
    // Blocks keep their height (`flex_shrink: 0`) so a scrolling parent
    // overflows instead of squashing them.
    let text_node = Node {
        flex_shrink: 0.0,
        box_sizing: BoxSizing::ContentBox,
        overflow: match block.kind {
            BlockKind::Preformatted => Overflow::clip_x(),
            _ => Overflow::DEFAULT,
        },
        ..default()
    };
    let text = TextSpec {
        prefix: match block.kind {
            BlockKind::ListItem => "• ",
            _ => "",
        },
        font: styler.text_font(block.style),
        color: block.style.color,
        no_wrap: matches!(block.kind, BlockKind::Preformatted),
        spans: block
            .runs
            .into_iter()
            .map(|run| (run.text, styler.text_font(run.style), run.style.color))
            .collect(),
    };

    if boxed.is_empty() {
        let mut node = Node {
            margin: indent,
            padding,
            ..text_node
        };
        boxed.layout.apply_to(&mut node);
        return NodeSpec {
            element: block.element,
            signals: block.signals,
            handle: block.handle,
            text: Some(text),
            ..NodeSpec::new(node)
        };
    }

    // Box properties go on a wrapper node: a node can't be both `Text` and
    // `ImageNode` (both size it from content). It's a column so the text
    // stretches to the content box and wraps there, like a CSS block (in a
    // row the non-shrinking text kept its max-content width and overflowed).
    let mut node = Node {
        flex_direction: FlexDirection::Column,
        flex_shrink: 0.0,
        box_sizing: BoxSizing::ContentBox,
        margin: indent,
        border: rect_over(UiRect::DEFAULT, boxed.border),
        padding,
        ..default()
    };
    boxed.layout.apply_to(&mut node);
    NodeSpec {
        element: block.element,
        signals: block.signals,
        handle: block.handle,
        background: boxed.background,
        image: boxed.sliced_image(),
        children: vec![NodeSpec {
            text: Some(text),
            ..NodeSpec::new(text_node)
        }],
        ..NodeSpec::new(node)
    }
}

/// The entity's DOM node: lets a restyle look up the element's
/// `:hover`/`:active` state.
#[derive(Component, Clone, Copy, Debug, Deref)]
pub(crate) struct DomNode(pub tl::NodeHandle);

fn spawn_spec(parent: &mut ChildSpawnerCommands, spec: NodeSpec) {
    let mut entity = parent.spawn(spec.node);
    if let Some(element) = spec.element {
        entity.insert(element);
    }
    if let Some(handle) = spec.handle {
        entity.insert(DomNode(handle));
    }
    if !spec.signals.is_empty() {
        let signals = ElementSignals(spec.signals);
        signals::attach_pointer_signals(&mut entity, &signals);
        entity.insert(signals);
    }
    if let Some(background) = spec.background {
        entity.insert(BackgroundColor(background));
    }
    if let Some(image) = spec.image {
        entity.insert(image);
    }
    match spec.text {
        Some(text) => {
            entity.insert((Text::new(text.prefix), text.font, TextColor(text.color)));
            if text.no_wrap {
                entity.insert(TextLayout::no_wrap());
            }
            entity.with_children(|spans| {
                for (span, font, color) in text.spans {
                    spans.spawn((TextSpan::new(span), font, TextColor(color)));
                }
            });
        }
        None => {
            entity.with_children(|children| {
                for child in spec.children {
                    spawn_spec(children, child);
                }
            });
        }
    }
}

/// Whether `entity` (an existing child) has the structure `spec` would
/// spawn: the same text/element/frame presence, span count and children.
/// Children the app attached — nested `HtmlUi` roots — aren't the spec's:
/// restyles keep them (and their subtrees), rebuilds replace them
/// wholesale.
fn same_shape(spec: &NodeSpec, entity: Entity, tree: &Tree) -> bool {
    let Ok((children, has_text, _, has_element, has_image, _is_ui)) = tree.get(entity) else {
        return false;
    };
    let owned: Vec<Entity> = children
        .map_or(&[][..], |children| children)
        .iter()
        .copied()
        .filter(|child| !tree.get(*child).is_ok_and(|(.., nested_ui)| nested_ui))
        .collect();
    if has_text != spec.text.is_some()
        || has_element != spec.element.is_some()
        || has_image != spec.image.is_some()
    {
        return false;
    }
    match &spec.text {
        Some(text) => {
            owned.len() == text.spans.len()
                && owned
                    .iter()
                    .all(|child| tree.get(*child).is_ok_and(|(_, _, has_span, ..)| has_span))
        }
        None => {
            owned.len() == spec.children.len()
                && owned
                    .iter()
                    .zip(&spec.children)
                    .all(|(child, spec)| same_shape(spec, *child, tree))
        }
    }
}

/// Restyles `entity` (checked by [`same_shape`]) to `spec` in place: the
/// entity and its children — and anything the app attached — stay.
fn apply_spec(commands: &mut Commands, entity: Entity, spec: NodeSpec, tree: &Tree) {
    let children: Vec<Entity> = tree
        .get(entity)
        .ok()
        .and_then(|(children, ..)| children)
        .map_or_else(Vec::new, |children| children.to_vec());
    let mut target = commands.entity(entity);
    target.insert(spec.node);
    if let Some(element) = spec.element {
        target.insert(element);
    }
    match spec.background {
        Some(background) => target.insert(BackgroundColor(background)),
        None => target.remove::<BackgroundColor>(),
    };
    if let Some(image) = spec.image {
        target.insert(image);
    }
    match spec.text {
        Some(text) => {
            // `TextLayout` is required by `Text`: replace, never remove.
            let layout = if text.no_wrap { TextLayout::no_wrap() } else { TextLayout::default() };
            target.insert((Text::new(text.prefix), text.font, TextColor(text.color), layout));
            for (span, (content, font, color)) in children.into_iter().zip(text.spans) {
                commands
                    .entity(span)
                    .insert((TextSpan::new(content), font, TextColor(color)));
            }
        }
        None => {
            for (child, spec) in children.into_iter().zip(spec.children) {
                apply_spec(commands, child, spec, tree);
            }
        }
    }
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
    /// Translations by node of `parser`'s document (empty inside a
    /// translation: its markup isn't localized again).
    localized: &'a HashMap<tl::NodeHandle, Result<String, String>>,
    /// The elements' `:hover`/`:active` state by DOM node (`None` inside a
    /// translation: fragment handles index the fragment, not the document).
    states: Option<&'a HashMap<tl::NodeHandle, Pseudo>>,
    styler: &'a Styler<'a>,
    /// Inside a translation: the source element's `data-l10n-name`
    /// descendants, each usable once (fluent-dom's named overlays).
    named: Option<&'a RefCell<Vec<(String, HtmlElement)>>>,
}

impl Ctx<'_, '_, '_> {
    /// The element `tag` is styled as. In a translation, a `data-l10n-name`
    /// element takes the source element of that name (tag, id, classes);
    /// `None` when there is no unused one of the same tag — its content is
    /// then plain text, as in fluent-dom.
    fn element(&self, tag: &tl::HTMLTag) -> Option<HtmlElement> {
        let element = element_of(tag);
        let (Some(named), Some(name)) = (self.named, tag.attributes().get("data-l10n-name").flatten())
        else {
            return Some(element);
        };
        let name = decode_entities(&name.as_utf8_str());
        let mut named = named.borrow_mut();
        let index = named.iter().position(|(source, _)| *source == name)?;
        if named[index].1.tag != element.tag {
            return None;
        }
        Some(named.remove(index).1)
    }
}

/// Walks the markup of `source`'s translation with `walk` (a fragment
/// context with `source`'s named elements). `false` when `source` has no
/// translation, or it doesn't parse: the caller walks the own content.
fn walk_translation(
    ctx: &Ctx,
    handle: tl::NodeHandle,
    source: &tl::HTMLTag,
    walk: impl FnOnce(&Ctx, &[tl::NodeHandle]),
) -> bool {
    let Some(Ok(translation)) = ctx.localized.get(&handle) else {
        return false;
    };
    let Ok(fragment) = tl::parse(translation, tl::ParserOptions::default()) else {
        return false;
    };
    let named: Vec<(String, HtmlElement)> = source
        .children()
        .all(ctx.parser)
        .iter()
        .filter_map(|node| {
            let tag = node.as_tag()?;
            let name = tag.attributes().get("data-l10n-name").flatten()?;
            Some((decode_entities(&name.as_utf8_str()), element_of(tag)))
        })
        .collect();
    let named = RefCell::new(named);
    // Fragment handles index the fragment, not the source document.
    let unlocalized = HashMap::default();
    let fragment_ctx = Ctx {
        parser: fragment.parser(),
        localized: &unlocalized,
        states: None,
        styler: ctx.styler,
        named: Some(&named),
    };
    walk(&fragment_ctx, fragment.children());
    true
}

fn collect_items(
    dom: &tl::VDom,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    states: Option<&HashMap<tl::NodeHandle, Pseudo>>,
    styler: &Styler,
    root: Style,
) -> Vec<Item> {
    let ctx = Ctx {
        parser: dom.parser(),
        localized,
        states,
        styler,
        named: None,
    };
    let mut items = Vec::new();
    for handle in dom.children() {
        collect_node(&ctx, *handle, root, &mut items);
    }
    items
}

fn collect_node(ctx: &Ctx, handle: tl::NodeHandle, inherited: Style, items: &mut Vec<Item>) {
    let Some(node) = handle.get(ctx.parser) else {
        return;
    };
    let tag = match node {
        tl::Node::Tag(tag) => tag,
        tl::Node::Raw(_) => {
            // Anonymous block: inherits the container's style.
            let mut runs = Vec::new();
            push_runs(ctx, handle, inherited, &mut runs);
            let runs = collapse_runs(runs);
            if !runs.is_empty() {
                items.push(Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    handle: None,
                    signals: Vec::new(),
                    style: inherited,
                    boxed: BoxStyle::default(),
                    runs,
                }));
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let signals = signals::signal_bindings(tag);
    let state = ctx.states.and_then(|states| states.get(&handle)).copied().unwrap_or_default();
    let Some(element) = ctx.element(tag) else {
        // Unmatched `data-l10n-name` in a translation: content only.
        for child in tag.children().top().iter() {
            collect_node(ctx, *child, inherited, items);
        }
        return;
    };
    let style = ctx.styler.style_of(&element, inherited, state);
    let boxed = ctx.styler.box_of(&element, state);
    let kind = match element.tag.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => BlockKind::Heading,
        "p" => BlockKind::Paragraph,
        "li" => BlockKind::ListItem,
        "pre" => BlockKind::Preformatted,
        "head" | "script" | "style" => return,
        // Containers hold their children's nodes; other elements outside a
        // block (`html`, `body`, unknown tags) are walked through, no node.
        // Either way a translation replaces the children.
        name => {
            let container = CONTAINERS.contains(&name);
            if !container && !signals.is_empty() {
                debug!(
                    "html signals: <{name}> has data-on-* hooks, but only blocks and \
                     containers become nodes; skipped"
                );
            }
            let mut children = Vec::new();
            let target = if container { &mut children } else { &mut *items };
            let translated = walk_translation(ctx, handle, tag, |fragment, nodes| {
                for node in nodes {
                    collect_node(fragment, *node, style, target);
                }
            });
            if !translated {
                for child in tag.children().top().iter() {
                    collect_node(ctx, *child, style, target);
                }
            }
            if container {
                items.push(Item::Container {
                    element,
                    handle: Some(handle),
                    signals,
                    boxed,
                    children,
                });
            }
            return;
        }
    };
    // A missing translation falls back to the element's own content.
    let preformatted = matches!(kind, BlockKind::Preformatted);
    let mut runs = Vec::new();
    push_content_runs(ctx, handle, tag, style, &mut runs);
    items.push(Item::Block(Block {
        kind,
        element: Some(element),
        handle: Some(handle),
        signals,
        style,
        boxed,
        runs: finish_runs(runs, preformatted),
    }));
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

/// Runs for `tag`'s content: its translation if it has one, else its
/// children. Translations are markup (fluent-dom style overlays): inline
/// elements in them are styled by the stylesheet like elements in the
/// document; entities are decoded.
fn push_content_runs(
    ctx: &Ctx,
    handle: tl::NodeHandle,
    tag: &tl::HTMLTag,
    style: Style,
    runs: &mut Vec<Run>,
) {
    let translated = walk_translation(ctx, handle, tag, |fragment, nodes| {
        for node in nodes {
            push_runs(fragment, *node, style, runs);
        }
    });
    if !translated {
        for child in tag.children().top().iter() {
            push_runs(ctx, *child, style, runs);
        }
    }
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
        let trimmed = last.text.trim_end_matches(is_html_whitespace).len();
        last.text.truncate(trimmed);
    }
    runs.retain(|run| !run.text.is_empty());
    runs
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
            // An unmatched `data-l10n-name` element is plain text.
            let style = ctx
                .element(tag)
                .map_or(style, |element| {
                    ctx.styler.style_of(&element, style, Pseudo::default())
                });
            push_content_runs(ctx, handle, tag, style, runs);
        }
        Some(tl::Node::Comment(_)) | None => {}
    }
}

/// HTML whitespace (space, tab, LF, FF, CR): what collapsing and trimming
/// touch. NBSP, U+3000 and other Unicode spaces are content.
fn is_html_whitespace(c: char) -> bool {
    c.is_ascii_whitespace()
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
            if is_html_whitespace(c) {
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
        let trimmed = last.text.trim_end_matches(is_html_whitespace).len();
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            out.pop();
        }
    }
    out
}
