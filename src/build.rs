//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use std::cell::RefCell;

use bevy::asset::{AssetEvent, LoadState};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use crate::cascade::{HtmlStyles, LayoutDecl, OutlineDecl, Pseudo, SliceValue};
use crate::custom_elements::{self, CustomElement, ElementConnected};
use crate::focus::{self, Focusable};
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
    /// `pointer-events: auto` (`false` = `none`); inherited, as in CSS.
    pub(crate) pointer_events: bool,
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
    /// Focusability from `tabindex` / `data-on-click` / `autofocus`.
    focus: Option<Focusable>,
    /// The `is` definition to run on the spawned element.
    custom: Option<CustomElement>,
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
        /// Focusability from `tabindex` / `data-on-click` / `autofocus`.
        focus: Option<Focusable>,
        /// The `is` definition to run on the spawned element.
        custom: Option<CustomElement>,
        /// Box properties, computed with the element's interaction state.
        boxed: BoxStyle,
        /// The element's computed `pointer-events` (inherited).
        pointer_events: bool,
        children: Vec<Item>,
    },
}

/// Resolved box properties of an element: `border-width`, `padding`,
/// `background-color`, `border-image`, plus layout (flex, sizes, margins).
#[derive(Default)]
pub(crate) struct BoxStyle {
    pub(crate) border: [Option<f32>; 4],
    pub(crate) border_color: [Option<Color>; 4],
    pub(crate) padding: [Option<f32>; 4],
    pub(crate) background: Option<Color>,
    pub(crate) image: Option<(Handle<Image>, TextureSlicer)>,
    /// `row-gap` for containers (not part of `is_empty`: blocks ignore it).
    pub(crate) row_gap: Option<f32>,
    /// `z-index` (`None` for `auto`/undeclared).
    pub(crate) z_index: Option<i32>,
    /// Declared `outline`; resolved against the text color by
    /// [`BoxStyle::outline`].
    pub(crate) outline: OutlineDecl,
    /// The `outline` to draw, resolved with the element's text color while
    /// collecting (`collect_node`).
    pub(crate) drawn_outline: Option<Outline>,
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

    /// The `outline` to draw, if its style is a visible one. `currentColor`
    /// (and an undeclared color) is the element's text `color`.
    fn outline(&self, text_color: Color) -> Option<Outline> {
        let decl = self.outline;
        if decl.visible != Some(true) {
            return None;
        }
        let width = decl.width.unwrap_or(3.0);
        (width > 0.0).then(|| Outline {
            width: Val::Px(width),
            offset: Val::Px(decl.offset.unwrap_or(0.0)),
            color: decl.color.flatten().unwrap_or(text_color),
        })
    }

