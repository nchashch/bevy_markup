//! Stylesheet → declared style per element (the subset documented in
//! [`crate::style`]): compound selectors matched against tag, id and classes.

use std::cell::RefCell;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::ui::{GridTrackRepetition, MaxTrackSizingFunction, MinTrackSizingFunction};
use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::Property;
use lightningcss::properties::align::{
    self as css_align, BaselinePosition, ContentDistribution, ContentPosition, GapValue,
    SelfPosition,
};
use lightningcss::properties::border::BorderSideWidth;
use lightningcss::properties::border_image::{
    BorderImageRepeat, BorderImageRepeatKeyword, BorderImageSlice,
};
use lightningcss::properties::custom::CustomPropertyName;
use lightningcss::properties::display::{self as css_display, DisplayInside, DisplayOutside};
use lightningcss::properties::flex as css_flex;
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight,
    GenericFontFamily, RelativeFontSize,
};
use lightningcss::properties::grid::{
    self as css_grid, RepeatCount, TrackBreadth, TrackListItem, TrackSize, TrackSizing,
};
use lightningcss::properties::position as css_position;
use lightningcss::properties::size::{self as css_size, MaxSize, Size};
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleAttribute, StyleSheet};
use lightningcss::traits::IntoOwned;
use lightningcss::traits::ToCss;
use lightningcss::values::color::{CssColor, RGBA};
use lightningcss::values::image::Image;
use lightningcss::values::length::{LengthPercentage, LengthPercentageOrAuto, LengthValue};
use lightningcss::values::percentage::NumberOrPercentage;
use lightningcss::values::size::Size2D;

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

/// One side (start or end) of a declared `grid-row`/`grid-column`
/// placement; named lines and areas are unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridLineDecl {
    Auto,
    /// A line number (negative counts from the end).
    Line(i16),
    Span(u16),
}

/// Declared flex, grid, size and margin properties of a container or block,
/// as Bevy values (`None` = not declared).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LayoutDecl {
    pub display: Option<Display>,
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub align_self: Option<AlignSelf>,
    pub justify_items: Option<JustifyItems>,
    pub justify_self: Option<JustifySelf>,
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
    pub grid_template_rows: Option<Vec<RepeatedGridTrack>>,
    pub grid_template_columns: Option<Vec<RepeatedGridTrack>>,
    pub grid_auto_rows: Option<Vec<GridTrack>>,
    pub grid_auto_columns: Option<Vec<GridTrack>>,
    pub grid_auto_flow: Option<GridAutoFlow>,
    /// `[start, end]` of `grid-row`.
    pub grid_row: [Option<GridLineDecl>; 2],
    /// `[start, end]` of `grid-column`.
    pub grid_column: [Option<GridLineDecl>; 2],
    /// `position`.
    pub position: Option<CssPosition>,
    /// `top`/`right`/`bottom`/`left` (`[top, right, bottom, left]`); they
    /// only apply to a `relative`/`absolute` element, as in CSS.
    pub inset: [Option<Val>; 4],
    /// `[top-left, top-right, bottom-right, bottom-left]` radii.
    pub border_radius: [Option<Val>; 4],
}

/// A declared `position`. `static` is CSS's initial value: Bevy's
/// `PositionType::Relative` with the insets ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CssPosition {
    Static,
    Relative,
    Absolute,
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
        set(&mut node.justify_items, &self.justify_items);
        set(&mut node.justify_self, &self.justify_self);
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
        set(&mut node.grid_template_rows, &self.grid_template_rows);
        set(&mut node.grid_template_columns, &self.grid_template_columns);
        set(&mut node.grid_auto_rows, &self.grid_auto_rows);
        set(&mut node.grid_auto_columns, &self.grid_auto_columns);
        set(&mut node.grid_auto_flow, &self.grid_auto_flow);
        set(&mut node.grid_row, &grid_placement(self.grid_row));
        set(&mut node.grid_column, &grid_placement(self.grid_column));
        let corners = &mut node.border_radius;
        set(&mut corners.top_left, &self.border_radius[0]);
        set(&mut corners.top_right, &self.border_radius[1]);
        set(&mut corners.bottom_right, &self.border_radius[2]);
        set(&mut corners.bottom_left, &self.border_radius[3]);
        let position_type = match self.position {
            Some(CssPosition::Absolute) => PositionType::Absolute,
            Some(CssPosition::Relative) => PositionType::Relative,
            // Static: insets are ignored (CSS), so they stay `auto`.
            Some(CssPosition::Static) | None => return,
        };
        node.position_type = position_type;
        set(&mut node.top, &self.inset[0]);
        set(&mut node.right, &self.inset[1]);
        set(&mut node.bottom, &self.inset[2]);
        set(&mut node.left, &self.inset[3]);
    }

    /// Undoes [`apply_to`](Self::apply_to): copies every field this
    /// declaration sets from `base` into `node`, leaving the rest alone.
    pub fn restore_from(&self, base: &Node, node: &mut Node) {
        fn put<T: Clone, D>(target: &mut T, base: &T, declared: &Option<D>) {
            if declared.is_some() {
                *target = base.clone();
            }
        }
        put(&mut node.display, &base.display, &self.display);
        put(
            &mut node.flex_direction,
            &base.flex_direction,
            &self.flex_direction,
        );
        put(&mut node.flex_wrap, &base.flex_wrap, &self.flex_wrap);
        put(
            &mut node.justify_content,
            &base.justify_content,
            &self.justify_content,
        );
        put(&mut node.align_items, &base.align_items, &self.align_items);
        put(
            &mut node.align_content,
            &base.align_content,
            &self.align_content,
        );
        put(&mut node.align_self, &base.align_self, &self.align_self);
        put(
            &mut node.justify_items,
            &base.justify_items,
            &self.justify_items,
        );
        put(
            &mut node.justify_self,
            &base.justify_self,
            &self.justify_self,
        );
        put(&mut node.flex_grow, &base.flex_grow, &self.flex_grow);
        put(&mut node.flex_shrink, &base.flex_shrink, &self.flex_shrink);
        put(&mut node.flex_basis, &base.flex_basis, &self.flex_basis);
        put(&mut node.width, &base.width, &self.width);
        put(&mut node.height, &base.height, &self.height);
        put(&mut node.min_width, &base.min_width, &self.min_width);
        put(&mut node.min_height, &base.min_height, &self.min_height);
        put(&mut node.max_width, &base.max_width, &self.max_width);
        put(&mut node.max_height, &base.max_height, &self.max_height);
        put(&mut node.margin.top, &base.margin.top, &self.margin[0]);
        put(&mut node.margin.right, &base.margin.right, &self.margin[1]);
        put(
            &mut node.margin.bottom,
            &base.margin.bottom,
            &self.margin[2],
        );
        put(&mut node.margin.left, &base.margin.left, &self.margin[3]);
        put(&mut node.column_gap, &base.column_gap, &self.column_gap);
        put(&mut node.box_sizing, &base.box_sizing, &self.box_sizing);
        put(
            &mut node.grid_template_rows,
            &base.grid_template_rows,
            &self.grid_template_rows,
        );
        put(
            &mut node.grid_template_columns,
            &base.grid_template_columns,
            &self.grid_template_columns,
        );
        put(
            &mut node.grid_auto_rows,
            &base.grid_auto_rows,
            &self.grid_auto_rows,
        );
        put(
            &mut node.grid_auto_columns,
            &base.grid_auto_columns,
            &self.grid_auto_columns,
        );
        put(
            &mut node.grid_auto_flow,
            &base.grid_auto_flow,
            &self.grid_auto_flow,
        );
        put(
            &mut node.grid_row,
            &base.grid_row,
            &grid_placement(self.grid_row),
        );
        put(
            &mut node.grid_column,
            &base.grid_column,
            &grid_placement(self.grid_column),
        );
        let (corners, base_corners) = (&mut node.border_radius, &base.border_radius);
        put(
            &mut corners.top_left,
            &base_corners.top_left,
            &self.border_radius[0],
        );
        put(
            &mut corners.top_right,
            &base_corners.top_right,
            &self.border_radius[1],
        );
        put(
            &mut corners.bottom_right,
            &base_corners.bottom_right,
            &self.border_radius[2],
        );
        put(
            &mut corners.bottom_left,
            &base_corners.bottom_left,
            &self.border_radius[3],
        );
        // `apply_to` sets the position and insets only for `relative`/`absolute`.
        if matches!(
            self.position,
            Some(CssPosition::Absolute | CssPosition::Relative)
        ) {
            node.position_type = base.position_type;
            put(&mut node.top, &base.top, &self.inset[0]);
            put(&mut node.right, &base.right, &self.inset[1]);
            put(&mut node.bottom, &base.bottom, &self.inset[2]);
            put(&mut node.left, &base.left, &self.inset[3]);
        }
    }
}

