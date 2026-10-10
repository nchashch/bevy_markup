//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use std::cell::RefCell;

use bevy::asset::{AssetEvent, AssetPath, LoadState};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::text::{InlineBox, InlineBoxKind};
use bevy::ecs::system::SystemParam;

use crate::cascade::{
    self, CssPosition, DecorationDecl, HtmlStyles, InlineStyle, LayoutDecl, OutlineDecl, Pseudo,
    SliceValue,
};
use crate::custom_elements::{self, ConnectedAs, CustomElement, ElementConnected};
use crate::focus::{self, Focusable};
use crate::fonts::FontFamilies;
use crate::html::{
    HtmlDebugOutline, HtmlElement, HtmlUi, HtmlUiBuilt, HtmlUiRestyled, RenderedHtml,
};
use crate::l10n::LocalizedText;
use crate::rebuild::{Decision, Frame, Phase, RebuildState, Source};
use crate::signals::{self, ElementSignals, PseudoState, SignalBinding};
use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
use crate::template::{HtmlTemplate, decode_entities};

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
    /// The product of the element's and its ancestors' `opacity`: CSS group
    /// opacity, applied by fading every color of the subtree ([`faded`]).
    pub(crate) opacity: f32,
}

/// `color` with its alpha scaled by `opacity`.
pub(crate) fn faded(color: Color, opacity: f32) -> Color {
    if opacity >= 1.0 {
        color
    } else {
        color.with_alpha(color.alpha() * opacity)
    }
}

/// A stretch of text in one style, or an inline `<img>` (empty `text`).
struct Run {
    text: String,
    style: Style,
    /// The owning inline element's `background-color` and `text-decoration`
    /// (not inherited in CSS, but propagated down the inline subtree here;
    /// a nested element's own declaration replaces them).
    extras: RunExtras,
    /// `Some` for an `<img>`: rendered as an inline box in the block's text.
    image: Option<ImageRun>,
}

/// Run-level (non-inherited) properties of an inline element.
#[derive(Clone, Copy, Default, PartialEq)]
struct RunExtras {
    background: Option<Color>,
    decoration: DecorationDecl,
}

impl RunExtras {
    /// Fades every color with the element's computed `opacity`.
    fn faded(self, opacity: f32) -> Self {
        Self {
            background: self.background.map(|color| faded(color, opacity)),
            decoration: DecorationDecl {
                color: self.decoration.color.map(|color| faded(color, opacity)),
                ..self.decoration
            },
        }
    }
}

/// An `<img>`: the loaded image and its `width`/`height` attributes (px;
/// `None` keeps the image's size, one `None` preserves the aspect ratio).
#[derive(Clone)]
struct ImageRun {
    image: Handle<Image>,
    width: Option<f32>,
    height: Option<f32>,
}

impl ImageRun {
    /// The inline box's size: the attributes over the image's pixel size,
    /// aspect ratio preserved when one is missing. `None` while the image
    /// is still loading (its load restyles the UI, which sizes the box).
    fn box_size(&self, images: &Assets<Image>) -> Option<Vec2> {
        let size = images.get(&self.image)?.size().as_vec2();
        Some(match (self.width, self.height) {
            (Some(w), Some(h)) => Vec2::new(w, h),
            (Some(w), None) => Vec2::new(w, size.y * (w / size.x)),
            (None, Some(h)) => Vec2::new(size.x * (h / size.y), h),
            (None, None) => size,
        })
    }
}

/// The `<img>` images a UI's current build uses: their loads restyle the UI
/// so the inline boxes take their sizes (Bevy's own `AssetChanged`-driven
/// sizer races the asset events here, so bevy_markup sizes the boxes).
#[derive(Component, Deref, Default)]
pub(crate) struct InlineImages(pub Vec<Handle<Image>>);

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
    /// The `border-image` frame's tint alpha (`opacity`); `None` = opaque.
    pub(crate) image_alpha: Option<f32>,
}

impl BoxStyle {
    /// Applies the element's effective `opacity` to its box colors (the
    /// resolved outline included, so call after [`BoxStyle::outline`]).
    pub(crate) fn fade(&mut self, opacity: f32) {
        if opacity >= 1.0 {
            return;
        }
        if let Some(background) = &mut self.background {
            *background = faded(*background, opacity);
        }
        for color in self.border_color.iter_mut().flatten() {
            *color = faded(*color, opacity);
        }
        if let Some(outline) = &mut self.drawn_outline {
            outline.color = faded(outline.color, opacity);
        }
        self.image_alpha = Some(opacity);
    }

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
            color: Color::WHITE.with_alpha(self.image_alpha.unwrap_or(1.0)),
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
    fixed: Option<Option<FixedNode>>,
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
    /// For `<img src>` loads. `None` where assets can't load (the fuzzing
    /// harness): `<img>` runs are then skipped.
    pub(crate) server: Option<&'a AssetServer>,
    /// The template's asset path: `<img src="…">` resolves against it.
    pub(crate) template: Option<AssetPath<'static>>,
}

