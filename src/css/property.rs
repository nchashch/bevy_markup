//! Declarations: one CSS property's value parsed into what the cascade
//! applies ([`Decl`]). Only the properties bevy_markup maps are parsed;
//! [`parse`] returns `None` for any other name and for an invalid value,
//! and the declaration is dropped.
//!
//! Each parser follows the property's CSS grammar. A value inside the
//! grammar that bevy_markup can't map (`width: fit-content`, a `%`
//! padding) is still valid: its `Decl` carries `None` (or
//! [`Length::Unsupported`]) and the cascade keeps the earlier value, as it
//! does for an invalid one, except where a shorthand applies its other
//! parts.

use bevy::color::Color;
use bevy::prelude::{
    AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap,
    JustifyContent, JustifyItems, JustifySelf, Val,
};
use bevy::ui::VisualBox;
use cssparser::{Parser, ParserInput};

use super::color::{CssColor, parse_color};
use super::grid::{
    self, Grid, GridAutoFlow, GridLine, GridTemplate, GridTemplateAreas, TrackSize, TrackSizing,
};
use super::value::{
    Lp, Numeric, Unit, parse_ident, parse_image, parse_integer, parse_lp, parse_non_negative,
    parse_number, parse_numeric, parse_time,
};
use super::{CssError, Parsed, invalid};
use crate::cascade::{
    BorderImageDecl, CalcExpr, CalcProp, CssPosition, DecorationDecl, FontSizeSpec,
    LetterSpacingSpec, LineHeightSpec, ListStyleSpec, OverflowAxisDecl, SliceValue, TextAlignSpec,
    TextTransformSpec, TransitionDecl, TransitionProp, WhiteSpaceSpec,
};
use crate::fonts::{FamilyRef, GenericFamily};

/// A length-valued property's value as the cascade's `set_calc` takes it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Length {
    Val(Val),
    Calc(CalcExpr),
    /// Valid CSS bevy_markup can't map (`fit-content`, `ex`): the earlier
    /// value stands.
    Unsupported,
}

impl Length {
    fn from_lp(lp: &Lp) -> Length {
        match lp {
            Lp::Calc(Some(expr)) => Length::Calc(expr.clone()),
            Lp::Calc(None) => Length::Unsupported,
            lp => lp.val().map_or(Length::Unsupported, Length::Val),
        }
    }
}

/// A transition's easing curve.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
    Steps { count: i32, position: StepPosition },
}

/// `steps()`'s jump position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StepPosition {
    /// `jump-start`/`start`.
    Start,
    /// `jump-end`/`end`.
    End,
    JumpNone,
    JumpBoth,
}

/// One declaration's parsed value, by what it sets. The cascade's
/// `apply` writes it into an `ElementStyle`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Decl {
    Opacity(f32),
    /// `None`: `currentColor` or a color without a fixed value.
    Color(Option<Color>),
    BackgroundColor(Option<Color>),
    /// The first layer's URL.
    BackgroundImage(Option<String>),
    /// The first layer's color (`None`: no fixed color) and URL.
    Background {
        color: Option<Color>,
        image: Option<String>,
    },
    BackgroundRepeat((bool, bool)),
    /// `None`: `text`.
    BackgroundClip(Option<VisualBox>),
    TextDecoration(DecorationDecl),
    TextDecorationLine {
        underline: bool,
        line_through: bool,
    },
    TextDecorationColor(Option<Color>),
    TextAlign(Option<TextAlignSpec>),
    LineHeight(Option<LineHeightSpec>),
    LetterSpacing(Option<LetterSpacingSpec>),
    TextTransform(Option<TextTransformSpec>),
    WhiteSpace(Option<WhiteSpaceSpec>),
    ListStyle(Option<ListStyleSpec>),
    FontFamily(Vec<FamilyRef>),
    FontSize(Option<FontSizeSpec>),
    Bold(bool),
    Italic(bool),
    BorderImage(BorderImageDecl),
    BorderImageSource(Option<String>),
    BorderImageSlice(([SliceValue; 4], bool)),
    BorderImageRepeat(bool),
    /// `border-image-width`/`-outset`: valid, not drawn.
    BorderImageIgnored,
    /// Per side, `None` where unsupported (then the declaration drops).
    BorderWidth([Option<f32>; 4]),
    BorderSideWidth(usize, Option<f32>),
    Padding([Option<f32>; 4]),
    PaddingSide(usize, Option<f32>),
    RowGap(Option<f32>),
    ColumnGap(Option<f32>),
    Gap(Option<f32>, Option<f32>),
    Display(Option<Display>),
    Visibility(Option<bool>),
    Transition(Vec<TransitionDecl>),
    /// `transition-property` & co.: unsupported (declare the shorthand).
    TransitionLonghand,
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    FlexFlow(FlexDirection, FlexWrap),
    FlexGrow(f32),
    FlexShrink(f32),
    Flex {
        grow: f32,
        shrink: f32,
        basis: Length,
    },
    Overflow([OverflowAxisDecl; 2]),
    OverflowX(OverflowAxisDecl),
    OverflowY(OverflowAxisDecl),
    /// Sizes, margins, insets and `flex-basis`.
    Lengths(Vec<(CalcProp, Length)>),
    JustifyContent(Option<JustifyContent>),
    AlignItems(Option<AlignItems>),
    AlignContent(Option<AlignContent>),
    AlignSelf(Option<AlignSelf>),
    JustifyItems(Option<JustifyItems>),
    JustifySelf(Option<JustifySelf>),
    BoxSizing(BoxSizing),
    /// `None`: `sticky` (clears an earlier value, as before).
    Position(Option<CssPosition>),
    ZIndex(Option<i32>),
    BorderColor([Option<Color>; 4]),
    BorderSideColor(usize, Option<Color>),
    BorderRadius([Option<(Val, Val)>; 4]),
    CornerRadius(usize, Option<(Val, Val)>),
    Outline {
        visible: bool,
        width: Option<f32>,
        color: Option<Color>,
    },
    OutlineStyle(bool),
    OutlineWidth(Option<f32>),
    OutlineColor(Option<Color>),
    OutlineOffset(Option<f32>),
    PointerEvents(Option<bool>),
    GridTemplateRows(TrackSizing),
    GridTemplateColumns(TrackSizing),
    GridTemplate(GridTemplate),
    GridAutoRows(Vec<TrackSize>),
    GridAutoColumns(Vec<TrackSize>),
    GridAutoFlow(GridAutoFlow),
    Grid(Grid),
    GridTemplateAreas(GridTemplateAreas),
    GridRowStart(GridLine),
    GridRowEnd(GridLine),
    GridColumnStart(GridLine),
    GridColumnEnd(GridLine),
    GridRow([GridLine; 2]),
    GridColumn([GridLine; 2]),
    /// `[row-start, row-end, column-start, column-end]`.
    GridArea([GridLine; 4]),
}

/// Properties whose vendor-prefixed spellings mean the same.
const PREFIXABLE: &[&str] = &[
    "flex",
    "flex-basis",
    "flex-direction",
    "flex-flow",
    "flex-grow",
    "flex-shrink",
    "flex-wrap",
    "align-content",
    "align-items",
    "align-self",
    "justify-content",
    "box-sizing",
    "transition",
    "transition-property",
    "transition-duration",
    "transition-delay",
    "transition-timing-function",
    "border-radius",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "border-image",
    "background-clip",
    "text-decoration",
    "text-decoration-line",
    "text-decoration-color",
    "position",
];

/// The property's unprefixed name, if bevy_markup reads its prefixed
/// spellings.
fn unprefixed(name: &str) -> &str {
    for prefix in ["-webkit-", "-moz-", "-ms-", "-o-"] {
        if let Some(base) = name.strip_prefix(prefix)
            && PREFIXABLE.contains(&base)
        {
            return base;
        }
    }
    name
}

/// `text` as the value of property `name` (lowercase), or `None`: an
/// unmapped property or an invalid value.
pub(crate) fn parse(name: &str, text: &str) -> Option<Decl> {
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    parser
        .parse_entirely(|input| value(unprefixed(name), input))
        .ok()
        .flatten()
}

/// [`parse`] on a parser already positioned at the value (the sheet's
/// declaration parser reads each value once, in place). `Ok(None)`: a
/// property bevy_markup doesn't map. The caller checks exhaustion.
pub(crate) fn parse_in<'i>(name: &str, input: &mut Parser<'i, '_>) -> Parsed<'i, Option<Decl>> {
    value(unprefixed(name), input)
}

