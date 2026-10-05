//! Stylesheet → declared style per element (the subset documented in
//! [`crate::style`]): compound selectors matched against tag, id and classes.

use std::cell::RefCell;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightningcss::properties::Property;
use lightningcss::properties::align::{
    self as css_align, BaselinePosition, ContentDistribution, ContentPosition, GapValue,
    SelfPosition,
};
use lightningcss::properties::display::{self as css_display, DisplayInside, DisplayOutside};
use lightningcss::properties::flex as css_flex;
use lightningcss::properties::size::{self as css_size, MaxSize, Size};
use lightningcss::properties::border::BorderSideWidth;
use lightningcss::properties::border_image::{
    BorderImageRepeat, BorderImageRepeatKeyword, BorderImageSlice,
};
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight,
    GenericFontFamily, RelativeFontSize,
};
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::color::{CssColor, RGBA};
use lightningcss::values::image::Image;
use lightningcss::values::length::{LengthPercentage, LengthPercentageOrAuto, LengthValue};
use lightningcss::values::percentage::NumberOrPercentage;

use crate::fonts::{FamilyRef, GenericFamily};
use crate::html::HtmlElement;

/// A declared `font-size`, resolved against inherited/root sizes later.
#[derive(Clone, Copy, Debug)]
pub(crate) enum FontSizeSpec {
    Px(f32),
    /// Multiple of the inherited size (`em`, `%`, `smaller`/`larger`).
    Inherited(f32),
    /// Multiple of the root (`html`) size.
    Root(f32),
}

impl FontSizeSpec {
    pub fn resolve(self, inherited: f32, root: f32) -> f32 {
        match self {
            Self::Px(px) => px,
            Self::Inherited(factor) => inherited * factor,
            Self::Root(factor) => root * factor,
        }
    }
}

/// One `border-image-slice` offset.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SliceValue {
    /// Image pixels (a plain number).
    Px(f32),
    /// Fraction of the image's width (left/right) or height (top/bottom).
    Fraction(f32),
}

/// Declared `border-image` (shorthand and/or longhands; each part optional so
/// longhands override parts of an earlier shorthand).
#[derive(Clone, Debug, Default)]
pub(crate) struct BorderImageDecl {
    /// The `url(...)` as written; `Some(None)` = `none`.
    pub source: Option<Option<String>>,
    /// `[top, right, bottom, left]` and `fill`.
    pub slice: Option<([SliceValue; 4], bool)>,
    /// `repeat`/`round`/`space` (tiled) vs `stretch`.
    pub tile: Option<bool>,
}

/// Declared flex, size and margin properties of a container or block, as
/// Bevy values (`None` = not declared).
#[derive(Clone, Debug, Default)]
pub(crate) struct LayoutDecl {
    pub display: Option<Display>,
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub align_self: Option<AlignSelf>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Val>,
    pub width: Option<Val>,
    pub height: Option<Val>,
    pub min_width: Option<Val>,
    pub min_height: Option<Val>,
    pub max_width: Option<Val>,
    pub max_height: Option<Val>,
    /// `[top, right, bottom, left]`.
    pub margin: [Option<Val>; 4],
    /// `column-gap` (or `gap`'s column part) in px.
    pub column_gap: Option<f32>,
    /// `box-sizing` (bevy_markup's nodes default to CSS's `content-box`).
    pub box_sizing: Option<BoxSizing>,
}

impl LayoutDecl {
    /// Overrides `node`'s fields with the declared ones.
    pub fn apply_to(&self, node: &mut Node) {
        fn set<T: Clone>(target: &mut T, value: &Option<T>) {
            if let Some(value) = value {
                *target = value.clone();
            }
        }
        set(&mut node.display, &self.display);
        set(&mut node.flex_direction, &self.flex_direction);
        set(&mut node.flex_wrap, &self.flex_wrap);
        set(&mut node.justify_content, &self.justify_content);
        set(&mut node.align_items, &self.align_items);
        set(&mut node.align_content, &self.align_content);
        set(&mut node.align_self, &self.align_self);
        set(&mut node.flex_grow, &self.flex_grow);
        set(&mut node.flex_shrink, &self.flex_shrink);
        set(&mut node.flex_basis, &self.flex_basis);
        set(&mut node.width, &self.width);
        set(&mut node.height, &self.height);
        set(&mut node.min_width, &self.min_width);
        set(&mut node.min_height, &self.min_height);
        set(&mut node.max_width, &self.max_width);
        set(&mut node.max_height, &self.max_height);
        set(&mut node.margin.top, &self.margin[0]);
        set(&mut node.margin.right, &self.margin[1]);
        set(&mut node.margin.bottom, &self.margin[2]);
        set(&mut node.margin.left, &self.margin[3]);
        set(&mut node.column_gap, &self.column_gap.map(Val::Px));
        set(&mut node.box_sizing, &self.box_sizing);
    }
}

/// Declared (not computed) style for one element type.
#[derive(Clone, Default, Debug)]
pub(crate) struct ElementStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// The `font-family` list as written.
    pub font_family: Option<Vec<FamilyRef>>,
    pub font_size: Option<FontSizeSpec>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub border_image: Option<BorderImageDecl>,
    /// `[top, right, bottom, left]` in px.
    pub border_width: [Option<f32>; 4],
    /// `[top, right, bottom, left]` in px.
    pub padding: [Option<f32>; 4],
    /// `row-gap` (or `gap`'s row part) in px, for containers.
    pub row_gap: Option<f32>,
    /// Flex, size and margin properties (containers and blocks).
    pub layout: LayoutDecl,
}