impl Styler<'_> {
    /// `element`'s computed style: its declared values (with its `style`
    /// attribute, `inline`) over `inherited`. `pseudo` is the element's
    /// interaction state (`:hover`/`:active`).
    pub(crate) fn style_of(
        &self,
        element: &HtmlElement,
        inherited: Style,
        pseudo: Pseudo,
        inline: Option<&InlineStyle>,
    ) -> Style {
        let declared = self.styles.get_with(element, pseudo, inline);
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
            opacity: inherited.opacity * declared.opacity.unwrap_or(1.0),
        }
    }

    /// The inline element's run-level properties (`background-color`,
    /// `text-decoration`); `None` when it declares neither. Not inherited:
    /// `push_runs` keeps the subtree's running values unless an element
    /// declares its own.
    fn run_extras(
        &self,
        element: &HtmlElement,
        inline: Option<&InlineStyle>,
    ) -> Option<RunExtras> {
        let declared = self.styles.get_with(element, Pseudo::default(), inline);
        if declared.background.is_none() && declared.text_decoration.is_none() {
            return None;
        }
        Some(RunExtras {
            background: declared.background,
            decoration: declared.text_decoration.unwrap_or_default(),
        })
    }

    /// An `<img>`'s handle and `width`/`height` attributes: `src` resolves
    /// relative to the template (like `{% include %}`), `/` from the asset
    /// root. URLs and data URIs are unsupported.
    fn image_of(&self, tag: &tl::HTMLTag) -> Option<ImageRun> {
        let src = tag
            .attributes()
            .get("src")
            .flatten()
            .map(|src| decode_entities(&src.as_utf8_str()))?;
        let template = match &self.template {
            Some(template) => template,
            None => {
                debug!("html img: {src} skipped (no template path)");
                return None;
            }
        };
        if src.contains("://") || src.starts_with("data:") {
            debug!("html img: {src} — only asset paths are supported; skipped");
            return None;
        }
        let resolved = template.resolve_embed_str(&src).inspect_err(|error| {
            debug!("html img: {src} skipped ({error})");
        }).ok()?;
        let width = attr_px(tag, "width");
        let height = attr_px(tag, "height");
        Some(ImageRun {
            image: self.server?.load(resolved),
            width,
            height,
        })
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
    pub(crate) fn box_of(
        &self,
        element: &HtmlElement,
        pseudo: Pseudo,
        inline: Option<&InlineStyle>,
    ) -> BoxStyle {
        let declared = self.styles.get_with(element, pseudo, inline);
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
            image_alpha: None,
        }
    }
}

/// The root rule's values (`root`: the document's `<html>` element, or a
/// bare `html` for fragments without one), over the defaults.
pub(crate) fn root_style(
    styles: &HtmlStyles,
    fonts: &FontFamilies,
    images: &Assets<Image>,
    server: Option<&AssetServer>,
    root: &HtmlElement,
    inline: Option<&InlineStyle>,
) -> Style {
    let defaults = Style {
        color: DEFAULT_COLOR,
        family: None,
        size: DEFAULT_FONT_SIZE,
        bold: false,
        italic: false,
        pointer_events: true,
        opacity: 1.0,
    };
    Styler {
        styles,
        fonts,
        root_size: DEFAULT_FONT_SIZE,
        sheet: None,
        images,
        server,
        template: None,
    }
    .style_of(root, defaults, Pseudo::default(), inline)
}

/// The element the `HtmlUi` node itself is styled as, with its `style`
/// attribute: the document's top-level `<html>` (with its `id`/`class`), or
/// a bare `html` for fragments.
pub(crate) fn root_element(dom: &tl::VDom) -> (HtmlElement, Option<InlineStyle>) {
    dom.children()
        .iter()
        .filter_map(|handle| handle.get(dom.parser())?.as_tag())
        .find(|tag| tag.name().as_utf8_str().eq_ignore_ascii_case("html"))
        .map_or_else(
            || (element_tag("html"), None),
            |tag| (element_of(tag), cascade::inline_style(tag)),
        )
}

/// The system's asset params, bundled: Bevy's `SystemParam` tuples end at
/// 16, and `build_html_ui` has more.
#[derive(SystemParam)]
pub(crate) struct BuildAssets<'w> {
    sheets: Res<'w, Assets<Stylesheet>>,
    server: Res<'w, AssetServer>,
    images: Res<'w, Assets<Image>>,
    templates: Res<'w, Assets<HtmlTemplate>>,
}