    /// The declared border colors (undeclared sides transparent), if any.
    fn border_color(&self) -> Option<BorderColor> {
        if self.border_color.iter().all(Option::is_none) {
            return None;
        }
        let side = |color: Option<Color>| color.unwrap_or(Color::NONE);
        Some(BorderColor {
            top: side(self.border_color[0]),
            right: side(self.border_color[1]),
            bottom: side(self.border_color[2]),
            left: side(self.border_color[3]),
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

/// What the root rule (the `html` rule, or the document's own `<html>`
/// element with its `id`/`class`) set on an `HtmlUi` entity, and the app's
/// values it replaced, so a later stylesheet or document that stops
/// declaring something gives it back. Fields and components the rule never
/// declared stay the app's (an app may move its UI every frame).
#[derive(Component)]
pub(crate) struct CssRoot {
    /// The `Node` before CSS: the declared fields hold the app's values.
    base: Node,
    layout: LayoutDecl,
    border: [Option<f32>; 4],
    padding: [Option<f32>; 4],
    row_gap: Option<f32>,
    owned: RootOwned,
}

/// Components the root rule set: `Some(app's)` while CSS owns one (`None`
/// inside = the app had none, so giving it back removes it).
#[derive(Default)]
struct RootOwned {
    background: Option<Option<BackgroundColor>>,
    border_color: Option<Option<BorderColor>>,
    z_index: Option<Option<ZIndex>>,
    pickable: Option<Option<Pickable>>,
    image: Option<Option<ImageNode>>,
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
            size: declared.font_size.map_or(inherited.size, |size| {
                size.resolve(inherited.size, self.root_size)
            }),
            bold: declared.bold.unwrap_or(inherited.bold),
            italic: declared.italic.unwrap_or(inherited.italic),
            pointer_events: declared.pointer_events.unwrap_or(inherited.pointer_events),
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
            border_color: declared.border_color,
            padding: declared.padding,
            background: declared.background,
            image,
            row_gap: declared.row_gap,
            z_index: declared.z_index.flatten(),
            outline: declared.outline,
            drawn_outline: None,
            layout: declared.layout.clone(),
        }
    }
}

/// The root rule's values (`root`: the document's `<html>` element, or a
/// bare `html` for fragments without one), over the defaults.
pub(crate) fn root_style(
    styles: &HtmlStyles,
    fonts: &FontFamilies,
    images: &Assets<Image>,
    root: &HtmlElement,
) -> Style {
    let defaults = Style {
        color: DEFAULT_COLOR,
        family: None,
        size: DEFAULT_FONT_SIZE,
        bold: false,
        italic: false,
        pointer_events: true,
    };
    Styler {
        styles,
        fonts,
        root_size: DEFAULT_FONT_SIZE,
        sheet: None,
        images,
    }
    .style_of(root, defaults, Pseudo::default())
}

/// The element the `HtmlUi` node itself is styled as: the document's
/// top-level `<html>` (with its `id`/`class`), or a bare `html` for
/// fragments.
pub(crate) fn root_element(dom: &tl::VDom) -> HtmlElement {
    dom.children()
        .iter()
        .filter_map(|handle| handle.get(dom.parser())?.as_tag())
        .find(|tag| tag.name().as_utf8_str().eq_ignore_ascii_case("html"))
        .map_or_else(|| element_tag("html"), element_of)
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
    roots: Query<(&Node, Option<&CssRoot>), With<HtmlUi>>,
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
                || own_sheet
                    .as_ref()
                    .is_some_and(|own| own.is_changed() || refreshed(&own.0)),
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
        let root_element = match (&*rendered, &outline) {
            (RenderedHtml::Ready(document), None) => root_element(document.dom()),
            _ => element_tag("html"),
        };
        let root = root_style(&styles, &fonts, &images, &root_element);
        let styler = Styler {
            styles: &styles,
            fonts: &fonts,
            root_size: root.size,
            sheet: css,
            images: &images,
        };

        let root_box = styler.box_of(&root_element, Pseudo::default());
        // Nested containers space their children like the root does.
        let default_gap = match root_box.row_gap {
            Some(gap) => Val::Px(gap),
            None => roots.get(entity).map_or(Val::Auto, |(node, state)| {
                match state.filter(|state| state.row_gap.is_some()) {
                    Some(state) => state.base.row_gap,
                    None => node.row_gap,
                }
            }),
        };
        let root_pointer_events = root.pointer_events;
        commands
            .entity(entity)
            .queue(move |mut entity: EntityWorldMut| {
                apply_root(&mut entity, root_box, root_pointer_events);
            });

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
                        focus: state.focused,
                        focus_visible: state.focus_visible,
                    }),
                );
            }
            let nested_root =
                current != entity && tree.get(current).is_ok_and(|(.., nested_ui)| nested_ui);
            if !nested_root && let Ok((children, ..)) = tree.get(current) {
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
                focus: None,
                custom: None,
                style,
                boxed: BoxStyle::default(),
                runs: vec![Run { text, style }],
            })]
        } else {
            match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => {
                    collect_items(document.dom(), &localized.0, Some(&states), &styler, root)
                }
                RenderedHtml::Failed(message) => vec![Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    handle: None,
                    signals: Vec::new(),
                    focus: None,
                    custom: None,
                    style: root,
                    boxed: BoxStyle::default(),
                    runs: vec![Run {
                        text: format!("failed to render: {message}"),
                        style: root,
                    }],
                })],
            }
        };

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
            && children
                .iter()
                .zip(&specs)
                .all(|(child, spec)| same_shape(spec, *child, &tree))
        {
            for (child, spec) in children.into_iter().zip(specs) {
                apply_spec(&mut commands, child, spec, &tree);
            }
            commands.trigger(HtmlUiRestyled { entity });
        } else {
            let mut connected = Vec::new();
            commands
                .entity(entity)
                .despawn_related::<Children>()
                .with_children(|parent| {
                    for spec in specs {
                        spawn_spec(parent, spec, entity, &mut connected);
                    }
                });
            // After the spawns, before `HtmlUiBuilt`: its observers see what
            // the definitions attached.
            for element in connected {
                commands.queue(move |world: &mut World| custom_elements::connect(world, element));
            }
            commands
                .entity(entity)
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
    /// Focusability from `tabindex` / `data-on-click` / `autofocus`.
    focus: Option<Focusable>,
    /// The `is` definition to run once spawned (not on restyles).
    custom: Option<CustomElement>,
    /// The DOM node, for `:hover`/`:active` restyles.
    handle: Option<tl::NodeHandle>,
    background: Option<Color>,
    image: Option<ImageNode>,
    border_color: Option<BorderColor>,
    z_index: Option<ZIndex>,
    outline: Option<Outline>,
    /// `false` for `pointer-events: none` (`Pickable::IGNORE`).
    pickable: bool,
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
            focus: None,
            custom: None,
            handle: None,
            background: None,
            image: None,
            border_color: None,
            z_index: None,
            outline: None,
            pickable: true,
            text: None,
            children: Vec::new(),
        }
    }
}

