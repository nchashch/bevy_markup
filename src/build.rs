//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use std::cell::RefCell;

use bevy::asset::{AssetEvent, LoadState};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::cascade::{HtmlStyles, LayoutDecl, SliceValue};
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
    /// The block element's own computed style (bullet; root `Text` font).
    style: Style,
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
    pub(crate) fn style_of(&self, element: &HtmlElement, inherited: Style) -> Style {
        let declared = self.styles.get(element);
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
        TextFont::default()
            .with_font(font)
            .with_font_size(style.size)
    }

    /// `element`'s box properties. A `border-image` whose `%` slices need the
    /// image size is skipped until the image has loaded (its load rebuilds).
    pub(crate) fn box_of(&self, element: &HtmlElement) -> BoxStyle {
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
            row_gap: declared.row_gap,
            layout: declared.layout.clone(),
        }
    }
}

/// Stylesheet-load failures already handled, per `HtmlUi` entity, so each
/// failure is rebuilt around exactly once (asset failures emit no asset
/// event). Entries are cleared when the entity successfully styles with a
/// ready sheet or when that sheet reloads, so re-selecting a broken sheet is
/// a fresh failure.
#[derive(Resource, Default)]
pub(crate) struct FailedSheets(HashSet<(Entity, AssetId<Stylesheet>)>);

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
    .style_of(&element_tag("html"), defaults)
}

pub(crate) fn build_html_ui(
    mut commands: Commands,
    mut sheet_events: MessageReader<AssetEvent<Stylesheet>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    sheets: Res<Assets<Stylesheet>>,
    server: Res<AssetServer>,
    mut failed_sheets: ResMut<FailedSheets>,
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
    mut removed_outlines: RemovedComponents<HtmlDebugOutline>,
    mut removed_sheets: RemovedComponents<HtmlStylesheet>,
    // Entities whose change arrived while their stylesheet was loading.
    mut deferred: Local<HashSet<Entity>>,
) {
    // Removals aren't `Ref` changes; the outline must still un-stick, and a
    // removed override falls back to the default stylesheet.
    let removed: HashSet<Entity> = removed_outlines.read().chain(removed_sheets.read()).collect();
    let reloaded_sheets: HashSet<AssetId<Stylesheet>> = sheet_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => {
                // Reloaded after a failure: let a later failure latch again.
                failed_sheets.0.retain(|(_, sheet)| sheet != id);
                Some(*id)
            }
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
    deferred.retain(|entity| views.contains(*entity));

    for (entity, rendered, localized, own_sheet, outline) in &views {
        // A stylesheet that failed to load is treated as absent — a broken
        // CSS file renders unstyled instead of blocking the UI forever —
        // while one that is still loading defers the build to its load
        // event. A failed per-entity override falls back to the default.
        enum Sheet<'a> {
            Ready(Option<&'a Stylesheet>),
            Loading,
            Failed,
        }
        let state = |handle: &Handle<Stylesheet>| match sheets.get(handle) {
            Some(sheet) => Sheet::Ready(Some(sheet)),
            None if matches!(server.load_state(handle.id()), LoadState::Failed(_)) => Sheet::Failed,
            None => Sheet::Loading,
        };
        // Change signals, gathered before a still-loading sheet defers the
        // build: re-requesting an already failed sheet reads as `Loading` for
        // a frame, and a signal seen only in that frame must not be lost.
        let own_ready = own_sheet.as_ref().is_some_and(|own| sheets.contains(&own.0));
        // The default stylesheet applies unless a ready override replaces it.
        let default_applies = !own_ready;
        let reloaded = |handle: &Handle<Stylesheet>| reloaded_sheets.contains(&handle.id());
        let changed = rendered.is_changed()
            || localized.is_changed()
            || own_sheet.as_ref().is_some_and(|own| own.is_changed() || reloaded(&own.0))
            || (default_applies
                && (default_sheet.is_changed() || default_sheet.0.as_ref().is_some_and(reloaded)))
            || removed.contains(&entity)
            || fonts.is_changed()
            || outline.as_ref().is_some_and(|outline| outline.is_changed());
        let resolved = match (&own_sheet, &default_sheet.0) {
            (Some(own), _) => match state(&own.0) {
                Sheet::Ready(css) => Some(css),
                Sheet::Loading => None,
                Sheet::Failed => match default_sheet.0.as_ref().map(state) {
                    Some(Sheet::Ready(css)) => Some(css),
                    Some(Sheet::Loading) => None,
                    _ => Some(None),
                },
            },
            (None, Some(default)) => match state(default) {
                Sheet::Ready(css) => Some(css),
                Sheet::Loading => None,
                Sheet::Failed => Some(None),
            },
            (None, None) => Some(None),
        };
        let Some(css) = resolved else {
            // Still loading: its load event (or failure) builds; keep the
            // signal for then.
            if changed {
                deferred.insert(entity);
            }
            continue;
        };
        let was_deferred = deferred.remove(&entity);
        let sheet_handle = match &own_sheet {
            Some(own) => Some(&own.0),
            None => default_sheet.0.as_ref(),
        };
        // Failures emit no asset event, so latch them to rebuild once. The
        // latch is per entity: re-selecting a failed sheet later is a new
        // user action, even if this entity failed on it before.
        let failed_handle = sheet_handle
            .filter(|handle| !sheets.contains(*handle))
            .filter(|handle| matches!(state(handle), Sheet::Failed));
        let newly_failed = failed_handle
            .is_some_and(|handle| !failed_sheets.0.contains(&(entity, handle.id())));
        let images_loaded = css.is_some_and(|css| {
            css.images()
                .iter()
                .any(|image| loaded_images.contains(&image.id()))
        });
        if !(changed || was_deferred || newly_failed || images_loaded) {
            continue;
        }
        if let Some(handle) = failed_handle {
            failed_sheets.0.insert((entity, handle.id()));
        } else if css.is_some() {
            // Styled with a ready sheet: any earlier failure of *other*
            // sheets no longer applies to this entity.
            failed_sheets.0.retain(|(owner, _)| *owner != entity);
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
            let style = styler.style_of(&element_tag("pre"), root);
            vec![Item::Block(Block {
                kind: BlockKind::Paragraph,
                element: None,
                style,
                runs: vec![Run { text, style }],
            })]
        } else {
            match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    collect_items(document.dom(), &localized.0, &styler, root)
                }
                RenderedHtml::Failed(message) => vec![Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    style: root,
                    runs: vec![Run {
                        text: format!("failed to render: {message}"),
                        style: root,
                    }],
                })],
            }
        };

        // Nested containers space their children like the root does.
        let default_gap = roots.get(entity).map_or(Val::Auto, |(node, _)| node.row_gap);
        commands
            .entity(entity)
            .despawn_related::<Children>()
            .with_children(|parent| {
                for item in items {
                    spawn_item(parent, &styler, item, default_gap);
                }
            })
            .trigger(|entity| HtmlUiBuilt { entity });
    }
}