/// Every `border-image-source` URL in `sheet`, as written.
pub(crate) fn image_urls(sheet: &StyleSheet) -> Vec<String> {
    let mut urls = Vec::new();
    for rule in &sheet.rules.0 {
        let CssRule::Style(rule) = rule else {
            continue;
        };
        let declarations = rule
            .declarations
            .declarations
            .iter()
            .chain(&rule.declarations.important_declarations);
        for declaration in declarations {
            let image = match declaration {
                Property::BorderImage(border_image, _) => &border_image.source,
                Property::BorderImageSource(image) => image,
                _ => continue,
            };
            if let Some(url) = image_url(image)
                && !urls.contains(&url)
            {
                urls.push(url);
            }
        }
    }
    urls
}

/// A compound selector: optional type (or `*`), then any number of `.class`
/// and `#id` parts, e.g. `p.note`, `.a.b`, `#title`.
#[derive(Clone, Debug)]
struct Compound {
    /// Lowercase; `None` for `*` or no type.
    tag: Option<String>,
    ids: Vec<String>,
    classes: Vec<String>,
}

impl Compound {
    /// `None` for anything else (combinators, attributes, pseudo-classes).
    fn parse(selector: &str) -> Option<Self> {
        let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        let mut rest = selector;
        let mut tag = None;
        if let Some(after) = rest.strip_prefix('*') {
            rest = after;
        } else {
            let end = rest.find(|c| !is_ident(c)).unwrap_or(rest.len());
            if end > 0 {
                tag = Some(rest[..end].to_ascii_lowercase());
                rest = &rest[end..];
            }
        }
        let mut ids = Vec::new();
        let mut classes = Vec::new();
        while let Some((_, kind)) = rest.char_indices().next() {
            // Only `.class`/`#id` parts continue the selector; anything else
            // (combinator, pseudo, or a non-ASCII ident character) is
            // unsupported. Check before slicing: `kind.len_utf8()` may be > 1.
            if kind != '.' && kind != '#' {
                return None;
            }
            let body = &rest[kind.len_utf8()..];
            let end = body.find(|c| !is_ident(c)).unwrap_or(body.len());
            if end == 0 {
                return None;
            }
            if kind == '.' {
                classes.push(body[..end].to_owned());
            } else {
                ids.push(body[..end].to_owned());
            }
            rest = &body[end..];
        }
        if tag.is_none() && ids.is_empty() && classes.is_empty() && selector != "*" {
            return None;
        }
        Some(Self { tag, ids, classes })
    }

    /// CSS specificity: (ids, classes, types).
    fn specificity(&self) -> (u32, u32, u32) {
        (
            self.ids.len() as u32,
            self.classes.len() as u32,
            self.tag.is_some() as u32,
        )
    }

    fn matches(&self, element: &HtmlElement) -> bool {
        self.tag.as_ref().is_none_or(|tag| *tag == element.tag)
            && self.ids.iter().all(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|class| element.has_class(class))
    }
}

/// One selector of a style rule with one importance level's declarations.
struct Rule<'a> {
    selector: Compound,
    /// Sort key: importance, then specificity, then source order.
    rank: (bool, (u32, u32, u32), usize),
    declarations: &'a [Property<'static>],
}

/// A stylesheet's rules, matched per element (cached by tag + id + classes).
#[derive(Default)]
pub(crate) struct HtmlStyles<'a> {
    rules: Vec<Rule<'a>>,
    cache: RefCell<HashMap<(String, Option<String>, Vec<String>), ElementStyle>>,
}

impl<'a> HtmlStyles<'a> {
    pub fn from_sheet(sheet: &'a StyleSheet<'static>) -> Self {
        let mut rules = Vec::new();
        for (order, rule) in sheet.rules.0.iter().enumerate() {
            let CssRule::Style(rule) = rule else {
                continue;
            };
            for selector in rule.selectors.0.iter() {
                let Ok(text) = selector.to_css_string(PrinterOptions::default()) else {
                    continue;
                };
                let Some(compound) = Compound::parse(&text) else {
                    debug!("html css: skipping unsupported selector `{text}`");
                    continue;
                };
                let specificity = compound.specificity();
                for (important, declarations) in [
                    (false, &rule.declarations.declarations),
                    (true, &rule.declarations.important_declarations),
                ] {
                    if declarations.is_empty() {
                        continue;
                    }
                    rules.push(Rule {
                        selector: compound.clone(),
                        rank: (important, specificity, order),
                        declarations,
                    });
                }
            }
        }
        // Apply in ascending rank: later application wins.
        rules.sort_by_key(|rule| rule.rank);
        Self {
            rules,
            cache: RefCell::default(),
        }
    }

    /// The declared style for `element`: every matching rule's declarations,
    /// applied in cascade order.
    pub fn get(&self, element: &HtmlElement) -> ElementStyle {
        let key = (element.tag.clone(), element.id.clone(), element.classes.clone());
        if let Some(style) = self.cache.borrow().get(&key) {
            return style.clone();
        }
        let mut style = ElementStyle::default();
        for rule in self.rules.iter().filter(|rule| rule.selector.matches(element)) {
            for declaration in rule.declarations {
                apply(&mut style, declaration);
            }
        }
        self.cache.borrow_mut().insert(key, style.clone());
        style
    }
}