/// `Ok(None)`: a property bevy_markup doesn't map.
fn value<'i>(name: &str, input: &mut Parser<'i, '_>) -> Parsed<'i, Option<Decl>> {
    use CalcProp::*;
    let decl = match name {
        "opacity" => Decl::Opacity(match parse_numeric(input)? {
            Numeric::Number(n) => n,
            Numeric::Percent(p) => p / 100.0,
            _ => return invalid(input),
        }),
        "color" => Decl::Color(parse_color(input)?.fixed()),
        "background-color" => Decl::BackgroundColor(parse_color(input)?.fixed()),
        "background-image" => {
            let images = input.parse_comma_separated(parse_image)?;
            Decl::BackgroundImage(images.into_iter().next().flatten())
        }
        "background" => background(input)?,
        "background-repeat" => {
            let repeats = input.parse_comma_separated(background_repeat)?;
            Decl::BackgroundRepeat(repeats[0])
        }
        "background-clip" => {
            let clips = input.parse_comma_separated(background_clip)?;
            Decl::BackgroundClip(clips[0])
        }
        "text-decoration" => text_decoration(input)?,
        "text-decoration-line" => {
            let (underline, line_through) = decoration_line(input)?;
            Decl::TextDecorationLine {
                underline,
                line_through,
            }
        }
        "text-decoration-color" => Decl::TextDecorationColor(parse_color(input)?.fixed()),
        "text-align" => Decl::TextAlign(text_align(input)?),
        "line-height" => Decl::LineHeight(line_height(input)?),
        "letter-spacing" => Decl::LetterSpacing(letter_spacing(input)?),
        "text-transform" => Decl::TextTransform(text_transform(input)?),
        "white-space" => Decl::WhiteSpace(white_space(input)?),
        "list-style-type" => Decl::ListStyle(list_style_type(input)?),
        "list-style" => Decl::ListStyle(list_style(input)?),
        "font-family" => Decl::FontFamily(font_family(input)?),
        "font-size" => Decl::FontSize(font_size(input)?),
        "font-weight" => Decl::Bold(font_weight(input)?),
        "font-style" => Decl::Italic(font_style(input)?),
        "border-image" => Decl::BorderImage(border_image(input)?),
        "border-image-source" => Decl::BorderImageSource(parse_image(input)?),
        "border-image-slice" => Decl::BorderImageSlice(border_image_slice(input)?),
        "border-image-repeat" => Decl::BorderImageRepeat(border_image_repeat(input)?),
        "border-image-width" => {
            sides(input, |i| {
                if i.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
                    return Ok(());
                }
                if i.try_parse(parse_non_negative).is_ok() {
                    return Ok(());
                }
                non_negative_lp(i).map(drop)
            })?;
            Decl::BorderImageIgnored
        }
        "border-image-outset" => {
            sides(input, |i| {
                if i.try_parse(parse_non_negative).is_ok() {
                    return Ok(());
                }
                non_negative_lp(i).map(drop)
            })?;
            Decl::BorderImageIgnored
        }
        "border-width" => Decl::BorderWidth(sides(input, line_width)?),
        "border-top-width" => Decl::BorderSideWidth(0, line_width(input)?),
        "border-right-width" => Decl::BorderSideWidth(1, line_width(input)?),
        "border-bottom-width" => Decl::BorderSideWidth(2, line_width(input)?),
        "border-left-width" => Decl::BorderSideWidth(3, line_width(input)?),
        "padding" => Decl::Padding(sides(input, padding_px)?),
        "padding-top" => Decl::PaddingSide(0, padding_px(input)?),
        "padding-right" => Decl::PaddingSide(1, padding_px(input)?),
        "padding-bottom" => Decl::PaddingSide(2, padding_px(input)?),
        "padding-left" => Decl::PaddingSide(3, padding_px(input)?),
        "row-gap" | "grid-row-gap" => Decl::RowGap(gap_px(input)?),
        "column-gap" | "grid-column-gap" => Decl::ColumnGap(gap_px(input)?),
        "gap" | "grid-gap" => {
            let row = gap_px(input)?;
            let column = match input.try_parse(gap_px) {
                Ok(column) => column,
                Err(_) => row,
            };
            Decl::Gap(row, column)
        }
        "display" => Decl::Display(display(input)?),
        "visibility" => {
            Decl::Visibility(match keyword(input, &["visible", "hidden", "collapse"])? {
                0 => Some(true),
                1 => Some(false),
                _ => None,
            })
        }
        "transition" => Decl::Transition(transition(input)?),
        "transition-property"
        | "transition-duration"
        | "transition-delay"
        | "transition-timing-function"
        | "transition-behavior" => {
            while input.next().is_ok() {}
            Decl::TransitionLonghand
        }
        "flex-direction" => Decl::FlexDirection(flex_direction(input)?),
        "flex-wrap" => Decl::FlexWrap(flex_wrap(input)?),
        "flex-flow" => {
            let mut direction = None;
            let mut wrap = None;
            for _ in 0..2 {
                if direction.is_none()
                    && let Ok(d) = input.try_parse(flex_direction)
                {
                    direction = Some(d);
                } else if wrap.is_none()
                    && let Ok(w) = input.try_parse(flex_wrap)
                {
                    wrap = Some(w);
                }
            }
            if direction.is_none() && wrap.is_none() {
                return invalid(input);
            }
            Decl::FlexFlow(
                direction.unwrap_or(FlexDirection::Row),
                wrap.unwrap_or(FlexWrap::NoWrap),
            )
        }
        "flex-grow" => Decl::FlexGrow(parse_non_negative(input)?),
        "flex-shrink" => Decl::FlexShrink(parse_non_negative(input)?),
        "flex-basis" => Decl::Lengths(vec![(FlexBasis, flex_basis(input)?)]),
        "flex" => flex(input)?,
        "overflow" => {
            let x = overflow_keyword(input)?;
            let y = input.try_parse(overflow_keyword).unwrap_or(x);
            Decl::Overflow([x, y])
        }
        "overflow-x" => Decl::OverflowX(overflow_keyword(input)?),
        "overflow-y" => Decl::OverflowY(overflow_keyword(input)?),
        "width" => Decl::Lengths(vec![(Width, size(input, false)?)]),
        "height" => Decl::Lengths(vec![(Height, size(input, false)?)]),
        "min-width" => Decl::Lengths(vec![(MinWidth, size(input, false)?)]),
        "min-height" => Decl::Lengths(vec![(MinHeight, size(input, false)?)]),
        "max-width" => Decl::Lengths(vec![(MaxWidth, size(input, true)?)]),
        "max-height" => Decl::Lengths(vec![(MaxHeight, size(input, true)?)]),
        "margin" => {
            let [t, r, b, l] = sides(input, auto_length)?;
            Decl::Lengths(vec![
                (MarginTop, t),
                (MarginRight, r),
                (MarginBottom, b),
                (MarginLeft, l),
            ])
        }
        "margin-top" => Decl::Lengths(vec![(MarginTop, auto_length(input)?)]),
        "margin-right" => Decl::Lengths(vec![(MarginRight, auto_length(input)?)]),
        "margin-bottom" => Decl::Lengths(vec![(MarginBottom, auto_length(input)?)]),
        "margin-left" => Decl::Lengths(vec![(MarginLeft, auto_length(input)?)]),
        "inset" => {
            let [t, r, b, l] = sides(input, auto_length)?;
            Decl::Lengths(vec![(Top, t), (Right, r), (Bottom, b), (Left, l)])
        }
        "top" => Decl::Lengths(vec![(Top, auto_length(input)?)]),
        "right" => Decl::Lengths(vec![(Right, auto_length(input)?)]),
        "bottom" => Decl::Lengths(vec![(Bottom, auto_length(input)?)]),
        "left" => Decl::Lengths(vec![(Left, auto_length(input)?)]),
        "justify-content" => Decl::JustifyContent(justify_content(input)?),
        "align-content" => Decl::AlignContent(align_content(input)?),
        "align-items" => Decl::AlignItems(align_items(input)?),
        "align-self" => Decl::AlignSelf(align_self(input)?),
        "justify-items" => Decl::JustifyItems(justify_items(input)?),
        "justify-self" => Decl::JustifySelf(justify_self(input)?),
        "box-sizing" => Decl::BoxSizing(match keyword(input, &["content-box", "border-box"])? {
            0 => BoxSizing::ContentBox,
            _ => BoxSizing::BorderBox,
        }),
        "position" => Decl::Position(
            match keyword(
                input,
                &[
                    "static",
                    "relative",
                    "absolute",
                    "fixed",
                    "sticky",
                    "-webkit-sticky",
                ],
            )? {
                0 => Some(CssPosition::Static),
                1 => Some(CssPosition::Relative),
                2 => Some(CssPosition::Absolute),
                3 => Some(CssPosition::Fixed),
                _ => {
                    bevy::log::debug!("html css: unsupported position sticky");
                    None
                }
            },
        ),
        "z-index" => Decl::ZIndex(
            if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
                None
            } else {
                Some(parse_integer(input)?)
            },
        ),
        "border-color" => Decl::BorderColor(sides(input, |i| Ok(parse_color(i)?.fixed()))?),
        "border-top-color" => Decl::BorderSideColor(0, parse_color(input)?.fixed()),
        "border-right-color" => Decl::BorderSideColor(1, parse_color(input)?.fixed()),
        "border-bottom-color" => Decl::BorderSideColor(2, parse_color(input)?.fixed()),
        "border-left-color" => Decl::BorderSideColor(3, parse_color(input)?.fixed()),
        "border-radius" => Decl::BorderRadius(border_radius(input)?),
        "border-top-left-radius" => Decl::CornerRadius(0, corner_radius(input)?),
        "border-top-right-radius" => Decl::CornerRadius(1, corner_radius(input)?),
        "border-bottom-right-radius" => Decl::CornerRadius(2, corner_radius(input)?),
        "border-bottom-left-radius" => Decl::CornerRadius(3, corner_radius(input)?),
        "outline" => outline(input)?,
        "outline-style" => Decl::OutlineStyle(outline_style(input)?),
        "outline-width" => Decl::OutlineWidth(line_width(input)?),
        "outline-color" => Decl::OutlineColor(parse_color(input)?.fixed()),
        "outline-offset" => Decl::OutlineOffset(match parse_lp(input)?.px() {
            Some(px) => Some(px),
            None => {
                bevy::log::debug!("html css: unsupported outline-offset value (absolute lengths only)");
                None
            }
        }),
        "pointer-events" => Decl::PointerEvents(match parse_ident(input)?.as_str() {
            "none" => Some(false),
            "auto" => Some(true),
            _ => {
                bevy::log::debug!(
                    "html css: unsupported pointer-events value (only `auto`/`none`)"
                );
                None
            }
        }),
        "grid-template-rows" => Decl::GridTemplateRows(grid::track_sizing(input)?),
        "grid-template-columns" => Decl::GridTemplateColumns(grid::track_sizing(input)?),
        "grid-template" => Decl::GridTemplate(grid::template(input)?),
        "grid-auto-rows" => Decl::GridAutoRows(grid::track_size_list(input)?),
        "grid-auto-columns" => Decl::GridAutoColumns(grid::track_size_list(input)?),
        "grid-auto-flow" => Decl::GridAutoFlow(grid::auto_flow(input)?),
        "grid" => Decl::Grid(grid::grid(input)?),
        "grid-template-areas" => Decl::GridTemplateAreas(grid::template_areas(input)?),
        "grid-row-start" => Decl::GridRowStart(grid::line(input)?),
        "grid-row-end" => Decl::GridRowEnd(grid::line(input)?),
        "grid-column-start" => Decl::GridColumnStart(grid::line(input)?),
        "grid-column-end" => Decl::GridColumnEnd(grid::line(input)?),
        "grid-row" => Decl::GridRow(grid::line_pair(input)?),
        "grid-column" => Decl::GridColumn(grid::line_pair(input)?),
        "grid-area" => Decl::GridArea(grid::area(input)?),
        _ => {
            while input.next().is_ok() {}
            return Ok(None);
        }
    };
    Ok(Some(decl))
}

