//! Stylesheet → declared style per element (the subset documented in
//! [`crate::style`]): compound selectors, joined by descendant/child
//! combinators against the element's ancestor chain, matched against
//! tag, id, classes and pseudo-state.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::collections::HashSet;

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
use lightningcss::properties::custom::Token;
use lightningcss::properties::custom::{TokenList, TokenOrValue};
use lightningcss::properties::display::{self as css_display, DisplayInside, DisplayOutside};
use lightningcss::properties::flex as css_flex;
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight,
    GenericFontFamily, RelativeFontSize,
};
use lightningcss::properties::grid::{
    self as css_grid, RepeatCount, TrackBreadth, TrackListItem, TrackSize, TrackSizing,
};
use lightningcss::properties::overflow::OverflowKeyword;
use lightningcss::properties::position as css_position;
use lightningcss::properties::size::{self as css_size, MaxSize, Size};
use lightningcss::properties::transition::Transition;
use lightningcss::properties::PropertyId;
use lightningcss::values::easing::EasingFunction;
use lightningcss::properties::text::TextDecorationLine;
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleAttribute, StyleSheet};
use lightningcss::traits::IntoOwned;
use lightningcss::traits::ToCss;
use lightningcss::values::calc::{Calc, MathFunction};
use lightningcss::values::color::{CssColor, RGBA};
use lightningcss::values::ident::CustomIdent;
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
/// placement. Named lines and areas resolve to line numbers against the
/// parent grid's template ([`GridNames`]) at collection time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GridLineDecl {
    Auto,
    /// A line number (negative counts from the end).
    Line(i16),
    Span(u16),
    /// A named line or area, optionally with an occurrence index (as in
    /// CSS's `2 main-start`; 1-based, negative from the end). Areas
    /// resolve to their region's start/end lines per side.
    Named {
        name: String,
        index: Option<i16>,
    },
}

/// A grid template's named lines and areas, for resolving a child's
/// named placement to line numbers. Built from a container's
/// `grid-template-columns`/`-rows` (their `[name]` line names) and
/// `grid-template-areas`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GridNames {
    /// Line indices by name, per axis, in document order: a line `i`
    /// (1-based from the start, `L` lines total) registers as `i` and
    /// `i - L - 1` (negative, from the end).
    rows: HashMap<String, Vec<i16>>,
    columns: HashMap<String, Vec<i16>>,
    /// Named areas: `[row start, row end, column start, column end]`
    /// (1-based lines).
    areas: HashMap<String, [i16; 4]>,
    /// The explicit rows/columns `grid-template-areas` implies (sized
    /// `auto`; declared templates win over them).
    implied_rows: u16,
    implied_columns: u16,
}

impl GridNames {
    /// Resolves one placement side's named line or area: an area name
    /// resolves to its region's line for that side; a line name to its
    /// occurrence (`index`: 1-based, negative from the end, as in CSS's
    /// `2 main-start`).
    fn line(&self, axis: Axis, side: Side, name: &str, index: Option<i16>) -> Option<GridLineDecl> {
        if let Some(area) = self.areas.get(name) {
            if index.is_some_and(|index| index != 1) {
                return None;
            }
            let [start, end] = match axis {
                Axis::Row => [area[0], area[1]],
                Axis::Column => [area[2], area[3]],
            };
            return Some(GridLineDecl::Line(match side {
                Side::Start => start,
                Side::End => end,
            }));
        }
        let lines = match axis {
            Axis::Row => &self.rows,
            Axis::Column => &self.columns,
        };
        // The registrations per name are pairs (the 1-based line and its
        // negative spelling), in document order: the negative spellings
        // grow with the lines, so `-k from the end` is the `k`th entry
        // counted backwards.
        let occurrences = lines.get(name)?;
        let (positive, negative): (Vec<i16>, Vec<i16>) =
            occurrences.iter().partition(|line| **line > 0);
        let index = index.unwrap_or(1) as i32;
        let line = if index > 0 {
            positive.get(index as usize - 1)
        } else {
            let position = negative.len() as i32 + index;
            if position < 0 {
                // More from the end than there are occurrences: no such
                // line (auto placement).
                return None;
            }
            negative.get(position as usize)
        };
        Some(GridLineDecl::Line(*line?))
    }
}

/// Which axis a line name is looked up on.
#[derive(Clone, Copy, Debug)]
enum Axis {
    Row,
    Column,
}

/// Which side (`[start, end]`) of a placement is being resolved.
#[derive(Clone, Copy, Debug)]
enum Side {
    Start,
    End,
}

/// A calc() expression, flattened from lightningcss's `Calc` AST: sums,
/// products by a number, min()/max()/clamp(), and leaves in the units
/// `Val` carries. Anything else (round()/mod()/abs()/…, bare constants in
/// sums) is skipped at parse with a debug log.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CalcExpr {
    Val(Val),
    Number(f32),
    Sum(Box<CalcExpr>, Box<CalcExpr>),
    /// A product: the expression times a number (`/ n` is `× 1/n`).
    Product(f32, Box<CalcExpr>),
    Min(Vec<CalcExpr>),
    Max(Vec<CalcExpr>),
}

/// Which layout property a calc() expression feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalcProp {
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    FlexBasis,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    Left,
    Top,
    Right,
    Bottom,
}

impl CalcProp {
    /// The axis its percentages resolve against: the containing block's
    /// inline (width) axis for sizes and the inline-relative properties,
    /// the block (height) axis for the vertical insets — per CSS.
    pub(crate) fn horizontal(self) -> bool {
        !matches!(
            self,
            CalcProp::Height
                | CalcProp::MinHeight
                | CalcProp::MaxHeight
                | CalcProp::Top
                | CalcProp::Bottom
        )
    }
}

/// The context a calc() resolves against at layout time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CalcBasis {
    /// The element's own font size (`em`).
    pub em: f32,
    /// The root rule's font size (`rem`).
    pub rem: f32,
    /// The viewport: the UI root's size.
    pub viewport: Vec2,
    /// The containing block's used content box.
    pub content: Vec2,
}

impl CalcExpr {
    /// Collapses to a single `Val` when every leaf shares one unit;
    /// `None` for mixed units (those wait for the layout pass).
    pub fn collapse(&self) -> Option<Val> {
        match self {
            CalcExpr::Val(val) => Some(val.clone()),
            CalcExpr::Number(_) => None,
            CalcExpr::Sum(a, b) => match (a.collapse()?, b.collapse()?) {
                (Val::Px(a), Val::Px(b)) => Some(Val::Px(a + b)),
                (Val::Em(a), Val::Em(b)) => Some(Val::Em(a + b)),
                (Val::Rem(a), Val::Rem(b)) => Some(Val::Rem(a + b)),
                (Val::Percent(a), Val::Percent(b)) => Some(Val::Percent(a + b)),
                (Val::Vw(a), Val::Vw(b)) => Some(Val::Vw(a + b)),
                (Val::Vh(a), Val::Vh(b)) => Some(Val::Vh(a + b)),
                (Val::VMin(a), Val::VMin(b)) => Some(Val::VMin(a + b)),
                (Val::VMax(a), Val::VMax(b)) => Some(Val::VMax(a + b)),
                _ => None,
            },
            CalcExpr::Product(k, expr) => match expr.collapse()? {
                Val::Px(v) => Some(Val::Px(v * k)),
                Val::Em(v) => Some(Val::Em(v * k)),
                Val::Rem(v) => Some(Val::Rem(v * k)),
                Val::Percent(v) => Some(Val::Percent(v * k)),
                Val::Vw(v) => Some(Val::Vw(v * k)),
                Val::Vh(v) => Some(Val::Vh(v * k)),
                Val::VMin(v) => Some(Val::VMin(v * k)),
                Val::VMax(v) => Some(Val::VMax(v * k)),
                _ => None,
            },
            CalcExpr::Min(args) => collapse_min_max(args, f32::min),
            CalcExpr::Max(args) => collapse_min_max(args, f32::max),
        }
    }

    /// The value, resolved against the basis: percentages against
    /// `basis.content`'s axis (per the property), `em`/`rem` against the
    /// font sizes, viewport units against the viewport.
    pub fn resolve(&self, basis: &CalcBasis, horizontal: bool) -> Option<f32> {
        match self {
            CalcExpr::Val(Val::Px(px)) => Some(*px),
            CalcExpr::Val(Val::Em(f)) => Some(f * basis.em),
            CalcExpr::Val(Val::Rem(f)) => Some(f * basis.rem),
            CalcExpr::Val(Val::Percent(f)) => Some(
                f / 100.0
                    * if horizontal {
                        basis.content.x
                    } else {
                        basis.content.y
                    },
            ),
            CalcExpr::Val(Val::Vw(f)) => Some(f / 100.0 * basis.viewport.x),
            CalcExpr::Val(Val::Vh(f)) => Some(f / 100.0 * basis.viewport.y),
            CalcExpr::Val(Val::VMin(f)) => Some(f / 100.0 * basis.viewport.x.min(basis.viewport.y)),
            CalcExpr::Val(Val::VMax(f)) => Some(f / 100.0 * basis.viewport.x.max(basis.viewport.y)),
            CalcExpr::Val(_) => None,
            CalcExpr::Number(n) => Some(*n),
            CalcExpr::Sum(a, b) => {
                Some(a.resolve(basis, horizontal)? + b.resolve(basis, horizontal)?)
            }
            CalcExpr::Product(k, expr) => Some(k * expr.resolve(basis, horizontal)?),
            CalcExpr::Min(args) => args
                .iter()
                .map(|arg| arg.resolve(basis, horizontal))
                .try_fold(f32::INFINITY, |acc, value| Some(acc.min(value?)))
                .filter(|value| value.is_finite()),
            CalcExpr::Max(args) => args
                .iter()
                .map(|arg| arg.resolve(basis, horizontal))
                .try_fold(f32::NEG_INFINITY, |acc, value| Some(acc.max(value?)))
                .filter(|value| value.is_finite()),
        }
    }
}

/// Collapses min()/max() when every argument shares one unit; comparing
/// values of different units depends on the resolution basis, so mixed
/// units go to the layout pass (`None`).
fn collapse_min_max(args: &[CalcExpr], pick: fn(f32, f32) -> f32) -> Option<Val> {
    let mut values = Vec::new();
    for arg in args {
        values.push(arg.collapse()?);
    }
    let mut best = None;
    for value in values {
        let (unit, amount) = match value {
            Val::Px(v) => (0, v),
            Val::Em(v) => (1, v),
            Val::Rem(v) => (2, v),
            Val::Percent(v) => (3, v),
            Val::Vw(v) => (4, v),
            Val::Vh(v) => (5, v),
            Val::VMin(v) => (6, v),
            Val::VMax(v) => (7, v),
            _ => return None,
        };
        best = Some(match best {
            Some((best_unit, best_amount)) if best_unit == unit => {
                (best_unit, pick(best_amount, amount))
            }
            Some(_) => return None,
            None => (unit, amount),
        });
    }
    match best {
        Some((0, v)) => Some(Val::Px(v)),
        Some((1, v)) => Some(Val::Em(v)),
        Some((2, v)) => Some(Val::Rem(v)),
        Some((3, v)) => Some(Val::Percent(v)),
        Some((4, v)) => Some(Val::Vw(v)),
        Some((5, v)) => Some(Val::Vh(v)),
        Some((6, v)) => Some(Val::VMin(v)),
        Some((7, v)) => Some(Val::VMax(v)),
        _ => None,
    }
}

/// Declared flex, grid, size and margin properties of a container or block,
/// as Bevy values (`None` = not declared).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LayoutDecl {
    pub display: Option<Display>,
    pub visibility: Option<bool>,
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
    /// The template's named lines (from `[name]`s in the track lists) and
    /// `grid-template-areas` (`Some` whenever either is declared).
    pub grid_names: Option<GridNames>,
    /// `[start, end]` of `grid-row`.
    pub grid_row: [Option<GridLineDecl>; 2],
    /// `[start, end]` of `grid-column`.
    pub grid_column: [Option<GridLineDecl>; 2],
    /// `position`.
    pub position: Option<CssPosition>,
    /// `top`/`right`/`bottom`/`left` (`[top, right, bottom, left]`); they
    /// only apply to a `relative`/`absolute`/`fixed` element, as in CSS.
    pub inset: [Option<Val>; 4],
    /// `[top-left, top-right, bottom-right, bottom-left]` radii as
    /// `[x, y]` pairs (CSS `border-radius: h / v`).
    pub border_radius: [Option<(Val, Val)>; 4],
    /// `overflow-x`, `overflow-y` (and the `overflow` shorthand).
    pub overflow: [Option<OverflowAxis>; 2],
}

/// A declared `position`. `static` is CSS's initial value: Bevy's
/// `PositionType::Relative` with the insets ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CssPosition {
    Static,
    Relative,
    Absolute,
    /// Bevy's `FixedNode`: positioned against the viewport like CSS's
    /// `position: fixed`, not the parent; no inherited clipping.
    Fixed,
}

impl LayoutDecl {
    /// Overrides `node`'s fields with the declared ones.
    /// Resolves the placement's named lines and areas against the parent
    /// grid's template; a name the template doesn't have (or one inside an
    /// `auto-fill`/`auto-fit` repeat, whose count is only known at layout)
    /// becomes `auto`, logged. Runs once per element, at collection.
    pub fn resolve_names(&mut self, grid: Option<&GridNames>) {
        for (axis, sides) in [
            (Axis::Row, &mut self.grid_row),
            (Axis::Column, &mut self.grid_column),
        ] {
            for (side, side_decl) in sides.iter_mut().enumerate() {
                let Some(GridLineDecl::Named { name, index }) = side_decl.clone() else {
                    continue;
                };
                let side = if side == 0 { Side::Start } else { Side::End };
                let resolved = grid.and_then(|grid| grid.line(axis, side, &name, index));
                *side_decl = Some(resolved.unwrap_or_else(|| {
                    debug!(
                        "html css: grid line name `{name}` doesn't resolve against the \
                             parent grid; treated as auto"
                    );
                    GridLineDecl::Auto
                }));
            }
        }
    }

    /// The grid this element's own template defines for its children's
    /// placement: named lines and areas. `None` without grid templates or
    /// areas (the element isn't a grid container, or it declares none of
    /// the names).
    pub fn grid_names(&self) -> Option<GridNames> {
        self.grid_names.clone()
    }

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
        // `grid-template-areas` implies the explicit rows and columns it
        // names (sized `auto`); declared templates win over them.
        if let Some(names) = &self.grid_names {
            let auto: RepeatedGridTrack = GridTrack::auto();
            if self.grid_template_rows.is_none() && names.implied_rows > 0 {
                node.grid_template_rows = vec![auto.clone(); names.implied_rows as usize];
            }
            if self.grid_template_columns.is_none() && names.implied_columns > 0 {
                node.grid_template_columns = vec![auto.clone(); names.implied_columns as usize];
            }
        }
        set(&mut node.grid_template_rows, &self.grid_template_rows);
        set(&mut node.grid_template_columns, &self.grid_template_columns);
        set(&mut node.grid_auto_rows, &self.grid_auto_rows);
        set(&mut node.grid_auto_columns, &self.grid_auto_columns);
        set(&mut node.grid_auto_flow, &self.grid_auto_flow);
        set(&mut node.grid_row, &grid_placement(self.grid_row.clone()));
        set(
            &mut node.grid_column,
            &grid_placement(self.grid_column.clone()),
        );
        let corners = &mut node.border_radius;
        set(
            &mut corners.top_left,
            &self.border_radius[0].map(CornerRadius::from),
        );
        set(
            &mut corners.top_right,
            &self.border_radius[1].map(CornerRadius::from),
        );
        set(
            &mut corners.bottom_right,
            &self.border_radius[2].map(CornerRadius::from),
        );
        set(
            &mut corners.bottom_left,
            &self.border_radius[3].map(CornerRadius::from),
        );
        set(&mut node.overflow.x, &self.overflow[0]);
        set(&mut node.overflow.y, &self.overflow[1]);
        let position_type = match self.position {
            // `fixed` positions against the viewport (via the `FixedNode`
            // component, added by the build), with absolute-style insets.
            Some(CssPosition::Absolute | CssPosition::Fixed) => PositionType::Absolute,
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
            &grid_placement(self.grid_row.clone()),
        );
        put(
            &mut node.grid_column,
            &base.grid_column,
            &grid_placement(self.grid_column.clone()),
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
        put(&mut node.overflow.x, &base.overflow.x, &self.overflow[0]);
        put(&mut node.overflow.y, &base.overflow.y, &self.overflow[1]);
        // `apply_to` sets the position and insets for every non-`static`
        // position (`fixed` resolves against the viewport through the
        // `FixedNode` component, but the insets and the position type are
        // the node's).
        if matches!(
            self.position,
            Some(CssPosition::Absolute | CssPosition::Relative | CssPosition::Fixed)
        ) {
            node.position_type = base.position_type;
            put(&mut node.top, &base.top, &self.inset[0]);
            put(&mut node.right, &base.right, &self.inset[1]);
            put(&mut node.bottom, &base.bottom, &self.inset[2]);
            put(&mut node.left, &base.left, &self.inset[3]);
        }
        // `grid-template-areas` implies `auto` tracks when the template
        // fields are undeclared (apply_to writes them); a later sheet
        // without the areas must take them back.
        if let Some(names) = &self.grid_names {
            if self.grid_template_rows.is_none() && names.implied_rows > 0 {
                node.grid_template_rows = base.grid_template_rows.clone();
            }
            if self.grid_template_columns.is_none() && names.implied_columns > 0 {
                node.grid_template_columns = base.grid_template_columns.clone();
            }
        }
    }
}