fn apply(style: &mut ElementStyle, declaration: &Property) {
    match declaration {
        Property::Color(color) => {
            if let Some(color) = to_color(color) {
                style.color = Some(color);
            }
        }
        Property::BackgroundColor(color) => {
            if let Some(color) = to_color(color) {
                style.background = Some(color);
            }
        }
        Property::FontFamily(families) => {
            let list: Vec<FamilyRef> = families.iter().filter_map(family_ref).collect();
            if !list.is_empty() {
                style.font_family = Some(list);
            }
        }
        Property::FontSize(size) => {
            if let Some(size) = font_size(size) {
                style.font_size = Some(size);
            }
        }
        Property::FontWeight(weight) => {
            style.bold = Some(match weight {
                FontWeight::Absolute(AbsoluteFontWeight::Bold) | FontWeight::Bolder => true,
                FontWeight::Absolute(AbsoluteFontWeight::Normal) | FontWeight::Lighter => false,
                // CSS font matching: above 500 prefers the heavier face.
                FontWeight::Absolute(AbsoluteFontWeight::Weight(weight)) => *weight > 500.0,
            });
        }
        Property::FontStyle(font_style) => {
            style.italic = Some(!matches!(font_style, FontStyle::Normal));
        }
        Property::BorderImage(border_image, _) => {
            style.border_image = Some(BorderImageDecl {
                source: Some(image_url(&border_image.source)),
                slice: Some(slice(&border_image.slice)),
                tile: Some(tile(&border_image.repeat)),
            });
        }
        Property::BorderImageSource(image) => {
            style.border_image.get_or_insert_default().source = Some(image_url(image));
        }
        Property::BorderImageSlice(value) => {
            style.border_image.get_or_insert_default().slice = Some(slice(value));
        }
        Property::BorderImageRepeat(repeat) => {
            style.border_image.get_or_insert_default().tile = Some(tile(repeat));
        }
        Property::BorderImageWidth(_) | Property::BorderImageOutset(_) => {
            debug!("html css: border-image-width/-outset unsupported (corners draw at image size)");
        }
        Property::BorderWidth(width) => {
            style.border_width = [&width.top, &width.right, &width.bottom, &width.left]
                .map(side_width);
        }
        Property::BorderTopWidth(width) => style.border_width[0] = side_width(width),
        Property::BorderRightWidth(width) => style.border_width[1] = side_width(width),
        Property::BorderBottomWidth(width) => style.border_width[2] = side_width(width),
        Property::BorderLeftWidth(width) => style.border_width[3] = side_width(width),
        Property::Padding(padding) => {
            style.padding = [&padding.top, &padding.right, &padding.bottom, &padding.left]
                .map(length_px);
        }
        Property::PaddingTop(value) => style.padding[0] = length_px(value),
        Property::PaddingRight(value) => style.padding[1] = length_px(value),
        Property::PaddingBottom(value) => style.padding[2] = length_px(value),
        Property::PaddingLeft(value) => style.padding[3] = length_px(value),
        Property::RowGap(gap) => style.row_gap = gap_px(gap),
        Property::ColumnGap(gap) => style.layout.column_gap = gap_px(gap),
        Property::Gap(gap) => {
            style.row_gap = gap_px(&gap.row);
            style.layout.column_gap = gap_px(&gap.column);
        }
        Property::Display(value) => style.layout.display = display(value),
        Property::FlexDirection(direction, _) => {
            style.layout.flex_direction = Some(flex_direction(direction));
        }
        Property::FlexWrap(wrap, _) => style.layout.flex_wrap = Some(flex_wrap(wrap)),
        Property::FlexFlow(flow, _) => {
            style.layout.flex_direction = Some(flex_direction(&flow.direction));
            style.layout.flex_wrap = Some(flex_wrap(&flow.wrap));
        }
        Property::FlexGrow(grow, _) => style.layout.flex_grow = Some(*grow),
        Property::FlexShrink(shrink, _) => style.layout.flex_shrink = Some(*shrink),
        Property::FlexBasis(basis, _) => style.layout.flex_basis = auto_val(basis),
        Property::Flex(flex, _) => {
            style.layout.flex_grow = Some(flex.grow);
            style.layout.flex_shrink = Some(flex.shrink);
            style.layout.flex_basis = auto_val(&flex.basis);
        }
        Property::JustifyContent(value, _) => {
            style.layout.justify_content = justify_content(value);
        }
        Property::AlignItems(value, _) => style.layout.align_items = align_items(value),
        Property::AlignContent(value, _) => style.layout.align_content = align_content(value),
        Property::AlignSelf(value, _) => style.layout.align_self = align_self(value),
        Property::Width(size) => style.layout.width = size_val(size),
        Property::Height(size) => style.layout.height = size_val(size),
        Property::MinWidth(size) => style.layout.min_width = size_val(size),
        Property::MinHeight(size) => style.layout.min_height = size_val(size),
        Property::MaxWidth(size) => style.layout.max_width = max_size_val(size),
        Property::MaxHeight(size) => style.layout.max_height = max_size_val(size),
        Property::Margin(margin) => {
            style.layout.margin =
                [&margin.top, &margin.right, &margin.bottom, &margin.left].map(auto_val);
        }
        Property::MarginTop(value) => style.layout.margin[0] = auto_val(value),
        Property::MarginRight(value) => style.layout.margin[1] = auto_val(value),
        Property::MarginBottom(value) => style.layout.margin[2] = auto_val(value),
        Property::MarginLeft(value) => style.layout.margin[3] = auto_val(value),
        Property::BoxSizing(sizing, _) => {
            style.layout.box_sizing = Some(match sizing {
                css_size::BoxSizing::ContentBox => BoxSizing::ContentBox,
                css_size::BoxSizing::BorderBox => BoxSizing::BorderBox,
            });
        }
        _ => {}
    }
}

fn image_url(image: &Image) -> Option<String> {
    match image {
        Image::Url(url) => Some(url.url.to_string()),
        Image::None => None,
        _ => {
            debug!("html css: only url() border-image sources are supported");
            None
        }
    }
}