/// Which of an element's components its stylesheet set (and may therefore
/// take back on a restyle): `BorderColor`, `ZIndex`, `Outline` and `Pickable` are also
/// things an app sets itself, so a restyle without the declaration resets
/// only what CSS set. `BorderColor`/`ZIndex` are `Node`'s required
/// components: taking them back means resetting them to their defaults.
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CssOwned {
    border_color: bool,
    z_index: bool,
    outline: bool,
    pickable: bool,
}

/// Sets a node's CSS-owned components; on a restyle, resets the ones CSS set
/// earlier and no longer declares.
fn apply_css_owned(
    target: &mut EntityCommands,
    border_color: Option<BorderColor>,
    z_index: Option<ZIndex>,
    outline: Option<Outline>,
    pickable: bool,
) {
    let owned = CssOwned {
        border_color: border_color.is_some(),
        z_index: z_index.is_some(),
        outline: outline.is_some(),
        pickable: !pickable,
    };
    target.queue(move |mut entity: EntityWorldMut| {
        let before = entity.get::<CssOwned>().copied().unwrap_or_default();
        match border_color {
            Some(color) => {
                entity.insert(color);
            }
            None if before.border_color => {
                entity.insert(BorderColor::DEFAULT);
            }
            None => {}
        }
        match z_index {
            Some(z) => {
                entity.insert(z);
            }
            None if before.z_index => {
                entity.insert(ZIndex::default());
            }
            None => {}
        }
        match outline {
            Some(outline) => {
                entity.insert(outline);
            }
            None if before.outline => {
                entity.remove::<Outline>();
            }
            None => {}
        }
        if owned.pickable {
            entity.insert(Pickable::IGNORE);
        } else if before.pickable {
            entity.remove::<Pickable>();
        }
        if owned == CssOwned::default() {
            entity.remove::<CssOwned>();
        } else {
            entity.insert(owned);
        }
    });
}

/// Marks an element whose `ImageNode` is its CSS `border-image` frame, i.e.
/// owned by the pipeline. [`same_shape`] compares this marker, not
/// `ImageNode` itself, so an `ImageNode` the app inserted on a built element
/// (an icon, say) is app state like any other component: restyles keep it
/// instead of rebuilding.
#[derive(Component)]
pub(crate) struct CssFrame;

/// The existing children's structure, for [`same_shape`] and [`apply_spec`].
type Tree<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Children>,
        Has<Text>,
        Has<TextSpan>,
        Has<HtmlElement>,
        Has<CssFrame>,
        Has<HtmlUi>,
    ),
>;

