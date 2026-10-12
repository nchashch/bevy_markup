//! Rendered + localized DOM → styled Bevy UI children of the `HtmlUi` entity.

use std::cell::RefCell;

use bevy::asset::{AssetEvent, AssetPath, LoadState};
use bevy::ecs::system::SystemParam;
use lightningcss::values::easing::{EasingFunction, StepPosition};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
#[cfg(feature = "text-input")]
use bevy::text::EditableText;
use bevy::text::{InlineBox, InlineBoxKind};
#[cfg(feature = "text-input")]
use bevy::ui_widgets::TextInput;

use crate::cascade::{
    self, Ancestor, CalcBasis, CalcExpr, CalcProp, CssPosition, DecorationDecl, GridNames,
    HtmlStyles, InlineStyle, LayoutDecl, OutlineDecl, Pseudo, SliceValue, TransitionDecl,
    TransitionProp, node_field_mut, var_names,
};
use crate::custom_elements::{self, ConnectedAs, CustomElement, ElementConnected};
use crate::focus::{self, Focusable};
use crate::fonts::FontFamilies;
use crate::html::{
    HtmlDebugOutline, HtmlElement, HtmlUi, HtmlUiBuilt, HtmlUiRestyled, RenderedHtml,
    TemplateContext,
};
use crate::l10n::LocalizedText;
use crate::rebuild::{Decision, Frame, Phase, RebuildState, Source};
use crate::signals::{self, ElementSignals, PseudoState, SignalBinding, SignalTrigger};
use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
use crate::template::{HtmlTemplate, decode_entities};

/// Values when no stylesheet rule applies (white suits the dark UIs Bevy
/// apps usually draw on; 16px is the CSS initial size).
const DEFAULT_COLOR: Color = Color::WHITE;
const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Computed (inherited) text style.
#[derive(Clone, PartialEq)]
pub(crate) struct Style {
    pub(crate) color: Color,
    /// The CSS `font-family` chain resolved to registered family indexes
    /// (ADR 0015): the text font composes the chain into Bevy 0.20's
    /// fallback list per weight/style. `None` = Bevy's default font.
    pub(crate) family: Option<Vec<usize>>,
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
#[derive(Clone, Default, PartialEq)]
struct RunExtras {
    background: Option<Color>,
    decoration: DecorationDecl,
    /// The `<a>` this run belongs to: its bindings (the `open` click signal
    /// carrying the href, plus the element's own `data-on-*` hooks), its
    /// element info, and its DOM node (for `:hover` state). Inherited by
    /// the runs inside the element; `None` outside links.
    link: Option<LinkRun>,
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
            link: self.link,
        }
    }
}

/// A link's identity for its spans: the click bindings and the element.
#[derive(Clone, PartialEq)]
struct LinkRun {
    bindings: Vec<SignalBinding>,
    element: HtmlElement,
    handle: tl::NodeHandle,
}

/// An editable text field (ADR 0016): the initial text plus the cascade's
/// text styling (the widget's `EditableText` requires `TextFont`/`TextColor`,
/// whose cascade values the field's text styles with).
#[cfg(feature = "text-input")]
#[derive(Clone)]
struct TextInputSpec {
    initial: String,
    font: TextFont,
    color: Color,
    /// The form identity: submissions collect the value under this name.
    name: Option<String>,
}

/// An `<img>`: the loaded image and its `width`/`height` attributes (px;
/// `None` keeps the image's size, one `None` preserves the aspect ratio).
/// Percentages resolve against the containing block's content box at
/// layout time (see [`size_images`]).
#[derive(Clone)]
struct ImageRun {
    image: Handle<Image>,
    width: Option<ImageSize>,
    height: Option<ImageSize>,
}

/// An `<img>`'s `width`/`height` attribute value.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ImageSize {
    /// Logical pixels.
    Px(f32),
    /// A fraction of the containing block's content box (`width="50%"`).
    Percent(f32),
}

impl ImageSize {
    fn px(self) -> Option<f32> {
        match self {
            ImageSize::Px(px) => Some(px),
            ImageSize::Percent(_) => None,
        }
    }
}