fn slice(value: &BorderImageSlice) -> ([SliceValue; 4], bool) {
    let offsets = &value.offsets;
    let side = |offset: &NumberOrPercentage| match offset {
        NumberOrPercentage::Number(px) => SliceValue::Px(*px),
        NumberOrPercentage::Percentage(percent) => SliceValue::Fraction(percent.0),
    };
    (
        [side(&offsets.0), side(&offsets.1), side(&offsets.2), side(&offsets.3)],
        value.fill,
    )
}

fn tile(repeat: &BorderImageRepeat) -> bool {
    // Bevy has one scale mode for all sides: tile if either axis tiles.
    [repeat.horizontal, repeat.vertical]
        .iter()
        .any(|keyword| !matches!(keyword, BorderImageRepeatKeyword::Stretch))
}

fn side_width(width: &BorderSideWidth) -> Option<f32> {
    match width {
        BorderSideWidth::Thin => Some(1.0),
        BorderSideWidth::Medium => Some(3.0),
        BorderSideWidth::Thick => Some(5.0),
        BorderSideWidth::Length(length) => length.to_px(),
    }
}

/// `row-gap` in px; `normal` (= no override), `%` and `calc()` give `None`.
fn gap_px(gap: &GapValue) -> Option<f32> {
    match gap {
        GapValue::LengthPercentage(LengthPercentage::Dimension(length)) => length.to_px(),
        GapValue::Normal => None,
        GapValue::LengthPercentage(_) => {
            debug!("html css: only absolute lengths are supported for gap");
            None
        }
    }
}

/// `display`: `none`, block-level `flow`/`flow-root` (Bevy block layout) and
/// `flex`; other values (inline, grid, table, …) are unsupported.
fn display(value: &css_display::Display) -> Option<Display> {
    match value {
        css_display::Display::Keyword(css_display::DisplayKeyword::None) => Some(Display::None),
        css_display::Display::Pair(pair) => match (&pair.outside, &pair.inside) {
            (_, DisplayInside::Flex(_)) => Some(Display::Flex),
            (DisplayOutside::Block, DisplayInside::Flow | DisplayInside::FlowRoot) => {
                Some(Display::Block)
            }
            _ => unsupported("display value"),
        },
        css_display::Display::Keyword(_) => unsupported("display value"),
    }
}

fn flex_direction(direction: &css_flex::FlexDirection) -> FlexDirection {
    match direction {
        css_flex::FlexDirection::Row => FlexDirection::Row,
        css_flex::FlexDirection::RowReverse => FlexDirection::RowReverse,
        css_flex::FlexDirection::Column => FlexDirection::Column,
        css_flex::FlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
    }
}

fn flex_wrap(wrap: &css_flex::FlexWrap) -> FlexWrap {
    match wrap {
        css_flex::FlexWrap::NoWrap => FlexWrap::NoWrap,
        css_flex::FlexWrap::Wrap => FlexWrap::Wrap,
        css_flex::FlexWrap::WrapReverse => FlexWrap::WrapReverse,
    }
}

fn justify_content(value: &css_align::JustifyContent) -> Option<JustifyContent> {
    Some(match value {
        css_align::JustifyContent::Normal => JustifyContent::Default,
        css_align::JustifyContent::ContentDistribution(distribution) => match distribution {
            ContentDistribution::SpaceBetween => JustifyContent::SpaceBetween,
            ContentDistribution::SpaceAround => JustifyContent::SpaceAround,
            ContentDistribution::SpaceEvenly => JustifyContent::SpaceEvenly,
            ContentDistribution::Stretch => JustifyContent::Stretch,
        },
        css_align::JustifyContent::ContentPosition { value, .. } => match value {
            ContentPosition::Center => JustifyContent::Center,
            ContentPosition::Start => JustifyContent::Start,
            ContentPosition::End => JustifyContent::End,
            ContentPosition::FlexStart => JustifyContent::FlexStart,
            ContentPosition::FlexEnd => JustifyContent::FlexEnd,
        },
        css_align::JustifyContent::Left { .. } | css_align::JustifyContent::Right { .. } => {
            return unsupported("justify-content left/right");
        }
    })
}

fn align_content(value: &css_align::AlignContent) -> Option<AlignContent> {
    Some(match value {
        css_align::AlignContent::Normal => AlignContent::Default,
        css_align::AlignContent::ContentDistribution(distribution) => match distribution {
            ContentDistribution::SpaceBetween => AlignContent::SpaceBetween,
            ContentDistribution::SpaceAround => AlignContent::SpaceAround,
            ContentDistribution::SpaceEvenly => AlignContent::SpaceEvenly,
            ContentDistribution::Stretch => AlignContent::Stretch,
        },
        css_align::AlignContent::ContentPosition { value, .. } => match value {
            ContentPosition::Center => AlignContent::Center,
            ContentPosition::Start => AlignContent::Start,
            ContentPosition::End => AlignContent::End,
            ContentPosition::FlexStart => AlignContent::FlexStart,
            ContentPosition::FlexEnd => AlignContent::FlexEnd,
        },
        css_align::AlignContent::BaselinePosition(_) => {
            return unsupported("align-content baseline");
        }
    })
}