/// One of `keywords`: its index.
fn keyword<'i>(input: &mut Parser<'i, '_>, keywords: &[&str]) -> Parsed<'i, usize> {
    super::value::parse_keyword(input, keywords)
}

/// One to four values expanded to `[top, right, bottom, left]`.
fn sides<'i, T: Clone>(
    input: &mut Parser<'i, '_>,
    mut one: impl FnMut(&mut Parser<'i, '_>) -> Parsed<'i, T>,
) -> Parsed<'i, [T; 4]> {
    let top = one(input)?;
    let Ok(right) = input.try_parse(&mut one) else {
        return Ok([top.clone(), top.clone(), top.clone(), top]);
    };
    let Ok(bottom) = input.try_parse(&mut one) else {
        return Ok([top.clone(), right.clone(), top, right]);
    };
    let Ok(left) = input.try_parse(&mut one) else {
        return Ok([top, right.clone(), bottom, right]);
    };
    Ok([top, right, bottom, left])
}

/// A length-percentage that may not be negative: negative literals are
/// invalid at parse time, while negative folded `calc()` values clamp to
/// `0` per CSS Values 4 (bug_0261).
fn non_negative_lp<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Lp> {
    let lp = parse_lp(input)?;
    let negative = match &lp {
        Lp::Dim(v, _) | Lp::Percent(v) => *v < 0.0,
        Lp::Calc(_) => false,
    };
    if negative {
        return invalid(input);
    }
    Ok(match lp {
        Lp::Calc(Some(CalcExpr::Val(val))) => {
            Lp::Calc(Some(CalcExpr::Val(non_negative_val(val))))
        }
        other => other,
    })
}

/// Clamps a single folded `Val` to `>= 0` (CSS Values 4 range clamping).
fn non_negative_val(val: Val) -> Val {
    match val {
        Val::Px(v) => Val::Px(v.max(0.0)),
        Val::Percent(v) => Val::Percent(v.max(0.0)),
        Val::Em(v) => Val::Em(v.max(0.0)),
        Val::Rem(v) => Val::Rem(v.max(0.0)),
        Val::Vw(v) => Val::Vw(v.max(0.0)),
        Val::Vh(v) => Val::Vh(v.max(0.0)),
        Val::VMin(v) => Val::VMin(v.max(0.0)),
        Val::VMax(v) => Val::VMax(v.max(0.0)),
        Val::Auto => Val::Auto,
    }
}

/// `width`/`height`/`min-*` (`max`: `max-*`, where `none` replaces `auto`).
fn size<'i>(input: &mut Parser<'i, '_>, max: bool) -> Parsed<'i, Length> {
    let state = input.state();
    if let Ok(ident) = parse_ident(input) {
        return match ident.as_str() {
            "auto" if !max => Ok(Length::Val(Val::Auto)),
            "none" if max => Ok(Length::Val(Val::Auto)),
            "min-content"
            | "max-content"
            | "fit-content"
            | "stretch"
            | "-webkit-fill-available"
            | "-moz-available"
            | "-webkit-min-content"
            | "-webkit-max-content"
            | "-moz-min-content"
            | "-moz-max-content"
            | "-webkit-fit-content"
            | "-moz-fit-content"
            | "contain" => {
                bevy::log::debug!("html css: unsupported intrinsic size keyword");
                Ok(Length::Unsupported)
            }
            _ => invalid(input),
        };
    }
    input.reset(&state);
    if let Ok(name) = input.try_parse(|i| i.expect_function().map(|n| n.to_ascii_lowercase()))
        && matches!(
            name.as_str(),
            "fit-content" | "-webkit-fit-content" | "-moz-fit-content"
        )
    {
        input.parse_nested_block(non_negative_lp)?;
        bevy::log::debug!("html css: unsupported intrinsic size keyword");
        return Ok(Length::Unsupported);
    }
    input.reset(&state);
    Ok(Length::from_lp(&non_negative_lp(input)?))
}

/// `margin`/inset sides: `auto` or a length-percentage.
fn auto_length<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Length> {
    if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
        return Ok(Length::Val(Val::Auto));
    }
    Ok(Length::from_lp(&parse_lp(input)?))
}

/// `flex-basis`: `content` or a `width` value.
fn flex_basis<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Length> {
    if input
        .try_parse(|i| i.expect_ident_matching("content"))
        .is_ok()
    {
        bevy::log::debug!("html css: unsupported flex-basis: content");
        return Ok(Length::Unsupported);
    }
    size(input, false)
}