fn item_spec(styler: &Styler, item: Item, default_gap: Val) -> NodeSpec {
    let (element, handle, signals, focus, custom, boxed, pointer_events, children) = match item {
        Item::Block(block) => return block_spec(styler, block),
        Item::Container {
            element,
            handle,
            signals,
            focus,
            custom,
            boxed,
            pointer_events,
            children,
        } => (
            element,
            handle,
            signals,
            focus,
            custom,
            boxed,
            pointer_events,
            children,
        ),
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
        focus,
        custom,
        handle,
        background: boxed.background,
        image: boxed.sliced_image(),
        border_color: boxed.border_color(),
        outline: boxed.drawn_outline,
        z_index: boxed.z_index.map(ZIndex),
        pickable: pointer_events,
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
            focus: block.focus,
            custom: block.custom,
            handle: block.handle,
            border_color: boxed.border_color(),
            outline: boxed.drawn_outline,
            z_index: boxed.z_index.map(ZIndex),
            pickable: block.style.pointer_events,
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
        focus: block.focus,
        custom: block.custom,
        handle: block.handle,
        background: boxed.background,
        image: boxed.sliced_image(),
        border_color: boxed.border_color(),
        outline: boxed.drawn_outline,
        z_index: boxed.z_index.map(ZIndex),
        pickable: block.style.pointer_events,
        children: vec![NodeSpec {
            pickable: block.style.pointer_events,
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

/// Spawns `spec` under `parent`, collecting its custom elements (in
/// document order) for [`custom_elements::connect`].
fn spawn_spec(
    parent: &mut ChildSpawnerCommands,
    spec: NodeSpec,
    root: Entity,
    connected: &mut Vec<ElementConnected>,
) {
    let mut entity = parent.spawn(spec.node);
    if let Some(custom) = spec.custom {
        connected.push(ElementConnected {
            entity: entity.id(),
            root,
            name: custom.name,
            dataset: custom.dataset,
        });
    }
    if let Some(element) = spec.element {
        entity.insert(element);
    }
    if let Some(handle) = spec.handle {
        entity.insert(DomNode(handle));
    }
    if let Some(focusable) = spec.focus {
        entity.insert(focusable);
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
        entity.insert((image, CssFrame));
    }
    apply_css_owned(
        &mut entity,
        spec.border_color,
        spec.z_index,
        spec.outline,
        spec.pickable,
    );
    match spec.text {
        Some(text) => {
            entity.insert((Text::new(text.prefix), text.font, TextColor(text.color)));
            if text.no_wrap {
                entity.insert(TextLayout::no_wrap());
            }
            // bevy_picking resolves text-section hits against the span
            // entity, so the block's `pointer-events: none` must be on the
            // spans too (or the ignored block stays clickable).
            entity.with_children(|spans| {
                for (span, font, color) in text.spans {
                    let mut span = spans.spawn((TextSpan::new(span), font, TextColor(color)));
                    if !spec.pickable {
                        span.insert(Pickable::IGNORE);
                    }
                }
            });
        }
        None => {
            entity.with_children(|children| {
                for child in spec.children {
                    spawn_spec(children, child, root, connected);
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
    let Ok((children, has_text, _, has_element, has_frame, _is_ui)) = tree.get(entity) else {
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
        || has_frame != spec.image.is_some()
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
        target.insert((image, CssFrame));
    }
    apply_css_owned(
        &mut target,
        spec.border_color,
        spec.z_index,
        spec.outline,
        spec.pickable,
    );
    match spec.text {
        Some(text) => {
            // `TextLayout` is required by `Text`: replace, never remove.
            let layout = if text.no_wrap {
                TextLayout::no_wrap()
            } else {
                TextLayout::default()
            };
            target.insert((
                Text::new(text.prefix),
                text.font,
                TextColor(text.color),
                layout,
            ));
            // Spans carry the block's `pointer-events: none` (see
            // `spawn_spec`); a restyle without it takes it back.
            for (span, (content, font, color)) in children.into_iter().zip(text.spans) {
                let mut span = commands.entity(span);
                span.insert((TextSpan::new(content), font, TextColor(color)));
                if !spec.pickable {
                    span.insert(Pickable::IGNORE);
                } else {
                    span.remove::<Pickable>();
                }
            }
        }
        None => {
            for (child, spec) in children.into_iter().zip(spec.children) {
                apply_spec(commands, child, spec, tree);
            }
        }
    }
}

/// Applies the root rule to the `HtmlUi` entity itself: its box
/// (`border-image`, `border-width`, `padding`, `gap`), layout (flex/grid,
/// sizes, margins, `position` and insets, `border-radius`) on the `Node`,
/// and `background-color`, `border-color`, `z-index` (`ZIndex`: among
/// sibling roots too, as Bevy sorts roots by `GlobalZIndex` then `ZIndex`)
/// and `pointer-events: none` as components. What it no longer declares goes
/// back to the app's value ([`CssRoot`]).
fn apply_root(entity: &mut EntityWorldMut, boxed: BoxStyle, pointer_events: bool) {
    let state = entity.take::<CssRoot>();
    let Some(current) = entity.get::<Node>().cloned() else {
        return;
    };
    // Back to the app's values, then the new declarations over them.
    let mut base = current.clone();
    if let Some(state) = &state {
        state.layout.restore_from(&state.base, &mut base);
        restore_sides(&mut base.border, state.base.border, state.border);
        restore_sides(&mut base.padding, state.base.padding, state.padding);
        if state.row_gap.is_some() {
            base.row_gap = state.base.row_gap;
        }
    }
    let mut node = base.clone();
    boxed.layout.apply_to(&mut node);
    node.border = rect_over(node.border, boxed.border);
    node.padding = rect_over(node.padding, boxed.padding);
    if let Some(gap) = boxed.row_gap {
        node.row_gap = Val::Px(gap);
    }
    if node != current {
        entity.insert(node);
    }

    let mut owned = state.map(|state| state.owned).unwrap_or_default();
    claim(
        entity,
        boxed.background.map(BackgroundColor),
        &mut owned.background,
    );
    claim(entity, boxed.border_color(), &mut owned.border_color);
    claim(entity, boxed.z_index.map(ZIndex), &mut owned.z_index);
    claim(
        entity,
        (!pointer_events).then_some(Pickable::IGNORE),
        &mut owned.pickable,
    );
    claim(entity, boxed.sliced_image(), &mut owned.image);

    let declares_node = boxed
        .border
        .iter()
        .chain(&boxed.padding)
        .any(Option::is_some)
        || boxed.row_gap.is_some()
        || boxed.layout != LayoutDecl::default();
    let owns_component = owned.background.is_some()
        || owned.border_color.is_some()
        || owned.z_index.is_some()
        || owned.pickable.is_some()
        || owned.image.is_some();
    if declares_node || owns_component {
        entity.insert(CssRoot {
            base,
            layout: boxed.layout,
            border: boxed.border,
            padding: boxed.padding,
            row_gap: boxed.row_gap,
            owned,
        });
    }
}

/// Undoes [`rect_over`]: the `declared` sides of `rect` back to `base`'s.
fn restore_sides(rect: &mut UiRect, base: UiRect, declared: [Option<f32>; 4]) {
    let sides = [
        (&mut rect.top, base.top),
        (&mut rect.right, base.right),
        (&mut rect.bottom, base.bottom),
        (&mut rect.left, base.left),
    ];
    for ((side, base), declared) in sides.into_iter().zip(declared) {
        if declared.is_some() {
            *side = base;
        }
    }
}

/// Sets `css` on `entity` while declared, remembering the app's value in
/// `owned` the first time; when no longer declared, gives the app's value
/// back (or removes the component if the app had none).
fn claim<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(
    entity: &mut EntityWorldMut,
    css: Option<C>,
    owned: &mut Option<Option<C>>,
) {
    match css {
        Some(value) => {
            if owned.is_none() {
                *owned = Some(entity.get::<C>().cloned());
            }
            entity.insert(value);
        }
        None => match owned.take() {
            Some(Some(app)) => {
                entity.insert(app);
            }
            Some(None) => {
                entity.remove::<C>();
            }
            None => {}
        },
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
        let (Some(named), Some(name)) =
            (self.named, tag.attributes().get("data-l10n-name").flatten())
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
                    focus: None,
                    custom: None,
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
    let focus = focus::focusable(tag, &signals);
    let custom = custom_elements::custom_element(tag);
    let state = ctx
        .states
        .and_then(|states| states.get(&handle))
        .copied()
        .unwrap_or_default();
    let Some(element) = ctx.element(tag) else {
        // Unmatched `data-l10n-name` in a translation: content only.
        for child in tag.children().top().iter() {
            collect_node(ctx, *child, inherited, items);
        }
        return;
    };
    let style = ctx.styler.style_of(&element, inherited, state);
    let mut boxed = ctx.styler.box_of(&element, state);
    boxed.drawn_outline = boxed.outline(style.color);
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
            if !container && custom.is_some() {
                debug!(
                    "html custom elements: <{name}> has an is attribute, but only blocks \
                     and containers become nodes; skipped"
                );
            }
            let mut children = Vec::new();
            let target = if container {
                &mut children
            } else {
                &mut *items
            };
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
                    focus,
                    custom,
                    boxed,
                    pointer_events: style.pointer_events,
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
        focus,
        custom,
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
pub(crate) fn element_tag(tag: &str) -> HtmlElement {
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
            let style = ctx.element(tag).map_or(style, |element| {
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