fn align_items(value: &css_align::AlignItems) -> Option<AlignItems> {
    Some(match value {
        css_align::AlignItems::Normal => AlignItems::Default,
        css_align::AlignItems::Stretch => AlignItems::Stretch,
        css_align::AlignItems::BaselinePosition(BaselinePosition::First) => AlignItems::Baseline,
        css_align::AlignItems::BaselinePosition(BaselinePosition::Last) => {
            return unsupported("last baseline");
        }
        css_align::AlignItems::SelfPosition { value, .. } => match value {
            SelfPosition::Center => AlignItems::Center,
            SelfPosition::Start | SelfPosition::SelfStart => AlignItems::Start,
            SelfPosition::End | SelfPosition::SelfEnd => AlignItems::End,
            SelfPosition::FlexStart => AlignItems::FlexStart,
            SelfPosition::FlexEnd => AlignItems::FlexEnd,
        },
    })
}

fn align_self(value: &css_align::AlignSelf) -> Option<AlignSelf> {
    Some(match value {
        css_align::AlignSelf::Auto => AlignSelf::Auto,
        // `normal` behaves as `stretch` for flex items.
        css_align::AlignSelf::Normal | css_align::AlignSelf::Stretch => AlignSelf::Stretch,
        css_align::AlignSelf::BaselinePosition(BaselinePosition::First) => AlignSelf::Baseline,
        css_align::AlignSelf::BaselinePosition(BaselinePosition::Last) => {
            return unsupported("last baseline");
        }
        css_align::AlignSelf::SelfPosition { value, .. } => match value {
            SelfPosition::Center => AlignSelf::Center,
            SelfPosition::Start | SelfPosition::SelfStart => AlignSelf::Start,
            SelfPosition::End | SelfPosition::SelfEnd => AlignSelf::End,
            SelfPosition::FlexStart => AlignSelf::FlexStart,
            SelfPosition::FlexEnd => AlignSelf::FlexEnd,
        },
    })
}

/// `width`/`height`/`min-*`: `auto`, lengths, `%`.
fn size_val(size: &Size) -> Option<Val> {
    match size {
        Size::Auto => Some(Val::Auto),
        Size::LengthPercentage(value) => length_percentage_val(value),
        _ => unsupported("intrinsic size keyword"),
    }
}

/// `max-*`: `none` (no limit), lengths, `%`.
fn max_size_val(size: &MaxSize) -> Option<Val> {
    match size {
        MaxSize::None => Some(Val::Auto),
        MaxSize::LengthPercentage(value) => length_percentage_val(value),
        _ => unsupported("intrinsic size keyword"),
    }
}

/// `margin`/`flex-basis`: `auto`, lengths, `%`.
fn auto_val(value: &LengthPercentageOrAuto) -> Option<Val> {
    match value {
        LengthPercentageOrAuto::Auto => Some(Val::Auto),
        LengthPercentageOrAuto::LengthPercentage(value) => length_percentage_val(value),
    }
}

/// Absolute lengths, viewport units and `%` (of the parent, as in CSS);
/// font-relative units and `calc()` are unsupported.
fn length_percentage_val(value: &LengthPercentage) -> Option<Val> {
    match value {
        LengthPercentage::Dimension(length) => match length {
            LengthValue::Vw(vw) => Some(Val::Vw(*vw)),
            LengthValue::Vh(vh) => Some(Val::Vh(*vh)),
            LengthValue::Vmin(v) => Some(Val::VMin(*v)),
            LengthValue::Vmax(v) => Some(Val::VMax(*v)),
            length => length.to_px().map(Val::Px).or_else(|| unsupported("relative length")),
        },
        LengthPercentage::Percentage(percentage) => Some(Val::Percent(percentage.0 * 100.0)),
        LengthPercentage::Calc(_) => unsupported("calc()"),
    }
}

fn unsupported<T>(what: &str) -> Option<T> {
    debug!("html css: unsupported {what}");
    None
}

/// `padding` in px; `auto`, `%` and `calc()` are unsupported.
fn length_px(value: &LengthPercentageOrAuto) -> Option<f32> {
    match value {
        LengthPercentageOrAuto::LengthPercentage(LengthPercentage::Dimension(length)) => {
            length.to_px()
        }
        _ => {
            debug!("html css: only absolute lengths are supported for padding");
            None
        }
    }
}

/// `None` for colors without a fixed sRGB value (`currentColor`, system colors).
fn to_color(color: &CssColor) -> Option<Color> {
    let rgba = RGBA::try_from(color).ok()?;
    Some(Color::srgba_u8(rgba.red, rgba.green, rgba.blue, rgba.alpha))
}

fn family_ref(family: &FontFamily) -> Option<FamilyRef> {
    Some(match family {
        FontFamily::FamilyName(name) => FamilyRef::Named(
            name.to_css_string(PrinterOptions::default())
                .ok()?
                .trim_matches(['"', '\''])
                .to_owned(),
        ),
        FontFamily::Generic(generic) => FamilyRef::Generic(match generic {
            GenericFontFamily::Serif => GenericFamily::Serif,
            GenericFontFamily::SansSerif => GenericFamily::SansSerif,
            GenericFontFamily::Monospace => GenericFamily::Monospace,
            GenericFontFamily::Cursive => GenericFamily::Cursive,
            GenericFontFamily::Fantasy => GenericFamily::Fantasy,
            GenericFontFamily::SystemUI => GenericFamily::SystemUi,
            _ => return None,
        }),
    })
}