/// `flex`: `none`, `auto`, or `<grow> <shrink>? || <basis>`.
fn flex<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Decl> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(Decl::Flex {
            grow: 0.0,
            shrink: 0.0,
            basis: Length::Val(Val::Auto),
        });
    }
    if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
        return Ok(Decl::Flex {
            grow: 1.0,
            shrink: 1.0,
            basis: Length::Val(Val::Auto),
        });
    }
    let mut factors = None;
    let mut basis = None;
    for _ in 0..2 {
        if factors.is_none()
            && let Ok(grow) = input.try_parse(parse_non_negative)
        {
            let shrink = input.try_parse(parse_non_negative).unwrap_or(1.0);
            factors = Some((grow, shrink));
        } else if basis.is_none()
            && let Ok(b) = input.try_parse(flex_basis)
        {
            basis = Some(b);
        }
    }
    if factors.is_none() && basis.is_none() {
        return invalid(input);
    }
    let (grow, shrink) = factors.unwrap_or((1.0, 1.0));
    Ok(Decl::Flex {
        grow,
        shrink,
        basis: basis.unwrap_or(Length::Val(Val::Percent(0.0))),
    })
}

fn flex_direction<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, FlexDirection> {
    Ok(
        match keyword(input, &["row", "row-reverse", "column", "column-reverse"])? {
            0 => FlexDirection::Row,
            1 => FlexDirection::RowReverse,
            2 => FlexDirection::Column,
            _ => FlexDirection::ColumnReverse,
        },
    )
}

fn flex_wrap<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, FlexWrap> {
    Ok(match keyword(input, &["nowrap", "wrap", "wrap-reverse"])? {
        0 => FlexWrap::NoWrap,
        1 => FlexWrap::Wrap,
        _ => FlexWrap::WrapReverse,
    })
}

fn overflow_keyword<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, OverflowAxisDecl> {
    Ok(
        match keyword(input, &["visible", "hidden", "clip", "scroll", "auto"])? {
            0 => OverflowAxisDecl::Visible,
            1 => OverflowAxisDecl::Hidden,
            2 => OverflowAxisDecl::Clip,
            3 => OverflowAxisDecl::Scroll,
            _ => OverflowAxisDecl::Auto,
        },
    )
}

/// `padding` sides: absolute lengths only (`%` and font-relative units
/// are valid but unsupported).
fn padding_px<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<f32>> {
    let lp = non_negative_lp(input)?;
    let px = lp.px();
    if px.is_none() {
        bevy::log::debug!("html css: only absolute lengths are supported for padding");
    }
    Ok(px)
}

/// A gap: `normal` (no override) or a length-percentage; absolute lengths
/// only.
fn gap_px<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<f32>> {
    if input
        .try_parse(|i| i.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(None);
    }
    let lp = non_negative_lp(input)?;
    let px = lp.px();
    if px.is_none() {
        bevy::log::debug!("html css: only absolute lengths are supported for gap");
    }
    Ok(px)
}

/// A border or outline width: `thin`/`medium`/`thick` or an absolute
/// length (others are valid, unsupported).
fn line_width<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<f32>> {
    if let Ok(index) = input.try_parse(|i| keyword(i, &["thin", "medium", "thick"])) {
        return Ok(Some([1.0, 3.0, 5.0][index]));
    }
    let location = input.current_source_location();
    match non_negative_lp(input)? {
        Lp::Percent(_) => Err(location.new_custom_error(CssError::Invalid)),
        lp => Ok(lp.px()),
    }
}

const LINE_STYLES: &[&str] = &[
    "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
];

/// Whether an `outline-style` draws (`none`/`hidden` don't; `auto` and
/// every line style draw Bevy's solid outline).
fn outline_style<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, bool> {
    if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
        return Ok(true);
    }
    Ok(keyword(input, LINE_STYLES)? > 1)
}

/// `outline`: `<width> || <style> || <color>`.
fn outline<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Decl> {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for _ in 0..3 {
        if width.is_none()
            && let Ok(w) = input.try_parse(line_width)
        {
            width = Some(w);
        } else if style.is_none()
            && let Ok(s) = input.try_parse(outline_style)
        {
            style = Some(s);
        } else if color.is_none()
            && let Ok(c) = input.try_parse(parse_color)
        {
            color = Some(c);
        }
    }
    if width.is_none() && style.is_none() && color.is_none() {
        return invalid(input);
    }
    Ok(Decl::Outline {
        visible: style.unwrap_or(false),
        width: width.unwrap_or(Some(3.0)),
        color: color.unwrap_or(CssColor::Current).fixed(),
    })
}

/// `border-radius`: one to four horizontal radii, optionally `/` and one
/// to four vertical ones.
fn border_radius<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, [Option<(Val, Val)>; 4]> {
    let corners = |input: &mut Parser<'i, '_>| -> Parsed<'i, [Lp; 4]> {
        let mut list = vec![non_negative_lp(input)?];
        while list.len() < 4
            && let Ok(lp) = input.try_parse(non_negative_lp)
        {
            list.push(lp);
        }
        // CSS's corner expansion: top-left, top-right, bottom-right,
        // bottom-left.
        Ok(match list.len() {
            1 => [
                list[0].clone(),
                list[0].clone(),
                list[0].clone(),
                list[0].clone(),
            ],
            2 => [
                list[0].clone(),
                list[1].clone(),
                list[0].clone(),
                list[1].clone(),
            ],
            3 => [
                list[0].clone(),
                list[1].clone(),
                list[2].clone(),
                list[1].clone(),
            ],
            _ => [
                list[0].clone(),
                list[1].clone(),
                list[2].clone(),
                list[3].clone(),
            ],
        })
    };
    let horizontal = corners(input)?;
    let vertical = if input.try_parse(|i| i.expect_delim('/')).is_ok() {
        corners(input)?
    } else {
        horizontal.clone()
    };
    Ok(std::array::from_fn(|i| {
        Some((horizontal[i].val()?, vertical[i].val()?))
    }))
}

/// One `border-*-radius` corner: one or two radii.
fn corner_radius<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<(Val, Val)>> {
    let horizontal = non_negative_lp(input)?;
    let vertical = input
        .try_parse(non_negative_lp)
        .unwrap_or_else(|_| horizontal.clone());
    Ok(horizontal.val().zip(vertical.val()))
}

/// `display`: `none`, `contents`, or `[outside] [inside] [list-item]` in
/// any order (the legacy single keywords included).
fn display<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<Display>> {
    let first = parse_ident(input)?;
    let pair = match first.as_str() {
        "none" => return Ok(Some(Display::None)),
        "contents"
        | "table-row-group"
        | "table-header-group"
        | "table-footer-group"
        | "table-row"
        | "table-cell"
        | "table-column-group"
        | "table-column"
        | "table-caption"
        | "ruby-base"
        | "ruby-text"
        | "ruby-base-container"
        | "ruby-text-container" => {
            bevy::log::debug!("html css: unsupported display value");
            return Ok(None);
        }
        "inline-block" => ("inline", "flow-root"),
        "inline-table" => ("inline", "table"),
        "inline-flex" | "-webkit-inline-flex" | "-ms-inline-flexbox" => ("inline", "flex"),
        "inline-grid" | "-ms-inline-grid" => ("inline", "grid"),
        "-webkit-flex" | "-ms-flexbox" => ("block", "flex"),
        "-webkit-box" | "-moz-box" | "-webkit-inline-box" | "-moz-inline-box" => {
            bevy::log::debug!("html css: unsupported display value");
            return Ok(None);
        }
        _ => {
            // The multi-keyword syntax: an outside, an inside and
            // `list-item`, each at most once.
            let mut outside = None;
            let mut inside = None;
            let mut list_item = false;
            let mut add = |word: &str| -> bool {
                match word {
                    "block" | "inline" | "run-in" if outside.is_none() => {
                        outside = Some(word.to_owned())
                    }
                    "flow" | "flow-root" | "table" | "flex" | "grid" | "ruby"
                        if inside.is_none() =>
                    {
                        inside = Some(word.to_owned())
                    }
                    "list-item" if !list_item => list_item = true,
                    _ => return false,
                }
                true
            };
            if !add(&first) {
                return invalid(input);
            }
            while let Ok(word) = input.try_parse(parse_ident) {
                if !add(&word) {
                    return invalid(input);
                }
            }
            let inside = inside.unwrap_or_else(|| "flow".into());
            let outside = outside.unwrap_or_else(|| {
                if inside == "ruby" {
                    "inline".into()
                } else {
                    "block".into()
                }
            });
            return Ok(display_pair(&outside, &inside));
        }
    };
    Ok(display_pair(pair.0, pair.1))
}