/// A declared `[start, end]` pair as a Bevy placement, resolved as CSS
/// grid placement does: two spans keep the start's; a line with a span
/// spans from (or back to) the line. `None` when neither side is declared.
fn grid_placement(sides: [Option<GridLineDecl>; 2]) -> Option<GridPlacement> {
    use GridLineDecl::{Auto, Line, Span};
    if sides == [None, None] {
        return None;
    }
    let [start, end] = sides.map(|side| side.unwrap_or(Auto));
    Some(match (start, end) {
        (Auto, Auto) => GridPlacement::auto(),
        (Line(start), Auto) => GridPlacement::start(start),
        (Auto, Line(end)) => GridPlacement::end(end),
        (Line(start), Line(end)) => GridPlacement::start_end(start, end),
        (Span(span), Auto | Span(_)) | (Auto, Span(span)) => GridPlacement::span(span),
        (Line(start), Span(span)) => GridPlacement::start_span(start, span),
        (Span(span), Line(end)) => GridPlacement::end_span(end, span),
    })
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
    /// `[top, right, bottom, left]` border colors. Undeclared sides stay
    /// transparent (CSS's initial `currentColor` would draw a border around
    /// every `border-image` frame).
    pub border_color: [Option<Color>; 4],
    /// `[top, right, bottom, left]` in px.
    pub padding: [Option<f32>; 4],
    /// `row-gap` (or `gap`'s row part) in px, for containers.
    pub row_gap: Option<f32>,
    /// `z-index`: `Some(None)` is `auto`.
    pub z_index: Option<Option<i32>>,
    /// `pointer-events` (inherited): `true` for `auto`, `false` for `none`.
    pub pointer_events: Option<bool>,
    /// `opacity`, clamped to `0..=1` (group opacity: multiplies down the
    /// subtree).
    pub opacity: Option<f32>,
    /// `outline` (shorthand and longhands, `outline-offset`).
    pub outline: OutlineDecl,
    /// Flex, size and margin properties (containers and blocks).
    pub layout: LayoutDecl,
}

/// Declared `outline` parts (each optional so longhands override parts of
/// an earlier shorthand). Drawn only when the style is a visible one, as in
/// CSS (the initial style is `none`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct OutlineDecl {
    /// `true` for a drawn style (`solid`, `auto`, …), `false` for `none`.
    pub visible: Option<bool>,
    /// In px; CSS's initial `medium` is 3px.
    pub width: Option<f32>,
    /// `Some(None)` = `currentColor` (the element's text color).
    pub color: Option<Option<Color>>,
    /// `outline-offset` in px.
    pub offset: Option<f32>,
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

/// Interaction pseudo-classes a compound selector requires: the rule only
/// matches elements whose [`PseudoState`](crate::signals::PseudoState) has
/// these set. Specificity counts them like classes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Pseudo {
    pub hover: bool,
    pub active: bool,
    /// `:focus`: the element holds `InputFocus`.
    pub focus: bool,
    /// `:focus-visible`: focused, and the focus should be shown
    /// (`InputFocusVisible`: set by keyboard/gamepad navigation, cleared by
    /// a pointer press — the browser heuristic).
    pub focus_visible: bool,
}

/// A compound selector: optional type (or `*`), then any number of `.class`
/// and `#id` parts, e.g. `p.note`, `.a.b`, `#title`; optionally qualified
/// by `:hover` / `:active` / `:focus` / `:focus-visible`.
#[derive(Clone, Debug)]
struct Compound {
    /// Lowercase; `None` for `*` or no type.
    tag: Option<String>,
    ids: Vec<String>,
    classes: Vec<String>,
    pseudo: Pseudo,
}

impl Compound {
    /// `None` for anything else (combinators, attributes, other
    /// pseudo-classes).
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
        let mut pseudo = Pseudo::default();
        while let Some((_, kind)) = rest.char_indices().next() {
            // Only `.class`, `#id` and `:pseudo` parts continue the selector;
            // anything else (combinator, attribute, or a non-ASCII ident
            // character) is unsupported. Check before slicing: `kind.len_utf8()`
            // may be > 1.
            let body = match kind {
                '.' | '#' => &rest[kind.len_utf8()..],
                ':' => {
                    let body = &rest[1..];
                    let end = body.find(|c| !is_ident(c)).unwrap_or(body.len());
                    match &body[..end] {
                        "hover" => pseudo.hover = true,
                        "active" => pseudo.active = true,
                        "focus" => pseudo.focus = true,
                        "focus-visible" => pseudo.focus_visible = true,
                        _ => return None,
                    }
                    rest = &body[end..];
                    continue;
                }
                _ => return None,
            };
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
        Some(Self {
            tag,
            ids,
            classes,
            pseudo,
        })
    }

    /// CSS specificity: (ids, classes, types); pseudo-classes count as
    /// classes.
    fn specificity(&self) -> (u32, u32, u32) {
        let classes = self.classes.len()
            + usize::from(self.pseudo.hover)
            + usize::from(self.pseudo.active)
            + usize::from(self.pseudo.focus)
            + usize::from(self.pseudo.focus_visible);
        (
            self.ids.len() as u32,
            classes as u32,
            self.tag.is_some() as u32,
        )
    }

    fn matches(&self, element: &HtmlElement, pseudo: Pseudo) -> bool {
        (!self.pseudo.hover || pseudo.hover)
            && (!self.pseudo.active || pseudo.active)
            && (!self.pseudo.focus || pseudo.focus)
            && (!self.pseudo.focus_visible || pseudo.focus_visible)
            && self.tag.as_ref().is_none_or(|tag| *tag == element.tag)
            && self.ids.iter().all(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|class| element.has_class(class))
    }
}