fn font_size(size: &FontSize) -> Option<FontSizeSpec> {
    Some(match size {
        FontSize::Length(LengthPercentage::Dimension(length)) => match length {
            LengthValue::Px(px) => FontSizeSpec::Px(*px),
            LengthValue::Em(em) => FontSizeSpec::Inherited(*em),
            LengthValue::Rem(rem) => FontSizeSpec::Root(*rem),
            _ => {
                debug!("html css: unsupported font-size unit");
                return None;
            }
        },
        FontSize::Length(LengthPercentage::Percentage(percent)) => {
            FontSizeSpec::Inherited(percent.0)
        }
        FontSize::Length(LengthPercentage::Calc(_)) => {
            debug!("html css: calc() font-size unsupported");
            return None;
        }
        // CSS Fonts 4 keyword scale with `medium` = 16px.
        FontSize::Absolute(keyword) => FontSizeSpec::Px(match keyword {
            AbsoluteFontSize::XXSmall => 9.0,
            AbsoluteFontSize::XSmall => 10.0,
            AbsoluteFontSize::Small => 13.0,
            AbsoluteFontSize::Medium => 16.0,
            AbsoluteFontSize::Large => 18.0,
            AbsoluteFontSize::XLarge => 24.0,
            AbsoluteFontSize::XXLarge => 32.0,
            AbsoluteFontSize::XXXLarge => 48.0,
        }),
        FontSize::Relative(RelativeFontSize::Smaller) => FontSizeSpec::Inherited(1.0 / 1.2),
        FontSize::Relative(RelativeFontSize::Larger) => FontSizeSpec::Inherited(1.2),
    })
}

#[cfg(test)]
mod tests {
    use lightningcss::stylesheet::ParserOptions;

    use super::*;

    fn sheet(css: &'static str) -> StyleSheet<'static> {
        StyleSheet::parse(css, ParserOptions::default()).expect("valid css")
    }

    fn element(tag: &str, id: Option<&str>, classes: &[&str]) -> HtmlElement {
        HtmlElement {
            tag: tag.to_owned(),
            id: id.map(str::to_owned),
            classes: classes.iter().map(|class| (*class).to_owned()).collect(),
        }
    }

    const RED: Color = Color::srgb_u8(255, 0, 0);
    const GREEN: Color = Color::srgb_u8(0, 128, 0);
    const BLUE: Color = Color::srgb_u8(0, 0, 255);

    fn color(css: &'static str, element: &HtmlElement) -> Option<Color> {
        let sheet = sheet(css);
        HtmlStyles::from_sheet(&sheet).get(element).color
    }

    #[test]
    fn more_specific_selector_wins_regardless_of_order() {
        let note = element("p", None, &["note"]);
        assert_eq!(color(".note { color: green } p { color: red }", &note), Some(GREEN));
        assert_eq!(color(".note { color: red } p.note { color: green }", &note), Some(GREEN));
        let titled = element("p", Some("title"), &["note", "big"]);
        assert_eq!(color("#title { color: green } p.note.big { color: red }", &titled), Some(GREEN));
    }

    #[test]
    fn equal_specificity_later_rule_wins() {
        let both = element("p", None, &["a", "b"]);
        assert_eq!(color(".a { color: red } .b { color: green }", &both), Some(GREEN));
        assert_eq!(color(".b { color: green } .a { color: red }", &both), Some(RED));
    }

    #[test]
    fn important_beats_specificity() {
        let titled = element("p", Some("title"), &[]);
        assert_eq!(color("p { color: green !important } #title { color: red }", &titled), Some(GREEN));
    }

    #[test]
    fn compound_requires_every_part() {
        let css = "p.note.big { color: red } h1#x { color: blue }";
        assert_eq!(color(css, &element("p", None, &["note"])), None);
        assert_eq!(color(css, &element("div", None, &["note", "big"])), None);
        assert_eq!(color(css, &element("p", None, &["big", "note", "extra"])), Some(RED));
        assert_eq!(color(css, &element("h1", Some("y"), &[])), None);
        assert_eq!(color(css, &element("h1", Some("x"), &[])), Some(BLUE));
    }

    #[test]
    fn comma_list_matches_each_selector_with_its_own_specificity() {
        // `.note` in the list beats the later, less specific `p`.
        let css = "h1, .note { color: green } p { color: red }";
        assert_eq!(color(css, &element("p", None, &["note"])), Some(GREEN));
        assert_eq!(color(css, &element("p", None, &[])), Some(RED));
        assert_eq!(color(css, &element("h1", None, &[])), Some(GREEN));
    }

    #[test]
    fn universal_loses_to_everything_and_unsupported_selectors_are_skipped() {
        let css = "* { color: red } p { color: green } div p { color: blue } p:hover { color: blue }";
        assert_eq!(color(css, &element("p", None, &[])), Some(GREEN));
        assert_eq!(color(css, &element("span", None, &[])), Some(RED));
    }

    /// Non-ASCII selector characters are unsupported idents here: they must
    /// skip the rule, not panic (a `[1..]` byte slice once crashed on them).
    #[test]
    fn multibyte_selector_characters_are_skipped() {
        let css = "é { color: red } .é { color: red } p.é { color: red } p { color: green }";
        assert_eq!(color(css, &element("p", None, &[])), Some(GREEN));
        assert_eq!(color(css, &element("span", None, &[])), None);
    }

    /// `css` parsed and owned (no borrowed input), like the asset loader does.
    fn owned_sheet(css: &str) -> StyleSheet<'static> {
        use lightningcss::traits::IntoOwned;
        StyleSheet::parse(css, ParserOptions::default()).expect("valid css").into_owned()
    }

    /// `p`'s declared style under one `p { declarations }` rule.
    fn declared(declarations: &str) -> ElementStyle {
        let sheet = owned_sheet(&format!("p {{ {declarations} }}"));
        HtmlStyles::from_sheet(&sheet).get(&element("p", None, &[]))
    }