/// An outside/inside pair: flex and grid of either outside, block-level
/// flow; anything else is unsupported.
fn display_pair(outside: &str, inside: &str) -> Option<Display> {
    match (outside, inside) {
        (_, "flex") => Some(Display::Flex),
        (_, "grid") => Some(Display::Grid),
        ("block", "flow" | "flow-root") => Some(Display::Block),
        _ => {
            bevy::log::debug!("html css: unsupported display value");
            None
        }
    }
}

/// `justify-content`.
fn justify_content<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<JustifyContent>> {
    let word = parse_ident(input)?;
    if let Some(value) = match word.as_str() {
        "normal" => Some(JustifyContent::Default),
        "space-between" => Some(JustifyContent::SpaceBetween),
        "space-around" => Some(JustifyContent::SpaceAround),
        "space-evenly" => Some(JustifyContent::SpaceEvenly),
        "stretch" => Some(JustifyContent::Stretch),
        _ => None,
    } {
        return Ok(Some(value));
    }
    let position = with_overflow(input, &word, |w| {
        Some(match w {
            "center" => JustifyContent::Center,
            "start" => JustifyContent::Start,
            "end" => JustifyContent::End,
            "flex-start" => JustifyContent::FlexStart,
            "flex-end" => JustifyContent::FlexEnd,
            _ => return None,
        })
    })?;
    Ok(position)
}

/// An `<overflow-position>?` (`safe`/`unsafe`) before a position keyword:
/// `word` is the first keyword read. `None` when the position is valid
/// but unmapped (`left`/`right`).
fn with_overflow<'i, T>(
    input: &mut Parser<'i, '_>,
    word: &str,
    position: impl Fn(&str) -> Option<T>,
) -> Parsed<'i, Option<T>> {
    let word = if word == "safe" || word == "unsafe" {
        parse_ident(input)?
    } else {
        word.to_owned()
    };
    if let Some(value) = position(&word) {
        return Ok(Some(value));
    }
    if word == "left" || word == "right" {
        bevy::log::debug!("html css: unsupported alignment left/right");
        return Ok(None);
    }
    invalid(input)
}

/// `first baseline` / `last baseline` / `baseline` after `word`: `Some(true)`
/// for first, `Some(false)` for last, `None` when `word` doesn't start one.
fn baseline<'i>(input: &mut Parser<'i, '_>, word: &str) -> Parsed<'i, Option<bool>> {
    Ok(match word {
        "baseline" => Some(true),
        "first" => {
            input.expect_ident_matching("baseline")?;
            Some(true)
        }
        "last" => {
            input.expect_ident_matching("baseline")?;
            Some(false)
        }
        _ => None,
    })
}

fn align_content<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<AlignContent>> {
    let word = parse_ident(input)?;
    if baseline(input, &word)?.is_some() {
        bevy::log::debug!("html css: unsupported align-content baseline");
        return Ok(None);
    }
    if let Some(value) = match word.as_str() {
        "normal" => Some(AlignContent::Default),
        "space-between" => Some(AlignContent::SpaceBetween),
        "space-around" => Some(AlignContent::SpaceAround),
        "space-evenly" => Some(AlignContent::SpaceEvenly),
        "stretch" => Some(AlignContent::Stretch),
        _ => None,
    } {
        return Ok(Some(value));
    }
    with_overflow(input, &word, |w| {
        Some(match w {
            "center" => AlignContent::Center,
            "start" => AlignContent::Start,
            "end" => AlignContent::End,
            "flex-start" => AlignContent::FlexStart,
            "flex-end" => AlignContent::FlexEnd,
            _ => return None,
        })
    })
    .and_then(|value| {
        if matches!(word.as_str(), "left" | "right") {
            invalid(input)
        } else {
            Ok(value)
        }
    })
}

/// A `<self-position>` keyword's role.
enum SelfPos {
    Center,
    Start,
    End,
    FlexStart,
    FlexEnd,
    SelfStart,
    SelfEnd,
}

fn self_position(word: &str) -> Option<SelfPos> {
    Some(match word {
        "center" => SelfPos::Center,
        "start" => SelfPos::Start,
        "end" => SelfPos::End,
        "flex-start" => SelfPos::FlexStart,
        "flex-end" => SelfPos::FlexEnd,
        "self-start" => SelfPos::SelfStart,
        "self-end" => SelfPos::SelfEnd,
        _ => return None,
    })
}

fn align_items<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<AlignItems>> {
    let word = parse_ident(input)?;
    match word.as_str() {
        "normal" => return Ok(Some(AlignItems::Default)),
        "stretch" => return Ok(Some(AlignItems::Stretch)),
        _ => {}
    }
    if let Some(first) = baseline(input, &word)? {
        if !first {
            bevy::log::debug!("html css: unsupported last baseline");
        }
        return Ok(first.then_some(AlignItems::Baseline));
    }
    let value = with_overflow(input, &word, |w| {
        Some(match self_position(w)? {
            SelfPos::Center => AlignItems::Center,
            SelfPos::Start | SelfPos::SelfStart => AlignItems::Start,
            SelfPos::End | SelfPos::SelfEnd => AlignItems::End,
            SelfPos::FlexStart => AlignItems::FlexStart,
            SelfPos::FlexEnd => AlignItems::FlexEnd,
        })
    })?;
    if value.is_none() {
        return invalid(input);
    }
    Ok(value)
}

fn align_self<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<AlignSelf>> {
    let word = parse_ident(input)?;
    match word.as_str() {
        "auto" => return Ok(Some(AlignSelf::Auto)),
        // `normal` behaves as `stretch` for flex items.
        "normal" | "stretch" => return Ok(Some(AlignSelf::Stretch)),
        _ => {}
    }
    if let Some(first) = baseline(input, &word)? {
        if !first {
            bevy::log::debug!("html css: unsupported last baseline");
        }
        return Ok(first.then_some(AlignSelf::Baseline));
    }
    let value = with_overflow(input, &word, |w| {
        Some(match self_position(w)? {
            SelfPos::Center => AlignSelf::Center,
            SelfPos::Start | SelfPos::SelfStart => AlignSelf::Start,
            SelfPos::End | SelfPos::SelfEnd => AlignSelf::End,
            SelfPos::FlexStart => AlignSelf::FlexStart,
            SelfPos::FlexEnd => AlignSelf::FlexEnd,
        })
    })?;
    if value.is_none() {
        return invalid(input);
    }
    Ok(value)
}

fn justify_items<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<JustifyItems>> {
    let word = parse_ident(input)?;
    match word.as_str() {
        "normal" => return Ok(Some(JustifyItems::Default)),
        "stretch" => return Ok(Some(JustifyItems::Stretch)),
        "legacy" => {
            let _ = input.try_parse(|i| keyword(i, &["left", "right", "center"]));
            bevy::log::debug!("html css: unsupported justify-items legacy");
            return Ok(None);
        }
        "left" | "right" | "center"
            if input
                .try_parse(|i| i.expect_ident_matching("legacy"))
                .is_ok() =>
        {
            bevy::log::debug!("html css: unsupported justify-items legacy");
            return Ok(None);
        }
        _ => {}
    }
    if let Some(first) = baseline(input, &word)? {
        if !first {
            bevy::log::debug!("html css: unsupported justify-items last baseline");
        }
        return Ok(first.then_some(JustifyItems::Baseline));
    }
    with_overflow(input, &word, |w| {
        Some(match self_position(w)? {
            SelfPos::Center => JustifyItems::Center,
            SelfPos::Start | SelfPos::SelfStart | SelfPos::FlexStart => JustifyItems::Start,
            SelfPos::End | SelfPos::SelfEnd | SelfPos::FlexEnd => JustifyItems::End,
        })
    })
}

fn justify_self<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<JustifySelf>> {
    let word = parse_ident(input)?;
    match word.as_str() {
        "auto" => return Ok(Some(JustifySelf::Auto)),
        // `normal` behaves as `stretch` for grid items.
        "normal" | "stretch" => return Ok(Some(JustifySelf::Stretch)),
        _ => {}
    }
    if let Some(first) = baseline(input, &word)? {
        if !first {
            bevy::log::debug!("html css: unsupported justify-self last baseline");
        }
        return Ok(first.then_some(JustifySelf::Baseline));
    }
    with_overflow(input, &word, |w| {
        Some(match self_position(w)? {
            SelfPos::Center => JustifySelf::Center,
            SelfPos::Start | SelfPos::SelfStart | SelfPos::FlexStart => JustifySelf::Start,
            SelfPos::End | SelfPos::SelfEnd | SelfPos::FlexEnd => JustifySelf::End,
        })
    })
}