/// An element's `style` attribute, parsed: declarations that apply to that
/// element only, after every normal rule and — for `!important` ones — after
/// every rule.
#[derive(Debug)]
pub(crate) struct InlineStyle(DeclarationBlock<'static>);

/// `tag`'s `style` attribute, if it has declarations.
pub(crate) fn inline_style(tag: &tl::HTMLTag) -> Option<InlineStyle> {
    let text =
        crate::template::decode_entities(&tag.attributes().get("style").flatten()?.as_utf8_str());
    match StyleAttribute::parse(&text, ParserOptions::default()) {
        Ok(attribute) => {
            let block = attribute.declarations.into_owned();
            (!block.declarations.is_empty() || !block.important_declarations.is_empty())
                .then_some(InlineStyle(block))
        }
        Err(error) => {
            debug!("html css: invalid style attribute ({error}): {text:?}");
            None
        }
    }
}

/// One selector of a style rule with one importance level's declarations.
struct Rule<'a> {
    selector: Compound,
    /// Sort key: importance, then specificity, then source order.
    rank: (bool, (u32, u32, u32), usize),
    declarations: &'a [Property<'static>],
}

/// A stylesheet's rules, matched per element (cached by tag + id + classes
/// + pseudo-state).
#[derive(Default)]
pub(crate) struct HtmlStyles<'a> {
    rules: Vec<Rule<'a>>,
    cache: RefCell<HashMap<(String, Option<String>, Vec<String>, Pseudo), ElementStyle>>,
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

    /// The declared style for `element` in the given interaction state:
    /// every matching rule's declarations, applied in cascade order.
    pub fn get(&self, element: &HtmlElement, pseudo: Pseudo) -> ElementStyle {
        let key = (
            element.tag.clone(),
            element.id.clone(),
            element.classes.clone(),
            pseudo,
        );
        if let Some(style) = self.cache.borrow().get(&key) {
            return style.clone();
        }
        let mut style = ElementStyle::default();
        for rule in self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, pseudo))
        {
            for declaration in rule.declarations {
                apply(&mut style, declaration);
            }
        }
        self.cache.borrow_mut().insert(key, style.clone());
        style
    }

    /// [`get`](Self::get) with the element's `style` attribute: normal rules,
    /// then its normal declarations, then `!important` rules, then its
    /// `!important` declarations (CSS's order). Not cached: the attribute
    /// belongs to one element.
    pub fn get_with(
        &self,
        element: &HtmlElement,
        pseudo: Pseudo,
        inline: Option<&InlineStyle>,
    ) -> ElementStyle {
        let Some(InlineStyle(block)) = inline else {
            return self.get(element, pseudo);
        };
        let mut style = ElementStyle::default();
        let matching: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, pseudo))
            .collect();
        // `rules` is sorted by rank, whose first key is importance.
        let (important, normal): (Vec<&Rule>, Vec<&Rule>) =
            matching.into_iter().partition(|rule| rule.rank.0);
        let rules = |rules: Vec<&Rule<'a>>| {
            rules
                .into_iter()
                .flat_map(|rule| rule.declarations.iter())
                .collect::<Vec<_>>()
        };
        for declaration in rules(normal)
            .into_iter()
            .chain(&block.declarations)
            .chain(rules(important))
            .chain(&block.important_declarations)
        {
            apply(&mut style, declaration);
        }
        style
    }
}