/// A declared `[start, end]` pair as a Bevy placement, resolved as CSS
/// grid placement does: two spans keep the start's; a line with a span
/// spans from (or back to) the line. `None` when neither side is declared.
fn grid_placement(sides: [Option<GridLineDecl>; 2]) -> Option<GridPlacement> {
    use GridLineDecl::{Auto, Line, Span};
    // `resolve_names` runs at collection, before this; a surviving `Named`
    // means the parent grid didn't have the name.
    let sides = sides.map(|side| {
        side.map(|side| match side {
            GridLineDecl::Named { .. } => {
                debug!("html css: unresolved grid line name; treated as auto");
                Auto
            }
            side => side,
        })
        .unwrap_or(Auto)
    });
    if sides == [Auto, Auto] {
        return None;
    }
    let [start, end] = sides;
    Some(match (start, end) {
        (Auto, Auto) => GridPlacement::auto(),
        (Line(start), Auto) => GridPlacement::start(start),
        (Auto, Line(end)) => GridPlacement::end(end),
        (Line(start), Line(end)) => GridPlacement::start_end(start, end),
        (Span(span), Auto | Span(_)) | (Auto, Span(span)) => GridPlacement::span(span),
        (Line(start), Span(span)) => GridPlacement::start_span(start, span),
        (Span(span), Line(end)) => GridPlacement::end_span(end, span),
        // `resolve_names` handled named lines at collection (or turned
        // them into `Auto` above); this is unreachable.
        (GridLineDecl::Named { .. }, _) | (_, GridLineDecl::Named { .. }) => GridPlacement::auto(),
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
    /// `text-decoration` (`underline`/`line-through` and its color;
    /// `overline`, `blink` and the thickness/style are skipped).
    pub text_decoration: Option<DecorationDecl>,
    /// Flex, size and margin properties (containers and blocks).
    pub layout: LayoutDecl,
    /// The `calc()`-valued properties: resolved at layout time (the
    /// property's field keeps its placeholder until then).
    pub calc: Vec<(CalcProp, CalcExpr)>,
    /// Declared `transition` (the shorthand), for the properties the crate
    /// animates; a repeated property keeps the last declaration.
    pub transitions: Vec<TransitionDecl>,
}

/// Declared `text-decoration` of an inline element.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DecorationDecl {
    pub underline: bool,
    pub line_through: bool,
    /// `text-decoration-color`; `None` = `currentColor` (the text color).
    pub color: Option<Color>,
}

/// A property the `transition` shorthand names and the crate animates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitionProp {
    TextColor,
    BackgroundColor,
    BorderColor,
}

/// One entry of a declared `transition` (shorthand). Longhands are skipped.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransitionDecl {
    pub property: TransitionProp,
    /// Seconds.
    pub duration: f32,
    /// Seconds, before the animation starts.
    pub delay: f32,
    /// lightningcss's parsed easing curve (`linear`, the named curves, and
    /// `cubic-bezier(a, b, c, d)`).
    pub easing: EasingFunction,
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

/// Every `url()` in the sheet's own `--` declarations: a `var()`-loaded
/// `border-image` resolves through a custom property, so its image is a
/// dependency of the sheet. These URLs can be anything (`data:` URIs, a
/// scheme the resolver rejects) — the loader skips the unresolvable ones
/// instead of failing the sheet (the `var()` consumer drops the
/// declaration then, like any unresolvable reference).
pub(crate) fn custom_image_urls(sheet: &StyleSheet) -> Vec<String> {
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
            let Property::Custom(custom) = declaration else {
                continue;
            };
            for token in &custom.value.0 {
                if let TokenOrValue::Url(url) = token {
                    let url = url.url.to_string();
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
        }
    }
    urls
}