/// `text-align`: `start`/`end` map to `left`/`right` (bevy_markup's text is
/// left-to-right).
fn text_align<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<TextAlignSpec>> {
    Ok(
        match keyword(
            input,
            &[
                "start",
                "left",
                "end",
                "right",
                "center",
                "justify",
                "match-parent",
                "justify-all",
            ],
        )? {
            0 | 1 => Some(TextAlignSpec::Left),
            2 | 3 => Some(TextAlignSpec::Right),
            4 => Some(TextAlignSpec::Center),
            5 => Some(TextAlignSpec::Justify),
            _ => {
                bevy::log::debug!("html css: unsupported text-align value");
                None
            }
        },
    )
}

/// `line-height`: `normal`, a number, a length or a percentage.
fn line_height<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<LineHeightSpec>> {
    if input
        .try_parse(|i| i.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(Some(LineHeightSpec::Normal));
    }
    if let Ok(n) = input.try_parse(parse_non_negative) {
        return Ok(Some(LineHeightSpec::Relative(n)));
    }
    let lp = non_negative_lp(input)?;
    Ok(match lp {
        Lp::Calc(None) => None,
        Lp::Calc(Some(CalcExpr::Val(_))) | Lp::Dim(..) | Lp::Percent(_) => match lp.val() {
            Some(Val::Px(px)) => Some(LineHeightSpec::Px(px)),
            // `em` and `%` are multiples of the element's own font size,
            // the same computation a unitless number is.
            Some(Val::Em(em)) => Some(LineHeightSpec::Relative(em)),
            Some(Val::Percent(p)) => Some(LineHeightSpec::Relative(p / 100.0)),
            Some(Val::Rem(rem)) => Some(LineHeightSpec::Rem(rem)),
            _ => None,
        },
        Lp::Calc(Some(_)) => {
            bevy::log::debug!("html css: unsupported line-height calc()");
            None
        }
    })
}

/// `letter-spacing`: `normal` or a length.
fn letter_spacing<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<LetterSpacingSpec>> {
    if input
        .try_parse(|i| i.expect_ident_matching("normal"))
        .is_ok()
    {
        return Ok(Some(LetterSpacingSpec::Normal));
    }
    let lp = parse_lp(input)?;
    Ok(match lp {
        Lp::Percent(_) => return invalid(input),
        Lp::Dim(v, Unit::Px | Unit::Abs) => Some(LetterSpacingSpec::Px(v)),
        Lp::Dim(v, Unit::Em) => Some(LetterSpacingSpec::Em(v)),
        Lp::Dim(v, Unit::Rem) => Some(LetterSpacingSpec::Rem(v)),
        Lp::Dim(..) => None,
        Lp::Calc(_) => {
            bevy::log::debug!("html css: unsupported letter-spacing calc()");
            None
        }
    })
}

/// `text-transform`: `none` or one case mapping; `full-width` and
/// `full-size-kana` are valid, unsupported.
fn text_transform<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<TextTransformSpec>> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(Some(TextTransformSpec::None));
    }
    let mut case = None;
    let mut other = false;
    let mut seen = [false; 2];
    while let Ok(word) = input.try_parse(parse_ident) {
        match word.as_str() {
            "uppercase" | "lowercase" | "capitalize" if case.is_none() => case = Some(word),
            "full-width" if !seen[0] => {
                seen[0] = true;
                other = true;
            }
            "full-size-kana" if !seen[1] => {
                seen[1] = true;
                other = true;
            }
            _ => return invalid(input),
        }
    }
    if case.is_none() && !other {
        return invalid(input);
    }
    if other {
        bevy::log::debug!("html css: unsupported text-transform value");
        return Ok(None);
    }
    Ok(match case.as_deref() {
        Some("uppercase") => Some(TextTransformSpec::Uppercase),
        Some("lowercase") => Some(TextTransformSpec::Lowercase),
        Some("capitalize") => Some(TextTransformSpec::Capitalize),
        _ => None,
    })
}

fn white_space<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<WhiteSpaceSpec>> {
    Ok(
        match keyword(
            input,
            &[
                "normal",
                "pre",
                "nowrap",
                "pre-wrap",
                "break-spaces",
                "pre-line",
            ],
        )? {
            0 => Some(WhiteSpaceSpec::Normal),
            1 => Some(WhiteSpaceSpec::Preserve),
            2 => Some(WhiteSpaceSpec::NoWrap),
            3 => Some(WhiteSpaceSpec::PreserveWrap),
            _ => {
                bevy::log::debug!("html css: unsupported white-space value");
                None
            }
        },
    )
}

/// `list-style-type`: `none`, a counter style or a string; the crate draws
/// `disc`, `circle`, `square` and `decimal`.
fn list_style_type<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<ListStyleSpec>> {
    if input.try_parse(|i| i.expect_string().map(drop)).is_ok() {
        bevy::log::debug!("html css: unsupported list-style-type counter style");
        return Ok(None);
    }
    if input
        .try_parse(|i| i.expect_function_matching("symbols"))
        .is_ok()
    {
        input.parse_nested_block(|args| {
            while args.next().is_ok() {}
            Ok(())
        })?;
        bevy::log::debug!("html css: unsupported list-style-type counter style");
        return Ok(None);
    }
    let word = parse_ident(input)?;
    Ok(counter_style(&word, input)?)
}

fn counter_style<'i>(word: &str, input: &Parser<'i, '_>) -> Parsed<'i, Option<ListStyleSpec>> {
    Ok(match word {
        "none" => Some(ListStyleSpec::None),
        "disc" => Some(ListStyleSpec::Disc),
        "circle" => Some(ListStyleSpec::Circle),
        "square" => Some(ListStyleSpec::Square),
        "decimal" | "decimal-leading-zero" => Some(ListStyleSpec::Decimal),
        "initial" | "inherit" | "unset" | "revert" | "default" => return invalid(input),
        _ => {
            bevy::log::debug!("html css: unsupported list-style-type counter style");
            None
        }
    })
}

/// `list-style`: `<position> || <image> || <type>`; only the type applies
/// (`disc` when omitted, `none` when only `none` is given).
fn list_style<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<ListStyleSpec>> {
    let mut position = false;
    let mut image = false;
    let mut kind: Option<Option<ListStyleSpec>> = None;
    let mut nones = 0;
    let mut any = false;
    for _ in 0..3 {
        if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
            nones += 1;
            any = true;
            continue;
        }
        if !position
            && input
                .try_parse(|i| keyword(i, &["inside", "outside"]))
                .is_ok()
        {
            position = true;
        } else if !image && input.try_parse(|i| i.expect_url().map(drop)).is_ok() {
            image = true;
        } else if kind.is_none()
            && let Ok(value) = input.try_parse(list_style_type)
        {
            kind = Some(value);
        } else {
            break;
        }
        any = true;
    }
    if !any || nones > 2 {
        return invalid(input);
    }
    Ok(match kind {
        Some(kind) => kind,
        None if nones > 0 => Some(ListStyleSpec::None),
        None => Some(ListStyleSpec::Disc),
    })
}

/// `font-family`: a comma-separated list of names (quoted, or identifiers
/// joined by spaces) and generic families. Generics bevy_markup doesn't
/// map are left out of the list.
fn font_family<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Vec<FamilyRef>> {
    let list = input.parse_comma_separated(|item| {
        if let Ok(name) = item.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_owned())) {
            return Ok(Some(FamilyRef::Named(name)));
        }
        let mut words = vec![item.expect_ident()?.as_ref().to_owned()];
        while let Ok(word) = item.try_parse(|i| i.expect_ident().map(|s| s.as_ref().to_owned())) {
            words.push(word);
        }
        if let [single] = words.as_slice() {
            let generic = match single.to_ascii_lowercase().as_str() {
                "serif" => Some(GenericFamily::Serif),
                "sans-serif" => Some(GenericFamily::SansSerif),
                "monospace" => Some(GenericFamily::Monospace),
                "cursive" => Some(GenericFamily::Cursive),
                "fantasy" => Some(GenericFamily::Fantasy),
                "system-ui" => Some(GenericFamily::SystemUi),
                "emoji" | "math" | "fangsong" | "ui-serif" | "ui-sans-serif" | "ui-monospace"
                | "ui-rounded" => return Ok(None),
                "initial" | "inherit" | "unset" | "revert" | "default" => {
                    return invalid(item);
                }
                _ => None,
            };
            if let Some(generic) = generic {
                return Ok(Some(FamilyRef::Generic(generic)));
            }
        }
        Ok(Some(FamilyRef::Named(words.join(" "))))
    })?;
    Ok(list.into_iter().flatten().collect())
}