    /// Every supported layout property and value maps onto the Bevy value
    /// CSS means, through its longhand and its shorthand, so a dropped or
    /// crossed arm shows up per property.
    #[test]
    fn layout_properties_map_to_bevy_values() {
        let layout = |css: &str| declared(css).layout;
        assert_eq!(layout("display: none").display, Some(Display::None));
        assert_eq!(layout("display: block").display, Some(Display::Block));
        assert_eq!(layout("display: flex").display, Some(Display::Flex));
        assert_eq!(layout("display: inline-flex").display, Some(Display::Flex));
        assert_eq!(layout("display: inline").display, None);

        let flow = layout("flex-flow: column-reverse wrap");
        assert_eq!(flow.flex_direction, Some(FlexDirection::ColumnReverse));
        assert_eq!(flow.flex_wrap, Some(FlexWrap::Wrap));
        assert_eq!(layout("flex-wrap: wrap-reverse").flex_wrap, Some(FlexWrap::WrapReverse));

        let flex = layout("flex: 2 3 40px");
        assert_eq!(
            (flex.flex_grow, flex.flex_shrink, flex.flex_basis),
            (Some(2.0), Some(3.0), Some(Val::Px(40.0)))
        );
        assert_eq!(layout("flex-shrink: 0.5").flex_shrink, Some(0.5));
        assert_eq!(layout("flex-basis: auto").flex_basis, Some(Val::Auto));

        assert_eq!(layout("justify-content: normal").justify_content, Some(JustifyContent::Default));
        assert_eq!(layout("justify-content: end").justify_content, Some(JustifyContent::End));
        assert_eq!(layout("justify-content: left").justify_content, None);
        for (value, expected) in [
            ("normal", AlignContent::Default),
            ("space-between", AlignContent::SpaceBetween),
            ("space-around", AlignContent::SpaceAround),
            ("space-evenly", AlignContent::SpaceEvenly),
            ("stretch", AlignContent::Stretch),
            ("center", AlignContent::Center),
            ("start", AlignContent::Start),
            ("end", AlignContent::End),
            ("flex-start", AlignContent::FlexStart),
            ("flex-end", AlignContent::FlexEnd),
        ] {
            let css = format!("align-content: {value}");
            assert_eq!(layout(&css).align_content, Some(expected), "{css}");
        }
        assert_eq!(layout("align-items: baseline").align_items, Some(AlignItems::Baseline));
        assert_eq!(layout("align-self: normal").align_self, Some(AlignSelf::Stretch));

        assert_eq!(layout("width: auto").width, Some(Val::Auto));
        assert_eq!(layout("height: 50%").height, Some(Val::Percent(50.0)));
        assert_eq!(layout("min-height: 10vh").min_height, Some(Val::Vh(10.0)));
        assert_eq!(layout("max-height: 30px").max_height, Some(Val::Px(30.0)));
        assert_eq!(layout("max-width: none").max_width, Some(Val::Auto));
        assert_eq!(layout("width: min-content").width, None);
        assert_eq!(layout("width: 2em").width, None);

        assert_eq!(layout("margin-top: 1px").margin, [Some(Val::Px(1.0)), None, None, None]);
        assert_eq!(layout("margin-right: 2px").margin, [None, Some(Val::Px(2.0)), None, None]);
        assert_eq!(layout("margin-bottom: auto").margin, [None, None, Some(Val::Auto), None]);
        assert_eq!(layout("margin-left: 10%").margin, [None, None, None, Some(Val::Percent(10.0))]);
        assert_eq!(layout("column-gap: 6px").column_gap, Some(6.0));
        assert_eq!(layout("box-sizing: border-box").box_sizing, Some(BoxSizing::BorderBox));
    }

    /// Per-side border widths go to their own side; `border-image-width` and
    /// `-outset` are ignored without disturbing the rest.
    #[test]
    fn border_sides_and_ignored_border_image_parts() {
        let sides = |css: &str| declared(css).border_width;
        assert_eq!(sides("border-top-width: 1px"), [Some(1.0), None, None, None]);
        assert_eq!(sides("border-right-width: 2px"), [None, Some(2.0), None, None]);
        assert_eq!(sides("border-bottom-width: 3px"), [None, None, Some(3.0), None]);
        assert_eq!(sides("border-left-width: 4px"), [None, None, None, Some(4.0)]);
        let style = declared("border-image-width: 9px; border-image-outset: 3px; border-width: 2px");
        assert_eq!(style.border_width, [Some(2.0); 4]);
        assert!(style.border_image.is_none(), "{:?}", style.border_image);
    }