/// An `<img>` with a CSS-declared `width`/`height` (px or `%`): its inline
/// box is sized against the containing block's content box (the last
/// frame's layout) by [`size_images`]. The box spawns at 0 and is sized in
/// the same frame, before `UiSystems::Prepare`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct InlineImageSized {
    /// The declared `width`, if any.
    width: Option<ImageSize>,
    /// The declared `height`, if any.
    height: Option<ImageSize>,
}
impl ImageRun {
    /// The inline box's size: the image's intrinsic size, unless a CSS
    /// `width`/`height` is declared (then [`size_images`] sizes the box at
    /// layout time). `None` also while the image is still loading (its load
    /// restyles the UI, which sizes the box).
    fn box_size(&self, images: &Assets<Image>) -> Option<Vec2> {
        let size = images.get(&self.image)?.size().as_vec2();
        match (self.width, self.height) {
            (None, None) => Some(size),
            _ => None,
        }
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
    /// `<button>`: a block carrying its bindings; `type="submit"` submits
    /// the enclosing form.
    Button,
}

struct Block {
    kind: BlockKind,
    /// `<button type="submit">`: clicking submits the enclosing form.
    submit: bool,
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
        /// The element's computed font-size (for `EmSize`).
        font_size: f32,
        children: Vec<Item>,
    },
    /// An `<input type="text">` (the `text-input` feature).
    #[cfg(feature = "text-input")]
    TextInput {
        element: HtmlElement,
        handle: Option<tl::NodeHandle>,
        signals: Vec<SignalBinding>,
        focus: Option<Focusable>,
        custom: Option<CustomElement>,
        boxed: BoxStyle,
        pointer_events: bool,
        font_size: f32,
        value: TextInputSpec,
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
    /// The `calc()`-valued properties (the layout pass resolves them
    /// per frame; the fields keep their placeholders until then).
    pub(crate) calc: Vec<(CalcProp, CalcExpr)>,
    /// Declared `transition` (the shorthand): what [`update_spec`]'s color
    /// writes animate.
    pub(crate) transitions: Vec<TransitionDecl>,
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

/// An in-flight CSS transition: one entry per transitioned property whose
/// value changed on the last update, animating from the previous visual to
/// the new value. Created by [`update_spec`]'s color writes; driven by
/// [`advance_transitions`]. Empty = idle.
#[derive(Component, Clone, Default, Debug)]
pub(crate) struct Transitions(pub Vec<ActiveTransition>);

/// One property's animation: `[from, to]` colors per border side (the
/// single-color entries repeat them).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveTransition {
    pub property: TransitionProp,
    pub from: [Color; 4],
    pub to: [Color; 4],
    /// `Time`'s virtual seconds when the change was styled.
    pub start: f32,
    /// Seconds; zero plays instantly.
    pub duration: f32,
    /// Seconds before the animation starts.
    pub delay: f32,
    pub easing: EasingFunction,
}

/// A `TemplateContext` value as a CSS `var()` replacement: strings as
/// written, numbers and bools as their CSS spelling, anything else (arrays,
/// objects, `null`) unresolvable.
fn css_value(value: &tera::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    if let Some(number) = value.as_i64() {
        return Some(number.to_string());
    }
    if let Some(number) = value.as_u64() {
        return Some(number.to_string());
    }
    if let Some(number) = value.as_f64() {
        return Some(format!("{number}"));
    }
    value.as_bool().map(|value| value.to_string())
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
/// A node with `calc()`-valued layout properties: [`resolve_calc`]
/// evaluates them per frame against the measured basis (the containing
/// block's content box, the font sizes, the viewport) and writes the
/// concrete pixels.
#[derive(Component, Debug, PartialEq)]
pub(crate) struct CalcSized {
    /// The element's own font size (`em`'s basis), captured at collection.
    em: f32,
    exprs: Vec<(CalcProp, CalcExpr)>,
}

#[derive(Component)]
pub(crate) struct CssRoot {
    /// The `Node` before CSS: the declared fields hold the app's values.
    base: Node,
    layout: LayoutDecl,
    border: [Option<f32>; 4],
    padding: [Option<f32>; 4],
    row_gap: Option<f32>,
    calc: Vec<(CalcProp, CalcExpr)>,
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
        inherited: &Style,
        pseudo: Pseudo,
        inline: Option<&InlineStyle>,
        ancestors: &[Ancestor],
    ) -> Style {
        let declared = self.styles.get_with(element, pseudo, inline, ancestors);
        let bold = declared.bold.unwrap_or(inherited.bold);
        let italic = declared.italic.unwrap_or(inherited.italic);
        Style {
            color: declared.color.unwrap_or(inherited.color),
            // A declared list with no registered family falls back to the
            // default font (as in browsers), not to the inherited family.
            // The whole chain resolves (not just the first hit): Bevy 0.20
            // composes it into a per-glyph fallback list (ADR 0015).
            family: match declared.font_family.as_deref() {
                Some(list) => Some(self.fonts.resolve_chain(list)),
                None => inherited.family.clone(),
            },
            size: declared.font_size.map_or(inherited.size, |size| {
                size.resolve(inherited.size, self.root_size)
            }),
            bold,
            italic,
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
        state: Pseudo,
        inline: Option<&InlineStyle>,
        ancestors: &[Ancestor],
    ) -> Option<RunExtras> {
        let declared = self.styles.get_with(element, state, inline, ancestors);
        if declared.background.is_none() && declared.text_decoration.is_none() {
            return None;
        }
        Some(RunExtras {
            background: declared.background,
            decoration: declared.text_decoration.unwrap_or_default(),
            link: None,
        })
    }

    /// An `<img>`'s handle and CSS `width`/`height`: `src` resolves
    /// relative to the template (like `{% include %}`), `/` from the asset
    /// root. URLs and data URIs are unsupported. Sizes come from the
    /// cascade (px, `%`; other units skipped); without a declaration the
    /// image keeps its intrinsic size.
    fn image_of(
        &self,
        tag: &tl::HTMLTag,
        element: &HtmlElement,
        state: Pseudo,
        ancestors: &[Ancestor],
    ) -> Option<ImageRun> {
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
        let resolved = template
            .resolve_embed_str(&src)
            .inspect_err(|error| {
                debug!("html img: {src} skipped ({error})");
            })
            .ok()?;
        let inline = cascade::inline_style(tag);
        let declared = self
            .styles
            .get_with(element, state, inline.as_ref(), ancestors);
        let size = |val: Option<Val>, name: &str| {
            val.and_then(|val| match val {
                Val::Px(px) => Some(ImageSize::Px(px)),
                Val::Percent(fraction) => Some(ImageSize::Percent(fraction / 100.0_f32)),
                val => {
                    debug!("html img: {name}: {val:?} — unsupported for images; skipped");
                    None
                }
            })
        };
        Some(ImageRun {
            image: self.server?.load(resolved),
            width: size(declared.layout.width, "width"),
            height: size(declared.layout.height, "height"),
        })
    }

    fn text_font(&self, style: &Style) -> TextFont {
        // The chain composes per weight/style: each family's face for the
        // request, in CSS order — one face stays a single source, several
        // become Bevy 0.20's fallback list (fontique picks per glyph).
        let font = match &style.family {
            Some(chain) => {
                let faces: Vec<FontSource> = chain
                    .iter()
                    .filter_map(|index| self.fonts.faces(*index))
                    .map(|faces| faces.face(style.bold, style.italic))
                    .collect();
                match faces.len() {
                    0 => FontSource::default(),
                    1 => faces.into_iter().next().unwrap(),
                    _ => FontSource::list(faces),
                }
            }
            None => FontSource::default(),
        };
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
        ancestors: &[Ancestor],
    ) -> BoxStyle {
        let declared = self.styles.get_with(element, pseudo, inline, ancestors);
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
            calc: declared.calc.clone(),
            transitions: declared.transitions.clone(),
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
    .style_of(root, &defaults, Pseudo::default(), inline, &[])
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
    rem_size: Res<'w, RemSize>,
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
    contexts: Query<&TemplateContext>,
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
        rem_size,
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
    for (entity, rendered, localized, own_sheet, outline, _ui, used_images, mut rebuild) in
        &mut views
    {
        use std::hash::{Hash, Hasher};
        // The context's values for the sheet's `var()` names: a change
        // restyles (the values style, they don't render). Hashed in name
        // order, so a per-frame context update to any other key (the calc
        // example's steppers, say) signals nothing. The names come from
        // the sheet the source resolution picks (a loading sheet waits
        // anyway; its frame recomputes once it resolves).
        let (own_phase, default_phase) = (
            own_sheet.as_ref().map(|own| phase(&own.0)),
            default_sheet.0.as_ref().map(phase),
        );
        let names_sheet = match (own_phase, default_phase) {
            (Some(Phase::Ready), _) => {
                own_sheet.as_ref().and_then(|own| sheets.get(&own.0))
            }
            (Some(Phase::Loading), _) => None,
            (Some(Phase::Failed) | None, Some(Phase::Ready)) => default_sheet
                .0
                .as_ref()
                .and_then(|default| sheets.get(default)),
            (Some(Phase::Failed) | None, _) => None,
        };
        let context = contexts.get(entity).ok();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for name in names_sheet
            .map(|sheet| var_names(sheet.sheet()))
            .unwrap_or_default()
        {
            name.hash(&mut hasher);
            context
                .and_then(|context| context.get(&name))
                .and_then(css_value)
                .hash(&mut hasher);
        }
        let vars_hash = hasher.finish();
        let frame = Frame {
            own: own_phase,
            default: default_phase,
            document_ready: !matches!(*rendered, RenderedHtml::Pending),
            own_changed: removed_sheets.contains(&entity)
                || own_sheet
                    .as_ref()
                    .is_some_and(|own| own.is_changed() || refreshed(&own.0)),
            default_changed,
            fonts_changed: fonts.is_changed(),
            state_changed: state_restyle.contains(&entity),
            vars_changed: vars_hash != rebuild.vars,
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
        let decision = rebuild.decide(frame);
        // The restyled builds record the hash; a `Wait` (a sheet still
        // loading) keeps the old one, so the change outlives the wait —
        // the same rule that keeps pending changes alive.
        if !matches!(decision, Decision::Wait | Decision::Skip) {
            rebuild.vars = vars_hash;
        }
        decisions.push((entity, decision));
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

        // `var(--name)` resolves against the UI's `TemplateContext` first
        // (values interpolated as written), then the sheet's own `--`
        // properties. Built per pass: the context is constant for one
        // styling run, so the per-element cache stays keyed on identity.
        let context = contexts.get(entity).ok();
        let lookup = |name: &str| context.and_then(|context| context.get(name)).and_then(css_value);
        let styles = match css {
            Some(css) => HtmlStyles::from_sheet(css.sheet()).with_context(&lookup),
            None => HtmlStyles::default().with_context(&lookup),
        };
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
            (RenderedHtml::Ready(_), HtmlUi(template)) => templates
                .get(template)
                .map(|template| AssetPath::parse(&template.name).clone_owned()),
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

        let mut root_box =
            styler.box_of(&root_element, Pseudo::default(), root_inline.as_ref(), &[]);
        root_box.fade(root.opacity);
        let root_grid = root_box.layout.grid_names();
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
        let root_size = root.size;
        let root_pointer_events = root.pointer_events;
        commands
            .entity(entity)
            .queue(move |mut entity: EntityWorldMut| {
                apply_root(&mut entity, root_box, root_pointer_events, root_size);
            });

        // The elements' interaction state by DOM node, so the collected
        // items style for `:hover`/`:active`. Nested `HtmlUi`s have their
        // own documents — their handles index those, not this one — so their
        // subtrees are skipped (the starting entity is the UI root itself).
        let mut states = HashMap::new();
        // A translated element's subtree is fragment-keyed (its spans'
        // `DomNode` handles index the translation fragment, re-parsed per
        // build), so those states are collected per fragment. The map holds
        // the last build's states: a changed translation misses and styles
        // unhovered until the next build's fresh map.
        let mut fragments: HashMap<tl::NodeHandle, HashMap<tl::NodeHandle, Pseudo>> =
            HashMap::new();
        let pseudo = |state: Option<&PseudoState>| {
            state.copied().map_or(Pseudo::default(), |state| Pseudo {
                hover: state.hovered,
                active: state.active,
                focus: state.focused,
                focus_visible: state.focus_visible,
            })
        };
        let mut stack = vec![(entity, None)];
        while let Some((current, fragment)) = stack.pop() {
            if let Ok((handle, state)) = dom_nodes.get(current) {
                match fragment {
                    Some(block) => {
                        fragments
                            .entry(block)
                            .or_default()
                            .insert(**handle, pseudo(state));
                    }
                    None => {
                        states.insert(**handle, pseudo(state));
                    }
                }
            }
            // A translated element starts its own fragment: its subtree's
            // spans are spawned from the re-parsed translation, so their
            // `DomNode` handles index the fragment. Only a document element
            // starts one (fragments aren't localized again).
            let fragment = match fragment {
                Some(fragment) => Some(fragment),
                None => dom_nodes.get(current).ok().and_then(|(handle, _)| {
                    let handle = **handle;
                    localized.0.contains_key(&handle).then_some(handle)
                }),
            };
            let nested_root =
                current != entity && tree.get(current).is_ok_and(|(.., nested_ui)| nested_ui);
            if !nested_root && let Ok((children, ..)) = tree.get(current) {
                stack.extend(
                    children
                        .iter()
                        .flat_map(|children| children.iter())
                        .map(|child| (child, fragment)),
                );
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
            let style = styler.style_of(&element_tag("pre"), &root, Pseudo::default(), None, &[]);
            vec![Item::Block(Block {
                kind: BlockKind::Paragraph,
                element: None,
                handle: None,
                signals: Vec::new(),
                focus: None,
                custom: None,
                submit: false,
                runs: vec![Run {
                    text,
                    style: style.clone(),
                    extras: RunExtras::default(),
                    image: None,
                }],
                boxed: BoxStyle::default(),
                style,
            })]
        } else {
            match &*rendered {
                RenderedHtml::Pending => continue,
                RenderedHtml::Ready(document) => collect_items(
                    document.dom(),
                    &localized.0,
                    Some(&states),
                    &fragments,
                    &styler,
                    root,
                    root_grid.as_ref(),
                ),
                RenderedHtml::Failed(message) => vec![Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    handle: None,
                    signals: Vec::new(),
                    focus: None,
                    custom: None,
                    submit: false,
                    runs: vec![Run {
                        text: format!("failed to render: {message}"),
                        style: root.clone(),
                        extras: RunExtras::default(),
                        image: None,
                    }],
                    style: root,
                    boxed: BoxStyle::default(),
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
        // CSS `rem` resolves against the root font-size: keep Bevy's
        // `RemSize` resource in sync. A change restyles this UI, so its
        // `Val::Rem`s re-convert against the new value.
        if **rem_size != root_size {
            commands.insert_resource(RemSize(root_size));
            commands.queue(move |world: &mut World| {
                if let Some(mut state) = world.get_mut::<RebuildState>(entity) {
                    state.queue_restyle();
                }
            });
        }
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
    /// `visibility: hidden` → `Visibility::Hidden` (subtree-hide: Bevy's
    /// visibility propagates to descendants; layout keeps the space).
    visibility: Option<Visibility>,
    /// The element's computed font-size (CSS `em` resolves against it via
    /// Bevy's `EmSize`; `rem` against the synced `RemSize`).
    font_size: f32,
    /// `<button type="submit">`: clicking emits the enclosing form's
    /// `Submit` signal.
    submit: bool,
    /// `<input type="text">` (the `text-input` feature): an editable text
    /// field. `Some` = initial text and the cascade's font/color.
    #[cfg(feature = "text-input")]
    text_input: Option<TextInputSpec>,
    /// The `<img>` handles this subtree's blocks use (aggregated up for the
    /// UI's [`InlineImages`]).
    images: Vec<Handle<Image>>,
    /// `Text` nodes hold spans, never child nodes.
    text: Option<TextSpec>,
    children: Vec<NodeSpec>,
    /// The `calc()`-valued layout properties: the layout pass resolves
    /// them per frame against the measured basis.
    calc: Vec<(CalcProp, CalcExpr)>,
    /// Declared `transition`: which color writes [`update_spec`]/[`apply_css_owned`]
    /// animate.
    transitions: Vec<TransitionDecl>,
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
    /// The link this span belongs to (`<a href>`): the span becomes a
    /// picked entity carrying the bindings (ADR: links emit the `open`
    /// signal).
    link: Option<LinkRun>,
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
            visibility: None,
            submit: false,
            font_size: DEFAULT_FONT_SIZE,
            #[cfg(feature = "text-input")]
            text_input: None,
            images: Vec::new(),
            calc: Vec::new(),
            transitions: Vec::new(),
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
    visibility: bool,
}

/// Sets a node's CSS-owned components; on a restyle, resets the ones CSS set
/// earlier and no longer declares. `transitions` (the element's declared
/// `transition`) animates the border color's change instead of writing it.
fn apply_css_owned(
    target: &mut EntityCommands,
    border_color: Option<BorderColor>,
    z_index: Option<ZIndex>,
    outline: Option<Outline>,
    pickable: bool,
    fixed: bool,
    visibility: Option<Visibility>,
    transitions: &[TransitionDecl],
) {
    let owned = CssOwned {
        border_color: border_color.is_some(),
        z_index: z_index.is_some(),
        outline: outline.is_some(),
        pickable: !pickable,
        fixed,
        visibility: visibility.is_some(),
    };
    let transitions = transitions.to_vec();
    target.queue(move |mut entity: EntityWorldMut| {
        let now = entity.world().resource::<Time>().elapsed_secs();
        let before = entity.get::<CssOwned>().copied().unwrap_or_default();
        // Written only when different (see `put`): unchanged values keep
        // their change ticks.
        match border_color {
            Some(color) => {
                if !transition_color(
                    &mut entity,
                    &transitions,
                    TransitionProp::BorderColor,
                    [color.top, color.right, color.bottom, color.left],
                    now,
                ) {
                    put(&mut entity, Some(color));
                }
            }
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
        match visibility {
            Some(visibility) => put(&mut entity, Some(visibility)),
            // `Visibility` is a `Node` required component: reset to CSS's
            // initial `visible`, never removed. (The root `HtmlUi` entity
            // never goes through here: `HtmlWorldAnchor` owns its visibility.)
            None if before.visibility => put(&mut entity, Some(Visibility::Visible)),
            None => {}
        }
        put(&mut entity, (owned != CssOwned::default()).then_some(owned));
    });
}

/// The element's current color for `property` (the animation's start: the
/// visual value, mid-animation included). `None` = not set (a spawn, or
/// another system owns it) — no transition.
fn read_color(entity: &EntityWorldMut, property: TransitionProp) -> Option<[Color; 4]> {
    match property {
        TransitionProp::TextColor => entity.get::<TextColor>().map(|color| [color.0; 4]),
        TransitionProp::BackgroundColor => {
            entity.get::<BackgroundColor>().map(|color| [color.0; 4])
        }
        TransitionProp::BorderColor => entity.get::<BorderColor>().map(|color| {
            [color.top, color.right, color.bottom, color.left]
        }),
    }
}

/// Writes the color(s) for `property` (a no-op when unchanged, like `put`).
fn write_color(entity: &mut EntityWorldMut, property: TransitionProp, colors: [Color; 4]) {
    match property {
        TransitionProp::TextColor => put(entity, Some(TextColor(colors[0]))),
        TransitionProp::BackgroundColor => put(entity, Some(BackgroundColor(colors[0]))),
        TransitionProp::BorderColor => put(
            entity,
            Some(BorderColor {
                top: colors[0],
                right: colors[1],
                bottom: colors[2],
                left: colors[3],
            }),
        ),
    }
}

/// Writes `property`'s value, as a transition when the element declares one
/// and the value changes. `true` when this handled the write (the caller
/// skips its own). `now` is `Time`'s virtual seconds at the update.
fn transition_color(
    entity: &mut EntityWorldMut,
    transitions: &[TransitionDecl],
    property: TransitionProp,
    to: [Color; 4],
    now: f32,
) -> bool {
    let Some(decl) = transitions.iter().rev().find(|decl| decl.property == property) else {
        return false;
    };
    let Some(from) = read_color(entity, property) else {
        return false;
    };
    // One entry per property: a change during an animation replaces it,
    // starting from the current visual.
    let mut active = entity.get::<Transitions>().cloned().unwrap_or_default();
    active.0.retain(|entry| entry.property != property);
    if from == to {
        // The target is the current value: the entry ends now.
        if active.0.is_empty() {
            entity.remove::<Transitions>();
        } else {
            entity.insert(active);
        }
        write_color(entity, property, to);
        return true;
    }
    active.0.push(ActiveTransition {
        property,
        from,
        to,
        start: now,
        duration: decl.duration,
        delay: decl.delay,
        easing: decl.easing.clone(),
    });
    entity.insert(active);
    // The start value shows until the advance system's first write (later
    // in this frame, before render — the target is never drawn early).
    write_color(entity, property, from);
    true
}

/// Advances every in-flight transition (PostUpdate, after [`Build`], before
/// `UiSystems::Prepare`): interpolates from the recorded start value to the
/// target over the entry's duration and easing, and drops finished entries
/// (whose final write is the target — an idle component holds the value the
/// style system would have written).
///
/// [`Build`]: HtmlUiSystems::Build
pub(crate) fn advance_transitions(
    time: Res<Time>,
    mut animated: Query<(
        &mut Transitions,
        Option<&mut TextColor>,
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
    )>,
) {
    let now = time.elapsed_secs();
    for (mut transitions, mut text, mut background, mut border) in &mut animated {
        let entries = std::mem::take(&mut transitions.0);
        let mut pending = Vec::new();
        for entry in entries {
            let progress = (now - entry.start - entry.delay)
                / entry.duration.max(f32::EPSILON);
            let done = progress >= 1.0;
            let value = if progress <= 0.0 {
                entry.from
            } else if done {
                entry.to
            } else {
                let t = eased(progress, &entry.easing);
                std::array::from_fn(|side| {
                    mix(entry.from[side], entry.to[side], t)
                })
            };
            match entry.property {
                TransitionProp::TextColor => {
                    if let Some(color) = text.as_mut() {
                        if color.0 != value[0] {
                            color.0 = value[0];
                        }
                    }
                }
                TransitionProp::BackgroundColor => {
                    if let Some(color) = background.as_mut() {
                        if color.0 != value[0] {
                            color.0 = value[0];
                        }
                    }
                }
                TransitionProp::BorderColor => {
                    if let Some(color) = border.as_mut() {
                        let current = color.bypass_change_detection();
                        if current.top != value[0]
                            || current.right != value[1]
                            || current.bottom != value[2]
                            || current.left != value[3]
                        {
                            **color = BorderColor {
                                top: value[0],
                                right: value[1],
                                bottom: value[2],
                                left: value[3],
                            };
                        }
                    }
                }
            }
            if !done {
                pending.push(entry);
            }
        }
        transitions.0 = pending;
    }
}

/// The easing curve at input progress `t` (0..1): linear, the named
/// cubic-Bézier curves, a custom one, or the step functions.
fn eased(t: f32, easing: &EasingFunction) -> f32 {
    match easing {
        EasingFunction::Linear => t,
        EasingFunction::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
        EasingFunction::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
        EasingFunction::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
        EasingFunction::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
        EasingFunction::CubicBezier { x1, y1, x2, y2, .. } => {
            cubic_bezier(*x1, *y1, *x2, *y2, t)
        }
        EasingFunction::Steps { count, position } => {
            let count = (*count).max(1) as f32;
            let value = match position {
                StepPosition::Start => f32::ceil(t * count) / count,
                StepPosition::End => f32::floor(t * count) / count,
                StepPosition::JumpBoth => (f32::floor(t * count) + 1.0) / (count + 1.0),
                StepPosition::JumpNone if count > 1.0 => {
                    f32::floor(t * count).clamp(0.0, count - 1.0) / (count - 1.0)
                }
                StepPosition::JumpNone => f32::floor(t * count) / count,
            };
            value.clamp(0.0, 1.0)
        }
    }
}

/// The cubic Bézier easing's y at input progress `t`: solves `x(s) = t` for
/// the parameter `s` (x is strictly monotone over CSS's allowed control
/// points) and evaluates `y(s)`. Bisection is plenty for a frame's
/// granularity; the endpoints are exact.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let at = |s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * y1 + 3.0 * u * s * s * y2 + s * s * s
    };
    let x_at = |s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * x1 + 3.0 * u * s * s * x2 + s * s * s
    };
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if x_at(mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at((lo + hi) / 2.0)
}

/// sRGB per-channel lerp (CSS interpolates sRGB colors in sRGB, not
/// linear, and this crate has no premultiplied-alpha transitions).
fn mix(from: Color, to: Color, t: f32) -> Color {
    let (from, to) = (from.to_srgba(), to.to_srgba());
    Color::srgba(
        from.red + (to.red - from.red) * t,
        from.green + (to.green - from.green) * t,
        from.blue + (to.blue - from.blue) * t,
        from.alpha + (to.alpha - from.alpha) * t,
    )
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
            let mut child = spans.spawn((
                InlineBox {
                    kind: InlineBoxKind::InFlow,
                    size: span.box_size.unwrap_or_default(),
                },
                InlineImage {
                    image: image.image,
                    width: image.width.and_then(ImageSize::px),
                    height: image.height.and_then(ImageSize::px),
                    ..default()
                },
            ));
            // A CSS-declared image is sized against its containing block
            // by the layout pass, not from its attributes.
            if image.width.is_some() || image.height.is_some() {
                child.insert(InlineImageSized {
                    width: image.width,
                    height: image.height,
                });
            }
            child
        } else {
            spans.spawn((TextSpan::new(span.text), span.font, TextColor(span.color)))
        };
        // A link span is a picked entity: bevy's per-section text picking
        // resolves clicks to it, and its bindings emit the signals (the
        // `open` click carries the href). `HtmlElement`/`DomNode` give it
        // hover state (`a:hover`) and element identity.
        if let Some(link) = span.link {
            child.insert((
                ElementSignals(link.bindings),
                link.element,
                DomNode(link.handle),
            ));
            signals::observe_pointer_signals(&mut child);
        }
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
    let (element, handle, signals, focus, custom, boxed, pointer_events, font_size, children) =
        match item {
            Item::Block(block) => return block_spec(styler, block),
            #[cfg(feature = "text-input")]
            Item::TextInput {
                element,
                handle,
                signals,
                focus,
                custom,
                boxed,
                pointer_events,
                font_size,
                value,
            } => {
                // Returns directly: the other arms feed the shared tuple tail.
                let mut node = Node {
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 0.0,
                    box_sizing: BoxSizing::ContentBox,
                    border: rect_over(UiRect::DEFAULT, boxed.border),
                    padding: rect_over(UiRect::DEFAULT, boxed.padding),
                    ..default()
                };
                boxed.layout.apply_to(&mut node);
                return NodeSpec {
                    element: Some(element),
                    signals,
                    focus,
                    custom,
                    handle,
                    border_color: boxed.border_color(),
                    background: boxed.background,
                    image: boxed.sliced_image(),
                    outline: boxed.drawn_outline,
                    z_index: boxed.z_index.map(ZIndex),
                    pickable: pointer_events,
                    fixed: boxed.layout.position == Some(CssPosition::Fixed),
                    visibility: boxed
                        .layout
                        .visibility
                        .map(|visible| if visible { Visibility::Visible } else { Visibility::Hidden }),
                    font_size,
                    #[cfg(feature = "text-input")]
                    text_input: Some(value),
                    children: Vec::new(),
                    calc: boxed.calc.clone(),
                    transitions: boxed.transitions.clone(),
                    ..NodeSpec::new(node)
                };
            }
            Item::Container {
                element,
                handle,
                signals,
                focus,
                custom,
                boxed,
                pointer_events,
                font_size,
                children,
            } => (
                element,
                handle,
                signals,
                focus,
                custom,
                boxed,
                pointer_events,
                font_size,
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
        visibility: boxed
            .layout
            .visibility
            .map(|visible| if visible { Visibility::Visible } else { Visibility::Hidden }),
        font_size,
        images,
        children,
        calc: boxed.calc.clone(),
        transitions: boxed.transitions.clone(),
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
        font: styler.text_font(&block.style),
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
                        link: None,
                    },
                    None => SpanSpec {
                        text: run.text,
                        font: styler.text_font(&run.style),
                        color,
                        background: run.extras.background,
                        underline: run.extras.decoration.underline,
                        line_through: run.extras.decoration.line_through,
                        decoration_color: run.extras.decoration.color,
                        image: None,
                        box_size: None,
                        link: run.extras.link,
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
            visibility: boxed
                .layout
                .visibility
                .map(|visible| if visible { Visibility::Visible } else { Visibility::Hidden }),
            submit: block.submit,
            font_size: block.style.size,
            images,
            text: Some(text),
            calc: boxed.calc.clone(),
            transitions: boxed.transitions.clone(),
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
        visibility: boxed
            .layout
            .visibility
            .map(|visible| if visible { Visibility::Visible } else { Visibility::Hidden }),
        submit: block.submit,
        font_size: block.style.size,
        images,
        children: vec![NodeSpec {
            pickable: block.style.pointer_events,
            text: Some(text),
            ..NodeSpec::new(text_node)
        }],
        calc: boxed.calc.clone(),
        transitions: boxed.transitions.clone(),
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
    // Non-text nodes get their `EmSize` from the cascade (text blocks have
    // a `TextFont`, which Bevy derives it from). CSS `em` lengths on the
    // node resolve against it. (Text inputs: `EditableText` requires a
    // `Node`, and their `EmSize` comes from their `TextFont`.)
    #[cfg(feature = "text-input")]
    let is_text_input = spec.text_input.is_some();
    #[cfg(not(feature = "text-input"))]
    let is_text_input = false;
    if spec.text.is_none() && !is_text_input {
        entity.insert(EmSize(spec.font_size));
    }
    let id = entity.id();
    #[cfg(feature = "text-input")]
    if is_text_input {
        // The editable text field (ADR 0016): the widget's editing behavior
        // keys off Bevy's `InputFocus` — the crate's focus (press-to-focus,
        // navigation, `HtmlModal`) drives it. The field's text styles from
        // the cascade via `TextFont`/`TextColor` (required by
        // `EditableText`), and the box from CSS on the node.
        let input = spec.text_input.unwrap();
        entity.insert((
            EditableText::new(input.initial),
            input.font,
            TextColor(input.color),
            TextInput,
        ));
        if let Some(name) = input.name {
            entity.insert(signals::InputName(name));
        }
    }
    #[cfg(not(feature = "text-input"))]
    let _ = is_text_input;
    if spec.submit {
        entity.insert(signals::FormSubmitButton);
    }
    if !spec.calc.is_empty() {
        entity.insert(CalcSized {
            em: spec.font_size,
            exprs: spec.calc.clone(),
        });
    }
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
    // A submit button has no bindings of its own, but its click submits
    // the enclosing form (`on_pointer` checks the marker).
    if !spec.signals.is_empty() || spec.submit {
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
        spec.visibility,
        // A spawn has no previous style: CSS transitions don't run.
        &[],
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
        font_size,
        calc,
        transitions,
        ..
    } = spec;
    let is_text = spec.text.is_some();
    // A submit button's click submits the enclosing form even though it
    // has no bindings of its own (`on_pointer` checks the marker).
    let submit = spec.submit;
    if !bindings.is_empty() || submit {
        signals::observe_pointer_signals(&mut target);
    }
    let background_transitions = transitions.clone();
    target.queue(move |mut entity: EntityWorldMut| {
        put(&mut entity, Some(node));
        // Non-text nodes get their `EmSize` from the cascade (text blocks
        // have a `TextFont`, which Bevy derives it from).
        if !is_text {
            put(&mut entity, Some(EmSize(font_size)));
        }
        put(
            &mut entity,
            (!calc.is_empty()).then_some(CalcSized {
                em: font_size,
                exprs: calc.clone(),
            }),
        );
        if element.is_some() {
            put(&mut entity, element);
        }
        put(&mut entity, handle.map(DomNode));
        put(&mut entity, focus);
        put(
            &mut entity,
            (!bindings.is_empty() || submit).then_some(ElementSignals(bindings)),
        );
        if submit {
            entity.insert(signals::FormSubmitButton);
        } else {
            entity.remove::<signals::FormSubmitButton>();
        }
        let now = entity.world().resource::<Time>().elapsed_secs();
        match background {
            Some(color) => {
                if !transition_color(
                    &mut entity,
                    &background_transitions,
                    TransitionProp::BackgroundColor,
                    [color; 4],
                    now,
                ) {
                    put(&mut entity, Some(BackgroundColor(color)));
                }
            }
            None => put(&mut entity, None::<BackgroundColor>),
        }
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
        spec.visibility,
        &transitions,
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
            let text_transitions = transitions.clone();
            target.queue(move |mut entity: EntityWorldMut| {
                put(&mut entity, Some(Text::new(prefix)));
                put(&mut entity, Some(font));
                let now = entity.world().resource::<Time>().elapsed_secs();
                if !transition_color(
                    &mut entity,
                    &text_transitions,
                    TransitionProp::TextColor,
                    [color; 4],
                    now,
                ) {
                    put(&mut entity, Some(TextColor(color)));
                }
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
                    // A span that becomes a link needs the click observers
                    // (idempotent: the marker check skips existing ones).
                    #[cfg_attr(not(feature = "text-input"), allow(clippy::used_underscore_binding))]
                    if spec.link.is_some() {
                        signals::observe_pointer_signals(&mut commands.entity(span));
                    }
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
                                // A span that shared the block's color (the
                                // inherited case) follows the block's color
                                // transition, so runs fade with their block.
                                let old_color = span.get::<TextColor>().map(|color| color.0);
                                let followed = span
                                    .get::<ChildOf>()
                                    .and_then(|child_of| {
                                        span.world().get::<Transitions>(child_of.0)
                                    })
                                    .and_then(|transitions| transitions.0.iter().find(|entry| entry.property == TransitionProp::TextColor).cloned());
                                let now = span.world().resource::<Time>().elapsed_secs();
                                match followed {
                                    Some(entry)
                                        if old_color == Some(entry.from[0])
                                            && entry.from[0] != entry.to[0] =>
                                    {
                                        let mut active = span
                                            .get::<Transitions>()
                                            .cloned()
                                            .unwrap_or_default();
                                        active.0
                                            .retain(|entry| entry.property != TransitionProp::TextColor);
                                        active.0.push(ActiveTransition {
                                            property: TransitionProp::TextColor,
                                            from: [old_color.unwrap(); 4],
                                            to: [spec.color; 4],
                                            start: entry.start,
                                            duration: entry.duration,
                                            delay: entry.delay,
                                            easing: entry.easing.clone(),
                                        });
                                        span.insert(active);
                                        put(&mut span, Some(TextColor(old_color.unwrap())));
                                    }
                                    _ => put(&mut span, Some(TextColor(spec.color))),
                                }
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
                                        width: image.width.and_then(ImageSize::px),
                                        height: image.height.and_then(ImageSize::px),
                                        ..default()
                                    }),
                                );
                                put(
                                    &mut span,
                                    (image.width.is_some() || image.height.is_some()).then(|| {
                                        InlineImageSized {
                                            width: image.width,
                                            height: image.height,
                                        }
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
                            // Link maintenance: bindings/identity on link
                            // spans, a clean removal when a run stops
                            // being a link.
                            match &spec.link {
                                Some(link) => {
                                    put(&mut span, Some(ElementSignals(link.bindings.clone())));
                                    put(&mut span, Some(link.element.clone()));
                                    put(&mut span, Some(DomNode(link.handle)));
                                }
                                None => {
                                    put::<ElementSignals>(&mut span, None);
                                    put::<HtmlElement>(&mut span, None);
                                    put::<DomNode>(&mut span, None);
                                }
                            }
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
/// Resolves `calc()`-valued layout properties per frame: `em`/`rem`
/// against the font sizes, `%` against the containing block's content box
/// per axis, viewport units against the UI root's size. Writes the
/// concrete pixels into the node's fields (the values keep their
/// placeholders until the first resolution; later builds overwrite the
/// fresh spec's placeholder each frame until the values are unchanged).
pub(crate) fn resolve_calc(
    mut sized: Query<(Entity, &ChildOf, &mut Node, &CalcSized)>,
    boxes: Query<&ComputedNode>,
    parents: Query<&ChildOf>,
    rem: Res<RemSize>,
) {
    for (_entity, child_of, mut node, calc) in &mut sized {
        let (Ok(parent_box), Ok(parent)) = (
            boxes.get(child_of.parent()),
            parents.get(child_of.parent()).map(|p| p.0),
        ) else {
            continue;
        };
        let inset = parent_box.border + parent_box.padding;
        let content = Vec2::new(
            (parent_box.size.x - inset.min_inset.x - inset.max_inset.x).max(0.0),
            (parent_box.size.y - inset.min_inset.y - inset.max_inset.y).max(0.0),
        );
        // The viewport: the topmost node's box (a UI root measures the
        // window; deeper roots measure their own layout, which is not the
        // viewport — walk past them).
        let mut top = parent;
        while let Ok(ancestor) = parents.get(top).map(|p| p.0) {
            top = ancestor;
        }
        let viewport = boxes.get(top).map(|box_| box_.size).unwrap_or_default();
        let basis = CalcBasis {
            em: calc.em,
            rem: rem.0,
            viewport,
            content,
        };
        for (prop, expr) in &calc.exprs {
            let Some(value) = expr.resolve(&basis, prop.horizontal()) else {
                continue;
            };
            let field = node_field_mut(&mut node, *prop);
            if *field != Val::Px(value) {
                *field = Val::Px(value);
            }
        }
    }
}

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
fn apply_root(entity: &mut EntityWorldMut, boxed: BoxStyle, pointer_events: bool, root_size: f32) {
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
        // A calc()'d property's field holds the last resolution, not a
        // declaration: when the new style drops the calc, restore the
        // app's own value.
        for (prop, _) in &state.calc {
            let value = match prop {
                CalcProp::Width => state.base.width.clone(),
                CalcProp::Height => state.base.height.clone(),
                CalcProp::MinWidth => state.base.min_width.clone(),
                CalcProp::MinHeight => state.base.min_height.clone(),
                CalcProp::MaxWidth => state.base.max_width.clone(),
                CalcProp::MaxHeight => state.base.max_height.clone(),
                CalcProp::FlexBasis => state.base.flex_basis.clone(),
                CalcProp::MarginTop => state.base.margin.top.clone(),
                CalcProp::MarginRight => state.base.margin.right.clone(),
                CalcProp::MarginBottom => state.base.margin.bottom.clone(),
                CalcProp::MarginLeft => state.base.margin.left.clone(),
                CalcProp::Left => state.base.left.clone(),
                CalcProp::Top => state.base.top.clone(),
                CalcProp::Right => state.base.right.clone(),
                CalcProp::Bottom => state.base.bottom.clone(),
            };
            *node_field_mut(&mut base, *prop) = value;
        }
    }
    let mut node = base.clone();
    boxed.layout.apply_to(&mut node);
    // CSS: `min-width`/`min-height: auto` compute to zero for the root (the
    // automatic minimum is a flex-*item* rule) — content overflows the
    // root, it doesn't size it. Taffy 0.9 applies the content minimum to
    // root nodes too, which would grow the root to its content instead of
    // letting the window clip it (a Bevy 0.19 → 0.20 behavior change; the
    // scrolling-panel recipe depends on it). Zero it unless the root rule
    // declares a minimum.
    if boxed.layout.min_width.is_none() {
        node.min_width = Val::Px(0.0);
    }
    if boxed.layout.min_height.is_none() {
        node.min_height = Val::Px(0.0);
    }
    node.border = rect_over(node.border, boxed.border);
    node.padding = rect_over(node.padding, boxed.padding);
    if let Some(gap) = boxed.row_gap {
        node.row_gap = Val::Px(gap);
    }
    if node != current {
        entity.insert(node);
    }
    // The root's font-size in concrete form: CSS `em` lengths on the root
    // resolve against it. (The root has no `TextFont`, so Bevy wouldn't
    // derive one.)
    entity.insert(EmSize(root_size));

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
        || !boxed.calc.is_empty()
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
            calc: boxed.calc.clone(),
            owned,
        });
    }
    // The root's own calc()'d properties: the pass resolves them per frame.
    if !boxed.calc.is_empty() {
        entity.insert(CalcSized {
            em: root_size,
            exprs: boxed.calc.clone(),
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
    /// The translated fragments' element states, keyed by the translated
    /// element's own DOM handle: a fragment re-parses per build, so its
    /// spans' `DomNode` handles index the fragment, not the document.
    fragments: Option<&'a HashMap<tl::NodeHandle, HashMap<tl::NodeHandle, Pseudo>>>,
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
    // Fragment handles index the fragment, not the source document — the
    // fragment gets its own states (collected per translated element by
    // `build_html_ui`), and no translations of its own.
    let unlocalized = HashMap::default();
    let fragment_ctx = Ctx {
        parser: fragment.parser(),
        localized: &unlocalized,
        states: ctx.fragments.and_then(|fragments| fragments.get(&handle)),
        fragments: None,
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
    fragments: &HashMap<tl::NodeHandle, HashMap<tl::NodeHandle, Pseudo>>,
    styler: &Styler,
    root: Style,
    grid: Option<&GridNames>,
) -> Vec<Item> {
    let ctx = Ctx {
        parser: dom.parser(),
        localized,
        states,
        fragments: Some(fragments),
        styler,
        named: None,
    };
    let mut items = Vec::new();
    for handle in dom.children() {
        collect_node(&ctx, *handle, &root, &mut items, &[], grid);
    }
    items
}

fn collect_node(
    ctx: &Ctx,
    handle: tl::NodeHandle,
    inherited: &Style,
    items: &mut Vec<Item>,
    chain: &[Ancestor],
    grid: Option<&GridNames>,
) {
    let Some(node) = handle.get(ctx.parser) else {
        return;
    };
    let tag = match node {
        tl::Node::Tag(tag) => tag,
        tl::Node::Raw(_) => {
            // Anonymous block: inherits the container's style.
            let mut runs = Vec::new();
            push_runs(
                ctx,
                handle,
                inherited,
                RunExtras::default(),
                &mut runs,
                chain,
            );
            let runs = collapse_runs(runs);
            if !runs.is_empty() {
                items.push(Item::Block(Block {
                    kind: BlockKind::Paragraph,
                    element: None,
                    handle: None,
                    signals: Vec::new(),
                    focus: None,
                    custom: None,
                    submit: false,
                    style: inherited.clone(),
                    boxed: BoxStyle::default(),
                    runs,
                }));
            }
            return;
        }
        tl::Node::Comment(_) => return,
    };

    let signals = signals::signal_bindings(tag);
    let mut focus = focus::focusable(tag, &signals);
    let custom = custom_elements::custom_element(tag);
    let state = ctx
        .states
        .and_then(|states| states.get(&handle))
        .copied()
        .unwrap_or_default();
    let Some(element) = ctx.element(tag) else {
        // Unmatched `data-l10n-name` in a translation: content only.
        for child in tag.children().top().iter() {
            collect_node(ctx, *child, inherited, items, chain, None);
        }
        return;
    };
    let inline = cascade::inline_style(tag);
    // This element becomes the nearest ancestor for everything inside it.
    let own = Ancestor {
        element: element.clone(),
        pseudo: state,
    };
    let child_chain = {
        let mut child_chain = Vec::with_capacity(chain.len() + 1);
        child_chain.push(own);
        child_chain.extend(chain.iter().cloned());
        child_chain
    };
    // The `<html>` element is what the root style was computed from:
    // re-applying it would square its `opacity`.
    let style = if element.tag == "html" {
        inherited.clone()
    } else {
        ctx.styler
            .style_of(&element, inherited, state, inline.as_ref(), chain)
    };
    let mut boxed = ctx.styler.box_of(&element, state, inline.as_ref(), chain);
    // This element's own placement resolves against its parent's grid
    // (named lines and areas); its own template then backs its children's.
    boxed.layout.resolve_names(grid);
    let grid_of_own = boxed.layout.grid_names();
    boxed.drawn_outline = boxed.outline(style.color);
    boxed.fade(style.opacity);
    #[cfg(feature = "text-input")]
    if element.tag == "input"
        && element
            .dataset
            .get("type")
            .is_none_or(|kind| kind == "text")
    {
        // An editable text field (ADR 0016): a block with the widget's
        // components; its content is the field's state, not walked markup.
        // An `<input>` is focusable like a block with `data-on-click`
        // (HTML inputs are focusable), unless `tabindex="-1"`.
        focus = Some(Focusable {
            autofocus: tag.attributes().get("autofocus").flatten().is_some(),
        });
        let value = tag
            .attributes()
            .get("value")
            .flatten()
            .map(|value| decode_entities(&value.as_utf8_str()).to_string());
        let name = tag
            .attributes()
            .get("name")
            .flatten()
            .map(|name| decode_entities(&name.as_utf8_str()).to_string());
        let mut boxed = ctx.styler.box_of(&element, state, inline.as_ref(), chain);
        boxed.drawn_outline = boxed.outline(style.color);
        boxed.fade(style.opacity);
        items.push(Item::TextInput {
            element,
            handle: Some(handle),
            signals,
            focus,
            custom,
            boxed,
            pointer_events: style.pointer_events,
            font_size: style.size,
            value: TextInputSpec {
                initial: value.unwrap_or_default(),
                font: ctx.styler.text_font(&style),
                color: faded(style.color, style.opacity),
                name,
            },
        });
        return;
    }
    // A button: a block carrying its bindings and element; clicking a
    // submit button submits the enclosing form (the marker is on the
    // spawn path, behind the `text-input` feature — without it the
    // Submit signal's payload is just empty).
    // An `<img>` directly in a container (raw HTML or Markdown): an
    // anonymous block carrying its image run — the same treatment a text
    // piece gets. In a block, `push_runs` handles it inline instead.
    if element.tag == "img" {
        let mut runs = Vec::new();
        push_runs(
            ctx,
            handle,
            inherited,
            RunExtras::default(),
            &mut runs,
            &child_chain,
        );
        if !runs.is_empty() {
            items.push(Item::Block(Block {
                kind: BlockKind::Paragraph,
                element: None,
                handle: None,
                signals: Vec::new(),
                focus: None,
                custom: None,
                submit: false,
                style: inherited.clone(),
                boxed: BoxStyle::default(),
                runs,
            }));
        }
        return;
    }

    let is_button = element.tag == "button";
    let kind = match element.tag.as_str() {
        "button" => {
            kind_button_focus(&mut focus, tag);
            BlockKind::Button
        }
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
                    collect_node(
                        fragment,
                        *node,
                        &style,
                        target,
                        &child_chain,
                        grid_of_own.as_ref(),
                    );
                }
            });
            if !translated {
                for child in tag.children().top().iter() {
                    collect_node(
                        ctx,
                        *child,
                        &style,
                        target,
                        &child_chain,
                        grid_of_own.as_ref(),
                    );
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
                    font_size: style.size,
                    children,
                });
            }
            return;
        }
    };
    // A missing translation falls back to the element's own content.
    let preformatted = matches!(kind, BlockKind::Preformatted);
    let submit = is_button
        && tag
            .attributes()
            .get("type")
            .flatten()
            .map(|kind| kind.as_utf8_str())
            .is_none_or(|kind| kind == "submit");
    let mut runs = Vec::new();
    push_content_runs(
        ctx,
        handle,
        tag,
        &style,
        RunExtras::default(),
        &mut runs,
        &child_chain,
    );
    items.push(Item::Block(Block {
        kind,
        element: Some(element),
        handle: Some(handle),
        signals,
        focus,
        custom,
        style,
        submit,
        boxed,
        runs: finish_runs(runs, preformatted),
    }));
}

/// Tag name (lowercase), `id` and `class` of an element.
/// A `<button>` is focusable like a block with `data-on-click` (HTML
/// buttons are focusable), unless the author opts out with `tabindex="-1"`.
fn kind_button_focus(focus: &mut Option<Focusable>, tag: &tl::HTMLTag) {
    if focus.is_none()
        && tag
            .attributes()
            .get("tabindex")
            .flatten()
            .map(|v| v.as_utf8_str())
            != Some("-1".into())
    {
        *focus = Some(Focusable {
            autofocus: tag.attributes().get("autofocus").flatten().is_some(),
        });
    }
}

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
    style: &Style,
    extras: RunExtras,
    runs: &mut Vec<Run>,
    chain: &[Ancestor],
) {
    let translated = walk_translation(ctx, handle, tag, |fragment, nodes| {
        for node in nodes {
            push_runs(fragment, *node, style, extras.clone(), runs, chain);
        }
    });
    if !translated {
        for child in tag.children().top().iter() {
            push_runs(ctx, *child, style, extras.clone(), runs, chain);
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
    style: &Style,
    extras: RunExtras,
    runs: &mut Vec<Run>,
    chain: &[Ancestor],
) {
    match handle.get(ctx.parser) {
        Some(tl::Node::Raw(text)) => {
            let text = decode_entities(&text.as_utf8_str());
            match runs.last_mut() {
                Some(last)
                    if last.style == *style && last.extras == extras && last.image.is_none() =>
                {
                    last.text.push_str(&text);
                }
                _ => runs.push(Run {
                    text,
                    style: style.clone(),
                    extras,
                    image: None,
                }),
            }
        }
        Some(tl::Node::Tag(tag)) => {
            // The tag's own interaction state (`:hover` on a link span, say):
            // entities with `DomNode` carry it (the link spans do).
            let state = ctx
                .states
                .and_then(|states| states.get(&handle))
                .copied()
                .unwrap_or_default();
            // The element, computed once: `element` consumes a named
            // overlay's entry (each usable once), so all uses share it.
            let element = ctx.element(tag);
            // This element becomes the nearest ancestor for its content.
            let child_chain = {
                let mut child_chain = Vec::with_capacity(chain.len() + 1);
                child_chain.push(Ancestor {
                    element: element.clone().unwrap_or_default(),
                    pseudo: state,
                });
                child_chain.extend(chain.iter().cloned());
                child_chain
            };
            if tag.name().as_utf8_str() == *"img" {
                push_image_run(ctx, tag, &element, state, style, extras, runs, chain);
                return;
            }
            // An unmatched `data-l10n-name` element is plain text.
            let style = element.as_ref().map_or_else(
                || style.clone(),
                |element| {
                    let inline = cascade::inline_style(tag);
                    ctx.styler
                        .style_of(element, style, state, inline.as_ref(), chain)
                },
            );
            // This element's `background`/`text-decoration` over the
            // subtree's running values, faded with its opacity.
            let inherited_link = extras.link.clone();
            let mut extras = element
                .as_ref()
                .and_then(|element| {
                    let inline = cascade::inline_style(tag);
                    ctx.styler
                        .run_extras(&element, state, inline.as_ref(), chain)
                })
                .map(|extras| extras.faded(style.opacity))
                .unwrap_or(extras);
            // A link: its runs become picked span entities that emit the
            // `open` signal with the href when clicked (plus the element's
            // own `data-on-*` hooks). No href → plain styled text, as in
            // browsers. Nested elements inherit the enclosing link (their
            // own extras would otherwise replace it).
            if tag.name().as_utf8_str() == *"a"
                && let Some(href) = tag.attributes().get("href").flatten()
            {
                let href = decode_entities(&href.as_utf8_str());
                if !href.is_empty() {
                    let mut bindings = signals::signal_bindings(tag);
                    bindings.push(SignalBinding {
                        trigger: SignalTrigger::Click,
                        name: "open".to_owned(),
                        payload: serde_json::json!({ "href": href }),
                    });
                    extras.link = Some(LinkRun {
                        bindings,
                        element: element_of(tag),
                        handle,
                    });
                    push_content_runs(ctx, handle, tag, &style, extras, runs, &child_chain);
                    return;
                }
            }
            extras.link = inherited_link;
            push_content_runs(ctx, handle, tag, &style, extras, runs, &child_chain);
        }
        Some(tl::Node::Comment(_)) | None => {}
    }
}

/// An `<img>` attribute in px (`width`/`height`); `%` and other units are
/// unsupported (Bevy's `InlineImage` takes logical pixels).
/// The `<img>` inline run: the image flows inline as an `InlineBox`/
/// `InlineImage` child of the block's `Text`. Unsupported sources (URLs,
/// data URIs) and a missing `src` attribute are skipped.
fn push_image_run(
    ctx: &Ctx,
    tag: &tl::HTMLTag,
    element: &Option<HtmlElement>,
    state: Pseudo,
    style: &Style,
    extras: RunExtras,
    runs: &mut Vec<Run>,
    ancestors: &[Ancestor],
) {
    let Some(image) = element
        .as_ref()
        .and_then(|element| ctx.styler.image_of(tag, element, state, ancestors))
    else {
        return;
    };
    runs.push(Run {
        text: String::new(),
        style: style.clone(),
        extras,
        image: Some(image),
    });
}

/// Sizes an `<img>` with a CSS-declared `width`/`height`
/// ([`InlineImageSized`]) against its containing block's content box (the
/// last frame's layout — the same slot
/// [`place_anchored`](crate::anchor::place_anchored) reads). Percentages
/// resolve against the content box; one dimension scales with the aspect
/// ratio preserved; both stretch. Runs every frame but writes only
/// changes, so a settled layout stops touching the entities (and
/// `width: 100%` of an image-sized container is the CSS cyclic case: it
/// converges when the container's width comes from elsewhere — a fixed
/// root or a sibling — and may thrash otherwise).
pub(crate) fn size_images(
    mut images: Query<(
        &ChildOf,
        &mut InlineBox,
        &mut InlineImage,
        &InlineImageSized,
    )>,
    blocks: Query<&ComputedNode>,
    assets: Res<Assets<Image>>,
) {
    for (parent, mut box_, mut image, sized) in &mut images {
        // Percentages resolve against the containing block's *used* content
        // box (`size` minus borders and padding — `content_size` is the
        // content-driven size, which is 0 for a block stretched by flex and
        // would fix `width: 100%` at zero).
        let Some(block) = blocks.get(parent.0).ok() else {
            continue;
        };
        // `size` is the border box; the insets (`min_inset` carries the
        // resolved border and padding per edge) come out of it.
        let inset = block.border + block.padding;
        let content = Vec2::new(
            (block.size.x - inset.min_inset.x - inset.max_inset.x).max(0.0),
            (block.size.y - inset.min_inset.y - inset.max_inset.y).max(0.0),
        );
        let Some(intrinsic) = assets.get(&image.image).map(|image| image.size().as_vec2()) else {
            continue;
        };
        let resolve = |size: ImageSize, base: f32| match size {
            ImageSize::Px(px) => px,
            ImageSize::Percent(fraction) => fraction * base,
        };
        let target = match (sized.width, sized.height) {
            (Some(w), Some(h)) => Vec2::new(resolve(w, content.x), resolve(h, content.y)),
            (Some(w), None) => {
                let width = resolve(w, content.x);
                Vec2::new(width, width * intrinsic.y / intrinsic.x)
            }
            (None, Some(h)) => {
                let height = resolve(h, content.y);
                Vec2::new(height * intrinsic.x / intrinsic.y, height)
            }
            (None, None) => continue,
        };
        if box_.size != target {
            box_.size = target;
        }
        if image.width != Some(target.x) || image.height != Some(target.y) {
            image.width = Some(target.x);
            image.height = Some(target.y);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;
    use lightningcss::values::easing::EasingFunction as CssEasing;

    /// The easing curves' endpoints are exact and their midpoints sane:
    /// linear is identity; the named cubic Béziers pass through 0 and 1
    /// and stay within (0, 1) inside; a step function jumps.
    #[test]
    fn easing_curves_have_exact_endpoints_and_sane_middles() {
        for easing in [
            EasingFunction::Linear,
            CssEasing::Ease,
            CssEasing::EaseIn,
            CssEasing::EaseOut,
            CssEasing::EaseInOut,
            CssEasing::CubicBezier {
                x1: 0.3,
                y1: 1.5,
                x2: 0.7,
                y2: -0.5,
            },
        ] {
            assert_eq!(eased(0.0, &easing), 0.0, "{easing:?}");
            assert_eq!(eased(1.0, &easing), 1.0, "{easing:?}");
            // Monotone in x-input by construction; a sane midpoint (except
            // the deliberately overshooting custom curve, which swings
            // past 1 and back).
            let middle = eased(0.5, &easing);
            if !matches!(easing, CssEasing::CubicBezier { .. }) {
                assert!((0.0..=1.0).contains(&middle), "{easing:?}: {middle}");
            }
        }
        // Linear is identity everywhere.
        for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!((eased(t, &EasingFunction::Linear) - t).abs() < 1e-6);
        }
        // Steps: end stays on the floor of the interval; start rises at 0.
        let end = EasingFunction::Steps {
            count: 2,
            position: StepPosition::End,
        };
        assert_eq!(eased(0.4, &end), 0.0);
        assert_eq!(eased(0.6, &end), 0.5);
        assert_eq!(eased(1.0, &end), 1.0);
        let start = EasingFunction::Steps {
            count: 2,
            position: StepPosition::Start,
        };
        assert_eq!(eased(0.0, &start), 0.0);
        assert_eq!(eased(0.1, &start), 0.5);
    }

    /// A background color transition: the entry animates from the current
    /// value (written so the target never shows early) to the target, and
    /// finished entries are dropped with the target in place.
    #[test]
    fn advance_transitions_interpolates_and_finishes() {
        let from = Color::srgb(0.0, 0.0, 0.0);
        let to = Color::srgb(1.0, 1.0, 1.0);
        let transition = |start: f32, duration: f32, delay: f32| Transitions(vec![ActiveTransition {
            property: TransitionProp::BackgroundColor,
            from: [from; 4],
            to: [to; 4],
            start,
            duration,
            delay,
            easing: CssEasing::Linear,
        }]);

        // Mid-animation: halfway between, at t = 0.5 of a 1s linear one.
        let mut world = World::new();
        world.insert_resource::<Time>(Time::default());
        let entity = world
            .spawn((transition(0.0, 1.0, 0.0), BackgroundColor(from)))
            .id();
        // Simulate 0.5s of elapsed time.
        let mut time = world.resource_mut::<Time>();
        let _ = time.advance_by(Duration::from_secs_f32(0.5));
        drop(time);
        world.run_system_once(super::advance_transitions).unwrap();
        let color = world.get::<BackgroundColor>(entity).unwrap().0;
        let (Srgba { red, green, blue, .. }, _) = (color.to_srgba(), ());
        assert!((red - 0.5).abs() < 1e-4, "{red}");
        assert!((green - 0.5).abs() < 1e-4, "{green}");
        assert!((blue - 0.5).abs() < 1e-4, "{blue}");
        // Still running: the entry stays.
        assert_eq!(world.get::<Transitions>(entity).unwrap().0.len(), 1);

        // Past the end: the target is written and the entry is gone.
        let mut time = world.resource_mut::<Time>();
        let _ = time.advance_by(Duration::from_secs_f32(1.0));
        drop(time);
        world.run_system_once(super::advance_transitions).unwrap();
        assert_eq!(world.get::<Transitions>(entity).unwrap().0, Vec::new());
        assert_eq!(world.get::<BackgroundColor>(entity).unwrap().0, to);

        // Delay: at `now` = 2.0 (the clock accumulated above), an entry
        // that started 0.2s ago with a 1.5s delay hasn't begun animating.
        let entity = world
            .spawn((transition(1.8, 1.0, 1.5), BackgroundColor(from)))
            .id();
        let mut time = world.resource_mut::<Time>();
        let _ = time.advance_by(Duration::from_secs_f32(0.5));
        drop(time);
        world.run_system_once(super::advance_transitions).unwrap();
        assert_eq!(world.get::<BackgroundColor>(entity).unwrap().0, from);
        assert_eq!(world.get::<Transitions>(entity).unwrap().0.len(), 1);

        // Zero duration: finishes immediately.
        let entity = world
            .spawn((transition(0.0, 0.0, 0.0), BackgroundColor(from)))
            .id();
        world.run_system_once(super::advance_transitions).unwrap();
        assert_eq!(world.get::<BackgroundColor>(entity).unwrap().0, to);
        assert_eq!(world.get::<Transitions>(entity).unwrap().0, Vec::new());
    }
}