fn apply(style: &mut ElementStyle, declaration: &Property) {
    match declaration {
        Property::Opacity(alpha) => {
            style.opacity = Some(alpha.0.clamp(0.0, 1.0));
        }
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
            style.border_width =
                [&width.top, &width.right, &width.bottom, &width.left].map(side_width);
        }
        Property::BorderTopWidth(width) => style.border_width[0] = side_width(width),
        Property::BorderRightWidth(width) => style.border_width[1] = side_width(width),
        Property::BorderBottomWidth(width) => style.border_width[2] = side_width(width),
        Property::BorderLeftWidth(width) => style.border_width[3] = side_width(width),
        Property::Padding(padding) => {
            style.padding =
                [&padding.top, &padding.right, &padding.bottom, &padding.left].map(length_px);
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
        Property::JustifyItems(value) => style.layout.justify_items = justify_items(value),
        Property::JustifySelf(value) => style.layout.justify_self = justify_self(value),
        Property::GridTemplateRows(rows) => style.layout.grid_template_rows = track_sizing(rows),
        Property::GridTemplateColumns(columns) => {
            style.layout.grid_template_columns = track_sizing(columns);
        }
        Property::GridTemplate(template) => {
            areas_unsupported(&template.areas);
            style.layout.grid_template_rows = track_sizing(&template.rows);
            style.layout.grid_template_columns = track_sizing(&template.columns);
        }
        Property::GridAutoRows(tracks) => style.layout.grid_auto_rows = track_size_list(tracks),
        Property::GridAutoColumns(tracks) => {
            style.layout.grid_auto_columns = track_size_list(tracks);
        }
        Property::GridAutoFlow(flow) => style.layout.grid_auto_flow = Some(grid_auto_flow(*flow)),
        Property::Grid(grid) => {
            areas_unsupported(&grid.areas);
            style.layout.grid_template_rows = track_sizing(&grid.rows);
            style.layout.grid_template_columns = track_sizing(&grid.columns);
            style.layout.grid_auto_rows = track_size_list(&grid.auto_rows);
            style.layout.grid_auto_columns = track_size_list(&grid.auto_columns);
            style.layout.grid_auto_flow = Some(grid_auto_flow(grid.auto_flow));
        }
        Property::GridTemplateAreas(areas) => areas_unsupported(areas),
        Property::GridRowStart(line) => style.layout.grid_row[0] = grid_line(line),
        Property::GridRowEnd(line) => style.layout.grid_row[1] = grid_line(line),
        Property::GridColumnStart(line) => style.layout.grid_column[0] = grid_line(line),
        Property::GridColumnEnd(line) => style.layout.grid_column[1] = grid_line(line),
        Property::GridRow(row) => {
            style.layout.grid_row = [grid_line(&row.start), grid_line(&row.end)];
        }
        Property::GridColumn(column) => {
            style.layout.grid_column = [grid_line(&column.start), grid_line(&column.end)];
        }
        Property::GridArea(area) => {
            style.layout.grid_row = [grid_line(&area.row_start), grid_line(&area.row_end)];
            style.layout.grid_column = [grid_line(&area.column_start), grid_line(&area.column_end)];
        }
        Property::Position(position) => {
            style.layout.position = match position {
                css_position::Position::Static => Some(CssPosition::Static),
                css_position::Position::Relative => Some(CssPosition::Relative),
                css_position::Position::Absolute => Some(CssPosition::Absolute),
                css_position::Position::Sticky(_) | css_position::Position::Fixed => {
                    unsupported("position sticky/fixed")
                }
            };
        }
        Property::Top(value) => style.layout.inset[0] = auto_val(value),
        Property::Right(value) => style.layout.inset[1] = auto_val(value),
        Property::Bottom(value) => style.layout.inset[2] = auto_val(value),
        Property::Left(value) => style.layout.inset[3] = auto_val(value),
        Property::Inset(inset) => {
            style.layout.inset =
                [&inset.top, &inset.right, &inset.bottom, &inset.left].map(auto_val);
        }
        Property::ZIndex(z) => {
            style.z_index = Some(match z {
                css_position::ZIndex::Auto => None,
                css_position::ZIndex::Integer(z) => Some(*z),
            });
        }
        Property::BorderColor(colors) => {
            style.border_color =
                [&colors.top, &colors.right, &colors.bottom, &colors.left].map(to_color);
        }
        Property::BorderTopColor(color) => style.border_color[0] = to_color(color),
        Property::BorderRightColor(color) => style.border_color[1] = to_color(color),
        Property::BorderBottomColor(color) => style.border_color[2] = to_color(color),
        Property::BorderLeftColor(color) => style.border_color[3] = to_color(color),
        Property::BorderRadius(radius, _) => {
            style.layout.border_radius = [
                &radius.top_left,
                &radius.top_right,
                &radius.bottom_right,
                &radius.bottom_left,
            ]
            .map(corner_radius);
        }
        Property::BorderTopLeftRadius(radius, _) => {
            style.layout.border_radius[0] = corner_radius(radius);
        }
        Property::BorderTopRightRadius(radius, _) => {
            style.layout.border_radius[1] = corner_radius(radius);
        }
        Property::BorderBottomRightRadius(radius, _) => {
            style.layout.border_radius[2] = corner_radius(radius);
        }
        Property::BorderBottomLeftRadius(radius, _) => {
            style.layout.border_radius[3] = corner_radius(radius);
        }
        Property::Outline(outline) => {
            style.outline = OutlineDecl {
                visible: Some(outline_visible(&outline.style)),
                width: side_width(&outline.width),
                color: Some(to_color(&outline.color)),
                offset: style.outline.offset,
            };
        }
        Property::OutlineStyle(outline_style) => {
            style.outline.visible = Some(outline_visible(outline_style));
        }
        Property::OutlineWidth(width) => style.outline.width = side_width(width),
        Property::OutlineColor(color) => style.outline.color = Some(to_color(color)),
        // lightningcss has no typed `outline-offset` either.
        Property::Custom(custom) if matches!(&custom.name, CustomPropertyName::Unknown(name) if name.as_ref() == "outline-offset") =>
        {
            let value = declaration
                .value_to_css_string(PrinterOptions::default())
                .unwrap_or_default();
            style.outline.offset = match value.trim().strip_suffix("px").map(str::parse::<f32>) {
                Some(Ok(px)) => Some(px),
                _ if value.trim() == "0" => Some(0.0),
                _ => unsupported("outline-offset value (px only)"),
            };
        }
        // lightningcss has no typed `pointer-events` (it's SVG/UI-only): it
        // arrives as an unknown property with its raw value.
        Property::Custom(custom) if matches!(&custom.name, CustomPropertyName::Unknown(name) if name.as_ref() == "pointer-events") =>
        {
            let value = declaration
                .value_to_css_string(PrinterOptions::default())
                .unwrap_or_default();
            style.pointer_events = match value.trim().to_ascii_lowercase().as_str() {
                "none" => Some(false),
                "auto" => Some(true),
                _ => unsupported("pointer-events value (only `auto`/`none`)"),
            };
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
        [
            side(&offsets.0),
            side(&offsets.1),
            side(&offsets.2),
            side(&offsets.3),
        ],
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

/// `display`: `none`, block-level `flow`/`flow-root` (Bevy block layout),
/// `flex` and `grid` (inline variants too); other values (inline, table, …)
/// are unsupported.
fn display(value: &css_display::Display) -> Option<Display> {
    match value {
        css_display::Display::Keyword(css_display::DisplayKeyword::None) => Some(Display::None),
        css_display::Display::Pair(pair) => match (&pair.outside, &pair.inside) {
            (_, DisplayInside::Flex(_)) => Some(Display::Flex),
            (_, DisplayInside::Grid) => Some(Display::Grid),
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

fn justify_items(value: &css_align::JustifyItems) -> Option<JustifyItems> {
    Some(match value {
        css_align::JustifyItems::Normal => JustifyItems::Default,
        css_align::JustifyItems::Stretch => JustifyItems::Stretch,
        css_align::JustifyItems::BaselinePosition(BaselinePosition::First) => {
            JustifyItems::Baseline
        }
        css_align::JustifyItems::SelfPosition { value, .. } => match value {
            SelfPosition::Center => JustifyItems::Center,
            SelfPosition::Start | SelfPosition::SelfStart | SelfPosition::FlexStart => {
                JustifyItems::Start
            }
            SelfPosition::End | SelfPosition::SelfEnd | SelfPosition::FlexEnd => JustifyItems::End,
        },
        css_align::JustifyItems::BaselinePosition(BaselinePosition::Last)
        | css_align::JustifyItems::Left { .. }
        | css_align::JustifyItems::Right { .. }
        | css_align::JustifyItems::Legacy(_) => {
            return unsupported("justify-items last baseline/left/right/legacy");
        }
    })
}

fn justify_self(value: &css_align::JustifySelf) -> Option<JustifySelf> {
    Some(match value {
        css_align::JustifySelf::Auto => JustifySelf::Auto,
        // `normal` behaves as `stretch` for grid items.
        css_align::JustifySelf::Normal | css_align::JustifySelf::Stretch => JustifySelf::Stretch,
        css_align::JustifySelf::BaselinePosition(BaselinePosition::First) => JustifySelf::Baseline,
        css_align::JustifySelf::SelfPosition { value, .. } => match value {
            SelfPosition::Center => JustifySelf::Center,
            SelfPosition::Start | SelfPosition::SelfStart | SelfPosition::FlexStart => {
                JustifySelf::Start
            }
            SelfPosition::End | SelfPosition::SelfEnd | SelfPosition::FlexEnd => JustifySelf::End,
        },
        css_align::JustifySelf::BaselinePosition(BaselinePosition::Last)
        | css_align::JustifySelf::Left { .. }
        | css_align::JustifySelf::Right { .. } => {
            return unsupported("justify-self last baseline/left/right");
        }
    })
}

/// `grid-template-rows`/`-columns`: `none` (no explicit tracks) or a track
/// list; line names are ignored (placements can't refer to them). One
/// unsupported track drops the whole declaration, as an invalid value would.
fn track_sizing(sizing: &TrackSizing) -> Option<Vec<RepeatedGridTrack>> {
    let list = match sizing {
        TrackSizing::None => return Some(Vec::new()),
        TrackSizing::TrackList(list) => list,
    };
    if list.line_names.iter().any(|names| !names.is_empty()) {
        debug!("html css: grid line names are ignored");
    }
    list.items
        .iter()
        .map(|item| match item {
            TrackListItem::TrackSize(size) => grid_track(size).map(RepeatedGridTrack::from),
            TrackListItem::TrackRepeat(repeat) => {
                if repeat.line_names.iter().any(|names| !names.is_empty()) {
                    debug!("html css: grid line names are ignored");
                }
                let count = match repeat.count {
                    RepeatCount::Number(n) => GridTrackRepetition::Count(u16::try_from(n).ok()?),
                    RepeatCount::AutoFill => GridTrackRepetition::AutoFill,
                    RepeatCount::AutoFit => GridTrackRepetition::AutoFit,
                };
                let tracks: Option<Vec<GridTrack>> =
                    repeat.track_sizes.iter().map(grid_track).collect();
                Some(RepeatedGridTrack::repeat_many(count, tracks?))
            }
        })
        .collect()
}

/// `grid-auto-rows`/`-columns`.
fn track_size_list(list: &css_grid::TrackSizeList) -> Option<Vec<GridTrack>> {
    list.0.iter().map(grid_track).collect()
}

/// One track size: a breadth (`<length>`, `%`, viewport units, `fr`,
/// `auto`, `min-content`, `max-content`), `minmax()` or `fit-content()`.
fn grid_track(size: &TrackSize) -> Option<GridTrack> {
    match size {
        // A bare `fr` track is `minmax(auto, <fr>)`.
        TrackSize::TrackBreadth(TrackBreadth::Flex(fr)) => Some(GridTrack::fr(*fr)),
        TrackSize::TrackBreadth(breadth) => Some(GridTrack::minmax(
            min_breadth(breadth)?,
            max_breadth(breadth)?,
        )),
        TrackSize::MinMax { min, max } => {
            Some(GridTrack::minmax(min_breadth(min)?, max_breadth(max)?))
        }
        TrackSize::FitContent(limit) => match length_percentage_val(limit)? {
            Val::Px(px) => Some(GridTrack::fit_content_px(px)),
            Val::Percent(percent) => Some(GridTrack::fit_content_percent(percent)),
            _ => unsupported("fit-content() limit"),
        },
    }
}

/// A track's minimum: never `fr` (invalid CSS there).
fn min_breadth(breadth: &TrackBreadth) -> Option<MinTrackSizingFunction> {
    Some(match breadth {
        TrackBreadth::Length(length) => match length_percentage_val(length)? {
            Val::Px(v) => MinTrackSizingFunction::Px(v),
            Val::Percent(v) => MinTrackSizingFunction::Percent(v),
            Val::Vw(v) => MinTrackSizingFunction::Vw(v),
            Val::Vh(v) => MinTrackSizingFunction::Vh(v),
            Val::VMin(v) => MinTrackSizingFunction::VMin(v),
            Val::VMax(v) => MinTrackSizingFunction::VMax(v),
            Val::Auto => MinTrackSizingFunction::Auto,
        },
        TrackBreadth::Auto => MinTrackSizingFunction::Auto,
        TrackBreadth::MinContent => MinTrackSizingFunction::MinContent,
        TrackBreadth::MaxContent => MinTrackSizingFunction::MaxContent,
        TrackBreadth::Flex(_) => return unsupported("fr as a track minimum"),
    })
}

fn max_breadth(breadth: &TrackBreadth) -> Option<MaxTrackSizingFunction> {
    Some(match breadth {
        TrackBreadth::Length(length) => match length_percentage_val(length)? {
            Val::Px(v) => MaxTrackSizingFunction::Px(v),
            Val::Percent(v) => MaxTrackSizingFunction::Percent(v),
            Val::Vw(v) => MaxTrackSizingFunction::Vw(v),
            Val::Vh(v) => MaxTrackSizingFunction::Vh(v),
            Val::VMin(v) => MaxTrackSizingFunction::VMin(v),
            Val::VMax(v) => MaxTrackSizingFunction::VMax(v),
            Val::Auto => MaxTrackSizingFunction::Auto,
        },
        TrackBreadth::Flex(fr) => MaxTrackSizingFunction::Fraction(*fr),
        TrackBreadth::Auto => MaxTrackSizingFunction::Auto,
        TrackBreadth::MinContent => MaxTrackSizingFunction::MinContent,
        TrackBreadth::MaxContent => MaxTrackSizingFunction::MaxContent,
    })
}

fn grid_auto_flow(flow: css_grid::GridAutoFlow) -> GridAutoFlow {
    let column = flow.contains(css_grid::GridAutoFlow::Column);
    match (column, flow.contains(css_grid::GridAutoFlow::Dense)) {
        (false, false) => GridAutoFlow::Row,
        (true, false) => GridAutoFlow::Column,
        (false, true) => GridAutoFlow::RowDense,
        (true, true) => GridAutoFlow::ColumnDense,
    }
}

/// One placement side: `auto`, a line number or a span; named lines and
/// areas are unsupported (Bevy places by number only).
fn grid_line(line: &css_grid::GridLine) -> Option<GridLineDecl> {
    match line {
        css_grid::GridLine::Auto => Some(GridLineDecl::Auto),
        css_grid::GridLine::Line { index, name: None } => {
            i16::try_from(*index).ok().map(GridLineDecl::Line)
        }
        css_grid::GridLine::Span { index, name: None } => {
            u16::try_from(*index).ok().map(GridLineDecl::Span)
        }
        _ => unsupported("named grid line or area"),
    }
}

fn areas_unsupported(areas: &css_grid::GridTemplateAreas) {
    if !matches!(areas, css_grid::GridTemplateAreas::None) {
        debug!("html css: grid-template-areas unsupported");
    }
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
            length => length
                .to_px()
                .map(Val::Px)
                .or_else(|| unsupported("relative length")),
        },
        LengthPercentage::Percentage(percentage) => Some(Val::Percent(percentage.0 * 100.0)),
        LengthPercentage::Calc(_) => unsupported("calc()"),
    }
}

fn unsupported<T>(what: &str) -> Option<T> {
    debug!("html css: unsupported {what}");
    None
}

/// Whether an `outline-style` draws anything (`none` doesn't; every other
/// style draws Bevy's solid outline).
fn outline_visible(style: &lightningcss::properties::outline::OutlineStyle) -> bool {
    use lightningcss::properties::border::LineStyle;
    use lightningcss::properties::outline::OutlineStyle;
    !matches!(
        style,
        OutlineStyle::LineStyle(LineStyle::None | LineStyle::Hidden)
    )
}

/// One `border-*-radius` corner: Bevy has one radius per corner, so an
/// elliptical corner (different horizontal and vertical radii) is
/// unsupported. `%` is Bevy's (of the node's smaller side), not CSS's
/// per-axis percentage.
fn corner_radius(radius: &Size2D<LengthPercentage>) -> Option<Val> {
    if radius.0 != radius.1 {
        return unsupported("elliptical border radius");
    }
    length_percentage_val(&radius.0)
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
        color_with(css, element, Pseudo::default())
    }

    fn color_with(css: &'static str, element: &HtmlElement, pseudo: Pseudo) -> Option<Color> {
        let sheet = sheet(css);
        HtmlStyles::from_sheet(&sheet).get(element, pseudo).color
    }

    #[test]
    fn more_specific_selector_wins_regardless_of_order() {
        let note = element("p", None, &["note"]);
        assert_eq!(
            color(".note { color: green } p { color: red }", &note),
            Some(GREEN)
        );
        assert_eq!(
            color(".note { color: red } p.note { color: green }", &note),
            Some(GREEN)
        );
        let titled = element("p", Some("title"), &["note", "big"]);
        assert_eq!(
            color("#title { color: green } p.note.big { color: red }", &titled),
            Some(GREEN)
        );
    }

    /// `:hover`/`:active` match only in that state; they add class-level
    /// specificity; other pseudo-classes skip the rule.
    #[test]
    fn interaction_pseudo_classes() {
        let p = element("p", None, &[]);
        let hovered = color_with(
            "p:hover { color: green } p { color: red }",
            &p,
            Pseudo {
                hover: true,
                ..Pseudo::default()
            },
        );
        assert_eq!(hovered, Some(GREEN));
        assert_eq!(
            color("p:hover { color: green } p { color: red }", &p),
            Some(RED)
        );

        let active = color_with(
            "p:active { color: green } p { color: red }",
            &p,
            Pseudo {
                active: true,
                ..Pseudo::default()
            },
        );
        assert_eq!(active, Some(GREEN));

        // `:hover` and `:active` combine; equal specificity → source order.
        let both = color_with(
            "p:hover { color: red } p:active { color: blue } p { color: green }",
            &p,
            Pseudo {
                hover: true,
                active: true,
                ..Pseudo::default()
            },
        );
        assert_eq!(both, Some(BLUE));
    }

    /// A state-qualified rule beats an unqualified one of equal base
    /// specificity (the pseudo-class counts).
    #[test]
    fn pseudo_class_adds_specificity() {
        let button = element("div", None, &["opt"]);
        let hovered = Pseudo {
            hover: true,
            ..Pseudo::default()
        };
        assert_eq!(
            color_with(
                ".opt { color: red } div:hover { color: blue }",
                &button,
                hovered
            ),
            Some(BLUE)
        );
        assert_eq!(
            color_with(
                ".opt { color: red } div:hover { color: blue }",
                &button,
                Pseudo::default()
            ),
            Some(RED)
        );
    }

    /// Unsupported pseudo-classes skip the whole rule.
    #[test]
    fn unsupported_pseudo_classes_are_skipped() {
        let p = element("p", None, &[]);
        let every = Pseudo {
            hover: true,
            active: true,
            focus: true,
            focus_visible: true,
        };
        for state in [Pseudo::default(), every] {
            assert_eq!(
                color("p:visited { color: green } p { color: red }", &p),
                Some(RED)
            );
            assert_eq!(
                color_with(
                    "p:focus-within { color: green } p { color: red }",
                    &p,
                    state
                ),
                Some(RED)
            );
        }
    }

    /// `:focus` matches the focused element, `:focus-visible` only while
    /// focus is shown; each adds class-level specificity.
    #[test]
    fn focus_pseudo_classes_match_and_add_specificity() {
        let p = element("p", None, &["note"]);
        let focused = Pseudo {
            focus: true,
            ..Pseudo::default()
        };
        let shown = Pseudo {
            focus: true,
            focus_visible: true,
            ..Pseudo::default()
        };
        let css = "p:focus { color: red } p:focus-visible { color: green } p { color: blue }";
        assert_eq!(
            color_with(css, &p, Pseudo::default()),
            Some(Color::srgb_u8(0, 0, 255))
        );
        assert_eq!(color_with(css, &p, focused), Some(RED));
        assert_eq!(color_with(css, &p, shown), Some(GREEN));
        // `p:focus` (0,1,1) beats `.note` (0,1,0) regardless of order.
        assert_eq!(
            color_with("p:focus { color: red } .note { color: green }", &p, focused),
            Some(RED)
        );
    }

    /// `outline` shorthand and longhands; `none` and an absent style draw
    /// nothing; `outline-offset` is px.
    #[test]
    fn outline_declarations() {
        let outline = |css: &str| declared(css).outline;
        assert_eq!(
            outline("outline: 2px solid #ff0000; outline-offset: 3px"),
            OutlineDecl {
                visible: Some(true),
                width: Some(2.0),
                color: Some(Some(RED)),
                offset: Some(3.0)
            }
        );
        assert_eq!(outline("outline: none").visible, Some(false));
        assert_eq!(outline("outline-width: 4px").visible, None);
        let longhands =
            outline("outline-style: dashed; outline-color: currentColor; outline-width: thin");
        assert_eq!(
            (longhands.visible, longhands.width, longhands.color),
            (Some(true), Some(1.0), Some(None))
        );
        assert_eq!(outline("outline-offset: 1em").offset, None);
    }

    #[test]
    fn equal_specificity_later_rule_wins() {
        let both = element("p", None, &["a", "b"]);
        assert_eq!(
            color(".a { color: red } .b { color: green }", &both),
            Some(GREEN)
        );
        assert_eq!(
            color(".b { color: green } .a { color: red }", &both),
            Some(RED)
        );
    }

    #[test]
    fn important_beats_specificity() {
        let titled = element("p", Some("title"), &[]);
        assert_eq!(
            color(
                "p { color: green !important } #title { color: red }",
                &titled
            ),
            Some(GREEN)
        );
    }

    #[test]
    fn compound_requires_every_part() {
        let css = "p.note.big { color: red } h1#x { color: blue }";
        assert_eq!(color(css, &element("p", None, &["note"])), None);
        assert_eq!(color(css, &element("div", None, &["note", "big"])), None);
        assert_eq!(
            color(css, &element("p", None, &["big", "note", "extra"])),
            Some(RED)
        );
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
        let css =
            "* { color: red } p { color: green } div p { color: blue } p:hover { color: blue }";
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
        StyleSheet::parse(css, ParserOptions::default())
            .expect("valid css")
            .into_owned()
    }

    /// `p`'s declared style under one `p { declarations }` rule.
    fn declared(declarations: &str) -> ElementStyle {
        let sheet = owned_sheet(&format!("p {{ {declarations} }}"));
        HtmlStyles::from_sheet(&sheet).get(&element("p", None, &[]), Pseudo::default())
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
        assert_eq!(
            layout("flex-wrap: wrap-reverse").flex_wrap,
            Some(FlexWrap::WrapReverse)
        );

        let flex = layout("flex: 2 3 40px");
        assert_eq!(
            (flex.flex_grow, flex.flex_shrink, flex.flex_basis),
            (Some(2.0), Some(3.0), Some(Val::Px(40.0)))
        );
        assert_eq!(layout("flex-shrink: 0.5").flex_shrink, Some(0.5));
        assert_eq!(layout("flex-basis: auto").flex_basis, Some(Val::Auto));

        assert_eq!(
            layout("justify-content: normal").justify_content,
            Some(JustifyContent::Default)
        );
        assert_eq!(
            layout("justify-content: end").justify_content,
            Some(JustifyContent::End)
        );
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
        assert_eq!(
            layout("align-items: baseline").align_items,
            Some(AlignItems::Baseline)
        );
        assert_eq!(
            layout("align-self: normal").align_self,
            Some(AlignSelf::Stretch)
        );

        assert_eq!(layout("width: auto").width, Some(Val::Auto));
        assert_eq!(layout("height: 50%").height, Some(Val::Percent(50.0)));
        assert_eq!(layout("min-height: 10vh").min_height, Some(Val::Vh(10.0)));
        assert_eq!(layout("max-height: 30px").max_height, Some(Val::Px(30.0)));
        assert_eq!(layout("max-width: none").max_width, Some(Val::Auto));
        assert_eq!(layout("width: min-content").width, None);
        assert_eq!(layout("width: 2em").width, None);

        assert_eq!(
            layout("margin-top: 1px").margin,
            [Some(Val::Px(1.0)), None, None, None]
        );
        assert_eq!(
            layout("margin-right: 2px").margin,
            [None, Some(Val::Px(2.0)), None, None]
        );
        assert_eq!(
            layout("margin-bottom: auto").margin,
            [None, None, Some(Val::Auto), None]
        );
        assert_eq!(
            layout("margin-left: 10%").margin,
            [None, None, None, Some(Val::Percent(10.0))]
        );
        assert_eq!(layout("column-gap: 6px").column_gap, Some(6.0));
        assert_eq!(
            layout("box-sizing: border-box").box_sizing,
            Some(BoxSizing::BorderBox)
        );
    }

    /// Grid containers: `display`, explicit and implicit tracks (every track
    /// size form, `repeat()`, shorthands), auto flow and box alignment's
    /// inline axis, mapped onto the Bevy value CSS means; unsupported parts
    /// (font-relative tracks, areas, named lines) drop the declaration.
    #[test]
    fn grid_container_properties_map_to_bevy_values() {
        use MaxTrackSizingFunction as Max;
        use MinTrackSizingFunction as Min;
        let layout = |css: &str| declared(css).layout;
        assert_eq!(layout("display: grid").display, Some(Display::Grid));
        assert_eq!(layout("display: inline-grid").display, Some(Display::Grid));

        let columns =
            |css: &str| layout(&format!("grid-template-columns: {css}")).grid_template_columns;
        assert_eq!(
            columns("100px 1fr 20% 10vw"),
            Some(vec![
                GridTrack::px(100.0),
                GridTrack::fr(1.0),
                GridTrack::percent(20.0),
                GridTrack::vw(10.0),
            ])
        );
        assert_eq!(
            columns("auto min-content max-content fit-content(40px) fit-content(50%)"),
            Some(vec![
                GridTrack::auto(),
                GridTrack::min_content(),
                GridTrack::max_content(),
                GridTrack::fit_content_px(40.0),
                GridTrack::fit_content_percent(50.0),
            ])
        );
        assert_eq!(
            columns("minmax(50px, 2fr) minmax(min-content, 25%)"),
            Some(vec![
                GridTrack::minmax(Min::Px(50.0), Max::Fraction(2.0)),
                GridTrack::minmax(Min::MinContent, Max::Percent(25.0)),
            ])
        );
        assert_eq!(
            columns("20px repeat(3, 1fr 10px)"),
            Some(vec![
                GridTrack::px(20.0),
                RepeatedGridTrack::repeat_many(3, vec![GridTrack::fr(1.0), GridTrack::px(10.0)]),
            ])
        );
        assert_eq!(
            columns("repeat(auto-fill, minmax(50px, 1fr))"),
            Some(vec![RepeatedGridTrack::minmax(
                GridTrackRepetition::AutoFill,
                Min::Px(50.0),
                Max::Fraction(1.0),
            )])
        );
        assert_eq!(
            columns("repeat(auto-fit, 40px)"),
            Some(vec![RepeatedGridTrack::px(
                GridTrackRepetition::AutoFit,
                40.0
            )])
        );
        assert_eq!(
            columns("[a] 1fr [b]"),
            Some(vec![GridTrack::fr(1.0)]),
            "names ignored"
        );
        assert_eq!(
            columns("1fr 2em"),
            None,
            "one unsupported track drops the list"
        );
        assert_eq!(
            layout("grid-template-rows: none").grid_template_rows,
            Some(Vec::new())
        );
        assert_eq!(
            layout("grid-template-rows: 30px auto").grid_template_rows,
            Some(vec![GridTrack::px(30.0), GridTrack::auto()])
        );

        assert_eq!(
            layout("grid-auto-rows: 30px").grid_auto_rows,
            Some(vec![GridTrack::px(30.0)])
        );
        assert_eq!(
            layout("grid-auto-columns: minmax(10px, auto) 1fr").grid_auto_columns,
            Some(vec![
                GridTrack::minmax(Min::Px(10.0), Max::Auto),
                GridTrack::fr(1.0)
            ])
        );
        for (value, expected) in [
            ("row", GridAutoFlow::Row),
            ("column", GridAutoFlow::Column),
            // Bare `dense` (= `row dense`) fails lightningcss's parser
            // (UPSTREAM.md U3): the declaration is dropped before us.
            ("dense row", GridAutoFlow::RowDense),
            ("column dense", GridAutoFlow::ColumnDense),
        ] {
            let css = format!("grid-auto-flow: {value}");
            assert_eq!(layout(&css).grid_auto_flow, Some(expected), "{css}");
        }

        let template = layout("grid-template: 30px auto / 1fr 2fr");
        assert_eq!(
            template.grid_template_rows,
            Some(vec![GridTrack::px(30.0), GridTrack::auto()])
        );
        assert_eq!(
            template.grid_template_columns,
            Some(vec![GridTrack::fr(1.0), GridTrack::fr(2.0)])
        );
        let grid = layout("grid: auto-flow dense 40px / repeat(2, 1fr)");
        assert_eq!(grid.grid_auto_flow, Some(GridAutoFlow::RowDense));
        assert_eq!(grid.grid_auto_rows, Some(vec![GridTrack::px(40.0)]));
        assert_eq!(
            grid.grid_template_columns,
            Some(vec![RepeatedGridTrack::fr(2, 1.0)])
        );
        assert_eq!(grid.grid_template_rows, Some(Vec::new()));
        let areas = layout("grid-template-areas: \"a b\"");
        assert_eq!(areas.grid_template_rows, None);
        assert_eq!(areas.grid_template_columns, None);

        for (value, expected) in [
            ("normal", Some(JustifyItems::Default)),
            ("stretch", Some(JustifyItems::Stretch)),
            ("baseline", Some(JustifyItems::Baseline)),
            ("center", Some(JustifyItems::Center)),
            ("start", Some(JustifyItems::Start)),
            ("flex-start", Some(JustifyItems::Start)),
            ("end", Some(JustifyItems::End)),
            ("self-end", Some(JustifyItems::End)),
            ("left", None),
            ("legacy", None),
        ] {
            let css = format!("justify-items: {value}");
            assert_eq!(layout(&css).justify_items, expected, "{css}");
        }
        for (value, expected) in [
            ("auto", Some(JustifySelf::Auto)),
            ("normal", Some(JustifySelf::Stretch)),
            ("stretch", Some(JustifySelf::Stretch)),
            ("baseline", Some(JustifySelf::Baseline)),
            ("center", Some(JustifySelf::Center)),
            ("self-start", Some(JustifySelf::Start)),
            ("flex-end", Some(JustifySelf::End)),
            ("right", None),
        ] {
            let css = format!("justify-self: {value}");
            assert_eq!(layout(&css).justify_self, expected, "{css}");
        }
    }

    /// Grid item placement (`grid-row`/`-column`, their longhands and
    /// `grid-area`) resolves onto Bevy's start/span/end the way CSS
    /// placement does, with later longhands overriding one side.
    #[test]
    fn grid_placements_map_to_bevy_values() {
        let node = |css: &str| {
            let mut node = Node::default();
            declared(css).layout.apply_to(&mut node);
            node
        };
        let column = |css: &str| node(css).grid_column;
        assert_eq!(column("grid-column: 1 / 3"), GridPlacement::start_end(1, 3));
        assert_eq!(column("grid-column: 2"), GridPlacement::start(2));
        assert_eq!(column("grid-column: span 2"), GridPlacement::span(2));
        assert_eq!(column("grid-column: auto"), GridPlacement::auto());
        assert_eq!(
            column("grid-column: 2 / span 3"),
            GridPlacement::start_span(2, 3)
        );
        assert_eq!(
            column("grid-column: span 2 / 4"),
            GridPlacement::end_span(4, 2)
        );
        assert_eq!(column("grid-column: auto / -1"), GridPlacement::end(-1));
        assert_eq!(
            column("grid-column: span 2 / span 3"),
            GridPlacement::span(2),
            "end span dropped"
        );
        assert_eq!(column("grid-column-start: 2"), GridPlacement::start(2));
        assert_eq!(column("grid-column-end: 3"), GridPlacement::end(3));
        assert_eq!(
            column("grid-column: 1 / 3; grid-column-end: span 2"),
            GridPlacement::start_span(1, 2)
        );
        assert_eq!(
            column("grid-column: a / 3"),
            GridPlacement::end(3),
            "named line skipped"
        );
        assert_eq!(
            column("color: red"),
            GridPlacement::default(),
            "undeclared keeps the node's"
        );

        let row = |css: &str| node(css).grid_row;
        assert_eq!(row("grid-row: 2 / span 3"), GridPlacement::start_span(2, 3));
        assert_eq!(row("grid-row-start: span 2"), GridPlacement::span(2));
        assert_eq!(row("grid-row-end: -2"), GridPlacement::end(-2));

        let area = node("grid-area: 1 / 2 / 3 / 4");
        assert_eq!(area.grid_row, GridPlacement::start_end(1, 3));
        assert_eq!(area.grid_column, GridPlacement::start_end(2, 4));
        let area = node("grid-area: 2 / 3");
        assert_eq!(area.grid_row, GridPlacement::start(2));
        assert_eq!(area.grid_column, GridPlacement::start(3));
    }

    /// Per-side border widths go to their own side; `border-image-width` and
    /// `-outset` are ignored without disturbing the rest.
    #[test]
    fn border_sides_and_ignored_border_image_parts() {
        let sides = |css: &str| declared(css).border_width;
        assert_eq!(
            sides("border-top-width: 1px"),
            [Some(1.0), None, None, None]
        );
        assert_eq!(
            sides("border-right-width: 2px"),
            [None, Some(2.0), None, None]
        );
        assert_eq!(
            sides("border-bottom-width: 3px"),
            [None, None, Some(3.0), None]
        );
        assert_eq!(
            sides("border-left-width: 4px"),
            [None, None, None, Some(4.0)]
        );
        let style =
            declared("border-image-width: 9px; border-image-outset: 3px; border-width: 2px");
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
            assert_eq!(
                layout(format!("justify-content: {value}")).justify_content,
                Some(expected),
                "{value}"
            );
        }
        assert_eq!(
            layout("justify-content: right".into()).justify_content,
            None
        );
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
            assert_eq!(
                layout(format!("align-items: {value}")).align_items,
                Some(expected),
                "{value}"
            );
        }
        assert_eq!(
            layout("align-items: last baseline".into()).align_items,
            None
        );
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
            assert_eq!(
                layout(format!("align-self: {value}")).align_self,
                Some(expected),
                "{value}"
            );
        }
        assert_eq!(layout("align-self: last baseline".into()).align_self, None);
        assert_eq!(layout("align-content: baseline".into()).align_content, None);

        assert_eq!(
            layout("flex-direction: column".into()).flex_direction,
            Some(FlexDirection::Column)
        );
        assert_eq!(
            layout("flex-direction: row".into()).flex_direction,
            Some(FlexDirection::Row)
        );
        assert_eq!(
            layout("flex-wrap: nowrap".into()).flex_wrap,
            Some(FlexWrap::NoWrap)
        );
        assert_eq!(
            layout("display: contents".into()).display,
            None,
            "unsupported keyword"
        );
        assert_eq!(
            layout("box-sizing: content-box".into()).box_sizing,
            Some(BoxSizing::ContentBox)
        );
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

        assert_eq!(
            declared("border-top-width: thin").border_width[0],
            Some(1.0)
        );
        assert_eq!(
            declared("border-top-width: medium").border_width[0],
            Some(3.0)
        );
        assert_eq!(
            declared("border-top-width: thick").border_width[0],
            Some(5.0)
        );
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
            assert_eq!(
                size(&format!("font-size: {keyword}")),
                Some(px),
                "{keyword}"
            );
        }
        assert_eq!(size("font-size: calc(1em + 2px)"), None);
        assert_eq!(declared("font-weight: normal").bold, Some(false));
        assert_eq!(declared("font-weight: lighter").bold, Some(false));

        let source = declared("border-image-source: linear-gradient(red, blue)")
            .border_image
            .and_then(|decl| decl.source);
        assert!(
            matches!(source, Some(None)),
            "gradient source treated as no image: {source:?}"
        );
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
            match declared(&format!("font-family: {keyword}"))
                .font_family
                .as_deref()
            {
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
        assert!(matches!(
            style.border_image.and_then(|decl| decl.source),
            Some(None)
        ));
        let sheet = owned_sheet(
            r#"p { border-image-source: url("b.png") } div { border-image: url("c.png") 4 }"#,
        );
        assert_eq!(image_urls(&sheet), ["b.png", "c.png"]);
    }

    /// `position` and the insets: shorthand and longhands, `static` kept as
    /// such (it ignores insets), `fixed`/`sticky` unsupported.
    #[test]
    fn position_and_insets() {
        let layout = |css: &str| declared(css).layout;
        assert_eq!(
            layout("position: absolute").position,
            Some(CssPosition::Absolute)
        );
        assert_eq!(
            layout("position: relative").position,
            Some(CssPosition::Relative)
        );
        assert_eq!(
            layout("position: static").position,
            Some(CssPosition::Static)
        );
        assert_eq!(layout("position: fixed").position, None);
        assert_eq!(
            layout("inset: 1px 2% auto 4px").inset,
            [
                Some(Val::Px(1.0)),
                Some(Val::Percent(2.0)),
                Some(Val::Auto),
                Some(Val::Px(4.0))
            ]
        );
        assert_eq!(
            layout("top: 1px; right: 2px; bottom: 3px; left: 4vw").inset,
            [
                Some(Val::Px(1.0)),
                Some(Val::Px(2.0)),
                Some(Val::Px(3.0)),
                Some(Val::Vw(4.0))
            ]
        );

        let mut node = Node::default();
        layout("position: absolute; top: 5px; left: 6px").apply_to(&mut node);
        assert_eq!(
            (node.position_type, node.top, node.left),
            (PositionType::Absolute, Val::Px(5.0), Val::Px(6.0))
        );
        // `static` (and no `position`): insets don't apply, as in CSS.
        for css in ["position: static; top: 5px", "top: 5px"] {
            let mut node = Node::default();
            layout(css).apply_to(&mut node);
            assert_eq!(
                (node.position_type, node.top),
                (PositionType::Relative, Val::Auto),
                "{css}"
            );
        }
    }

    /// `border-radius` per corner (circular only), `border-color` per side,
    /// `z-index` and `pointer-events`.
    #[test]
    fn radius_border_color_z_index_and_pointer_events() {
        assert_eq!(
            declared("border-radius: 1px 2px 3px 50%")
                .layout
                .border_radius,
            [
                Some(Val::Px(1.0)),
                Some(Val::Px(2.0)),
                Some(Val::Px(3.0)),
                Some(Val::Percent(50.0))
            ]
        );
        assert_eq!(
            declared("border-top-right-radius: 7px")
                .layout
                .border_radius[1],
            Some(Val::Px(7.0))
        );
        assert_eq!(
            declared("border-radius: 4px / 8px").layout.border_radius[0],
            None,
            "elliptical"
        );
        let mut node = Node::default();
        declared("border-radius: 9px").layout.apply_to(&mut node);
        assert_eq!(node.border_radius, BorderRadius::all(Val::Px(9.0)));

        let colors =
            declared("border-color: #ff0000 green; border-left-color: #0000ff").border_color;
        assert_eq!(
            colors,
            [
                Some(RED),
                Some(GREEN),
                Some(RED),
                Some(Color::srgb_u8(0, 0, 255))
            ]
        );
        assert_eq!(
            declared("border-color: currentColor").border_color,
            [None; 4]
        );

        assert_eq!(declared("z-index: 7").z_index, Some(Some(7)));
        assert_eq!(declared("z-index: -2").z_index, Some(Some(-2)));
        assert_eq!(declared("z-index: auto").z_index, Some(None));

        assert_eq!(declared("pointer-events: none").pointer_events, Some(false));
        assert_eq!(declared("pointer-events: AUTO").pointer_events, Some(true));
        assert_eq!(
            declared("pointer-events: visiblePainted").pointer_events,
            None
        );
        assert_eq!(declared("color: red").pointer_events, None);
    }
}