pub(crate) fn build_html_ui(
    mut commands: Commands,
    mut sheet_events: MessageReader<AssetEvent<Stylesheet>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    assets: BuildAssets<'_>,
    default_sheet: Res<DefaultStylesheet>,
    fonts: Res<FontFamilies>,
    mut views: Query<
        (
            Entity,
            Ref<RenderedHtml>,
            Ref<LocalizedText>,
            Option<Ref<HtmlStylesheet>>,
            Option<Ref<HtmlDebugOutline>>,
            &HtmlUi,
            Option<&InlineImages>,
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
    let BuildAssets {
        sheets,
        server,
        images,
        templates,
    } = assets;
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
    for (entity, rendered, localized, own_sheet, outline, _ui, used_images, mut rebuild) in &mut views {
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
            images_changed: used_images.is_some_and(|images| {
                images
                    .0
                    .iter()
                    .any(|handle| loaded_images.contains(&handle.id()))
            }),
            content_changed: rendered.is_changed()
                || localized.is_changed()
                || removed_outlines.contains(&entity)
                || outline.as_ref().is_some_and(|outline| outline.is_changed()),
        };
        decisions.push((entity, rebuild.decide(frame)));
    }
    // Deepest UIs first: a nested `HtmlUi`'s commands are queued before an
    // ancestor's, so an ancestor update that despawns the element holding it
    // comes after them (and one that keeps it keeps the nested UI updated).
    let depth = |entity: Entity| parents.iter_ancestors(entity).count();
    decisions.sort_by_key(|(entity, _)| std::cmp::Reverse(depth(*entity)));

    for (entity, decision) in decisions {
        let (source, restyle) = match decision {
            Decision::Wait | Decision::Skip => continue,
            Decision::Build(source) => (source, false),
            Decision::Restyle(source) => (source, true),
        };
        let Ok((_, rendered, localized, own_sheet, outline, ui, _used_images, _rebuild)) =
            views.get(entity)
        else {
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
        let (root_element, root_inline) = match (&*rendered, &outline) {
            (RenderedHtml::Ready(document), None) => root_element(document.dom()),
            _ => (element_tag("html"), None),
        };
        let root = root_style(
            &styles,
            &fonts,
            &images,
            Some(&server),
            &root_element,
            root_inline.as_ref(),
        );
        let template = match (&*rendered, ui) {
            (RenderedHtml::Ready(_), HtmlUi(template)) => templates.get(template).map(|template| {
                AssetPath::parse(&template.name).clone_owned()
            }),
            _ => None,
        };
        let styler = Styler {
            styles: &styles,
            fonts: &fonts,
            root_size: root.size,
            sheet: css,
            images: &images,
            server: Some(&server),
            template,
        };

        let mut root_box = styler.box_of(&root_element, Pseudo::default(), root_inline.as_ref());
        root_box.fade(root.opacity);
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
            let style = styler.style_of(&element_tag("pre"), root, Pseudo::default(), None);
            vec![Item::Block(Block {
                kind: BlockKind::Paragraph,
                element: None,
                handle: None,
                signals: Vec::new(),
                focus: None,
                custom: None,
                style,
                boxed: BoxStyle::default(),
                runs: vec![Run {
                    text,
                    style,
                    extras: RunExtras::default(),
                    image: None,
                }],
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
                        extras: RunExtras::default(),
                        image: None,
                    }],
                })],
            }
        };

        let specs: Vec<NodeSpec> = items
            .into_iter()
            .map(|item| item_spec(&styler, item, default_gap))
            .collect();
        // Content changes and restyles alike update the existing children in
        // place where they still match, spawning and despawning only what
        // differs.
        let inline_images: Vec<Handle<Image>> = specs
            .iter()
            .flat_map(|spec| spec.images.iter().cloned())
            .collect();
        commands.entity(entity).insert(InlineImages(inline_images));
        let mut connected = Vec::new();
        let spawned = update_children(&mut commands, entity, specs, &tree, entity, &mut connected);
        // After the spawns, before `HtmlUiBuilt`: its observers see what the
        // definitions attached.
        for element in connected {
            commands.queue(move |world: &mut World| custom_elements::connect(world, element));
        }
        // `HtmlUiRestyled` promises every element (and what the app attached)
        // was kept; anything else is a build.
        if restyle && !spawned {
            commands.trigger(HtmlUiRestyled { entity });
        } else {
            commands.trigger(HtmlUiBuilt { entity });
        }
    }
}