    /// Every `justify-content`, `align-items` and `align-self` value and
    /// the remaining flex keywords map onto the Bevy value CSS means; the
    /// unsupported ones (`left`/`right`, `last baseline`) are skipped.
    #[test]
    fn alignment_values_map_to_bevy_values() {
        let layout = |css: String| declared(&css).layout;
        for (value, expected) in [
            ("space-between", JustifyContent::SpaceBetween),
            ("space-around", JustifyContent::SpaceAround),
            ("space-evenly", JustifyContent::SpaceEvenly),
            ("stretch", JustifyContent::Stretch),
            ("center", JustifyContent::Center),
            ("start", JustifyContent::Start),
            ("flex-start", JustifyContent::FlexStart),
            ("flex-end", JustifyContent::FlexEnd),
        ] {
            assert_eq!(layout(format!("justify-content: {value}")).justify_content, Some(expected), "{value}");
        }
        assert_eq!(layout("justify-content: right".into()).justify_content, None);
        for (value, expected) in [
            ("normal", AlignItems::Default),
            ("stretch", AlignItems::Stretch),
            ("center", AlignItems::Center),
            ("start", AlignItems::Start),
            ("self-start", AlignItems::Start),
            ("end", AlignItems::End),
            ("self-end", AlignItems::End),
            ("flex-start", AlignItems::FlexStart),
            ("flex-end", AlignItems::FlexEnd),
        ] {
            assert_eq!(layout(format!("align-items: {value}")).align_items, Some(expected), "{value}");
        }
        assert_eq!(layout("align-items: last baseline".into()).align_items, None);
        for (value, expected) in [
            ("auto", AlignSelf::Auto),
            ("stretch", AlignSelf::Stretch),
            ("baseline", AlignSelf::Baseline),
            ("center", AlignSelf::Center),
            ("start", AlignSelf::Start),
            ("self-start", AlignSelf::Start),
            ("end", AlignSelf::End),
            ("self-end", AlignSelf::End),
            ("flex-start", AlignSelf::FlexStart),
            ("flex-end", AlignSelf::FlexEnd),
        ] {
            assert_eq!(layout(format!("align-self: {value}")).align_self, Some(expected), "{value}");
        }
        assert_eq!(layout("align-self: last baseline".into()).align_self, None);
        assert_eq!(layout("align-content: baseline".into()).align_content, None);

        assert_eq!(layout("flex-direction: column".into()).flex_direction, Some(FlexDirection::Column));
        assert_eq!(layout("flex-direction: row".into()).flex_direction, Some(FlexDirection::Row));
        assert_eq!(layout("flex-wrap: nowrap".into()).flex_wrap, Some(FlexWrap::NoWrap));
        assert_eq!(layout("display: contents".into()).display, None, "unsupported keyword");
        assert_eq!(layout("box-sizing: content-box".into()).box_sizing, Some(BoxSizing::ContentBox));
    }

    /// Lengths, keywords and fallbacks: viewport units, the border-width
    /// keywords, the CSS Fonts 4 font-size keyword scale (medium = 16px),
    /// `normal`/`lighter` weights; `calc()`, `%` padding/gap, intrinsic max
    /// sizes and non-`url()` border-image sources are skipped (left
    /// undeclared), never mis-mapped.
    #[test]
    fn units_keywords_and_unsupported_values() {
        let layout = |css: &str| declared(css).layout;
        assert_eq!(layout("min-width: 10vmin").min_width, Some(Val::VMin(10.0)));
        assert_eq!(layout("max-width: 20vmax").max_width, Some(Val::VMax(20.0)));
        assert_eq!(layout("width: calc(10px + 5%)").width, None);
        assert_eq!(layout("max-height: min-content").max_height, None);

        assert_eq!(declared("border-top-width: thin").border_width[0], Some(1.0));
        assert_eq!(declared("border-top-width: medium").border_width[0], Some(3.0));
        assert_eq!(declared("border-top-width: thick").border_width[0], Some(5.0));
        assert_eq!(declared("padding-top: 10%").padding[0], None);
        assert_eq!(declared("row-gap: normal").row_gap, None);
        assert_eq!(declared("row-gap: 5%").row_gap, None);

        let size = |css: &str| declared(css).font_size.map(|size| size.resolve(20.0, 10.0));
        for (keyword, px) in [
            ("xx-small", 9.0),
            ("x-small", 10.0),
            ("small", 13.0),
            ("medium", 16.0),
            ("large", 18.0),
            ("x-large", 24.0),
            ("xx-large", 32.0),
            ("xxx-large", 48.0),
        ] {
            assert_eq!(size(&format!("font-size: {keyword}")), Some(px), "{keyword}");
        }
        assert_eq!(size("font-size: calc(1em + 2px)"), None);
        assert_eq!(declared("font-weight: normal").bold, Some(false));
        assert_eq!(declared("font-weight: lighter").bold, Some(false));

        let source = declared("border-image-source: linear-gradient(red, blue)")
            .border_image
            .and_then(|decl| decl.source);
        assert!(matches!(source, Some(None)), "gradient source treated as no image: {source:?}");
    }

    /// `font-weight` 500 stays regular, anything above picks bold (the CSS
    /// font-matching boundary).
    #[test]
    fn font_weight_boundary_is_above_500() {
        assert_eq!(declared("font-weight: 500").bold, Some(false));
        assert_eq!(declared("font-weight: 501").bold, Some(true));
    }

    /// Each CSS generic family keyword maps to its own `GenericFamily`.
    #[test]
    fn generic_family_keywords_map_one_to_one() {
        for (keyword, generic) in [
            ("serif", GenericFamily::Serif),
            ("sans-serif", GenericFamily::SansSerif),
            ("monospace", GenericFamily::Monospace),
            ("cursive", GenericFamily::Cursive),
            ("fantasy", GenericFamily::Fantasy),
            ("system-ui", GenericFamily::SystemUi),
        ] {
            match declared(&format!("font-family: {keyword}")).font_family.as_deref() {
                Some([FamilyRef::Generic(found)]) => assert_eq!(*found, generic, "{keyword}"),
                other => panic!("{keyword}: {other:?}"),
            }
        }
    }

    /// `border-image-source: none` declares "no image" (overriding an
    /// earlier shorthand), and a sheet's image URLs include longhand sources,
    /// so they load as dependencies.
    #[test]
    fn border_image_sources_none_and_longhands() {
        let style = declared(r#"border-image: url("a.png") 4; border-image-source: none"#);
        assert!(matches!(style.border_image.and_then(|decl| decl.source), Some(None)));
        let sheet = owned_sheet(r#"p { border-image-source: url("b.png") } div { border-image: url("c.png") 4 }"#);
        assert_eq!(image_urls(&sheet), ["b.png", "c.png"]);
    }
}