/// Every `var()` name the sheet's supported declarations reference,
/// sorted: the names whose context values a change restyles
/// (`Frame::vars_changed`). Transitively: a `--a: var(--brand)`
/// declaration contributes `--brand` when `--a` is itself referenced.
/// A name a sheet-level `--` declaration references but nothing reads is
/// not tracked. Computed once at load (cached on `Stylesheet`).
pub(crate) fn var_names(sheet: &StyleSheet) -> Vec<String> {
    let mut names = BTreeSet::new();
    // The sheet's `--` declarations: name → the `var()` names their
    // values reference (joined into the set by the fixpoint below).
    let mut declarations: Vec<(String, Vec<String>)> = Vec::new();
    for rule in sheet.rules.0.iter() {
        let CssRule::Style(rule) = rule else {
            continue;
        };
        for declaration in rule
            .declarations
            .declarations
            .iter()
            .chain(&rule.declarations.important_declarations)
        {
            match declaration {
                Property::Unparsed(unparsed) => {
                    collect_var_names(&unparsed.value, &mut names);
                }
                Property::Custom(custom) => {
                    let mut refs = BTreeSet::new();
                    collect_var_names(&custom.value, &mut refs);
                    match &custom.name {
                        CustomPropertyName::Custom(dashed) => {
                            if !refs.is_empty() {
                                declarations
                                    .push((dashed.0.as_ref().to_owned(), refs.into_iter().collect()));
                            }
                        }
                        // An unknown property the build reads raw
                        // (`pointer-events`, `outline-offset`): its refs
                        // are direct dependencies of the declaration.
                        CustomPropertyName::Unknown(_) => {
                            names.extend(refs);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // The transitive closure, as a worklist (linear: every name enters
    // once — the fixpoint loop re-scanned every declaration per round,
    // quadratic in the chain length).
    let by_name: HashMap<String, Vec<String>> =
        declarations.into_iter().collect();
    let mut work: Vec<String> = names.iter().cloned().collect();
    while let Some(active) = work.pop() {
        if let Some(refs) = by_name.get(&active) {
            for r in refs {
                if names.insert(r.clone()) {
                    work.push(r.clone());
                }
            }
        }
    }
    names.into_iter().collect()
}

/// The `var(--name)` names a raw `style` attribute references: a cheap
/// scan of the decoded text (the names feed the frame's vars hash — an
/// inline style is rebuilt with the element, so the exact nested-fallback
/// resolution of `var_names` isn't needed).
pub(crate) fn raw_var_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("var(") {
        rest = &rest[at + 4..];
        let trimmed = rest.trim_start();
        if let Some(name) = trimmed.strip_prefix("--") {
            let end = name
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(name.len());
            if end > 0 {
                names.push(format!("--{}", &name[..end]));
            }
        }
    }
    names
}

/// The `var()` names a token list references, function arguments included.
fn collect_var_names(tokens: &TokenList, names: &mut BTreeSet<String>) {
    for token in &tokens.0 {
        match token {
            TokenOrValue::Var(variable) => {
                names.insert(variable.name.ident.as_ref().to_owned());
                if let Some(fallback) = &variable.fallback {
                    collect_var_names(fallback, names);
                }
            }
            TokenOrValue::Function(function) => collect_var_names(&function.arguments, names),
            _ => {}
        }
    }
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

/// An ancestor for selector matching: its identity plus its interaction
/// state (for pseudo-classes in a compound selector), nearest first.
#[derive(Clone, Debug)]
pub(crate) struct Ancestor {
    pub element: HtmlElement,
    pub pseudo: Pseudo,
}

/// How a compound selector relates to the next one to its right.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Combinator {
    /// Whitespace: any ancestor.
    Descendant,
    /// `>`: the direct parent.
    Child,
}

/// A selector: compound selectors joined by combinators (only descendant
/// and child are supported). The rightmost compound is the subject.
#[derive(Clone, Debug)]
struct Selector {
    /// Left to right; the last one matches the element itself.
    compounds: Vec<Compound>,
    /// `compounds.len() - 1` combinators; `combinators[i]` joins
    /// `compounds[i]` to `compounds[i + 1]`.
    combinators: Vec<Combinator>,
}

impl Selector {
    /// `None` for anything unsupported (attributes, other pseudo-classes,
    /// other combinators, non-ASCII idents): the whole rule is skipped.
    fn parse(selector: &str) -> Option<Self> {
        let mut compounds = Vec::new();
        let mut combinators = Vec::new();
        let mut pending = Combinator::Descendant;
        let mut rest = selector.trim();
        while !rest.is_empty() {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(rest.len());
            compounds.push(Compound::parse(&rest[..end])?);
            if !combinators.is_empty() || compounds.len() > 1 {
                combinators.push(pending);
            }
            rest = &rest[end..];
            let whitespace = rest.len() - rest.trim_start().len();
            rest = &rest[whitespace..];
            if rest.starts_with('>') {
                pending = Combinator::Child;
                rest = &rest[1..];
                rest = &rest[rest.len() - rest.trim_start().len()..];
            } else if !rest.is_empty() {
                pending = Combinator::Descendant;
            }
        }
        if compounds.is_empty() {
            return None;
        }
        Some(Self {
            compounds,
            combinators,
        })
    }

    /// CSS specificity: (ids, classes, types), summed over the compounds.
    fn specificity(&self) -> (u32, u32, u32) {
        self.compounds.iter().fold((0, 0, 0), |mut sum, compound| {
            let (ids, classes, tags) = compound.specificity();
            sum.0 += ids;
            sum.1 += classes;
            sum.2 += tags;
            sum
        })
    }

    /// `element` (with its `pseudo`) against the subject compound, then the
    /// rest against `ancestors` (nearest first).
    fn matches(&self, element: &HtmlElement, pseudo: Pseudo, ancestors: &[Ancestor]) -> bool {
        let Some(last) = self.compounds.last() else {
            return false;
        };
        if !last.matches(element, pseudo) {
            return false;
        }
        self.matches_rest(self.compounds.len() - 1, ancestors)
    }

    /// Matches `compounds[..index]` against `ancestors` (nearest first).
    /// Memoized by (compound index, ancestor offset): a `.b .a ×n p`
    /// selector against deep `.a` ancestors retried every combination
    /// (hundreds of ms per element); with the failure memo it is linear.
    fn matches_rest(&self, index: usize, ancestors: &[Ancestor]) -> bool {
        let mut failed = HashSet::new();
        self.matches_from(index, ancestors, 0, &mut failed)
    }

    /// [`matches_rest`](Self::matches_rest) from `offset` (the absolute
    /// ancestor position), recording failed states in `failed`.
    fn matches_from(
        &self,
        index: usize,
        ancestors: &[Ancestor],
        offset: usize,
        failed: &mut HashSet<(usize, usize)>,
    ) -> bool {
        if index == 0 {
            return true;
        }
        if failed.contains(&(index, offset)) {
            return false;
        }
        let combinator = self.combinators[index - 1];
        let compound = &self.compounds[index - 1];
        let matched = match combinator {
            Combinator::Child => ancestors.get(offset).is_some_and(|parent| {
                compound.matches(&parent.element, parent.pseudo)
                    && self.matches_from(index - 1, ancestors, offset + 1, failed)
            }),
            Combinator::Descendant => {
                let mut any = false;
                for i in offset..ancestors.len() {
                    let ancestor = &ancestors[i];
                    if compound.matches(&ancestor.element, ancestor.pseudo)
                        && self.matches_from(index - 1, ancestors, i + 1, failed)
                    {
                        any = true;
                        break;
                    }
                }
                any
            }
        };
        if !matched {
            failed.insert((index, offset));
        }
        matched
    }
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
        // A bare pseudo-class list (`*:hover` prints as `:hover`,
        // `:focus-visible` alone) matches any element in that state — the
        // standard focus-ring rule.
        if tag.is_none()
            && ids.is_empty()
            && classes.is_empty()
            && selector != "*"
            && pseudo == Pseudo::default()
        {
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
    selector: Selector,
    /// Sort key: importance, then specificity, then source order.
    rank: (bool, (u32, u32, u32), usize),
    declarations: &'a [Property<'static>],
}

/// A stylesheet's rules, matched per element (cached by tag + id + classes
/// + pseudo-state).
#[derive(Default)]
pub(crate) struct HtmlStyles<'a> {
    rules: Vec<Rule<'a>>,
    /// The sheet's custom properties (`--x: …`, any top-level rule; last
    /// declaration wins): what `var(--x)` falls back to after the UI's
    /// [`TemplateContext`](crate::html::TemplateContext).
    variables: HashMap<String, TokenList<'static>>,
    /// The context's values by name, per UI (per build). Looked up before
    /// `variables`; values interpolate as written (strings raw, numbers
    /// and bools as their CSS spelling). `None` = no context.
    context: Option<&'a dyn Fn(&str) -> Option<String>>,
    cache: RefCell<HashMap<(String, Option<String>, Vec<String>, Pseudo, u64), ElementStyle>>,
}

impl<'a> HtmlStyles<'a> {
    /// Looks `var(--name)` up in `context` (the UI's `TemplateContext`,
    /// stringified) before the sheet's own custom properties.
    pub fn with_context(
        mut self,
        context: &'a dyn Fn(&str) -> Option<String>,
    ) -> Self {
        self.context = Some(context);
        self
    }

    pub fn from_sheet(sheet: &'a StyleSheet<'static>) -> Self {
        let mut rules = Vec::new();
        let mut variables: HashMap<String, TokenList<'static>> = HashMap::default();
        for (order, rule) in sheet.rules.0.iter().enumerate() {
            let CssRule::Style(rule) = rule else {
                continue;
            };
            // Custom properties (`--x: …`) are sheet-level here, whatever
            // rule they sit in (last wins): this cascade has no
            // per-element custom-property inheritance. Collected before
            // selector filtering, so a rule with an unsupported selector
            // still registers its variables.
            for declaration in rule.declarations.declarations.iter().chain(&rule.declarations.important_declarations) {
                if let Property::Custom(custom) = declaration {
                    if let CustomPropertyName::Custom(name) = &custom.name {
                        variables.insert(name.0.as_ref().to_owned(), custom.value.clone());
                    }
                }
            }
            for selector in rule.selectors.0.iter() {
                let Ok(text) = selector.to_css_string(PrinterOptions::default()) else {
                    continue;
                };
                let Some(selector) = Selector::parse(&text) else {
                    debug!("html css: skipping unsupported selector `{text}`");
                    continue;
                };
                let specificity = selector.specificity();
                for (important, declarations) in [
                    (false, &rule.declarations.declarations),
                    (true, &rule.declarations.important_declarations),
                ] {
                    if declarations.is_empty() {
                        continue;
                    }
                    rules.push(Rule {
                        selector: selector.clone(),
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
            variables,
            context: None,
            cache: RefCell::default(),
        }
    }

    /// The declared style for `element` in the given interaction state:
    /// every matching rule's declarations, applied in cascade order.
    /// `ancestors` (nearest first) backs descendant and child combinators;
    /// rules without them ignore it. The cache folds the ancestor chain
    /// into its key, since matching depends on it.
    pub fn get(
        &self,
        element: &HtmlElement,
        pseudo: Pseudo,
        ancestors: &[Ancestor],
    ) -> ElementStyle {
        let key = (
            element.tag.clone(),
            element.id.clone(),
            element.classes.clone(),
            pseudo,
            ancestors.iter().fold(0u64, |mut hash, ancestor| {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                ancestor.element.tag.hash(&mut hasher);
                ancestor.element.id.hash(&mut hasher);
                ancestor.element.classes.hash(&mut hasher);
                ancestor.pseudo.hash(&mut hasher);
                hash = hash.wrapping_mul(31).wrapping_add(hasher.finish());
                hash
            }),
        );
        if let Some(style) = self.cache.borrow().get(&key) {
            return style.clone();
        }
        let mut style = ElementStyle::default();
        for rule in self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, pseudo, ancestors))
        {
            for declaration in rule.declarations {
                apply_resolved(&mut style, declaration, &self.variables, self.context);
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
        ancestors: &[Ancestor],
    ) -> ElementStyle {
        let Some(InlineStyle(block)) = inline else {
            return self.get(element, pseudo, ancestors);
        };
        let mut style = ElementStyle::default();
        let matching: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|rule| rule.selector.matches(element, pseudo, ancestors))
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
            apply_resolved(&mut style, declaration, &self.variables, self.context);
        }
        style
    }
}

/// CSS `overflow` per axis; `auto` scrolls (Bevy has no "only if needed").
fn overflow_axis(keyword: &OverflowKeyword) -> OverflowAxis {
    match keyword {
        OverflowKeyword::Visible => OverflowAxis::Visible,
        OverflowKeyword::Hidden => OverflowAxis::Hidden,
        OverflowKeyword::Clip => OverflowAxis::Clip,
        OverflowKeyword::Scroll | OverflowKeyword::Auto => OverflowAxis::Scroll,
    }
}

impl ElementStyle {
    /// Sets `prop` to a plain value (dropping any calc for it), or — when
    /// the declaration was a calc() — records the expression instead (the
    /// property's field keeps its placeholder until the layout pass).
    fn set_calc(&mut self, prop: CalcProp, value: Option<Val>, exprs: Vec<CalcExpr>) {
        if value.is_none() && exprs.is_empty() {
            // An unsupported value (`fit-content`, …): the declaration is
            // invalid and dropped — the earlier declaration's value (and
            // its calc) stand.
            return;
        }
        self.calc.retain(|(p, _)| p != &prop);
        match exprs.into_iter().next() {
            Some(expr) => match expr.collapse() {
                Some(value) => self.layout.set_field(prop, Some(value)),
                None => {
                    // Mixed units: the layout pass resolves per frame; the
                    // field stays cleared until (and unless) it does.
                    self.layout.set_field(prop, None);
                    self.calc.push((prop, expr));
                }
            },
            None => {
                if let Some(value) = value {
                    layout_field(&mut self.layout, prop, value);
                }
            }
        }
    }
}

impl LayoutDecl {
    /// Sets or clears the property's field. A `calc()`-valued declaration
    /// clears it: the concrete value arrives from the layout pass, and a
    /// stale earlier declaration must not show through.
    fn set_field(&mut self, prop: CalcProp, value: Option<Val>) {
        match prop {
            CalcProp::Width => self.width = value,
            CalcProp::Height => self.height = value,
            CalcProp::MinWidth => self.min_width = value,
            CalcProp::MinHeight => self.min_height = value,
            CalcProp::MaxWidth => self.max_width = value,
            CalcProp::MaxHeight => self.max_height = value,
            CalcProp::FlexBasis => self.flex_basis = value,
            CalcProp::MarginTop => self.margin[0] = value,
            CalcProp::MarginRight => self.margin[1] = value,
            CalcProp::MarginBottom => self.margin[2] = value,
            CalcProp::MarginLeft => self.margin[3] = value,
            CalcProp::Left => self.inset[3] = value,
            CalcProp::Top => self.inset[0] = value,
            CalcProp::Right => self.inset[1] = value,
            CalcProp::Bottom => self.inset[2] = value,
        }
    }
}

/// The `Val`-carrying `Node` field for a calc-able property (the layout
/// pass writes it; the root's restore reads it back).
pub(crate) fn node_field_mut(node: &mut Node, prop: CalcProp) -> &mut Val {
    match prop {
        CalcProp::Width => &mut node.width,
        CalcProp::Height => &mut node.height,
        CalcProp::MinWidth => &mut node.min_width,
        CalcProp::MinHeight => &mut node.min_height,
        CalcProp::MaxWidth => &mut node.max_width,
        CalcProp::MaxHeight => &mut node.max_height,
        CalcProp::FlexBasis => &mut node.flex_basis,
        CalcProp::MarginTop => &mut node.margin.top,
        CalcProp::MarginRight => &mut node.margin.right,
        CalcProp::MarginBottom => &mut node.margin.bottom,
        CalcProp::MarginLeft => &mut node.margin.left,
        CalcProp::Left => &mut node.left,
        CalcProp::Top => &mut node.top,
        CalcProp::Right => &mut node.right,
        CalcProp::Bottom => &mut node.bottom,
    }
}

/// The `Val`-carrying layout field for a calc-able property.
pub(crate) fn layout_field(layout: &mut LayoutDecl, prop: CalcProp, value: Val) {
    match prop {
        CalcProp::Width => layout.width = Some(value),
        CalcProp::Height => layout.height = Some(value),
        CalcProp::MinWidth => layout.min_width = Some(value),
        CalcProp::MinHeight => layout.min_height = Some(value),
        CalcProp::MaxWidth => layout.max_width = Some(value),
        CalcProp::MaxHeight => layout.max_height = Some(value),
        CalcProp::FlexBasis => layout.flex_basis = Some(value),
        CalcProp::MarginTop => layout.margin[0] = Some(value),
        CalcProp::MarginRight => layout.margin[1] = Some(value),
        CalcProp::MarginBottom => layout.margin[2] = Some(value),
        CalcProp::MarginLeft => layout.margin[3] = Some(value),
        CalcProp::Left => layout.inset[3] = Some(value),
        CalcProp::Top => layout.inset[0] = Some(value),
        CalcProp::Right => layout.inset[1] = Some(value),
        CalcProp::Bottom => layout.inset[2] = Some(value),
    }
}

/// Applies `declaration`, resolving its `var()` references first: from the
/// UI's context (`context`, a `TemplateContext` lookup), then the sheet's
/// own `--` properties (`variables`). An unresolvable reference without a
/// fallback drops the declaration (CSS: invalid at computed-value time, the
/// declaration is unset; we skip it entirely — an earlier rule's value may
/// show through).
fn apply_resolved(
    style: &mut ElementStyle,
    declaration: &Property<'static>,
    variables: &HashMap<String, TokenList<'static>>,
    context: Option<&dyn Fn(&str) -> Option<String>>,
) {
    // `initial`/`inherit`/`unset`/`revert` arrive as unparsed bare
    // keywords: the field returns to its initial value (the cascade has
    // no parent computed style, so `inherit`/`unset` on inherited
    // properties resolve to it too — documented deviation).
    if let Property::Unparsed(unparsed) = declaration
        && unparsed.value.0.len() == 1
        && let TokenOrValue::Token(token) = &unparsed.value.0[0]
        && matches!(token, Token::Ident(ident)
            if matches!(ident.as_ref().to_ascii_lowercase().as_str(),
                "initial" | "inherit" | "unset" | "revert"))
    {
        apply_keyword(style, &unparsed.property_id);
        return;
    }
    // The tokens to substitute: an unparsed value, or an unknown/custom
    // property's raw tokens (`pointer-events` has no typed form in
    // lightningcss and arrives as a custom property).
    let (reparse, tokens) = match declaration {
        Property::Unparsed(unparsed) => {
            if !contains_var(&unparsed.value) {
                apply(style, declaration);
                return;
            }
            (Some(unparsed.property_id.clone()), &unparsed.value)
        }
        Property::Custom(custom) => {
            if !contains_var(&custom.value) {
                apply(style, declaration);
                return;
            }
            (Some(PropertyId::Custom(custom.name.clone())), &custom.value)
        }
        _ => {
            apply(style, declaration);
            return;
        }
    };
    let mut substitution = Substitution {
        variables,
        context: &context.unwrap_or(&|_| None),
        resolved: HashMap::new(),
        resolving: HashSet::new(),
        budget: VAR_BUDGET,
        cycle: false,
    };
    let Some(text) = substitution.substitute(tokens) else {
        debug!("html css: `var()` substitution failed");
        return;
    };
    match Property::parse_string(reparse.unwrap(), &text, ParserOptions::default()) {
        Ok(property) => apply(style, &property.into_owned()),
        Err(_) => {
            debug!("html css: `var()` substitution produced an invalid value: {text:?}");
        }
    }
}

/// Whether any `var()` appears in the tokens (function arguments included).
fn contains_var(tokens: &TokenList) -> bool {
    tokens.0.iter().any(|token| match token {
        TokenOrValue::Var(_) => true,
        TokenOrValue::Function(function) => contains_var(&function.arguments),
        _ => false,
    })
}

/// The total substitution output one declaration may produce: a `var()`
/// graph can be exponential (a doubling reference chain), and the budget
/// bounds it. Beyond it the declaration is dropped, like an unresolvable
/// one.
const VAR_BUDGET: usize = 64 * 1024;

/// How deep one `var()` resolution may chain (`--v1` referencing
/// `--v0`, transitively). The memoized substitution bounds repeat work
/// and output size, but a plain linear chain recurses once per link —
/// deeper than this, the name is unresolvable (the declaration is
/// dropped), like a cycle. Well past any authored CSS; pathological
/// sheets must not abort the render thread.
const MAX_VAR_DEPTH: usize = 64;

/// One declaration's `var()` substitution: each custom property is
/// resolved at most once (memoized), a name in the current chain is
/// unresolvable (a cycle is invalid at computed-value time), and the
/// output is budget-capped. Linear in the sheet, where the naive
/// depth-bounded recursion was exponential.
struct Substitution<'a> {
    variables: &'a HashMap<String, TokenList<'static>>,
    context: &'a dyn Fn(&str) -> Option<String>,
    /// Resolved values by name; `None` = unresolvable (memoized). Keys
    /// are owned: the names borrow the tokens being substituted.
    resolved: HashMap<String, Option<String>>,
    /// Names on the current resolution chain: re-entering one fails.
    resolving: HashSet<String>,
    budget: usize,
    /// Set when the resolution just hit a cycle: cycle members are
    /// invalid at computed-value time *including their fallbacks*
    /// (CSS Variables 1 §2.3) — the enclosing `var()` skips its
    /// fallback instead of applying it.
    cycle: bool,
}

impl<'a> Substitution<'a> {
    /// The value for one `var()` reference: the context first, then the
    /// sheet's custom properties (resolved once, cycles fail), then —
    /// handled by the caller — the reference's fallback.
    fn var(&mut self, name: &str) -> Option<String> {
        if let Some(value) = self.resolved.get(name) {
            return value.clone();
        }
        if self.resolving.contains(name) {
            // A cycle: invalid at computed-value time, fallbacks included.
            self.cycle = true;
            return None;
        }
        if self.resolving.len() >= MAX_VAR_DEPTH {
            // Deeper than any authored chain: unresolvable (memoized, so
            // the rest of the chain fails fast instead of overflowing).
            return None;
        }
        let value = if let Some(value) = (self.context)(name) {
            Some(value)
        } else {
            self.resolving.insert(name.to_owned());
            let value = self
                .variables
                .get(name)
                .and_then(|tokens| self.substitute(tokens));
            self.resolving.remove(name);
            value
        };
        // Memoize even the failures: a name that doesn't resolve stays
        // unresolvable for this declaration.
        self.resolved.insert(name.to_owned(), value.clone());
        value
    }

    /// The tokens' CSS text with every `var()` replaced, else `None`
    /// (unresolvable or over budget).
    fn substitute(&mut self, tokens: &TokenList) -> Option<String> {
        let mut out = String::new();
        for token in &tokens.0 {
            let piece = match token {
                TokenOrValue::Var(variable) => {
                    let name = variable.name.ident.as_ref();
                    match self.var(name) {
                        Some(value) => value,
                        None if self.cycle => {
                            // The cycle closes at *this* reference: its
                            // fallback doesn't apply.
                            self.cycle = false;
                            return None;
                        }
                        None => match &variable.fallback {
                            Some(fallback) => self.substitute(fallback)?,
                            None => return None,
                        },
                    }
                }
                TokenOrValue::Function(function) => {
                    let arguments = self.substitute(&function.arguments)?;
                    format!("{}({arguments})", function.name.as_ref())
                }
                TokenOrValue::Token(token) => token.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Color(color) => color.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Url(url) => url.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Length(value) => value.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Angle(value) => value.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Time(value) => value.to_css_string(PrinterOptions::default()).ok()?,
                TokenOrValue::Resolution(value) => {
                    value.to_css_string(PrinterOptions::default()).ok()?
                }
                TokenOrValue::DashedIdent(name) => {
                    name.to_css_string(PrinterOptions::default()).ok()?
                }
                TokenOrValue::AnimationName(name) => {
                    name.to_css_string(PrinterOptions::default()).ok()?
                }
                // `UnresolvedColor` and `env()` serialize only
                // crate-internally: a `var()` among their components can't
                // be resolved here, so the declaration is dropped (as an
                // unresolvable one is).
                TokenOrValue::UnresolvedColor(_) | TokenOrValue::Env(_) => return None,
            };
            self.budget = self.budget.checked_sub(piece.len())?;
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(piece.trim());
        }
        Some(out)
    }
}

/// `initial`/`inherit`/`unset`/`revert` for one property: its field(s)
/// return to the initial value (the `ElementStyle` default — the build's
/// defaults). `inherit` and `unset`-on-inherited also resolve here: the
/// cascade keeps no parent computed style to inherit from (a documented
/// deviation; the earlier rule's value no longer shows through either
/// way). Unknown properties are ignored.
/// The calc-able properties a property name owns: a CSS-wide keyword
/// drops the entries (the field and its calc reset together).
fn calc_props(name: &str) -> &'static [CalcProp] {
    use CalcProp::*;
    match name {
        "width" => &[Width],
        "height" => &[Height],
        "min-width" => &[MinWidth],
        "min-height" => &[MinHeight],
        "max-width" => &[MaxWidth],
        "max-height" => &[MaxHeight],
        "flex-basis" | "flex" => &[FlexBasis],
        "margin" | "inset" => &[MarginTop, MarginRight, MarginBottom, MarginLeft, Left, Top, Right, Bottom],
        "margin-top" => &[MarginTop],
        "margin-right" => &[MarginRight],
        "margin-bottom" => &[MarginBottom],
        "margin-left" => &[MarginLeft],
        "top" => &[Top],
        "right" => &[Right],
        "bottom" => &[Bottom],
        "left" => &[Left],
        _ => &[],
    }
}

fn apply_keyword(style: &mut ElementStyle, id: &PropertyId) {
    // Matched by name: `PropertyId`'s variants carry vendor prefixes for
    // some properties and not others; the names are stable.
    match id.name() {
        "color" => style.color = None,
        "background-color" => style.background = None,
        "opacity" => style.opacity = None,
        "z-index" => style.z_index = None,
        "font-family" => style.font_family = None,
        "font-size" => style.font_size = None,
        "font-weight" => style.bold = None,
        "font-style" => style.italic = None,
        "text-decoration" | "text-decoration-line" => style.text_decoration = None,
        "text-decoration-color" => {
            if let Some(decoration) = &mut style.text_decoration {
                decoration.color = None;
            }
        }
        "outline" => style.outline = OutlineDecl::default(),
        "outline-width" => style.outline.width = None,
        "outline-color" => style.outline.color = None,
        "outline-style" => style.outline.visible = None,
        "outline-offset" => style.outline.offset = None,
        "border-image" | "border-image-source" | "border-image-slice" | "border-image-repeat" => {
            style.border_image = None;
        }
        "border-width" => style.border_width = [None; 4],
        "border-top-width" => style.border_width[0] = None,
        "border-right-width" => style.border_width[1] = None,
        "border-bottom-width" => style.border_width[2] = None,
        "border-left-width" => style.border_width[3] = None,
        "border-color" => style.border_color = [None; 4],
        "border-top-color" => style.border_color[0] = None,
        "border-right-color" => style.border_color[1] = None,
        "border-bottom-color" => style.border_color[2] = None,
        "border-left-color" => style.border_color[3] = None,
        "border-radius" => style.layout.border_radius = [None; 4],
        // A per-corner keyword resets its corner only (the shorthand
        // resets all four).
        "border-top-left-radius" => style.layout.border_radius[0] = None,
        "border-top-right-radius" => style.layout.border_radius[1] = None,
        "border-bottom-right-radius" => style.layout.border_radius[2] = None,
        "border-bottom-left-radius" => style.layout.border_radius[3] = None,
        "padding" => style.padding = [None; 4],
        "padding-top" => style.padding[0] = None,
        "padding-right" => style.padding[1] = None,
        "padding-bottom" => style.padding[2] = None,
        "padding-left" => style.padding[3] = None,
        "row-gap" => style.row_gap = None,
        "column-gap" => style.layout.column_gap = None,
        "gap" => {
            style.row_gap = None;
            style.layout.column_gap = None;
        }
        "display" => style.layout.display = None,
        "visibility" => style.layout.visibility = None,
        "flex-direction" => style.layout.flex_direction = None,
        "flex-wrap" => style.layout.flex_wrap = None,
        "flex-flow" => {
            style.layout.flex_direction = None;
            style.layout.flex_wrap = None;
        }
        "flex-grow" => style.layout.flex_grow = None,
        "flex-shrink" => style.layout.flex_shrink = None,
        "flex-basis" => style.layout.flex_basis = None,
        "flex" => {
            style.layout.flex_grow = None;
            style.layout.flex_shrink = None;
            style.layout.flex_basis = None;
        }
        "overflow" => style.layout.overflow = [None, None],
        "overflow-x" => style.layout.overflow[0] = None,
        "overflow-y" => style.layout.overflow[1] = None,
        "justify-content" => style.layout.justify_content = None,
        "align-items" => style.layout.align_items = None,
        "align-content" => style.layout.align_content = None,
        "align-self" => style.layout.align_self = None,
        "justify-items" => style.layout.justify_items = None,
        "justify-self" => style.layout.justify_self = None,
        "width" => style.layout.width = None,
        "height" => style.layout.height = None,
        "min-width" => style.layout.min_width = None,
        "min-height" => style.layout.min_height = None,
        "max-width" => style.layout.max_width = None,
        "max-height" => style.layout.max_height = None,
        "margin" => style.layout.margin = [None; 4],
        "margin-top" => style.layout.margin[0] = None,
        "margin-right" => style.layout.margin[1] = None,
        "margin-bottom" => style.layout.margin[2] = None,
        "margin-left" => style.layout.margin[3] = None,
        "box-sizing" => style.layout.box_sizing = None,
        "grid-template-rows" => style.layout.grid_template_rows = None,
        "grid-template-columns" => style.layout.grid_template_columns = None,
        "grid-template" => {
            style.layout.grid_template_rows = None;
            style.layout.grid_template_columns = None;
            style.layout.grid_names = None;
        }
        "grid-auto-rows" => style.layout.grid_auto_rows = None,
        "grid-auto-columns" => style.layout.grid_auto_columns = None,
        "grid-auto-flow" => style.layout.grid_auto_flow = None,
        "grid" => {
            style.layout.grid_template_rows = None;
            style.layout.grid_template_columns = None;
            style.layout.grid_auto_rows = None;
            style.layout.grid_auto_columns = None;
            style.layout.grid_auto_flow = None;
            style.layout.grid_names = None;
        }
        "grid-template-areas" => {
            // The names table also carries the track names; dropping the
            // areas takes the whole table.
            style.layout.grid_names = None;
        }
        "grid-row" => style.layout.grid_row = [None, None],
        "grid-row-start" => style.layout.grid_row[0] = None,
        "grid-row-end" => style.layout.grid_row[1] = None,
        "grid-column" => style.layout.grid_column = [None, None],
        "grid-column-start" => style.layout.grid_column[0] = None,
        "grid-column-end" => style.layout.grid_column[1] = None,
        "grid-area" => {
            style.layout.grid_row = [None, None];
            style.layout.grid_column = [None, None];
        }
        "position" => style.layout.position = None,
        "top" => {
            style.layout.inset[0] = None;
            style.calc.retain(|(p, _)| p != &CalcProp::Top);
        }
        "right" => {
            style.layout.inset[1] = None;
            style.calc.retain(|(p, _)| p != &CalcProp::Right);
        }
        "bottom" => {
            style.layout.inset[2] = None;
            style.calc.retain(|(p, _)| p != &CalcProp::Bottom);
        }
        "left" => {
            style.layout.inset[3] = None;
            style.calc.retain(|(p, _)| p != &CalcProp::Left);
        }
        "inset" => {
            style.layout.inset = [None; 4];
            style
                .calc
                .retain(|(p, _)| !matches!(p, CalcProp::Top | CalcProp::Right | CalcProp::Bottom | CalcProp::Left));
        }
        "transition" => style.transitions.clear(),
        _ => {}
    }
    // A keyword also drops the property's `calc()` entries: the field
    // and its calc expression reset together (`width: initial` after
    // `width: calc(50% + 10px)` used to keep resolving the calc).
    for prop in calc_props(id.name()) {
        style.calc.retain(|(p, _)| p != prop);
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
        Property::TextDecoration(decoration, _) => {
            style.text_decoration = Some(decoration_decl(decoration.line, Some(&decoration.color)));
        }
        Property::TextDecorationLine(line, _) => {
            let mut declared = style.text_decoration.take().unwrap_or_default();
            let next = decoration_decl(*line, None);
            declared.underline = next.underline;
            declared.line_through = next.line_through;
            style.text_decoration = Some(declared);
        }
        Property::TextDecorationColor(color, _) => {
            let mut declared = style.text_decoration.take().unwrap_or_default();
            declared.color = to_color(color);
            style.text_decoration = Some(declared);
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
            let sides = [&width.top, &width.right, &width.bottom, &width.left].map(side_width);
            // An unsupported value on one side drops the declaration.
            if sides.iter().all(Option::is_some) {
                style.border_width = sides;
            }
        }
        Property::BorderTopWidth(width) => {
            if let Some(width) = side_width(width) {
                style.border_width[0] = Some(width);
            }
        }
        Property::BorderRightWidth(width) => {
            if let Some(width) = side_width(width) {
                style.border_width[1] = Some(width);
            }
        }
        Property::BorderBottomWidth(width) => {
            if let Some(width) = side_width(width) {
                style.border_width[2] = Some(width);
            }
        }
        Property::BorderLeftWidth(width) => {
            if let Some(width) = side_width(width) {
                style.border_width[3] = Some(width);
            }
        }
        Property::Padding(padding) => {
            let sides =
                [&padding.top, &padding.right, &padding.bottom, &padding.left].map(length_px);
            if sides.iter().all(Option::is_some) {
                style.padding = sides;
            }
        }
        Property::PaddingTop(value) => {
            if let Some(value) = length_px(value) {
                style.padding[0] = Some(value);
            }
        }
        Property::PaddingRight(value) => {
            if let Some(value) = length_px(value) {
                style.padding[1] = Some(value);
            }
        }
        Property::PaddingBottom(value) => {
            if let Some(value) = length_px(value) {
                style.padding[2] = Some(value);
            }
        }
        Property::PaddingLeft(value) => {
            if let Some(value) = length_px(value) {
                style.padding[3] = Some(value);
            }
        }
        Property::RowGap(gap) => {
            // An unsupported value (`%` gap) drops the declaration: the
            // earlier value stands.
            if let Some(gap) = gap_px(gap) {
                style.row_gap = Some(gap);
            }
        }
        Property::ColumnGap(gap) => {
            if let Some(gap) = gap_px(gap) {
                style.layout.column_gap = Some(gap);
            }
        }
        Property::Gap(gap) => {
            if let Some(row) = gap_px(&gap.row) {
                style.row_gap = Some(row);
            }
            if let Some(column) = gap_px(&gap.column) {
                style.layout.column_gap = Some(column);
            }
        }
        Property::Display(value) => {
            if let Some(display) = display(value) {
                style.layout.display = Some(display);
            }
        }
        Property::Visibility(visibility) => {
            // `subtree-hide` semantics: `hidden` maps to Bevy's propagating
            // `Visibility::Hidden`; `collapse` (table-specific) is skipped.
            let visible = match visibility {
                lightningcss::properties::display::Visibility::Visible => Some(true),
                lightningcss::properties::display::Visibility::Hidden => Some(false),
                // `collapse` is unsupported: the declaration is dropped,
                // the earlier value stands.
                lightningcss::properties::display::Visibility::Collapse => {
                    debug!("html css: unsupported visibility: collapse");
                    None
                }
            };
            if visible.is_some() {
                style.layout.visibility = visible;
            };
        }
        Property::Transition(entries, _) => {
            style.transitions = entries
                .iter()
                .filter_map(transition_decl)
                // A repeated property keeps the last declaration.
                .fold(Vec::new(), |mut list, decl| {
                    list.retain(|kept: &TransitionDecl| kept.property != decl.property);
                    list.push(decl);
                    list
                });
        }
        Property::TransitionProperty(..)
        | Property::TransitionDuration(..)
        | Property::TransitionDelay(..)
        | Property::TransitionTimingFunction(..) => {
            let _: Option<()> = unsupported("transition longhands (declare the `transition` shorthand)");
        }
        Property::FlexDirection(direction, _) => {
            style.layout.flex_direction = Some(flex_direction(direction));
        }
        Property::FlexWrap(wrap, _) => style.layout.flex_wrap = Some(flex_wrap(wrap)),
        Property::Overflow(overflow) => {
            style.layout.overflow = [
                Some(overflow_axis(&overflow.x)),
                Some(overflow_axis(&overflow.y)),
            ];
        }
        Property::OverflowX(x) => style.layout.overflow[0] = Some(overflow_axis(x)),
        Property::OverflowY(y) => style.layout.overflow[1] = Some(overflow_axis(y)),
        Property::FlexFlow(flow, _) => {
            style.layout.flex_direction = Some(flex_direction(&flow.direction));
            style.layout.flex_wrap = Some(flex_wrap(&flow.wrap));
        }
        Property::FlexGrow(grow, _) => style.layout.flex_grow = Some(*grow),
        Property::FlexShrink(shrink, _) => style.layout.flex_shrink = Some(*shrink),
        Property::FlexBasis(basis, _) => {
            let mut exprs = Vec::new();
            let value = auto_val(basis, &mut exprs);
            style.set_calc(CalcProp::FlexBasis, value, exprs);
        }
        Property::Flex(flex, _) => {
            style.layout.flex_grow = Some(flex.grow);
            style.layout.flex_shrink = Some(flex.shrink);
            let mut exprs = Vec::new();
            let value = auto_val(&flex.basis, &mut exprs);
            style.set_calc(CalcProp::FlexBasis, value, exprs);
        }
        Property::JustifyContent(value, _) => {
            if let Some(value) = justify_content(value) {
                style.layout.justify_content = Some(value);
            }
        }
        Property::AlignItems(value, _) => {
            if let Some(value) = align_items(value) {
                style.layout.align_items = Some(value);
            }
        }
        Property::AlignContent(value, _) => {
            if let Some(value) = align_content(value) {
                style.layout.align_content = Some(value);
            }
        }
        Property::AlignSelf(value, _) => {
            if let Some(value) = align_self(value) {
                style.layout.align_self = Some(value);
            }
        }
        Property::Width(size) => {
            let mut exprs = Vec::new();
            let value = size_val(size, &mut exprs);
            style.set_calc(CalcProp::Width, value, exprs);
        }
        Property::Height(size) => {
            let mut exprs = Vec::new();
            let value = size_val(size, &mut exprs);
            style.set_calc(CalcProp::Height, value, exprs);
        }
        Property::MinWidth(size) => {
            let mut exprs = Vec::new();
            let value = size_val(size, &mut exprs);
            style.set_calc(CalcProp::MinWidth, value, exprs);
        }
        Property::MinHeight(size) => {
            let mut exprs = Vec::new();
            let value = size_val(size, &mut exprs);
            style.set_calc(CalcProp::MinHeight, value, exprs);
        }
        Property::MaxWidth(size) => {
            let mut exprs = Vec::new();
            let value = max_size_val(size, &mut exprs);
            style.set_calc(CalcProp::MaxWidth, value, exprs);
        }
        Property::MaxHeight(size) => {
            let mut exprs = Vec::new();
            let value = max_size_val(size, &mut exprs);
            style.set_calc(CalcProp::MaxHeight, value, exprs);
        }
        Property::Margin(margin) => {
            for (side, value) in [
                (CalcProp::MarginTop, &margin.top),
                (CalcProp::MarginRight, &margin.right),
                (CalcProp::MarginBottom, &margin.bottom),
                (CalcProp::MarginLeft, &margin.left),
            ] {
                let mut exprs = Vec::new();
                let value = auto_val(value, &mut exprs);
                style.set_calc(side, value, exprs);
            }
        }
        Property::MarginTop(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::MarginTop, value, exprs);
        }
        Property::MarginRight(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::MarginRight, value, exprs);
        }
        Property::MarginBottom(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::MarginBottom, value, exprs);
        }
        Property::MarginLeft(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::MarginLeft, value, exprs);
        }
        Property::BoxSizing(sizing, _) => {
            style.layout.box_sizing = Some(match sizing {
                css_size::BoxSizing::ContentBox => BoxSizing::ContentBox,
                css_size::BoxSizing::BorderBox => BoxSizing::BorderBox,
            });
        }
        Property::JustifyItems(value) => {
            if let Some(value) = justify_items(value) {
                style.layout.justify_items = Some(value);
            }
        }
        Property::JustifySelf(value) => {
            if let Some(value) = justify_self(value) {
                style.layout.justify_self = Some(value);
            }
        }
        Property::GridTemplateRows(rows) => {
            let (tracks, names) = track_sizing(rows, Axis::Row);
            // One unsupported track drops the declaration: the earlier
            // track list (and names) stand.
            if let Some(tracks) = tracks {
                style.layout.grid_template_rows = Some(tracks);
                // A nameless later declaration takes the earlier rule's
                // row names back.
                let table = style
                    .layout
                    .grid_names
                    .get_or_insert_with(GridNames::default);
                table.rows = names.map(|names| names.rows).unwrap_or_default();
            }
        }
        Property::GridTemplateColumns(columns) => {
            let (tracks, names) = track_sizing(columns, Axis::Column);
            if let Some(tracks) = tracks {
                style.layout.grid_template_columns = Some(tracks);
                let table = style
                    .layout
                    .grid_names
                    .get_or_insert_with(GridNames::default);
                table.columns = names.map(|names| names.columns).unwrap_or_default();
            }
        }
        Property::GridTemplate(template) => {
            let (rows, row_lines) = track_sizing(&template.rows, Axis::Row);
            let (columns, column_lines) = track_sizing(&template.columns, Axis::Column);
            // One unsupported track drops the declaration.
            let (Some(rows), Some(columns)) = (rows, columns) else {
                return;
            };
            style.layout.grid_template_rows = Some(rows);
            style.layout.grid_template_columns = Some(columns);
            // The shorthand resets the whole name table: an earlier
            // rule's names don't survive a new `grid-template`.
            style.layout.grid_names = Some(GridNames::default());
            let names = style.layout.grid_names.as_mut().unwrap();
            if let Some(lines) = row_lines {
                names.rows = lines.rows;
            }
            if let Some(lines) = column_lines {
                names.columns = lines.columns;
            }
            if let Some(areas) = parse_areas(&template.areas) {
                names.areas = areas.areas;
                names.implied_rows = areas.implied_rows;
                names.implied_columns = areas.implied_columns;
            }
        }
        Property::GridAutoRows(tracks) => {
            if let Some(tracks) = track_size_list(tracks) {
                style.layout.grid_auto_rows = Some(tracks);
            }
        }
        Property::GridAutoColumns(tracks) => {
            if let Some(tracks) = track_size_list(tracks) {
                style.layout.grid_auto_columns = Some(tracks);
            }
        }
        Property::GridAutoFlow(flow) => style.layout.grid_auto_flow = Some(grid_auto_flow(*flow)),
        Property::Grid(grid) => {
            let (rows, row_names) = track_sizing(&grid.rows, Axis::Row);
            let (columns, column_names) = track_sizing(&grid.columns, Axis::Column);
            // One unsupported track drops the declaration.
            let (Some(rows), Some(columns)) = (rows, columns) else {
                return;
            };
            let auto_rows = track_size_list(&grid.auto_rows);
            let auto_columns = track_size_list(&grid.auto_columns);
            if auto_rows.is_none() || auto_columns.is_none() {
                return;
            }
            style.layout.grid_template_rows = Some(rows);
            style.layout.grid_template_columns = Some(columns);
            style.layout.grid_auto_rows = auto_rows;
            style.layout.grid_auto_columns = auto_columns;
            style.layout.grid_auto_flow = Some(grid_auto_flow(grid.auto_flow));
            // The shorthand resets the name table; an earlier rule's
            // names don't survive a new `grid`.
            style.layout.grid_names = Some(GridNames::default());
            let names = style.layout.grid_names.as_mut().unwrap();
            if let Some(areas) = parse_areas(&grid.areas) {
                names.areas = areas.areas;
                names.implied_rows = areas.implied_rows;
                names.implied_columns = areas.implied_columns;
            }
            if let Some(names) = row_names {
                let table = style
                    .layout
                    .grid_names
                    .get_or_insert_with(GridNames::default);
                table.rows = names.rows;
            }
            if let Some(names) = column_names {
                let table = style
                    .layout
                    .grid_names
                    .get_or_insert_with(GridNames::default);
                table.columns = names.columns;
            }
        }
        Property::GridTemplateAreas(areas) => {
            // A later declaration replaces the areas and the tracks they
            // implied; `none` (or an unparsable matrix) takes them back.
            let names = style
                .layout
                .grid_names
                .get_or_insert_with(GridNames::default);
            if let Some(areas) = parse_areas(areas) {
                names.areas = areas.areas;
                names.implied_rows = areas.implied_rows;
                names.implied_columns = areas.implied_columns;
            } else {
                names.areas.clear();
                names.implied_rows = 0;
                names.implied_columns = 0;
            }
        }
        // An unsupported line drops the declaration: the earlier side
        // keeps its value.
        Property::GridRowStart(line) => style.layout.grid_row[0] = grid_line(line).or(style.layout.grid_row[0].take()),
        Property::GridRowEnd(line) => style.layout.grid_row[1] = grid_line(line).or(style.layout.grid_row[1].take()),
        Property::GridColumnStart(line) => style.layout.grid_column[0] = grid_line(line).or(style.layout.grid_column[0].take()),
        Property::GridColumnEnd(line) => style.layout.grid_column[1] = grid_line(line).or(style.layout.grid_column[1].take()),
        Property::GridRow(row) => {
            let sides = [grid_line(&row.start), grid_line(&row.end)];
            for (side, value) in sides.into_iter().enumerate() {
                style.layout.grid_row[side] = value.or(style.layout.grid_row[side].take());
            }
        }
        Property::GridColumn(column) => {
            let sides = [grid_line(&column.start), grid_line(&column.end)];
            for (side, value) in sides.into_iter().enumerate() {
                style.layout.grid_column[side] =
                    value.or(style.layout.grid_column[side].take());
            }
        }
        Property::GridArea(area) => {
            let sides =
                [grid_line(&area.row_start), grid_line(&area.row_end), grid_line(&area.column_start), grid_line(&area.column_end)];
            for (side, value) in sides.into_iter().enumerate() {
                let target = if side < 2 { &mut style.layout.grid_row } else { &mut style.layout.grid_column };
                target[side % 2] = value.or(target[side % 2].take());
            }
        }
        Property::Position(position) => {
            style.layout.position = match position {
                css_position::Position::Static => Some(CssPosition::Static),
                css_position::Position::Relative => Some(CssPosition::Relative),
                css_position::Position::Absolute => Some(CssPosition::Absolute),
                css_position::Position::Fixed => Some(CssPosition::Fixed),
                css_position::Position::Sticky(_) => unsupported("position sticky"),
            };
        }
        Property::Top(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::Top, value, exprs);
        }
        Property::Right(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::Right, value, exprs);
        }
        Property::Bottom(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::Bottom, value, exprs);
        }
        Property::Left(value) => {
            let mut exprs = Vec::new();
            let value = auto_val(value, &mut exprs);
            style.set_calc(CalcProp::Left, value, exprs);
        }
        Property::Inset(inset) => {
            for (side, value) in [
                (CalcProp::Top, &inset.top),
                (CalcProp::Right, &inset.right),
                (CalcProp::Bottom, &inset.bottom),
                (CalcProp::Left, &inset.left),
            ] {
                let mut exprs = Vec::new();
                let value = auto_val(value, &mut exprs);
                style.set_calc(side, value, exprs);
            }
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
            // An unsupported corner drops the declaration: the earlier
            // corners keep their values.
            let corners = [
                &radius.top_left,
                &radius.top_right,
                &radius.bottom_right,
                &radius.bottom_left,
            ]
            .map(corner_radius);
            for (corner, value) in corners.into_iter().enumerate() {
                style.layout.border_radius[corner] =
                    value.or(style.layout.border_radius[corner].take());
            }
        }
        // An unsupported value drops the declaration: the earlier
        // corners keep their values.
        Property::BorderTopLeftRadius(radius, _) => {
            style.layout.border_radius[0] = corner_radius(radius).or(style.layout.border_radius[0].take());
        }
        Property::BorderTopRightRadius(radius, _) => {
            style.layout.border_radius[1] = corner_radius(radius).or(style.layout.border_radius[1].take());
        }
        Property::BorderBottomRightRadius(radius, _) => {
            style.layout.border_radius[2] = corner_radius(radius).or(style.layout.border_radius[2].take());
        }
        Property::BorderBottomLeftRadius(radius, _) => {
            style.layout.border_radius[3] = corner_radius(radius).or(style.layout.border_radius[3].take());
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
        Property::OutlineWidth(width) => style.outline.width = side_width(width).or(style.outline.width.take()),
        Property::OutlineColor(color) => style.outline.color = Some(to_color(color)),
        // lightningcss has no typed `outline-offset` either.
        Property::Custom(custom) if matches!(&custom.name, CustomPropertyName::Unknown(name) if name.as_ref() == "outline-offset") =>
        {
            let value = declaration
                .value_to_css_string(PrinterOptions::default())
                .unwrap_or_default();
            let offset = match value.trim().strip_suffix("px").map(str::parse::<f32>) {
                Some(Ok(px)) => Some(px),
                _ if value.trim() == "0" => Some(0.0),
                _ => unsupported("outline-offset value (px only)"),
            };
            if offset.is_some() {
                style.outline.offset = offset;
            }
        }
        // lightningcss has no typed `pointer-events` (it's SVG/UI-only): it
        // arrives as an unknown property with its raw value.
        Property::Custom(custom) if matches!(&custom.name, CustomPropertyName::Unknown(name) if name.as_ref() == "pointer-events") =>
        {
            let value = declaration
                .value_to_css_string(PrinterOptions::default())
                .unwrap_or_default();
            let pointer_events = match value.trim().to_ascii_lowercase().as_str() {
                "none" => Some(false),
                "auto" => Some(true),
                _ => unsupported("pointer-events value (only `auto`/`none`)"),
            };
            if pointer_events.is_some() {
                style.pointer_events = pointer_events;
            }
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
/// list; the `[name]` line names register on the container's name table
/// (placements resolve against them). One unsupported track drops the
/// whole declaration, as an invalid value would.
/// The most tracks one axis may declare: beyond it the declaration drops
/// (line numbers are `i16` — a `repeat(40000, …)` overflowed the
/// arithmetic and a 32 767-row `grid-template-areas` overflowed the area
/// bounds — and no authored grid needs a pathological count anyway).
const MAX_GRID_TRACKS: usize = 1000;

fn track_sizing(
    sizing: &TrackSizing,
    axis: Axis,
) -> (Option<Vec<RepeatedGridTrack>>, Option<GridNames>) {
    let list = match sizing {
        TrackSizing::None => return (Some(Vec::new()), None),
        TrackSizing::TrackList(list) => list,
    };
    if total_lines(list) > MAX_GRID_TRACKS {
        return (None, None);
    }
    let has_names = list.line_names.iter().any(|names| !names.is_empty())
        || list.items.iter().any(|item| {
            matches!(item, TrackListItem::TrackRepeat(repeat) if repeat.line_names.iter().any(|names| !names.is_empty()))
        });
    let auto_count = list.items.iter().any(|item| {
        matches!(item, TrackListItem::TrackRepeat(repeat) if matches!(repeat.count, RepeatCount::AutoFill | RepeatCount::AutoFit))
    });
    let mut table = GridNames::default();
    let map = match axis {
        Axis::Row => &mut table.rows,
        Axis::Column => &mut table.columns,
    };
    // Registers a 0-based line index under its names: as the 1-based line
    // and its negative (from the end).
    let register = |line: usize, names: &[CustomIdent<'_>], map: &mut HashMap<String, Vec<i16>>| {
        for name in names {
            let lower = (line + 1) as i16;
            let entries = map.entry(name.0.to_string()).or_default();
            // A merged repeat boundary is one line: the same name on it
            // registers once (a duplicate occurrence made `3 a` resolve
            // to the first occurrence instead of the third).
            if entries.contains(&lower) {
                continue;
            }
            entries.push(lower);
            // CSS: line `i` of `L` lines is also `i - L - 1` (line 1 of 4
            // is -4); `total_lines` counts tracks, so `L` is tracks + 1.
            entries.push(lower - total_lines(list) as i16 - 2);
        }
    };
    if has_names && !auto_count {
        let mut line = 0usize;
        for (index, item) in list.items.iter().enumerate() {
            for name in &list.line_names[index] {
                register(line, std::slice::from_ref(name), map);
            }
            match item {
                TrackListItem::TrackSize(_) => line += 1,
                TrackListItem::TrackRepeat(repeat) => {
                    let tracks = repeat.track_sizes.len();
                    let count = match &repeat.count {
                        RepeatCount::Number(n) => *n as usize,
                        _ => 1,
                    };
                    // The repeat's internal boundaries, once per
                    // repetition. The end boundary of repetition k is the
                    // SAME line as repetition k+1's start (adjacent
                    // repetitions merge): registering it every repetition
                    // puts both names on that line.
                    for repetition in 0..count {
                        for (j, boundary) in repeat.line_names.iter().enumerate() {
                            if j <= tracks {
                                register(line + repetition * tracks + j, boundary, map);
                            }
                        }
                    }
                    line += tracks * count;
                }
            }
        }
        for name in &list.line_names[list.items.len()] {
            register(line, std::slice::from_ref(name), map);
        }
    } else if has_names {
        // An auto-fill/auto-fit repeat's count is layout-dependent: the
        // names around it keep what is knowable — boundaries before the
        // first repeat resolve as positive lines, the list's final
        // boundary (the grid's last line) resolves from the end. Names
        // inside or after (before the last line) stay unresolved.
        let first_repeat = list
            .items
            .iter()
            .position(|item| !matches!(item, TrackListItem::TrackSize(_)));
        let mut line = 0usize;
        for (index, item) in list.items.iter().enumerate() {
            // Boundary `index` precedes item `index`: boundaries up to and
            // including the first repeat's own start are knowable (their
            // line doesn't depend on the repeat count); everything after
            // the repeat starts is not.
            let before_or_at_first_repeat =
                first_repeat.is_none_or(|first| index <= first);
            for name in &list.line_names[index] {
                if before_or_at_first_repeat {
                    // Positive spelling only: the negative spelling
                    // depends on the (layout-dependent) total line count.
                    let entries = map.entry(name.0.to_string()).or_default();
                    entries.push((line + 1) as i16);
                }
            }
            match item {
                TrackListItem::TrackSize(_) => line += 1,
                TrackListItem::TrackRepeat(repeat) => {
                    line += repeat.track_sizes.len();
                }
            }
        }
        let last = list.items.len();
        for name in &list.line_names[last] {
            let entries = map.entry(name.0.to_string()).or_default();
            entries.push(-1);
        }
    }
    let tracks: Option<Vec<RepeatedGridTrack>> = list
        .items
        .iter()
        .map(|item| match item {
            TrackListItem::TrackSize(size) => grid_track(size).map(RepeatedGridTrack::from),
            TrackListItem::TrackRepeat(repeat) => {
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
        .collect();
    let Some(tracks) = tracks else {
        return (None, None);
    };
    (Some(tracks), has_names.then_some(table))
}

/// The total track count of a track list (repeats expanded); `auto-fill`/
/// `auto-fit` repeats count as one track (their count is layout-dependent).
fn total_lines(list: &css_grid::TrackList) -> usize {
    list.items
        .iter()
        .map(|item| match item {
            TrackListItem::TrackSize(_) => 1,
            TrackListItem::TrackRepeat(repeat) => {
                repeat.track_sizes.len()
                    * match &repeat.count {
                        RepeatCount::Number(n) => *n as usize,
                        _ => 1,
                    }
            }
        })
        .sum()
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
        TrackSize::FitContent(limit) => match length_percentage_val(limit, &mut Vec::new())? {
            Val::Px(px) => Some(GridTrack::fit_content_px(px)),
            Val::Percent(percent) => Some(GridTrack::fit_content_percent(percent)),
            _ => unsupported("fit-content() limit"),
        },
    }
}

/// A track's minimum: never `fr` (invalid CSS there).
fn min_breadth(breadth: &TrackBreadth) -> Option<MinTrackSizingFunction> {
    Some(match breadth {
        TrackBreadth::Length(length) => match length_percentage_val(length, &mut Vec::new())? {
            Val::Px(v) => MinTrackSizingFunction::Px(v),
            Val::Percent(v) => MinTrackSizingFunction::Percent(v),
            Val::Vw(v) => MinTrackSizingFunction::Vw(v),
            Val::Vh(v) => MinTrackSizingFunction::Vh(v),
            Val::VMin(v) => MinTrackSizingFunction::VMin(v),
            Val::VMax(v) => MinTrackSizingFunction::VMax(v),
            Val::Auto => MinTrackSizingFunction::Auto,
            Val::Em(v) => MinTrackSizingFunction::Em(v),
            Val::Rem(v) => MinTrackSizingFunction::Rem(v),
        },
        TrackBreadth::Auto => MinTrackSizingFunction::Auto,
        TrackBreadth::MinContent => MinTrackSizingFunction::MinContent,
        TrackBreadth::MaxContent => MinTrackSizingFunction::MaxContent,
        TrackBreadth::Flex(_) => return unsupported("fr as a track minimum"),
    })
}

fn max_breadth(breadth: &TrackBreadth) -> Option<MaxTrackSizingFunction> {
    Some(match breadth {
        TrackBreadth::Length(length) => match length_percentage_val(length, &mut Vec::new())? {
            Val::Px(v) => MaxTrackSizingFunction::Px(v),
            Val::Percent(v) => MaxTrackSizingFunction::Percent(v),
            Val::Vw(v) => MaxTrackSizingFunction::Vw(v),
            Val::Vh(v) => MaxTrackSizingFunction::Vh(v),
            Val::VMin(v) => MaxTrackSizingFunction::VMin(v),
            Val::VMax(v) => MaxTrackSizingFunction::VMax(v),
            Val::Auto => MaxTrackSizingFunction::Auto,
            Val::Em(v) => MaxTrackSizingFunction::Em(v),
            Val::Rem(v) => MaxTrackSizingFunction::Rem(v),
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

/// One placement side: `auto`, a line number, a named line or area (resolved
/// against the parent grid's `GridNames` at collection — see
/// `LayoutDecl::resolve_names`) or a span.
fn grid_line(line: &css_grid::GridLine) -> Option<GridLineDecl> {
    match line {
        css_grid::GridLine::Auto => Some(GridLineDecl::Auto),
        // A named grid area (lightningcss postfixed it `-start`/`-end`?
        // No — the plain name): resolved against the parent grid.
        css_grid::GridLine::Area { name } => Some(GridLineDecl::Named {
            name: name.0.to_string(),
            index: None,
        }),
        css_grid::GridLine::Line { index, name: None } => {
            i16::try_from(*index).ok().map(GridLineDecl::Line)
        }
        // A named line filtered to an occurrence (`2 main-start`).
        css_grid::GridLine::Line {
            index,
            name: Some(name),
        } => Some(GridLineDecl::Named {
            name: name.0.to_string(),
            index: i16::try_from(*index).ok(),
        }),
        css_grid::GridLine::Span { index, name: None } => {
            u16::try_from(*index).ok().map(GridLineDecl::Span)
        }
        css_grid::GridLine::Span { .. } => {
            unsupported::<GridLineDecl>("a span anchored at a named line")
        }
    }
}

/// `grid-template-areas`: the areas' line bounds and the implied track
/// counts. Invalid areas (a non-rectangular region can't reach here —
/// lightningcss rejects them at parse; an empty declaration is `None`).
fn parse_areas(areas: &css_grid::GridTemplateAreas) -> Option<GridNames> {
    let css_grid::GridTemplateAreas::Areas { columns, areas } = areas else {
        return None;
    };
    let columns = *columns as usize;
    let rows = areas.len() / columns.max(1);
    if columns == 0 || columns > MAX_GRID_TRACKS || rows > MAX_GRID_TRACKS {
        return None;
    }
    // Each name's region: the min/max row and column it spans.
    let mut bounds: HashMap<String, (u16, u16, u16, u16)> = HashMap::new();
    for (index, name) in areas.iter().enumerate() {
        let Some(name) = name else { continue };
        let (row, column) = (index / columns, index % columns);
        let entry = bounds.entry(name.clone()).or_insert((
            row as u16,
            row as u16,
            column as u16,
            column as u16,
        ));
        entry.0 = entry.0.min(row as u16);
        entry.1 = entry.1.max(row as u16);
        entry.2 = entry.2.min(column as u16);
        entry.3 = entry.3.max(column as u16);
    }
    let areas = bounds
        .into_iter()
        .map(|(name, (r1, r2, c1, c2))| {
            (
                name,
                [r1 as i16 + 1, r2 as i16 + 2, c1 as i16 + 1, c2 as i16 + 2],
            )
        })
        .collect();
    Some(GridNames {
        areas,
        implied_rows: rows as u16,
        implied_columns: columns as u16,
        ..GridNames::default()
    })
}

/// `width`/`height`/`min-*`: `auto`, lengths, `%`.
fn size_val(size: &Size, sink: &mut Vec<CalcExpr>) -> Option<Val> {
    match size {
        Size::Auto => Some(Val::Auto),
        Size::LengthPercentage(value) => length_percentage_val(value, sink),
        _ => unsupported("intrinsic size keyword"),
    }
}

/// `max-*`: `none` (no limit), lengths, `%`.
fn max_size_val(size: &MaxSize, sink: &mut Vec<CalcExpr>) -> Option<Val> {
    match size {
        MaxSize::None => Some(Val::Auto),
        MaxSize::LengthPercentage(value) => length_percentage_val(value, sink),
        _ => unsupported("intrinsic size keyword"),
    }
}

/// `margin`/`flex-basis`: `auto`, lengths, `%`.
fn auto_val(value: &LengthPercentageOrAuto, sink: &mut Vec<CalcExpr>) -> Option<Val> {
    match value {
        LengthPercentageOrAuto::Auto => Some(Val::Auto),
        LengthPercentageOrAuto::LengthPercentage(value) => length_percentage_val(value, sink),
    }
}

/// Absolute lengths, viewport units and `%` (of the parent, as in CSS);
/// font-relative units are leaves and `calc()` flattens into the crate's
/// expression tree (mixed-unit sums resolve at layout time).
/// Converts lightningcss's `Calc` AST into the crate's expression tree,
/// through `leaf` for the value leaves. `None` for anything unsupported
/// (round()/mod()/abs()/… — logged).
fn calc_expr<V>(calc: &Calc<V>, leaf: &impl Fn(&V) -> Option<CalcExpr>) -> Option<CalcExpr> {
    match calc {
        Calc::Value(value) => leaf(value),
        Calc::Number(n) => Some(CalcExpr::Number(*n as f32)),
        Calc::Sum(a, b) => Some(CalcExpr::Sum(
            Box::new(calc_expr(a, leaf)?),
            Box::new(calc_expr(b, leaf)?),
        )),
        Calc::Product(n, v) => Some(CalcExpr::Product(*n as f32, Box::new(calc_expr(v, leaf)?))),
        Calc::Function(function) => match &**function {
            MathFunction::Calc(inner) => calc_expr(inner, leaf),
            MathFunction::Min(args) => Some(CalcExpr::Min(calc_args(args, leaf)?)),
            MathFunction::Max(args) => Some(CalcExpr::Max(calc_args(args, leaf)?)),
            MathFunction::Clamp(min, value, max) => Some(CalcExpr::Max(vec![
                calc_expr(min, leaf)?,
                CalcExpr::Min(vec![calc_expr(value, leaf)?, calc_expr(max, leaf)?]),
            ])),
            _ => {
                unsupported::<GridPlacement>("math function in calc()");
                None
            }
        },
    }
}

fn calc_args<'i, V>(
    args: &[Calc<V>],
    leaf: &impl Fn(&V) -> Option<CalcExpr>,
) -> Option<Vec<CalcExpr>> {
    args.iter().map(|arg| calc_expr(arg, leaf)).collect()
}

fn length_percentage_val(value: &LengthPercentage, sink: &mut Vec<CalcExpr>) -> Option<Val> {
    match value {
        LengthPercentage::Dimension(length) => match length {
            LengthValue::Vw(vw) => Some(Val::Vw(*vw)),
            LengthValue::Vh(vh) => Some(Val::Vh(*vh)),
            LengthValue::Vmin(v) => Some(Val::VMin(*v)),
            LengthValue::Vmax(v) => Some(Val::VMax(*v)),
            // Bevy resolves `em` against the node's `EmSize` (which the
            // build sets from the cascade's computed font-size) and `rem`
            // against the `RemSize` resource (synced to the root rule's
            // font-size).
            LengthValue::Em(em) => Some(Val::Em(*em)),
            LengthValue::Rem(rem) => Some(Val::Rem(*rem)),
            length => length
                .to_px()
                .map(Val::Px)
                .or_else(|| unsupported("relative length")),
        },
        LengthPercentage::Percentage(percentage) => Some(Val::Percent(percentage.0 * 100.0)),
        LengthPercentage::Calc(calc) => {
            let expr = calc_expr(calc, &length_percentage_leaf)?;
            sink.push(expr);
            None
        }
    }
}

/// A `LengthPercentage`'s leaf as a calc term.
fn length_percentage_leaf(value: &LengthPercentage) -> Option<CalcExpr> {
    match value {
        LengthPercentage::Dimension(length) => length_val(length),
        LengthPercentage::Percentage(percentage) => {
            Some(CalcExpr::Val(Val::Percent(percentage.0 * 100.0)))
        }
        LengthPercentage::Calc(calc) => calc_expr(calc, &length_percentage_leaf),
    }
}

/// A `LengthValue` leaf as a calc term.
fn length_val(length: &LengthValue) -> Option<CalcExpr> {
    Some(CalcExpr::Val(match length {
        LengthValue::Vw(vw) => Val::Vw(*vw),
        LengthValue::Vh(vh) => Val::Vh(*vh),
        LengthValue::Vmin(v) => Val::VMin(*v),
        LengthValue::Vmax(v) => Val::VMax(*v),
        LengthValue::Em(em) => Val::Em(*em),
        LengthValue::Rem(rem) => Val::Rem(*rem),
        length => length.to_px().map(Val::Px)?,
    }))
}

fn unsupported<T>(what: &str) -> Option<T> {
    debug!("html css: unsupported {what}");
    None
}

/// One declared `transition` entry over the properties the crate animates;
/// every other property (and `all`) is skipped, logged.
fn transition_decl(entry: &Transition) -> Option<TransitionDecl> {
    let property = match &entry.property {
        PropertyId::Color => TransitionProp::TextColor,
        PropertyId::BackgroundColor => TransitionProp::BackgroundColor,
        PropertyId::BorderColor => TransitionProp::BorderColor,
        id => {
            let _: Option<()> = unsupported(&format!("transition on {id:?}"));
            return None;
        }
    };
    Some(TransitionDecl {
        property,
        duration: entry.duration.to_ms() / 1000.0,
        delay: entry.delay.to_ms() / 1000.0,
        easing: entry.timing_function.clone(),
    })
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

/// One `border-*-radius` corner as CSS's `[horizontal, vertical]` pair.
/// `%` resolves per-axis against the node's dimensions, as CSS does.
fn corner_radius(radius: &Size2D<LengthPercentage>) -> Option<(Val, Val)> {
    Some((
        length_percentage_val(&radius.0, &mut Vec::new())?,
        length_percentage_val(&radius.1, &mut Vec::new())?,
    ))
}

/// `text-decoration-line` bits (and the shorthand's color): Bevy draws
/// underline and line-through; `overline`, `blink` and the spelling/grammar
/// decorations have no Bevy counterpart and are skipped.
fn decoration_decl(line: TextDecorationLine, color: Option<&CssColor>) -> DecorationDecl {
    DecorationDecl {
        underline: line.contains(TextDecorationLine::Underline),
        line_through: line.contains(TextDecorationLine::LineThrough),
        color: color.and_then(to_color),
    }
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
    #[test]
    fn calc_collapses_to_a_single_value() {
        // Single-unit expressions become the field directly.
        let style = declared("width: calc(10px + 5px); height: calc(2em); margin-left: calc(3rem)");
        assert_eq!(style.layout.width, Some(Val::Px(15.0)));
        assert_eq!(style.layout.height, Some(Val::Em(2.0)));
        assert_eq!(style.layout.margin[3], Some(Val::Rem(3.0)));
        assert!(style.calc.is_empty());
    }

    #[test]
    fn calc_mixed_units_go_to_the_layout_pass() {
        let style = declared("width: calc(50% + 10px); top: calc(1em + 2vh)");
        assert_eq!(style.layout.width, None); // placeholder: the pass resolves
        assert_eq!(style.layout.inset[0], None);
        assert_eq!(style.calc.len(), 2);
        assert_eq!(style.calc[0].0, CalcProp::Width);
        assert_eq!(style.calc[1].0, CalcProp::Top);
    }

    #[test]
    fn calc_later_declaration_replaces_the_calc() {
        // The cascade order: a later plain declaration wins over the calc.
        let style = declared("width: calc(50% + 10px); width: 80px");
        assert_eq!(style.layout.width, Some(Val::Px(80.0)));
        assert!(style.calc.is_empty());
        // And vice versa.
        let style = declared("width: 80px; width: calc(50% + 10px)");
        assert_eq!(style.layout.width, None);
        assert_eq!(style.calc.len(), 1);
    }

    #[test]
    fn calc_min_max_clamp_and_products() {
        let style = declared(
            "margin-top: calc(min(10px, 20px)); margin-right: calc(max(10px, 20px)); \
             margin-bottom: calc(clamp(5px, 50%, 30px)); top: calc(2 * (10px + 5px))",
        );
        assert_eq!(style.layout.margin[0], Some(Val::Px(10.0)));
        assert_eq!(style.layout.margin[1], Some(Val::Px(20.0)));
        // clamp(5px, 50%, 30px): mixed (px bounds, % value) → the pass.
        assert_eq!(style.layout.margin[2], None);
        assert_eq!(style.calc[0].0, CalcProp::MarginBottom);
        // A product over a parenthesized sum collapses (all px).
        assert_eq!(style.layout.inset[0], Some(Val::Px(30.0)));
    }

    #[test]
    fn calc_percent_resolves_by_axis() {
        // Percentages in sizes resolve against the containing block's
        // content box per axis; the pass is checked in the layout vector.
        let style = declared("height: calc(50% + 1px)");
        assert_eq!(style.layout.height, None);
        assert_eq!(style.calc[0].0, CalcProp::Height);
        assert!(
            style.calc[0]
                .1
                .resolve(
                    &CalcBasis {
                        em: 16.0,
                        rem: 16.0,
                        viewport: Vec2::new(800.0, 600.0),
                        content: Vec2::new(400.0, 300.0),
                    },
                    false
                )
                .map(|v| (v - 151.0).abs() < f32::EPSILON)
                .unwrap_or(false)
        );
    }

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
            ..Default::default()
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
        HtmlStyles::from_sheet(&sheet)
            .get(element, pseudo, &[])
            .color
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
    /// THROWAWAY: the demo's `.columns` rule, parsed.
    #[test]
    fn throwaway_columns_decl() {
        let style = declared(
            "flex-direction: row; gap: 16px; padding: 16px; flex-grow: 1; \
             flex-shrink: 1; flex-basis: 0; min-height: 0;",
        );
        eprintln!(
            "DECL grow={:?} shrink={:?} basis={:?} min_h={:?} dir={:?}",
            style.layout.flex_grow,
            style.layout.flex_shrink,
            style.layout.flex_basis,
            style.layout.min_height,
            style.layout.flex_direction,
        );
    }

    /// THROWAWAY-TO-KEEP: combinator matching.
    fn selector(text: &str) -> Selector {
        Selector::parse(text).expect(text)
    }

    fn ancestor(tag: &str, classes: &[&str]) -> Ancestor {
        Ancestor {
            element: element(tag, None, classes),
            pseudo: Pseudo::default(),
        }
    }

    /// `.panel p` matches a paragraph under a `.panel` at any depth, and
    /// nothing when the panel isn't an ancestor.
    #[test]
    fn descendant_combinator_matches_at_any_depth() {
        // A deep chain of non-matching candidates: the descendant
        // backtracking retried every combination and took ~600 ms per
        // element; the failure memo makes it linear.
        let start = std::time::Instant::now();
        let deep = selector(".b .a .a .a .a .a .a p");
        let ancestors: Vec<Ancestor> = (0..40)
            .map(|_| ancestor("div", &["a"]))
            .collect();
        assert!(!deep.matches(&element("p", None, &[]), Pseudo::default(), &ancestors));
        assert!(
            start.elapsed() < std::time::Duration::from_millis(100),
            "the combinator matching took {:?}",
            start.elapsed()
        );
        let selector = selector(".panel p");
        let ancestors = vec![ancestor("div", &["panel"]), ancestor("section", &["page"])];
        assert!(
            selector.matches(&element("p", None, &[]), Pseudo::default(), &ancestors),
            "the panel is an ancestor"
        );
        assert!(
            !selector.matches(&element("p", None, &[]), Pseudo::default(), &[]),
            "no ancestors"
        );
        assert!(
            !selector.matches(
                &element("p", None, &[]),
                Pseudo::default(),
                &[ancestor("div", &["page"])]
            ),
            "the wrong ancestor"
        );
    }

    /// `.panel > p` matches only a direct child of a `.panel`.
    #[test]
    fn child_combinator_needs_the_direct_parent() {
        let selector = selector(".panel > p");
        let direct = vec![ancestor("div", &["panel"]), ancestor("section", &["page"])];
        assert!(
            selector.matches(&element("p", None, &[]), Pseudo::default(), &direct),
            "the panel is the parent"
        );
        let deep = vec![ancestor("div", &["other"]), ancestor("div", &["panel"])];
        assert!(
            !selector.matches(&element("p", None, &[]), Pseudo::default(), &deep),
            "the panel is a grandparent"
        );
        assert!(
            !selector.matches(&element("p", None, &[]), Pseudo::default(), &[]),
            "no parent"
        );
    }

    /// A descendant compound may match a non-nearest ancestor; the walk
    /// continues above it.
    #[test]
    fn descendant_picks_the_matching_ancestor() {
        let selector = selector(".a .b p");
        let ancestors = vec![
            ancestor("div", &["b"]),
            ancestor("section", &["a"]),
            ancestor("main", &["outer"]),
        ];
        assert!(
            selector.matches(&element("p", None, &[]), Pseudo::default(), &ancestors),
            ".b is the parent, .a is above it"
        );
        // .b and .a are the other way round: no chain satisfies both.
        let wrong_order = vec![ancestor("div", &["a"]), ancestor("section", &["b"])];
        assert!(
            !selector.matches(&element("p", None, &[]), Pseudo::default(), &wrong_order),
            ".a must be above .b"
        );
    }

    /// Pseudo-classes work on ancestors too (`.panel:hover p`).
    #[test]
    fn pseudo_classes_work_on_ancestors() {
        let selector = selector(".panel:hover p");
        let hovered = Ancestor {
            element: element("div", None, &["panel"]),
            pseudo: Pseudo {
                hover: true,
                ..Pseudo::default()
            },
        };
        assert!(selector.matches(
            &element("p", None, &[]),
            Pseudo::default(),
            std::slice::from_ref(&hovered)
        ));
        assert!(!selector.matches(
            &element("p", None, &[]),
            Pseudo::default(),
            &[ancestor("div", &["panel"])]
        ));
    }

    /// Specificity sums over the compounds (`.panel p` beats `p`).
    #[test]
    fn specificity_sums_over_compounds() {
        assert_eq!(selector(".panel p").specificity(), (0, 1, 1));
        assert_eq!(selector("#a .b > p").specificity(), (1, 1, 1));
        assert_eq!(selector("p").specificity(), (0, 0, 1));
    }

    /// Unsupported selectors skip the whole rule: sibling combinators,
    /// attributes, and other pseudo-classes.
    #[test]
    fn unsupported_selectors_are_skipped() {
        assert!(Selector::parse(".a + p").is_none());
        assert!(Selector::parse(".a ~ p").is_none());
        assert!(Selector::parse("p[attr]").is_none());
        assert!(Selector::parse("p:focus-within").is_none());
        assert!(
            Selector::parse(".a > p b").is_some(),
            "chains of any length"
        );
    }

    /// `grid-template-areas` parses the areas' bounds and the implied
    /// track counts; a declaration without areas is `None`.
    #[test]
    fn grid_areas_bounds_and_implied_tracks() {
        let style = declared(
            "display: grid; grid-template-areas: \"logo nav\" \"main side\" \"main foot\";",
        );
        let names = style.layout.grid_names.clone().expect("areas");
        eprintln!("AREAS: {:?}", names.areas);
        assert_eq!(names.implied_rows, 3);
        assert_eq!(names.implied_columns, 2);
        assert_eq!(names.areas["logo"], [1, 2, 1, 2]);
        assert_eq!(names.areas["main"], [2, 4, 1, 2]);
        assert_eq!(names.areas["side"], [2, 3, 2, 3]);
        assert_eq!(names.areas["foot"], [3, 4, 2, 3]);
        // The implied tracks materialize on the node.
        let mut node = Node::default();
        style.layout.apply_to(&mut node);
        assert_eq!(node.grid_template_rows.len(), 3);
        assert_eq!(node.grid_template_columns.len(), 2);
    }

    /// `[name]` line names in the track lists register at their lines
    /// (positive and negative).
    #[test]
    fn grid_line_names_register() {
        let style = declared("display: grid; grid-template-columns: [start] 1fr [mid] 2fr [end];");
        let names = style.layout.grid_names.expect("line names");
        // Two tracks, three lines: line `i` of 3 is also `i - 4`.
        assert_eq!(names.columns["start"], &[1, -3]);
        assert_eq!(names.columns["mid"], &[2, -2]);
        assert_eq!(names.columns["end"], &[3, -1]);
    }

    /// A child's named placement resolves against the parent's template:
    /// area names to their region's lines per side, line names to their
    /// line; unknown names become `auto`.
    #[test]
    fn named_placement_resolves_against_the_parent() {
        let mut child = declared("grid-column: main / span 2; grid-row: side;");
        child.layout.resolve_names(None); // no parent grid: names become auto
        assert_eq!(child.layout.grid_column[0], Some(GridLineDecl::Auto));

        let parent = declared(
            "display: grid; grid-template-columns: [main-start] 1fr [main-end] 1fr; \
             grid-template-areas: \"head head\" \"side main\";",
        );
        let grid = parent.layout.grid_names.as_ref();
        let mut child = declared("grid-column: main / span 2; grid-row: side;");
        child.layout.resolve_names(grid);
        // The area `main`: column 2/3, row 2/3.
        assert_eq!(
            child.layout.grid_column,
            [Some(GridLineDecl::Line(2)), Some(GridLineDecl::Span(2)),]
        );
        assert_eq!(
            child.layout.grid_row,
            [Some(GridLineDecl::Line(2)), Some(GridLineDecl::Line(3)),]
        );
        // A named LINE: `main-start` is column line 1.
        let mut child = declared("grid-column: main-start / main-end;");
        child.layout.resolve_names(grid);
        assert_eq!(
            child.layout.grid_column,
            [Some(GridLineDecl::Line(1)), Some(GridLineDecl::Line(2)),]
        );
        // Unknown names become auto.
        let mut child = declared("grid-column: nope / nope;");
        child.layout.resolve_names(grid);
        assert_eq!(
            child.layout.grid_column,
            [Some(GridLineDecl::Auto), Some(GridLineDecl::Auto)]
        );
        // An occurrence index: the second line named `main-start` (none
        // here — only one) stays unresolvable.
        let mut child = declared("grid-column: 2 main-start;");
        child.layout.resolve_names(grid);
        assert_eq!(child.layout.grid_column[0], Some(GridLineDecl::Auto));
    }

    /// Negative named-line occurrences count from the end: `-1 a` is the
    /// last `a` line (its negative spelling), and going further back than
    /// there are occurrences leaves the placement auto (the old code
    /// clamped to the first).
    #[test]
    fn negative_named_lines_count_from_the_end() {
        let style = declared(
            "display: grid; grid-template-columns: [a] 1fr [a] 1fr [a] 1fr [b]",
        );
        let grid = style.layout.grid_names.as_ref();
        let resolve = |placement: &str| {
            let mut child = declared(&format!("grid-column: {placement}"));
            child.layout.resolve_names(grid);
            child.layout.grid_column[0].clone()
        };
        // `-1 a` = the last `a` (line 3 of 4 = `-2`), `-2 a` = line 2
        // (`-3`), `-3 a` = line 1 (`-4`), and `-4 a` doesn't exist.
        assert_eq!(resolve("-1 a"), Some(GridLineDecl::Line(-2)));
        assert_eq!(resolve("-2 a"), Some(GridLineDecl::Line(-3)));
        assert_eq!(resolve("-3 a"), Some(GridLineDecl::Line(-4)));
        assert_eq!(resolve("-4 a"), Some(GridLineDecl::Auto));
    }

    /// A name at the end of a `repeat()` registers (the end boundary
    /// exists after the last repetition), and the names around an
    /// `auto-fill` repeat keep their knowable spellings: before the
    /// repeat as positive lines, the final boundary from the end.
    #[test]
    fn repeat_names_register() {
        let style = declared(
            "display: grid; grid-template-columns: repeat(2, [a] 1fr [b]) [c]",
        );
        let names = style.layout.grid_names.expect("line names");
        // One track per repetition: `a`@1, the merged boundary (`b` and
        // `a` on the SAME line — registered once each; a duplicate
        // occurrence made `3 a` resolve to the first occurrence), then
        // `b` and the trailing `c`.
        assert_eq!(names.columns["a"], &[1, -3, 2, -2]);
        assert_eq!(names.columns["b"], &[2, -2, 3, -1]);
        assert_eq!(names.columns["c"], &[3, -1]);

        // A name on both sides of a merged boundary counts once: `3 a`
        // resolves to the third occurrence (the second track's end).
        let style = declared(
            "display: grid; grid-template-columns: repeat(2, [a] 1fr [a]); grid-column: 3 a",
        );
        let names = style.layout.grid_names.expect("line names");
        assert_eq!(names.columns["a"], &[1, -3, 2, -2, 3, -1]);
        let line = names.line(Axis::Column, Side::Start, "a", Some(3));
        assert_eq!(
            line,
            Some(GridLineDecl::Line(3)),
            "the merged boundary counts once, so the third occurrence is the last line"
        );
        let line = names.line(Axis::Column, Side::Start, "a", Some(-3));
        assert_eq!(
            line,
            Some(GridLineDecl::Line(-3)),
            "the third occurrence from the end resolves to the first line"
        );

        let style = declared(
            "display: grid; grid-template-columns: [full-start] repeat(auto-fill, 100px) [full-end]",
        );
        let names = style.layout.grid_names.expect("line names");
        assert_eq!(names.columns["full-start"], &[1]);
        assert_eq!(names.columns["full-end"], &[-1]);
    }

    /// A later grid declaration takes the earlier rule's names back: a
    /// nameless `grid-template-rows` clears the row names, `none` clears
    /// the areas, and the `grid-template`/`grid` shorthands reset the
    /// whole table.
    #[test]
    fn later_grid_declarations_reset_names() {
        let style = declared(
            "display: grid; grid-template-rows: [a] 1fr; grid-template-rows: 1fr",
        );
        assert_eq!(
            style.layout.grid_names.map(|names| names.rows.is_empty()),
            Some(true)
        );
        let style = declared(
            "display: grid; grid-template-areas: \"x y\"; grid-template-areas: none",
        );
        let names = style.layout.grid_names.expect("table");
        assert!(names.areas.is_empty() && names.implied_rows == 0);
        let style = declared(
            "display: grid; grid-template-areas: \"x y\"; grid-template: 1fr / 1fr",
        );
        let names = style.layout.grid_names.expect("table");
        assert!(names.areas.is_empty() && names.rows.is_empty() && names.columns.is_empty());
    }

    /// `grid-area: name` places on the area's whole region (all four
    /// sides, side-aware).
    #[test]
    fn grid_area_places_the_region() {
        let parent = declared("display: grid; grid-template-areas: \"head head\" \"side main\";");
        let grid = parent.layout.grid_names.as_ref();
        let mut child = declared("grid-area: main;");
        child.layout.resolve_names(grid);
        assert_eq!(
            child.layout.grid_row,
            [Some(GridLineDecl::Line(2)), Some(GridLineDecl::Line(3)),]
        );
        assert_eq!(
            child.layout.grid_column,
            [Some(GridLineDecl::Line(2)), Some(GridLineDecl::Line(3)),]
        );
        let placement = grid_placement(child.layout.grid_row);
        assert_eq!(placement, Some(GridPlacement::start_end(2, 3)));
    }

    fn declared(declarations: &str) -> ElementStyle {
        let sheet = owned_sheet(&format!("p {{ {declarations} }}"));
        HtmlStyles::from_sheet(&sheet).get(&element("p", None, &[]), Pseudo::default(), &[])
    }

    /// `declared` with a `TemplateContext` lookup: `context(name)` answers
    /// `var(--name)`.
    fn declared_with_context(declarations: &str, context: &[(&str, &str)]) -> ElementStyle {
        let sheet = owned_sheet(&format!("p {{ {declarations} }}"));
        let context = |name: &str| {
            context
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        };
        HtmlStyles::from_sheet(&sheet)
            .with_context(&context)
            .get(&element("p", None, &[]), Pseudo::default(), &[])
    }

    /// A `var()` graph can be exponential: a self-reference with two
    /// fallbacks recursed ~2³² times, and a doubling reference chain
    /// doubled the output per level (20 levels ≈ 1.5 s). Resolution is
    /// memoized per declaration with cycle detection and a budget, so
    /// both terminate fast; the declaration is dropped when unresolvable
    /// or over budget.
    #[test]
    fn var_substitution_terminates_on_pathological_sheets() {
        let start = std::time::Instant::now();
        // A self-reference with fallbacks (the depth-bounded recursion
        // never finished this).
        let _ = declared("p { --a: var(--a, 1px) var(--a, 1px); width: var(--a) }");
        // A doubling chain whose output would exceed the budget.
        let mut sheet = String::from("--v0: 1px; ");
        for i in 1..24 {
            sheet.push_str(&format!("--v{i}: var(--v{}) var(--v{}); ", i - 1, i - 1));
        }
        sheet.push_str("width: var(--v23)");
        let _ = declared(&sheet);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "the pathological sheets took {:?}",
            start.elapsed()
        );
    }

    /// The transitive var() closure runs in linear time (a worklist): a
    /// 4000-level chain used to re-scan every declaration per fixpoint
    /// round (quadratic, seconds at load).
    #[test]
    fn var_names_closure_is_linear() {
        let mut sheet = String::from("p { ");
        for i in 0..20000 {
            sheet.push_str(&format!("--v{i}: var(--v{}); ", i + 1));
        }
        sheet.push_str("width: var(--v0) }");
        let parsed = owned_sheet(&sheet);
        let start = std::time::Instant::now();
        let names = var_names(&parsed);
        assert!(
            start.elapsed() < std::time::Duration::from_millis(50),
            "the closure took {:?}",
            start.elapsed()
        );
        assert!(names.contains(&"--v19999".to_owned()));
    }

    /// Pathologically large track counts drop the declaration instead of
    /// overflowing the line arithmetic (an `i16` line number; debug
    /// builds panicked on the subtraction).
    #[test]
    fn pathological_track_counts_drop_the_declaration() {
        // A 40 000-way repeat: no panic, no columns.
        let style = declared("display: grid; grid-template-columns: repeat(40000, 1fr)");
        assert_eq!(style.layout.grid_template_columns, None);
        // With names (the registration path).
        let style = declared("display: grid; grid-template-columns: repeat(40000, [a] 1fr)");
        assert_eq!(style.layout.grid_template_columns, None);
        // A 40 000-row template with names.
        let style = declared(
            concat!(
                "display: grid; grid-template-columns: 1fr; grid-template-rows: ",
                "repeat(40000, auto)"
            ),
        );
        assert_eq!(style.layout.grid_template_rows, None);
        // A normal grid still resolves.
        let style = declared("display: grid; grid-template-columns: repeat(3, [a] 1fr)");
        assert!(
            style.layout.grid_template_columns.is_some(),
            "a normal repeat was dropped"
        );
        let names = style.layout.grid_names.expect("line names");
        assert_eq!(names.columns["a"], &[1, -4, 2, -3, 3, -2]);
    }

    /// A cycle is invalid at computed-value time, *including* the
    /// fallbacks on the cycle's `var()`s (CSS Variables 1 §2.3): the
    /// declaration drops instead of resolving through the fallback.
    #[test]
    fn var_cycle_fallbacks_dont_apply() {
        // A self-reference with a fallback: the fallback doesn't rescue it.
        let style = declared("--a: var(--a, 5px); width: var(--a)");
        assert_eq!(style.layout.width, None);
        // An indirect cycle: --a → --b → --a.
        let style = declared("--a: var(--b); --b: var(--a, 5px); width: var(--a)");
        assert_eq!(style.layout.width, None);
        // The cycle is local: an unrelated property still resolves.
        let style = declared("--a: var(--a, 5px); color: #00ff00; width: 10px");
        assert_eq!(style.color, Some(Color::srgb(0.0, 1.0, 0.0)));
        assert_eq!(style.layout.width, Some(Val::Px(10.0)));
    }

    #[test]
    fn var_chain_depth_is_capped() {
        // Deep: drops instead of overflowing.
        let mut sheet = String::from("--v0: 7px; ");
        for i in 1..1000 {
            sheet.push_str(&format!("--v{i}: var(--v{}); ", i - 1));
        }
        sheet.push_str("width: var(--v999)");
        assert_eq!(declared(&sheet).layout.width, None);

        // Within the cap: resolves through the chain.
        let mut sheet = String::from("--v0: 7px; ");
        for i in 1..20 {
            sheet.push_str(&format!("--v{i}: var(--v{}); ", i - 1));
        }
        sheet.push_str("width: var(--v19)");
        assert_eq!(declared(&sheet).layout.width, Some(Val::Px(7.0)));
    }

    /// `var()` resolves in properties lightningcss has no typed form for
    /// (`pointer-events`, `outline-offset`): they arrive as custom
    /// properties, whose raw tokens are substituted like any other value.
    #[test]
    fn var_resolves_in_unknown_properties() {
        let style = declared("--pe: none; pointer-events: var(--pe)");
        assert_eq!(style.pointer_events, Some(false));
        let style = declared("--offset: 2px; outline-offset: var(--offset)");
        assert_eq!(style.outline.offset, Some(2.0));
        let style = declared_with_context(
            "pointer-events: var(--pe)",
            &[("--pe", "auto")],
        );
        assert_eq!(style.pointer_events, Some(true));
    }

    /// `var_names` follows references through the sheet's own `--`
    /// declarations: `--a: var(--brand)` contributes `--brand` when `--a`
    /// is referenced (a template change to `--brand` must restyle), and
    /// an unreferenced declaration contributes nothing.
    #[test]
    fn var_names_are_transitive() {
        assert_eq!(
            var_names(&owned_sheet(
                "p { --a: var(--brand); color: var(--a) } div { --unused: var(--other) }"
            )),
            ["--a", "--brand"]
        );
        assert_eq!(var_names(&owned_sheet("p { color: var(--brand) }")), ["--brand"]);
    }

    /// `restore_from` undoes everything `apply_to` writes: a `fixed`
    /// position (the insets and the position type are the node's; the
    /// old code restored only `absolute`/`relative`) and the `auto`
    /// tracks implied by `grid-template-areas` (written when the
    /// template fields are undeclared).
    #[test]
    fn restore_from_undoes_fixed_and_implied_tracks() {
        let mut names = GridNames::default();
        names.implied_rows = 2;
        let layout = LayoutDecl {
            position: Some(CssPosition::Fixed),
            inset: [Some(Val::Px(10.0)), None, None, None],
            grid_names: Some(names),
            ..LayoutDecl::default()
        };
        let base = Node::default();
        let mut node = base.clone();
        layout.apply_to(&mut node);
        assert_eq!(node.position_type, PositionType::Absolute);
        assert_eq!(node.top, Val::Px(10.0));
        assert_eq!(node.grid_template_rows.len(), 2);
        layout.restore_from(&base, &mut node);
        assert_eq!(node.position_type, base.position_type);
        assert_eq!(node.top, base.top);
        assert_eq!(node.grid_template_rows, base.grid_template_rows);
    }

    /// An unsupported value drops the declaration: the earlier rule's
    /// value (and its calc) stand, instead of being erased.
    #[test]
    fn unsupported_values_dont_erase_earlier_values() {
        let get = |rules: &str, classes: &[&str]| {
            HtmlStyles::from_sheet(&owned_sheet(rules)).get(
                &element("p", None, classes),
                Pseudo::default(),
                &[],
            )
        };
        // An unsupported track invalidates the whole declaration.
        let style = get(
            "p { grid-template-columns: 100px 1fr } p.x { grid-template-columns: fit-content(2em) 1fr }",
            &["x"],
        );
        assert_eq!(
            style.layout.grid_template_columns.as_ref().map(|t| t.len()),
            Some(2)
        );
        let style = get("p { row-gap: 8px } p { row-gap: 5% }", &[]);
        assert_eq!(style.row_gap, Some(8.0));
        let style = get("p { width: calc(50% - 10px) } p { width: fit-content }", &[]);
        assert!(style.calc.iter().any(|(prop, _)| *prop == CalcProp::Width));
        // Control: without the invalid value the later declaration wins.
        let style = get("p { width: 10px } p { width: 20px }", &[]);
        assert_eq!(style.layout.width, Some(Val::Px(20.0)));
        // The remaining `= helper(value)` sites keep the earlier value
        // when the helper rejects it.
        let style = get(
            "p { grid-column: 2 / 3 } p { grid-column: span x / 4 }",
            &[],
        );
        assert_eq!(style.layout.grid_column[0], Some(GridLineDecl::Line(2)));
        let style = get("p { outline-width: 2px } p { outline-width: 1ex }", &[]);
        assert_eq!(style.outline.width, Some(2.0));
        let style = get("p { outline-offset: 2px } p { outline-offset: 1em }", &[]);
        assert_eq!(style.outline.offset, Some(2.0));
        let style = get("p { pointer-events: none } p { pointer-events: visiblePainted }", &[]);
        assert_eq!(style.pointer_events, Some(false));
        let style = get("p { visibility: hidden } p { visibility: collapse }", &[]);
        assert_eq!(style.layout.visibility, Some(false));
        let style = get(
            "p { border-radius: 8px } p { border-radius: calc(10% + 2px) }",
            &[],
        );
        assert_eq!(style.layout.border_radius[0], Some((Val::Px(8.0), Val::Px(8.0))));
    }

    /// `initial`/`inherit`/`unset`/`revert` return the field to its
    /// initial value instead of being ignored (the earlier rule's value
    /// used to show through). `inherit` resolves to the initial value
    /// too: the cascade keeps no parent computed style (documented).
    #[test]
    fn inherit_initial_unset_clear_the_field() {
        let get = |rules: &str| {
            HtmlStyles::from_sheet(&owned_sheet(rules))
                .get(&element("p", None, &[]), Pseudo::default(), &[])
        };
        assert_eq!(get("p { color: #ff0000 } p { color: initial }").color, None);
        assert_eq!(get("p { color: #ff0000 } p { color: inherit }").color, None);
        assert_eq!(get("p { color: #ff0000 } p { color: unset }").color, None);
        // A layout keyword: the field clears.
        assert_eq!(
            get("p { width: 200px } p { width: initial }").layout.width,
            None
        );
        // Control: without the keyword the later value still wins.
        assert_eq!(
            get("p { color: #ff0000 } p { color: #0000ff }").color,
            Some(Color::srgb(0.0, 0.0, 1.0))
        );
        // A keyword drops the property's calc() too (the field and its
        // calc reset together; the calc used to survive the keyword and
        // resolve again at layout).
        let style = get("p { width: calc(50% + 10px) } p { width: initial }");
        assert!(style.layout.width.is_none());
        assert!(style.calc.iter().all(|(p, _)| p != &CalcProp::Width));
        let style = get("p { margin: calc(1em + 2px) } p { margin: unset }");
        assert!(style.calc.iter().all(|(p, _)| !matches!(
            p,
            CalcProp::MarginTop
                | CalcProp::MarginRight
                | CalcProp::MarginBottom
                | CalcProp::MarginLeft
        )));

        // A per-corner keyword resets its corner only.
        let style = get(concat!(
            "p { border-radius: 8px } p { border-top-left-radius: initial }"
        ));
        assert_eq!(style.layout.border_radius[0], None);
        assert_eq!(style.layout.border_radius[1], Some((Val::Px(8.0), Val::Px(8.0))));
        assert_eq!(style.layout.border_radius[3], Some((Val::Px(8.0), Val::Px(8.0))));
    }

    /// `*:hover` (lightningcss prints it back without the `*`) and a bare
    /// `:focus-visible` match any element in that state — the focus-ring
    /// rule's shape.
    #[test]
    fn pseudo_only_compounds_match_any_element() {
        let style = |pseudo| {
            let sheet = owned_sheet(":focus-visible { color: #ff0000 } *:hover { color: #00ff00 }");
            HtmlStyles::from_sheet(&sheet).get(&element("p", None, &[]), pseudo, &[])
        };
        assert_eq!(
            style(Pseudo {
                focus_visible: true,
                ..default()
            })
            .color,
            Some(Color::srgb(1.0, 0.0, 0.0))
        );
        assert_eq!(
            style(Pseudo {
                hover: true,
                ..default()
            })
            .color,
            Some(Color::srgb(0.0, 1.0, 0.0))
        );
        assert_eq!(style(Pseudo::default()).color, None);
    }

    /// `visibility`: `hidden`/`visible` map to the subtree-hide flag;
    /// `collapse` (table-specific) is skipped, left undeclared.
    #[test]
    fn visibility_maps_to_subtree_hide() {
        let layout = |css: &str| declared(css).layout;
        assert_eq!(layout("visibility: hidden").visibility, Some(false));
        assert_eq!(layout("visibility: visible").visibility, Some(true));
        assert_eq!(layout("visibility: collapse").visibility, None);
        assert_eq!(layout("width: 5px").visibility, None);
    }

    /// The `transition` shorthand parses to the animatable properties'
    /// entries: durations/delays in seconds, easings kept as curves; other
    /// properties (and `all`) are skipped; a repeated property keeps the
    /// last declaration; the longhands are unsupported.
    #[test]
    fn transition_shorthand_parses_per_property() {
        let style = declared(
            "transition: background-color 0.2s ease-in, color 0.1s linear 0.05s",
        );
        assert_eq!(style.transitions.len(), 2);
        assert_eq!(
            style.transitions[0].property,
            TransitionProp::BackgroundColor
        );
        assert!((style.transitions[0].duration - 0.2).abs() < 1e-5);
        assert_eq!(style.transitions[0].delay, 0.0);
        assert_eq!(
            style.transitions[0].easing,
            EasingFunction::EaseIn
        );
        assert_eq!(style.transitions[1].property, TransitionProp::TextColor);
        assert!((style.transitions[1].duration - 0.1).abs() < 1e-5);
        assert!((style.transitions[1].delay - 0.05).abs() < 1e-5);
        assert_eq!(style.transitions[1].easing, EasingFunction::Linear);

        // Unanimatable properties and `all` are dropped; border-color (all
        // four sides at once) is animatable.
        let style = declared("transition: width 1s, all 1s, border-color 2s");
        assert_eq!(style.transitions.len(), 1);
        assert_eq!(style.transitions[0].property, TransitionProp::BorderColor);
        assert!((style.transitions[0].duration - 2.0).abs() < 1e-5);

        // A repeated property keeps the last declaration.
        let style = declared("transition: color 1s, color 3s");
        assert_eq!(style.transitions.len(), 1);
        assert!((style.transitions[0].duration - 3.0).abs() < 1e-5);

        // Longhands are unsupported (skipped, logged).
        let style = declared("transition-property: color; transition-duration: 1s");
        assert_eq!(style.transitions, Vec::new());

        // No transition: nothing declared.
        let style = declared("color: #ff0000");
        assert_eq!(style.transitions, Vec::new());
    }

    /// CSS variables: `var(--x)` resolves from the sheet's `--x`
    /// declarations, then the context (which wins), then the reference's
    /// fallback; an unresolvable reference without a fallback drops the
    /// declaration; recursion is bounded.
    #[test]
    fn css_variables_resolve_from_sheet_context_and_fallback() {
        let style = declared("--accent: #f0b429; color: var(--accent)");
        assert_eq!(
            style.color,
            Some(Color::srgb(240.0 / 255.0, 180.0 / 255.0, 41.0 / 255.0))
        );

        // Context wins over the sheet, and works for names the sheet
        // doesn't declare.
        let style = declared_with_context(
            "--accent: red; color: var(--accent); width: var(--w)",
            &[("--accent", "blue"), ("--w", "160")],
        );
        assert_eq!(style.color, Some(Color::srgb(0.0, 0.0, 1.0)));
        assert_eq!(style.layout.width, Some(Val::Px(160.0)));

        // Fallback: used when neither the context nor the sheet has the
        // name; the fallback's own `var()`s resolve.
        let style = declared("color: var(--missing, #ffffff)");
        assert_eq!(style.color, Some(Color::srgb(1.0, 1.0, 1.0)));
        let style = declared("--c: #00ff00; color: var(--missing, var(--c))");
        assert_eq!(style.color, Some(Color::srgb(0.0, 1.0, 0.0)));

        // Unresolvable (no fallback): the declaration is dropped, the
        // element styles as if it weren't declared.
        let style = declared("color: var(--missing); width: 5px");
        assert_eq!(style.color, None);
        assert_eq!(style.layout.width, Some(Val::Px(5.0)));

        // A fallback does not rescue a resolution failure inside itself.
        let style = declared("color: var(--missing, var(--also-missing))");
        assert_eq!(style.color, None);

        // Self-referential variables drop the declaration instead of
        // recursing forever.
        let style = declared("--a: var(--b); --b: var(--a); color: var(--a)");
        assert_eq!(style.color, None);

        // A var() in a multi-part (unparsed) property resolves too.
        let style = declared_with_context("margin: var(--m)", &[("--m", "1px 2px")]);
        assert_eq!(
            style.layout.margin,
            [
                Some(Val::Px(1.0)),
                Some(Val::Px(2.0)),
                Some(Val::Px(1.0)),
                Some(Val::Px(2.0))
            ]
        );

        // calc() around var(): substitution is textual (as in browsers —
        // a unitless number where a length is required is invalid), so a
        // same-unit sum collapses at style time.
        let style = declared_with_context(
            "width: calc(var(--w) + 40px)",
            &[("--w", "120px")],
        );
        assert_eq!(style.layout.width, Some(Val::Px(160.0)));
        let style = declared_with_context(
            "width: calc(var(--w) + 40px)",
            &[("--w", "120")],
        );
        assert_eq!(style.layout.width, None);
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

        assert_eq!(
            layout("overflow: hidden scroll").overflow,
            [Some(OverflowAxis::Hidden), Some(OverflowAxis::Scroll)]
        );
        assert_eq!(
            layout("overflow: clip").overflow,
            [Some(OverflowAxis::Clip), Some(OverflowAxis::Clip)]
        );
        assert_eq!(
            layout("overflow-y: auto").overflow,
            [None, Some(OverflowAxis::Scroll)]
        );
        assert_eq!(
            layout("overflow-x: visible").overflow,
            [Some(OverflowAxis::Visible), None]
        );

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
        assert_eq!(layout("width: 2em").width, Some(Val::Em(2.0)));

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
            "the tracks map; the names register on the table"
        );
        // Font-relative tracks map to taffy's `Em`/`Rem` sizing functions.
        assert_eq!(
            columns("1fr 2em"),
            Some(vec![
                GridTrack::fr(1.0),
                GridTrack::minmax(
                    MinTrackSizingFunction::Em(2.0),
                    MaxTrackSizingFunction::Em(2.0)
                )
            ])
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

    /// A `strong` type selector matches the element by tag: translation
    /// markup (fluent) and Markdown (`**x**` → `<strong>`) rely on it.
    #[test]
    fn strong_type_selector_matches() {
        let sheet = owned_sheet("strong { font-weight: bold }");
        let style = HtmlStyles::from_sheet(&sheet).get(
            &element("strong", None, &[]),
            Pseudo::default(),
            &[],
        );
        assert_eq!(style.bold, Some(true));
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
        // A url() behind a var(): the sheet's `--` declaration carries the
        // URL — it is loaded even though the frame declaration only
        // references the name. The custom-property URL is a separate
        // list: unresolvable ones (a data URI, a scheme) are skipped by
        // the loader instead of failing the sheet.
        let sheet = owned_sheet(
            r#"p { --frame: url("a.png"); border-image: var(--frame) 4 }"#,
        );
        assert_eq!(image_urls(&sheet), Vec::<String>::new());
        assert_eq!(custom_image_urls(&sheet), ["a.png"]);
    }

    /// `position` and the insets: shorthand and longhands, `static` kept as
    /// such (it ignores insets), `sticky` unsupported, `fixed` mapped to
    /// absolute insets (the `FixedNode` component is the build's).
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
        assert_eq!(layout("position: fixed").position, Some(CssPosition::Fixed));
        assert_eq!(layout("position: sticky").position, None);
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
        // `fixed` uses the same inset mapping (its `FixedNode` component
        // makes the viewport the containing block).
        let mut node = Node::default();
        layout("position: fixed; top: 5px; left: 6px").apply_to(&mut node);
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

    /// `border-radius` per corner as `[x, y]` pairs, `border-color` per
    /// side, `z-index` and `pointer-events`.
    #[test]
    fn radius_border_color_z_index_and_pointer_events() {
        assert_eq!(
            declared("border-radius: 1px 2px 3px 50%")
                .layout
                .border_radius,
            [
                Some((Val::Px(1.0), Val::Px(1.0))),
                Some((Val::Px(2.0), Val::Px(2.0))),
                Some((Val::Px(3.0), Val::Px(3.0))),
                Some((Val::Percent(50.0), Val::Percent(50.0)))
            ]
        );
        assert_eq!(
            declared("border-top-right-radius: 7px")
                .layout
                .border_radius[1],
            Some((Val::Px(7.0), Val::Px(7.0)))
        );
        assert_eq!(
            declared("border-radius: 4px / 8px").layout.border_radius[0],
            Some((Val::Px(4.0), Val::Px(8.0))),
            "elliptical"
        );
        let mut node = Node::default();
        declared("border-radius: 9px").layout.apply_to(&mut node);
        assert_eq!(
            node.border_radius,
            BorderRadius::all((Val::Px(9.0), Val::Px(9.0)))
        );

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

    /// Elliptical `border-radius`: the `h / v` shorthand and per-corner
    /// longhands keep both radii.
    #[test]
    fn elliptical_border_radius() {
        let layout = |css: &str| declared(css).layout;
        // One pair: all four corners share it.
        assert_eq!(
            layout("border-radius: 10px / 20px").border_radius,
            [
                Some((Val::Px(10.0), Val::Px(20.0))),
                Some((Val::Px(10.0), Val::Px(20.0))),
                Some((Val::Px(10.0), Val::Px(20.0))),
                Some((Val::Px(10.0), Val::Px(20.0)))
            ]
        );
        // Per-corner pairs.
        assert_eq!(
            layout("border-radius: 1px 2px / 3px 4px").border_radius,
            [
                Some((Val::Px(1.0), Val::Px(3.0))),
                Some((Val::Px(2.0), Val::Px(4.0))),
                Some((Val::Px(1.0), Val::Px(3.0))),
                Some((Val::Px(2.0), Val::Px(4.0)))
            ]
        );
        // Longhand with mixed units; `%` resolves per-axis against the
        // node's dimensions.
        assert_eq!(
            layout("border-top-left-radius: 25% 10px").border_radius[0],
            Some((Val::Percent(25.0), Val::Px(10.0)))
        );
        // Relative lengths map to Bevy's font-relative `Val`s (resolved
        // against `EmSize`/`RemSize` at layout time).
        assert_eq!(
            layout("border-radius: 1em").border_radius,
            [
                Some((Val::Em(1.0), Val::Em(1.0))),
                Some((Val::Em(1.0), Val::Em(1.0))),
                Some((Val::Em(1.0), Val::Em(1.0))),
                Some((Val::Em(1.0), Val::Em(1.0)))
            ]
        );
    }

    /// Font-relative lengths map to `Val::Em`/`Val::Rem` (Bevy resolves
    /// them against the node's `EmSize` and the `RemSize` resource).
    #[test]
    fn font_relative_lengths_map_to_em_rem() {
        let layout = |css: &str| declared(css).layout;
        assert_eq!(
            layout("width: 1.5em; margin: 2rem; inset: 1em").inset[0],
            Some(Val::Em(1.0))
        );
        assert_eq!(layout("width: 1.5em").width, Some(Val::Em(1.5)));
        assert_eq!(layout("margin: 2rem").margin, [Some(Val::Rem(2.0)); 4]);
        // `ex`/`ch` have no Bevy counterpart.
        assert_eq!(layout("width: 3ch").width, None);
    }
}