/// `font-size`: `px`, `em`, `rem`, `%`, the absolute keywords (`medium` =
/// 16px) and `smaller`/`larger`. Other units are valid, unsupported.
fn font_size<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<FontSizeSpec>> {
    if let Ok(index) = input.try_parse(|i| {
        keyword(
            i,
            &[
                "xx-small",
                "x-small",
                "small",
                "medium",
                "large",
                "x-large",
                "xx-large",
                "xxx-large",
                "smaller",
                "larger",
            ],
        )
    }) {
        return Ok(Some(match index {
            8 => FontSizeSpec::Inherited(1.0 / 1.2),
            9 => FontSizeSpec::Inherited(1.2),
            index => FontSizeSpec::Px([9.0, 10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0][index]),
        }));
    }
    Ok(match non_negative_lp(input)? {
        Lp::Dim(px, Unit::Px | Unit::Abs) => Some(FontSizeSpec::Px(px)),
        Lp::Dim(em, Unit::Em) => Some(FontSizeSpec::Inherited(em)),
        Lp::Dim(rem, Unit::Rem) => Some(FontSizeSpec::Root(rem)),
        Lp::Percent(p) => Some(FontSizeSpec::Inherited(p / 100.0)),
        Lp::Calc(Some(CalcExpr::Val(val))) => match val {
            Val::Px(px) => Some(FontSizeSpec::Px(px)),
            Val::Em(em) => Some(FontSizeSpec::Inherited(em)),
            Val::Rem(rem) => Some(FontSizeSpec::Root(rem)),
            Val::Percent(p) => Some(FontSizeSpec::Inherited(p / 100.0)),
            _ => None,
        },
        Lp::Calc(_) => {
            bevy::log::debug!("html css: calc() font-size unsupported");
            None
        }
        Lp::Dim(..) => {
            bevy::log::debug!("html css: unsupported font-size unit");
            None
        }
    })
}

/// `font-weight` → bold: above 500 prefers the heavier face (CSS font
/// matching).
fn font_weight<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, bool> {
    if let Ok(index) = input.try_parse(|i| keyword(i, &["normal", "bold", "bolder", "lighter"])) {
        return Ok(matches!(index, 1 | 2));
    }
    let weight = parse_number(input)?;
    if !(1.0..=1000.0).contains(&weight) {
        return invalid(input);
    }
    Ok(weight > 500.0)
}

/// `font-style` → italic: anything but `normal`.
fn font_style<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, bool> {
    match keyword(input, &["normal", "italic", "oblique"])? {
        0 => Ok(false),
        1 => Ok(true),
        _ => {
            if let Ok(Numeric::Angle(_)) = input.try_parse(parse_numeric) {}
            Ok(true)
        }
    }
}

/// `text-decoration-line`: `none` or any of `underline`, `overline`,
/// `line-through`, `blink` (or one of the spelling/grammar lines). Bevy
/// draws underline and line-through.
fn decoration_line<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, (bool, bool)> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok((false, false));
    }
    if input
        .try_parse(|i| keyword(i, &["spelling-error", "grammar-error"]))
        .is_ok()
    {
        return Ok((false, false));
    }
    let mut seen = [false; 4];
    let mut any = false;
    while let Ok(index) =
        input.try_parse(|i| keyword(i, &["underline", "overline", "line-through", "blink"]))
    {
        if seen[index] {
            return invalid(input);
        }
        seen[index] = true;
        any = true;
    }
    if !any {
        return invalid(input);
    }
    Ok((seen[0], seen[2]))
}

/// `text-decoration`: `<line> || <style> || <color> || <thickness>`.
fn text_decoration<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Decl> {
    let mut line = None;
    let mut style = false;
    let mut color = None;
    let mut thickness = false;
    for _ in 0..4 {
        if line.is_none()
            && let Ok(l) = input.try_parse(decoration_line)
        {
            line = Some(l);
        } else if !style
            && input
                .try_parse(|i| keyword(i, &["solid", "double", "dotted", "dashed", "wavy"]))
                .is_ok()
        {
            style = true;
        } else if color.is_none()
            && let Ok(c) = input.try_parse(parse_color)
        {
            color = Some(c);
        } else if !thickness
            && input
                .try_parse(|i| {
                    if i.try_parse(|i| keyword(i, &["auto", "from-font"])).is_ok() {
                        return Ok(());
                    }
                    parse_lp(i).map(drop)
                })
                .is_ok()
        {
            thickness = true;
        }
    }
    if line.is_none() && !style && color.is_none() && !thickness {
        return invalid(input);
    }
    let (underline, line_through) = line.unwrap_or((false, false));
    Ok(Decl::TextDecoration(DecorationDecl {
        underline,
        line_through,
        color: color.unwrap_or(CssColor::Current).fixed(),
    }))
}

fn repeat_keyword<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, bool> {
    Ok(keyword(input, &["repeat", "space", "round", "no-repeat"])? != 3)
}

/// One `background-repeat` layer: its `(x, y)` tiling.
fn background_repeat<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, (bool, bool)> {
    if let Ok(index) = input.try_parse(|i| keyword(i, &["repeat-x", "repeat-y"])) {
        return Ok(if index == 0 {
            (true, false)
        } else {
            (false, true)
        });
    }
    let x = repeat_keyword(input)?;
    let y = input.try_parse(repeat_keyword).unwrap_or(x);
    Ok((x, y))
}

/// One `background-clip` layer.
fn background_clip<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<VisualBox>> {
    Ok(
        match keyword(
            input,
            &["border-box", "border", "padding-box", "content-box", "text"],
        )? {
            0 | 1 => Some(VisualBox::BorderBox),
            2 => Some(VisualBox::PaddingBox),
            3 => Some(VisualBox::ContentBox),
            _ => {
                bevy::log::debug!("html css: unsupported background-clip: text");
                None
            }
        },
    )
}

/// `background`: its layers (only the final layer may carry a color per CSS
/// Backgrounds 3; the first layer's image and the final layer's color apply).
fn background<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Decl> {
    let layers = input.parse_comma_separated(background_layer)?;
    if layers[..layers.len() - 1]
        .iter()
        .any(|(color, _)| color.is_some())
    {
        return invalid(input);
    }
    let image = layers[0].1.clone();
    let color = match &layers.last().unwrap().0 {
        Some(c) => c.fixed(),
        None => Some(Color::srgba_u8(0, 0, 0, 0)),
    };
    Ok(Decl::Background { color, image })
}

/// One `background` layer: its explicit color (if any) and URL.
fn background_layer<'i>(
    input: &mut Parser<'i, '_>,
) -> Parsed<'i, (Option<CssColor>, Option<String>)> {
    let mut color = None;
    let mut image = None;
    let mut position = false;
    let mut repeat = false;
    let mut attachment = false;
    let mut boxes = 0;
    let mut any = false;
    loop {
        if color.is_none()
            && let Ok(c) = input.try_parse(parse_color)
        {
            color = Some(c);
        } else if image.is_none()
            && let Ok(url) = input.try_parse(parse_image)
        {
            image = Some(url);
        } else if !position && input.try_parse(position_and_size).is_ok() {
            position = true;
        } else if !repeat && input.try_parse(background_repeat).is_ok() {
            repeat = true;
        } else if !attachment
            && input
                .try_parse(|i| keyword(i, &["scroll", "fixed", "local"]))
                .is_ok()
        {
            attachment = true;
        } else if boxes < 2
            && input
                .try_parse(|i| keyword(i, &["border-box", "padding-box", "content-box", "text"]))
                .is_ok()
        {
            boxes += 1;
        } else {
            break;
        }
        any = true;
    }
    if !any {
        return invalid(input);
    }
    Ok((color, image.flatten()))
}