fn spawn_item(parent: &mut ChildSpawnerCommands, styler: &Styler, item: Item, default_gap: Val) {
    let (element, children) = match item {
        Item::Block(block) => return spawn_block(parent, styler, block),
        Item::Container { element, children } => (element, children),
    };
    let boxed = styler.box_of(&element);
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
    let mut container = parent.spawn((node, element));
    if let Some(background) = boxed.background {
        container.insert(BackgroundColor(background));
    }
    if let Some(image) = boxed.sliced_image() {
        container.insert(image);
    }
    container.with_children(|container| {
        for child in children {
            spawn_item(container, styler, child, default_gap);
        }
    });
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
        let mut node = Node {
            margin: indent,
            padding,
            ..text_node
        };
        boxed.layout.apply_to(&mut node);
        let mut entity = parent.spawn((text, node));
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
    let mut node = Node {
        flex_shrink: 0.0,
        box_sizing: BoxSizing::ContentBox,
        margin: indent,
        border: rect_over(UiRect::DEFAULT, boxed.border),
        padding,
        ..default()
    };
    boxed.layout.apply_to(&mut node);
    let mut wrapper = parent.spawn(node);
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
    /// Translations by node of `parser`'s document (empty inside a
    /// translation: its markup isn't localized again).
    localized: &'a HashMap<tl::NodeHandle, Result<String, String>>,
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
        styler: ctx.styler,
        named: Some(&named),
    };
    walk(&fragment_ctx, fragment.children());
    true
}

fn collect_items(
    dom: &tl::VDom,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    styler: &Styler,
    root: Style,
) -> Vec<Item> {
    let ctx = Ctx {
        parser: dom.parser(),
        localized,
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
                    style: inherited,
                    runs,
                }));
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let Some(element) = ctx.element(tag) else {
        // Unmatched `data-l10n-name` in a translation: content only.
        for child in tag.children().top().iter() {
            collect_node(ctx, *child, inherited, items);
        }
        return;
    };
    let style = ctx.styler.style_of(&element, inherited);
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
                items.push(Item::Container { element, children });
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
        style,
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
                .map_or(style, |element| ctx.styler.style_of(&element, style));
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