/// What one UI node should be, computed from the item tree, then spawned
/// ([`spawn_spec`]) or applied onto a matching existing entity
/// ([`update_spec`]): one description for builds, updates and restyles.
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
    /// `position: fixed` (`FixedNode` component).
    fixed: bool,
    /// The `<img>` handles this subtree's blocks use (aggregated up for the
    /// UI's [`InlineImages`]).
    images: Vec<Handle<Image>>,
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
    spans: Vec<SpanSpec>,
}

/// One inline child of the block's `Text`: a styled run, or an `<img>`
/// (`image.is_some()`; its `text` is empty).
struct SpanSpec {
    text: String,
    font: TextFont,
    color: Color,
    /// The owning inline element's `background-color` → `TextBackgroundColor`.
    background: Option<Color>,
    /// `text-decoration` → `Underline`/`Strikethrough`.
    underline: bool,
    line_through: bool,
    decoration_color: Option<Color>,
    image: Option<ImageRun>,
    /// The `<img>`'s inline box size (see [`ImageRun::box_size`]).
    box_size: Option<Vec2>,
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
            fixed: false,
            images: Vec::new(),
            text: None,
            children: Vec::new(),
        }
    }
}

/// Which of an element's components its stylesheet set (and may therefore
/// take back on a restyle): `BorderColor`, `ZIndex`, `Outline`, `Pickable`
/// and `FixedNode` are also things an app sets itself, so a restyle without
/// the declaration resets only what CSS set. `BorderColor`/`ZIndex` are
/// `Node`'s required components: taking them back means resetting them to
/// their defaults.
#[derive(Component, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CssOwned {
    border_color: bool,
    z_index: bool,
    outline: bool,
    pickable: bool,
    fixed: bool,
}

/// Sets a node's CSS-owned components; on a restyle, resets the ones CSS set
/// earlier and no longer declares.
fn apply_css_owned(
    target: &mut EntityCommands,
    border_color: Option<BorderColor>,
    z_index: Option<ZIndex>,
    outline: Option<Outline>,
    pickable: bool,
    fixed: bool,
) {
    let owned = CssOwned {
        border_color: border_color.is_some(),
        z_index: z_index.is_some(),
        outline: outline.is_some(),
        pickable: !pickable,
        fixed,
    };
    target.queue(move |mut entity: EntityWorldMut| {
        let before = entity.get::<CssOwned>().copied().unwrap_or_default();
        // Written only when different (see `put`): unchanged values keep
        // their change ticks.
        match border_color {
            Some(color) => put(&mut entity, Some(color)),
            None if before.border_color => put(&mut entity, Some(BorderColor::DEFAULT)),
            None => {}
        }
        match z_index {
            Some(z) => put(&mut entity, Some(z)),
            None if before.z_index => put(&mut entity, Some(ZIndex::default())),
            None => {}
        }
        match outline {
            Some(outline) => put(&mut entity, Some(outline)),
            None if before.outline => put::<Outline>(&mut entity, None),
            None => {}
        }
        if owned.pickable {
            put(&mut entity, Some(Pickable::IGNORE));
        } else if before.pickable {
            put::<Pickable>(&mut entity, None);
        }
        if owned.fixed {
            if !entity.contains::<FixedNode>() {
                entity.insert(FixedNode);
            }
        } else if before.fixed && entity.contains::<FixedNode>() {
            entity.remove::<FixedNode>();
        }
        put(&mut entity, (owned != CssOwned::default()).then_some(owned));
    });
}

/// Marks an element whose `ImageNode` is its CSS `border-image` frame, i.e.
/// owned by the pipeline. [`update_spec`] removes a frame only when it
/// carries this marker, so an `ImageNode` the app inserted on a built
/// element (an icon, say) is app state like any other component: updates
/// keep it.
#[derive(Component, PartialEq)]
pub(crate) struct CssFrame;

/// Spawns a block's inline children: one entity per run — a `TextSpan`
/// with its `TextBackgroundColor`/decoration components, or an inline
/// `<img>` (`InlineBox` + `InlineImage`; Bevy sizes the box from the image
/// when the attributes don't set one). Spans and images carry the block's
/// `pointer-events: none` (bevy_picking resolves text-section hits against
/// the span entity, so an ignored block must ignore its spans too — or the
/// ignored block stays clickable).
fn spawn_spans(spans: &mut ChildSpawnerCommands<'_>, text: TextSpec, pickable: bool) {
    for span in text.spans {
        let mut child = if let Some(image) = span.image {
            spans.spawn((
                InlineBox {
                    kind: InlineBoxKind::InFlow,
                    size: span.box_size.unwrap_or_default(),
                },
                InlineImage {
                    image: image.image,
                    width: image.width,
                    height: image.height,
                    ..default()
                },
            ))
        } else {
            spans.spawn((TextSpan::new(span.text), span.font, TextColor(span.color)))
        };
        if let Some(background) = span.background {
            child.insert(TextBackgroundColor(background));
        }
        if span.underline {
            child.insert(Underline);
        }
        if span.line_through {
            child.insert(Strikethrough);
        }
        // A declared decoration color applies to whichever lines exist
        // (`currentColor` otherwise).
        if let Some(color) = span.decoration_color {
            child.insert(UnderlineColor(color));
        }
        if let Some(color) = span.line_through.then_some(span.decoration_color).flatten() {
            child.insert(StrikethroughColor(color));
        }
        if !pickable {
            child.insert(Pickable::IGNORE);
        }
    }
}