/// A `<bg-position>` with an optional `/ <bg-size>`.
fn position_and_size<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, ()> {
    let part = |i: &mut Parser<'i, '_>| -> Parsed<'i, ()> {
        if i.try_parse(|i| keyword(i, &["left", "right", "top", "bottom", "center"]))
            .is_ok()
        {
            return Ok(());
        }
        parse_lp(i).map(drop)
    };
    part(input)?;
    for _ in 0..3 {
        if input.try_parse(part).is_err() {
            break;
        }
    }
    if input.try_parse(|i| i.expect_delim('/')).is_ok() {
        let size = |i: &mut Parser<'i, '_>| -> Parsed<'i, ()> {
            if i.try_parse(|i| keyword(i, &["auto", "cover", "contain"]))
                .is_ok()
            {
                return Ok(());
            }
            non_negative_lp(i).map(drop)
        };
        size(input)?;
        let _ = input.try_parse(size);
    }
    Ok(())
}

/// `border-image-slice`: one to four numbers or percentages, with `fill`
/// before or after.
fn border_image_slice<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, ([SliceValue; 4], bool)> {
    let fill_first = input.try_parse(|i| i.expect_ident_matching("fill")).is_ok();
    let offsets = sides(input, |i| match parse_numeric(i)? {
        Numeric::Number(n) if n >= 0.0 => Ok(SliceValue::Px(n)),
        Numeric::Percent(p) if p >= 0.0 => Ok(SliceValue::Fraction(p / 100.0)),
        _ => invalid(i),
    })?;
    let fill =
        fill_first || (!fill_first && input.try_parse(|i| i.expect_ident_matching("fill")).is_ok());
    Ok((offsets, fill))
}

/// `border-image-repeat` → whether any side tiles (Bevy has one scale
/// mode for all sides).
fn border_image_repeat<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, bool> {
    let mode = |i: &mut Parser<'i, '_>| keyword(i, &["stretch", "repeat", "round", "space"]);
    let horizontal = mode(input)?;
    let vertical = input.try_parse(mode).unwrap_or(horizontal);
    Ok(horizontal != 0 || vertical != 0)
}

/// `border-image`: `<source> || <slice> [/ <width>? [/ <outset>]?]? ||
/// <repeat>`.
fn border_image<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, BorderImageDecl> {
    let mut source = None;
    let mut slice = None;
    let mut repeat = None;
    for _ in 0..3 {
        if source.is_none()
            && let Ok(image) = input.try_parse(parse_image)
        {
            source = Some(image);
        } else if slice.is_none()
            && let Ok(value) = input.try_parse(border_image_slice)
        {
            slice = Some(value);
            if input.try_parse(|i| i.expect_delim('/')).is_ok() {
                let width = |i: &mut Parser<'i, '_>| -> Parsed<'i, ()> {
                    if i.try_parse(|i| i.expect_ident_matching("auto")).is_ok()
                        || i.try_parse(parse_non_negative).is_ok()
                    {
                        return Ok(());
                    }
                    non_negative_lp(i).map(drop)
                };
                let had_width = input.try_parse(|i| sides(i, width)).is_ok();
                if input.try_parse(|i| i.expect_delim('/')).is_ok() {
                    sides(input, |i| {
                        if i.try_parse(parse_non_negative).is_ok() {
                            return Ok(());
                        }
                        non_negative_lp(i).map(drop)
                    })?;
                } else if !had_width {
                    return invalid(input);
                }
            }
        } else if repeat.is_none()
            && let Ok(value) = input.try_parse(border_image_repeat)
        {
            repeat = Some(value);
        }
    }
    if source.is_none() && slice.is_none() && repeat.is_none() {
        return invalid(input);
    }
    Ok(BorderImageDecl {
        source: Some(source.flatten()),
        slice: Some(slice.unwrap_or(([SliceValue::Fraction(1.0); 4], false))),
        tile: Some(repeat.unwrap_or(false)),
    })
}

/// An easing function.
fn easing<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Easing> {
    if let Ok(index) = input.try_parse(|i| {
        keyword(
            i,
            &[
                "linear",
                "ease",
                "ease-in",
                "ease-out",
                "ease-in-out",
                "step-start",
                "step-end",
            ],
        )
    }) {
        return Ok(match index {
            0 => Easing::Linear,
            1 => Easing::Ease,
            2 => Easing::EaseIn,
            3 => Easing::EaseOut,
            4 => Easing::EaseInOut,
            5 => Easing::Steps {
                count: 1,
                position: StepPosition::Start,
            },
            _ => Easing::Steps {
                count: 1,
                position: StepPosition::End,
            },
        });
    }
    let location = input.current_source_location();
    let name = input.expect_function()?.to_ascii_lowercase();
    match name.as_str() {
        "cubic-bezier" => input.parse_nested_block(|args| {
            let x1 = parse_number(args)?;
            args.expect_comma()?;
            let y1 = parse_number(args)?;
            args.expect_comma()?;
            let x2 = parse_number(args)?;
            args.expect_comma()?;
            let y2 = parse_number(args)?;
            if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
                return invalid(args);
            }
            Ok(Easing::CubicBezier { x1, y1, x2, y2 })
        }),
        "steps" => input.parse_nested_block(|args| {
            let count = parse_integer(args)?;
            let position = if args.try_parse(|i| i.expect_comma()).is_ok() {
                match keyword(
                    args,
                    &[
                        "jump-start",
                        "start",
                        "jump-end",
                        "end",
                        "jump-none",
                        "jump-both",
                    ],
                )? {
                    0 | 1 => StepPosition::Start,
                    2 | 3 => StepPosition::End,
                    4 => StepPosition::JumpNone,
                    _ => StepPosition::JumpBoth,
                }
            } else {
                StepPosition::End
            };
            let minimum = if position == StepPosition::JumpNone {
                2
            } else {
                1
            };
            if count < minimum {
                return invalid(args);
            }
            Ok(Easing::Steps { count, position })
        }),
        _ => Err(location.new_custom_error(CssError::Invalid)),
    }
}

/// `transition`: the entries on properties the crate animates (`color`,
/// `background-color`, `border-color`); a repeated property keeps its
/// last entry, every other property (and `all`) is skipped.
fn transition<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Vec<TransitionDecl>> {
    let entries = input.parse_comma_separated(|entry| {
        let mut property = None;
        let mut duration = None;
        let mut delay = None;
        let mut timing = None;
        let mut behavior = false;
        let mut any = false;
        loop {
            if duration.is_none()
                && let Ok(t) = entry.try_parse(parse_time)
            {
                duration = Some(t);
            } else if duration.is_some()
                && delay.is_none()
                && let Ok(t) = entry.try_parse(parse_time)
            {
                delay = Some(t);
            } else if timing.is_none()
                && let Ok(e) = entry.try_parse(easing)
            {
                timing = Some(e);
            } else if !behavior
                && entry
                    .try_parse(|i| keyword(i, &["normal", "allow-discrete"]))
                    .is_ok()
            {
                behavior = true;
            } else if property.is_none()
                && let Ok(name) = entry.try_parse(parse_ident)
            {
                if matches!(
                    name.as_str(),
                    "initial" | "inherit" | "unset" | "revert" | "default"
                ) {
                    return invalid(entry);
                }
                property = Some(name);
            } else {
                break;
            }
            any = true;
        }
        if !any || duration.is_some_and(|d| d < 0.0) {
            return invalid(entry);
        }
        Ok((
            property.unwrap_or_else(|| "all".into()),
            duration.unwrap_or(0.0),
            delay.unwrap_or(0.0),
            timing.unwrap_or(Easing::Ease),
        ))
    })?;
    if entries.len() > 1 && entries.iter().any(|(property, ..)| property == "none") {
        return invalid(input);
    }
    Ok(entries
        .into_iter()
        .filter_map(|(property, duration, delay, easing)| {
            let property = match property.as_str() {
                "color" => TransitionProp::TextColor,
                "background-color" => TransitionProp::BackgroundColor,
                "border-color" => TransitionProp::BorderColor,
                other => {
                    bevy::log::debug!("html css: unsupported transition on {other}");
                    return None;
                }
            };
            Some(TransitionDecl {
                property,
                duration,
                delay,
                easing,
            })
        })
        .fold(Vec::new(), |mut list, decl| {
            list.retain(|kept: &TransitionDecl| kept.property != decl.property);
            list.push(decl);
            list
        }))
}