/// The existing children's structure and identity, for [`update_children`].
type Tree<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Children>,
        Has<Text>,
        Has<TextSpan>,
        Option<&'static HtmlElement>,
        Option<&'static ConnectedAs>,
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
    let children: Vec<NodeSpec> = children
        .into_iter()
        .map(|child| item_spec(styler, child, default_gap))
        .collect();
    let images = children
        .iter()
        .flat_map(|spec| spec.images.iter().cloned())
        .collect();
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
        fixed: boxed.layout.position == Some(CssPosition::Fixed),
        images,
        children,
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
        color: faded(block.style.color, block.style.opacity),
        no_wrap: matches!(block.kind, BlockKind::Preformatted),
        spans: block
            .runs
            .into_iter()
            .map(|run| {
                let color = faded(run.style.color, run.style.opacity);
                match run.image {
                    Some(image) => SpanSpec {
                        text: String::new(),
                        font: TextFont::default(),
                        color,
                        background: None,
                        underline: false,
                        line_through: false,
                        decoration_color: None,
                        box_size: image.box_size(styler.images),
                        image: Some(image),
                    },
                    None => SpanSpec {
                        text: run.text,
                        font: styler.text_font(run.style),
                        color,
                        background: run.extras.background,
                        underline: run.extras.decoration.underline,
                        line_through: run.extras.decoration.line_through,
                        decoration_color: run.extras.decoration.color,
                        image: None,
                        box_size: None,
                    },
                }
            })
            .collect(),
    };
    // For the UI's `InlineImages`: their loads restyle the UI (sizing the
    // inline boxes).
    let images: Vec<Handle<Image>> = text
        .spans
        .iter()
        .filter_map(|span| span.image.as_ref())
        .map(|image| image.image.clone())
        .collect();

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
            fixed: boxed.layout.position == Some(CssPosition::Fixed),
            images,
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
        fixed: boxed.layout.position == Some(CssPosition::Fixed),
        images,
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
#[derive(Component, Clone, Copy, Debug, Deref, PartialEq)]
pub(crate) struct DomNode(pub tl::NodeHandle);

/// Spawns `spec` under `parent`, collecting its custom elements (in
/// document order) for [`custom_elements::connect`].
fn spawn_spec(
    parent: &mut ChildSpawnerCommands,
    spec: NodeSpec,
    root: Entity,
    connected: &mut Vec<ElementConnected>,
) -> Entity {
    let mut entity = parent.spawn(spec.node);
    let id = entity.id();
    if let Some(custom) = spec.custom {
        connected.push(ElementConnected {
            entity: id,
            root,
            name: custom.name.clone(),
            dataset: custom.dataset.clone(),
        });
        entity.insert(ConnectedAs(custom));
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
        signals::observe_pointer_signals(&mut entity);
        entity.insert(ElementSignals(spec.signals));
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
        spec.fixed,
    );
    match spec.text {
        Some(text) => {
            entity.insert((
                Text::new(text.prefix),
                text.font.clone(),
                TextColor(text.color),
            ));
            if text.no_wrap {
                entity.insert(TextLayout::no_wrap());
            }
            // bevy_picking resolves text-section hits against the span
            // entity, so the block's `pointer-events: none` must be on the
            // spans too (or the ignored block stays clickable).
            entity.with_children(|spans| {
                spawn_spans(spans, text, spec.pickable);
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
    id
}

/// Whether the existing child `entity` can become `spec` in place: the same
/// node kind (text block or not) and the same element (tag, `id`, and `is`
/// with its dataset). Spans and app-nested `HtmlUi` roots never match.
fn same_identity(spec: &NodeSpec, entity: Entity, tree: &Tree) -> bool {
    let Ok((_, has_text, has_span, element, connected, _, nested_ui)) = tree.get(entity) else {
        return false;
    };
    if nested_ui || has_span || has_text != spec.text.is_some() {
        return false;
    }
    let same_element = match (element, &spec.element) {
        (None, None) => true,
        (Some(old), Some(new)) => old.tag == new.tag && old.id == new.id,
        _ => false,
    };
    same_element && connected.map(|connected| &connected.0) == spec.custom.as_ref()
}

/// Updates `parent`'s pipeline-owned children to `specs`. An element with
/// an `id` unique among its old and its new siblings is matched by it, the
/// rest by position among themselves (like keyed list diffing; a duplicated
/// `id` can't key anything). A match with the same
/// identity ([`same_identity`]) is updated in place ([`update_spec`]) —
/// keeping its entity, its interaction state and whatever the app attached;
/// the rest are despawned and spawned, then everything is put in document
/// order. Children the app nested (`HtmlUi` roots) are kept, after the
/// pipeline's own. Returns whether anything below `parent` was spawned.
fn update_children(
    commands: &mut Commands,
    parent: Entity,
    specs: Vec<NodeSpec>,
    tree: &Tree,
    root: Entity,
    connected: &mut Vec<ElementConnected>,
) -> bool {
    let current: Vec<Entity> = tree
        .get(parent)
        .ok()
        .and_then(|(children, ..)| children)
        .map_or_else(Vec::new, |children| children.to_vec());
    let (owned, nested): (Vec<Entity>, Vec<Entity>) = current
        .iter()
        .partition(|child| !tree.get(**child).is_ok_and(|(.., nested_ui)| nested_ui));
    let id_of = |entity: Entity| {
        tree.get(entity)
            .ok()
            .and_then(|(_, _, _, element, ..)| element?.id.clone())
    };
    let spec_id = |spec: &NodeSpec| spec.element.as_ref().and_then(|element| element.id.clone());
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for id in owned.iter().filter_map(|&child| id_of(child)) {
        counts.entry(id).or_default().0 += 1;
    }
    for id in specs.iter().filter_map(spec_id) {
        counts.entry(id).or_default().1 += 1;
    }
    let key = |id: Option<String>| id.filter(|id| counts.get(id) == Some(&(1, 1)));
    let mut keyed: HashMap<String, Entity> = HashMap::new();
    let mut unkeyed = Vec::new();
    for &child in &owned {
        match key(id_of(child)) {
            Some(id) => {
                keyed.insert(id, child);
            }
            None => unkeyed.push(child),
        }
    }
    let mut unkeyed = unkeyed.into_iter();
    let mut kept = HashSet::new();
    let mut desired = Vec::with_capacity(specs.len() + nested.len());
    let mut spawned = false;
    for spec in specs {
        let candidate = match key(spec_id(&spec)) {
            Some(id) => keyed.remove(&id),
            None => unkeyed.next(),
        };
        match candidate {
            Some(child) if same_identity(&spec, child, tree) => {
                spawned |= update_spec(commands, child, spec, tree, root, connected);
                kept.insert(child);
                desired.push(child);
            }
            _ => {
                let mut id = None;
                commands.entity(parent).with_children(|children| {
                    id = Some(spawn_spec(children, spec, root, connected));
                });
                desired.extend(id);
                spawned = true;
            }
        }
    }
    for &stale in owned.iter().filter(|child| !kept.contains(*child)) {
        commands.entity(stale).despawn();
    }
    desired.extend(nested);
    let remaining: Vec<Entity> = current
        .iter()
        .copied()
        .filter(|child| kept.contains(child) || !owned.contains(child))
        .collect();
    if desired != remaining {
        commands.entity(parent).replace_children(&desired);
    }
    spawned
}

/// Updates `entity` (matched by [`same_identity`]) to `spec` in place: the
/// entity and anything the app attached stay; components the spec no longer
/// has are removed. Returns whether elements were spawned below it.
fn update_spec(
    commands: &mut Commands,
    entity: Entity,
    spec: NodeSpec,
    tree: &Tree,
    root: Entity,
    connected: &mut Vec<ElementConnected>,
) -> bool {
    let (children, has_frame) = tree
        .get(entity)
        .map(|(children, _, _, _, _, has_frame, _)| {
            (
                children.map_or_else(Vec::new, |children| children.to_vec()),
                has_frame,
            )
        })
        .unwrap_or_default();
    let mut target = commands.entity(entity);
    // Only what differs is written: an unchanged component keeps its change
    // tick, so layout, text and style systems skip unchanged elements.
    let NodeSpec {
        node,
        element,
        handle,
        focus,
        signals: bindings,
        background,
        image,
        ..
    } = spec;
    if !bindings.is_empty() {
        signals::observe_pointer_signals(&mut target);
    }
    target.queue(move |mut entity: EntityWorldMut| {
        put(&mut entity, Some(node));
        if element.is_some() {
            put(&mut entity, element);
        }
        put(&mut entity, handle.map(DomNode));
        put(&mut entity, focus);
        put(
            &mut entity,
            (!bindings.is_empty()).then_some(ElementSignals(bindings)),
        );
        put(&mut entity, background.map(BackgroundColor));
        match image {
            // `ImageNode` has no `PartialEq`; frames are rare.
            Some(image) => {
                entity.insert((image, CssFrame));
            }
            None if has_frame => {
                entity.remove::<(ImageNode, CssFrame)>();
            }
            None => {}
        }
    });
    apply_css_owned(
        &mut target,
        spec.border_color,
        spec.z_index,
        spec.outline,
        spec.pickable,
        spec.fixed,
    );
    match spec.text {
        Some(text) => {
            // `TextLayout` is required by `Text`: replace, never remove.
            let layout = if text.no_wrap {
                TextLayout::no_wrap()
            } else {
                TextLayout::default()
            };
            let (prefix, color) = (text.prefix, text.color);
            let font = text.font.clone();
            target.queue(move |mut entity: EntityWorldMut| {
                put(&mut entity, Some(Text::new(prefix)));
                put(&mut entity, Some(font));
                put(&mut entity, Some(TextColor(color)));
                // `TextLayout` has no `PartialEq`: compare its fields.
                let same = entity.get::<TextLayout>().is_some_and(|current| {
                    current.justify == layout.justify && current.linebreak == layout.linebreak
                });
                if !same {
                    entity.insert(layout);
                }
            });
            // Spans carry the block's `pointer-events: none` (see
            // `spawn_spec`); an update without it takes it back. An image
            // span (an `InlineBox` child) can't become a `TextSpan` in
            // place: any kind mismatch respawns every span.
            let kinds_match = children.len() == text.spans.len()
                && children.iter().zip(&text.spans).all(|(child, span)| {
                    tree.get(*child)
                        .is_ok_and(|(_, _, is_span, ..)| is_span == span.image.is_none())
                });
            if kinds_match {
                let pickable = spec.pickable;
                for (span, spec) in children.into_iter().zip(text.spans) {
                    commands
                        .entity(span)
                        .queue(move |mut span: EntityWorldMut| {
                            if spec.image.is_none() {
                                let content = spec.text;
                                // `TextSpan` has no `PartialEq`: compare the text.
                                if span
                                    .get::<TextSpan>()
                                    .is_none_or(|current| **current != content)
                                {
                                    span.insert(TextSpan::new(content));
                                }
                                put(&mut span, Some(spec.font));
                                put(&mut span, Some(TextColor(spec.color)));
                            } else {
                                let image = spec.image.unwrap();
                                put(
                                    &mut span,
                                    Some(InlineBox {
                                        kind: InlineBoxKind::InFlow,
                                        size: spec.box_size.unwrap_or_default(),
                                    }),
                                );
                                put(
                                    &mut span,
                                    Some(InlineImage {
                                        image: image.image,
                                        width: image.width,
                                        height: image.height,
                                        ..default()
                                    }),
                                );
                            }
                            put(&mut span, spec.background.map(TextBackgroundColor));
                            put_marker::<Underline>(&mut span, spec.underline);
                            put(&mut span, spec.decoration_color.map(UnderlineColor));
                            put_marker::<Strikethrough>(&mut span, spec.line_through);
                            put(
                                &mut span,
                                spec.line_through
                                    .then_some(spec.decoration_color)
                                    .flatten()
                                    .map(StrikethroughColor),
                            );
                            put(&mut span, (!pickable).then_some(Pickable::IGNORE));
                        });
                }
            } else {
                for span in children {
                    commands.entity(span).despawn();
                }
                commands.entity(entity).with_children(|spans| {
                    spawn_spans(spans, text, spec.pickable);
                });
            }
            false
        }
        None => update_children(commands, entity, spec.children, tree, root, connected),
    }
}

/// Sets `value` on `entity` if it differs from what's there (inserting it if
/// absent), or removes the component for `None`: an unchanged component
/// keeps its change tick.
fn put<C: Component<Mutability = bevy::ecs::component::Mutable> + PartialEq>(
    entity: &mut EntityWorldMut,
    value: Option<C>,
) {
    match (value, entity.get_mut::<C>()) {
        (Some(value), Some(mut current)) => {
            current.set_if_neq(value);
        }
        (Some(value), None) => {
            entity.insert(value);
        }
        (None, Some(_)) => {
            entity.remove::<C>();
        }
        (None, None) => {}
    }
}

/// Sets or removes a marker component: an unchanged marker keeps its change
/// tick. (Markers like `Underline` have no `PartialEq` for [`put`].)
fn put_marker<C: Component + Default>(entity: &mut EntityWorldMut, set: bool) {
    if set == entity.contains::<C>() {
        return;
    }
    if set {
        entity.insert(C::default());
    } else {
        entity.remove::<C>();
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
    claim(
        entity,
        (boxed.layout.position == Some(CssPosition::Fixed)).then_some(FixedNode),
        &mut owned.fixed,
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
        || owned.fixed.is_some()
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
            push_runs(ctx, handle, inherited, RunExtras::default(), &mut runs);
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
    let inline = cascade::inline_style(tag);
    // The `<html>` element is what the root style was computed from:
    // re-applying it would square its `opacity`.
    let style = if element.tag == "html" {
        inherited
    } else {
        ctx.styler
            .style_of(&element, inherited, state, inline.as_ref())
    };
    let mut boxed = ctx.styler.box_of(&element, state, inline.as_ref());
    boxed.drawn_outline = boxed.outline(style.color);
    boxed.fade(style.opacity);
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
    push_content_runs(ctx, handle, tag, style, RunExtras::default(), &mut runs);
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
        dataset: custom_elements::dataset(tag),
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
    extras: RunExtras,
    runs: &mut Vec<Run>,
) {
    let translated = walk_translation(ctx, handle, tag, |fragment, nodes| {
        for node in nodes {
            push_runs(fragment, *node, style, extras, runs);
        }
    });
    if !translated {
        for child in tag.children().top().iter() {
            push_runs(ctx, *child, style, extras, runs);
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
    runs.retain(|run| !run.text.is_empty() || run.image.is_some());
    runs
}

fn push_runs(
    ctx: &Ctx,
    handle: tl::NodeHandle,
    style: Style,
    extras: RunExtras,
    runs: &mut Vec<Run>,
) {
    match handle.get(ctx.parser) {
        Some(tl::Node::Raw(text)) => {
            let text = decode_entities(&text.as_utf8_str());
            match runs.last_mut() {
                Some(last)
                    if last.style == style && last.extras == extras && last.image.is_none() =>
                {
                    last.text.push_str(&text);
                }
                _ => runs.push(Run {
                    text,
                    style,
                    extras,
                    image: None,
                }),
            }
        }
        Some(tl::Node::Tag(tag)) => {
            if tag.name().as_utf8_str() == *"img" {
                push_image_run(ctx, tag, style, runs);
                return;
            }
            // An unmatched `data-l10n-name` element is plain text.
            let style = ctx.element(tag).map_or(style, |element| {
                let inline = cascade::inline_style(tag);
                ctx.styler
                    .style_of(&element, style, Pseudo::default(), inline.as_ref())
            });
            // This element's `background`/`text-decoration` over the
            // subtree's running values, faded with its opacity.
            let extras = ctx
                .element(tag)
                .and_then(|element| {
                    let inline = cascade::inline_style(tag);
                    ctx.styler.run_extras(&element, inline.as_ref())
                })
                .map(|extras| extras.faded(style.opacity))
                .unwrap_or(extras);
            push_content_runs(ctx, handle, tag, style, extras, runs);
        }
        Some(tl::Node::Comment(_)) | None => {}
    }
}

/// An `<img>` attribute in px (`width`/`height`); `%` and other units are
/// unsupported (Bevy's `InlineImage` takes logical pixels).
fn attr_px(tag: &tl::HTMLTag, name: &str) -> Option<f32> {
    let value = tag.attributes().get(name).flatten()?;
    let value = decode_entities(&value.as_utf8_str());
    let trimmed = value.trim();
    if trimmed.ends_with('%') {
        debug!("html img: {name}={value:?} — percentages are unsupported; skipped");
        return None;
    }
    trimmed.parse().ok()
}

/// The `<img>` inline run: the image flows inline as an `InlineBox`/
/// `InlineImage` child of the block's `Text`. Unsupported sources (URLs,
/// data URIs) and a missing `src` attribute are skipped.
fn push_image_run(ctx: &Ctx, tag: &tl::HTMLTag, style: Style, runs: &mut Vec<Run>) {
    let Some(image) = ctx.styler.image_of(tag) else {
        return;
    };
    runs.push(Run {
        text: String::new(),
        style,
        extras: RunExtras::default(),
        image: Some(image),
    });
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
        if let Some(image) = run.image {
            // An inline image ends the whitespace sequence: spaces beside it
            // are significant, like around a word.
            out.push(Run {
                text: String::new(),
                style: run.style,
                extras: run.extras,
                image: Some(image),
            });
            after_space = false;
            continue;
        }
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
                extras: run.extras,
                image: None,
            });
        }
    }
    if let Some(last) = out.last_mut().filter(|last| last.image.is_none()) {
        let trimmed = last.text.trim_end_matches(is_html_whitespace).len();
        last.text.truncate(trimmed);
        if last.text.is_empty() {
            out.pop();
        }
    }
    out
}
