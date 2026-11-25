//! Value primitives: lengths, percentages, numbers, angles, times, URLs and
//! CSS math (`calc()`, `min()`, `max()`, `clamp()`).
//!
//! Math parses into a small tree, then evaluates with CSS's type rules:
//! numbers fold to constants, a length-percentage becomes the cascade's
//! [`CalcExpr`] (same-unit terms combined, so `calc(50% - 10px)` is
//! `50% + -10px` and `calc(4px * 2)` is `8px`). Every literal and every
//! folded result must be finite: an infinite or NaN value makes the value
//! invalid, so the cascade never sees one (lightningcss carried them,
//! UPSTREAM U45/U49). The other math functions (`round()`, `abs()`, …)
//! parse as unsupported, which drops the declaration.

use bevy::ui::Val;
use cssparser::{Parser, Token};

use super::{Parsed, invalid};
use crate::cascade::CalcExpr;

/// A length unit as the cascade distinguishes it. Absolute units arrive
/// converted to pixels (`Abs`); `Other` is a known relative unit bevy_markup can't
/// resolve (`ex`, `ch`, `lh`, the small/large/dynamic viewport and
/// container units): a declaration holding one is unsupported, not invalid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Unit {
    Px,
    /// An absolute unit other than `px` (`in`, `cm`, `mm`, `Q`, `pt`,
    /// `pc`), its value converted to px.
    Abs,
    Em,
    Rem,
    Vw,
    Vh,
    Vmin,
    Vmax,
    Other,
}

// CSS's absolute units, as f32 like the values they scale.
const PX_PER_IN: f32 = 96.0;
const PX_PER_CM: f32 = PX_PER_IN / 2.54;
const PX_PER_MM: f32 = PX_PER_CM / 10.0;
const PX_PER_Q: f32 = PX_PER_CM / 40.0;
const PX_PER_PT: f32 = PX_PER_IN / 72.0;
const PX_PER_PC: f32 = PX_PER_IN / 6.0;

/// The largest magnitude a parsed number, length or percentage keeps:
/// larger ones are clamped to it, as browsers clamp huge lengths. Finite
/// is not enough: Bevy's layout adds and multiplies these values (width +
/// margin, `em` × font size, `vw` × viewport), and a sum of two values
/// near `f32::MAX` is infinity, which panics in bevy_ui and parley
/// (UPSTREAM U52, U53). At 10⁸ no sum or product the layout forms can
/// overflow; no authored CSS comes near it.
pub(crate) const MAX_CSS_MAGNITUDE: f32 = 1e8;

/// `value` within ±[`MAX_CSS_MAGNITUDE`].
pub(crate) fn bounded(value: f32) -> f32 {
    value.clamp(-MAX_CSS_MAGNITUDE, MAX_CSS_MAGNITUDE)
}

/// `val` within ±[`MAX_CSS_MAGNITUDE`].
pub(crate) fn bounded_val(val: Val) -> Val {
    match val {
        Val::Px(v) => Val::Px(bounded(v)),
        Val::Percent(v) => Val::Percent(bounded(v)),
        Val::Em(v) => Val::Em(bounded(v)),
        Val::Rem(v) => Val::Rem(bounded(v)),
        Val::Vw(v) => Val::Vw(bounded(v)),
        Val::Vh(v) => Val::Vh(bounded(v)),
        Val::VMin(v) => Val::VMin(bounded(v)),
        Val::VMax(v) => Val::VMax(bounded(v)),
        Val::Auto => Val::Auto,
    }
}

/// `expr` with every term within ±[`MAX_CSS_MAGNITUDE`].
fn bounded_expr(expr: CalcExpr) -> CalcExpr {
    match expr {
        CalcExpr::Val(val) => CalcExpr::Val(bounded_val(val)),
        CalcExpr::Sum(a, b) => {
            CalcExpr::Sum(Box::new(bounded_expr(*a)), Box::new(bounded_expr(*b)))
        }
        CalcExpr::Product(k, inner) => {
            CalcExpr::Product(bounded(k), Box::new(bounded_expr(*inner)))
        }
        CalcExpr::Min(args) => CalcExpr::Min(args.into_iter().map(bounded_expr).collect()),
        CalcExpr::Max(args) => CalcExpr::Max(args.into_iter().map(bounded_expr).collect()),
    }
}

/// A dimension's value in its cascade unit, or `None` when `unit` isn't a
/// length unit at all (then the value is invalid).
pub(crate) fn length_unit(value: f32, unit: &str) -> Option<(f32, Unit)> {
    let unit = unit.to_ascii_lowercase();
    Some(match unit.as_str() {
        "px" => (value, Unit::Px),
        "in" => (value * PX_PER_IN, Unit::Abs),
        "cm" => (value * PX_PER_CM, Unit::Abs),
        "mm" => (value * PX_PER_MM, Unit::Abs),
        "q" => (value * PX_PER_Q, Unit::Abs),
        "pt" => (value * PX_PER_PT, Unit::Abs),
        "pc" => (value * PX_PER_PC, Unit::Abs),
        "em" => (value, Unit::Em),
        "rem" => (value, Unit::Rem),
        "vw" => (value, Unit::Vw),
        "vh" => (value, Unit::Vh),
        "vmin" => (value, Unit::Vmin),
        "vmax" => (value, Unit::Vmax),
        "ex" | "rex" | "ch" | "rch" | "cap" | "rcap" | "ic" | "ric" | "lh" | "rlh" | "vi"
        | "vb" | "svw" | "svh" | "svi" | "svb" | "svmin" | "svmax" | "lvw" | "lvh" | "lvi"
        | "lvb" | "lvmin" | "lvmax" | "dvw" | "dvh" | "dvi" | "dvb" | "dvmin" | "dvmax" | "cqw"
        | "cqh" | "cqi" | "cqb" | "cqmin" | "cqmax" => (value, Unit::Other),
        _ => return None,
    })
}

/// A length in a cascade unit as a Bevy `Val` (`None` for `Other`).
pub(crate) fn unit_val(value: f32, unit: Unit) -> Option<Val> {
    Some(match unit {
        Unit::Px | Unit::Abs => Val::Px(value),
        Unit::Em => Val::Em(value),
        Unit::Rem => Val::Rem(value),
        Unit::Vw => Val::Vw(value),
        Unit::Vh => Val::Vh(value),
        Unit::Vmin => Val::VMin(value),
        Unit::Vmax => Val::VMax(value),
        Unit::Other => return None,
    })
}

/// A `<length-percentage>`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Lp {
    /// A length in a cascade unit (absolute units in `Px`).
    Dim(f32, Unit),
    /// A percentage, 0–100.
    Percent(f32),
    /// A math function: its expression, or `None` when it uses something
    /// unsupported (an `Other` unit, `round()`, …).
    Calc(Option<CalcExpr>),
}

impl Lp {
    /// A single `Val`: a plain length or percentage, or math that folds to
    /// one (`calc(4px + 4px)`). Mixed-unit math is `None`.
    pub(crate) fn val(&self) -> Option<Val> {
        match self {
            Lp::Dim(value, unit) => unit_val(*value, *unit),
            Lp::Percent(percent) => Some(Val::Percent(*percent)),
            Lp::Calc(Some(CalcExpr::Val(val))) => Some(*val),
            Lp::Calc(_) => None,
        }
    }

    /// Absolute pixels: a length in `px` (or an absolute unit), or math
    /// folding to one.
    pub(crate) fn px(&self) -> Option<f32> {
        match self.val() {
            Some(Val::Px(px)) => Some(px),
            _ => None,
        }
    }
}

/// Whether `name` names a CSS math function.
pub(crate) fn is_math(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "calc"
            | "-webkit-calc"
            | "-moz-calc"
            | "min"
            | "max"
            | "clamp"
            | "round"
            | "mod"
            | "rem"
            | "abs"
            | "sign"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "pow"
            | "sqrt"
            | "hypot"
            | "log"
            | "exp"
            | "calc-size"
    )
}

/// A finite `f32`, else invalid.
fn finite<'i>(input: &Parser<'i, '_>, value: f32) -> Parsed<'i, f32> {
    if value.is_finite() {
        Ok(value)
    } else {
        invalid(input)
    }
}

/// A `<length-percentage>`: a length (a unitless number is pixels), a
/// percentage, or math.
pub(crate) fn parse_lp<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Lp> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    match token {
        Token::Dimension { value, unit, .. } => {
            let value = finite(input, value)?;
            match length_unit(value, &unit) {
                Some((value, unit)) => Ok(Lp::Dim(bounded(finite(input, value)?), unit)),
                None => Err(location.new_custom_error(super::CssError::Invalid)),
            }
        }
        Token::Percentage { unit_value, .. } => {
            Ok(Lp::Percent(bounded(finite(input, unit_value * 100.0)?)))
        }
        // A unitless number is pixels, as lightningcss read it: apps pass
        // numbers through `var()` from the `TemplateContext` (`--w: 160`),
        // and their sheets rely on it.
        Token::Number { value, .. } => Ok(Lp::Dim(bounded(finite(input, value)?), Unit::Px)),
        Token::Function(name) if is_math(&name) => {
            match evaluate(&math_function(&name, input, 0)?, true) {
                Some(Typed::Len(expr)) => Ok(Lp::Calc(Some(simplify(bounded_expr(expr))))),
                Some(Typed::Unsupported) => Ok(Lp::Calc(None)),
                _ => Err(location.new_custom_error(super::CssError::Invalid)),
            }
        }
        _ => Err(location.new_custom_error(super::CssError::Invalid)),
    }
}

/// A number, an angle, a time or a percentage (with math folded).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Numeric {
    Number(f32),
    /// 0–100.
    Percent(f32),
    /// Degrees.
    Angle(f32),
    /// Seconds.
    Time(f32),
}

/// A number, percentage, angle or time (`deg`/`grad`/`rad`/`turn`,
/// `s`/`ms`), or math folding to one. Lengths are invalid here.
pub(crate) fn parse_numeric<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Numeric> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    let numeric = match token {
        Token::Number { value, .. } => Numeric::Number(value),
        Token::Percentage { unit_value, .. } => Numeric::Percent(unit_value * 100.0),
        Token::Dimension { value, unit, .. } => match other_unit(value, &unit) {
            Some(Leaf::Angle(degrees)) => Numeric::Angle(degrees),
            Some(Leaf::Time(seconds)) => Numeric::Time(seconds),
            _ => return Err(location.new_custom_error(super::CssError::Invalid)),
        },
        Token::Function(name) if is_math(&name) => {
            match evaluate(&math_function(&name, input, 0)?, false) {
                Some(Typed::Num(n)) => Numeric::Number(n),
                Some(Typed::Pct(p)) => Numeric::Percent(p),
                Some(Typed::Angle(a)) => Numeric::Angle(a),
                Some(Typed::Time(t)) => Numeric::Time(t),
                _ => return Err(location.new_custom_error(super::CssError::Invalid)),
            }
        }
        _ => return Err(location.new_custom_error(super::CssError::Invalid)),
    };
    let value = match numeric {
        Numeric::Number(v) | Numeric::Percent(v) | Numeric::Angle(v) | Numeric::Time(v) => v,
    };
    finite(input, value)?;
    Ok(match numeric {
        Numeric::Number(v) => Numeric::Number(bounded(v)),
        Numeric::Percent(v) => Numeric::Percent(bounded(v)),
        Numeric::Angle(v) => Numeric::Angle(bounded(v)),
        Numeric::Time(v) => Numeric::Time(bounded(v)),
    })
}

/// A number (or math folding to one).
pub(crate) fn parse_number<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, f32> {
    match parse_numeric(input)? {
        Numeric::Number(n) => Ok(n),
        _ => invalid(input),
    }
}

/// A non-negative number.
pub(crate) fn parse_non_negative<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, f32> {
    let n = parse_number(input)?;
    if n < 0.0 { invalid(input) } else { Ok(n) }
}

/// An integer token (math isn't evaluated here).
pub(crate) fn parse_integer<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, i32> {
    let location = input.current_source_location();
    match input.next()? {
        Token::Number {
            int_value: Some(value),
            ..
        } => Ok(*value),
        _ => Err(location.new_custom_error(super::CssError::Invalid)),
    }
}

/// A time in seconds (`s`, `ms`).
pub(crate) fn parse_time<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, f32> {
    match parse_numeric(input)? {
        Numeric::Time(seconds) => Ok(seconds),
        _ => invalid(input),
    }
}

/// An identifier, ASCII-lowercased.
pub(crate) fn parse_ident<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, String> {
    Ok(input.expect_ident()?.as_ref().to_ascii_lowercase())
}

/// One of `keywords` (ASCII case-insensitive): its index.
pub(crate) fn parse_keyword<'i>(
    input: &mut Parser<'i, '_>,
    keywords: &[&str],
) -> Parsed<'i, usize> {
    let location = input.current_source_location();
    let ident = parse_ident(input)?;
    keywords
        .iter()
        .position(|keyword| *keyword == ident)
        .ok_or_else(|| location.new_custom_error(super::CssError::Invalid))
}

/// A `url(…)`: the URL as written.
pub(crate) fn parse_url<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, String> {
    Ok(input.expect_url()?.as_ref().to_owned())
}

/// An `<image>`: `Some(url)` for a `url()`, `None` for `none` or a
/// gradient/`image-set()` (accepted, but bevy_markup draws only URLs).
pub(crate) fn parse_image<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Option<String>> {
    if let Ok(url) = input.try_parse(parse_url) {
        return Ok(Some(url));
    }
    let location = input.current_source_location();
    match input.next()?.clone() {
        Token::Ident(ident) if ident.eq_ignore_ascii_case("none") => Ok(None),
        Token::Function(name) if is_image_function(&name) => {
            input.parse_nested_block(|args| {
                while args.next().is_ok() {}
                Ok(())
            })?;
            bevy::log::debug!("html css: only url() image sources are supported");
            Ok(None)
        }
        _ => Err(location.new_custom_error(super::CssError::Invalid)),
    }
}

fn is_image_function(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let name = name
        .strip_prefix("-webkit-")
        .or_else(|| name.strip_prefix("-moz-"))
        .or_else(|| name.strip_prefix("-o-"))
        .unwrap_or(&name);
    matches!(
        name,
        "linear-gradient"
            | "radial-gradient"
            | "conic-gradient"
            | "repeating-linear-gradient"
            | "repeating-radial-gradient"
            | "repeating-conic-gradient"
            | "image-set"
            | "image"
            | "cross-fade"
            | "element"
            | "paint"
    )
}

/// The maximum nesting of math functions and parentheses within one math
/// expression (the pre-scan already bounds parentheses at 8).
const MAX_CALC_DEPTH: usize = 32;

/// A math expression, before type checking.
#[derive(Clone, Debug)]
enum Ast {
    Leaf(Leaf),
    Sum(Box<Ast>, Box<Ast>),
    Mul(Box<Ast>, Box<Ast>),
    Div(Box<Ast>, Box<Ast>),
    Min(Vec<Ast>),
    Max(Vec<Ast>),
    Unsupported,
}

#[derive(Clone, Copy, Debug)]
enum Leaf {
    Number(f32),
    Percent(f32),
    Length(f32, Unit),
    Angle(f32),
    Time(f32),
}

/// A dimension that isn't a length: an angle in degrees or a time in
/// seconds.
fn other_unit(value: f32, unit: &str) -> Option<Leaf> {
    Some(match unit.to_ascii_lowercase().as_str() {
        "deg" => Leaf::Angle(value),
        "grad" => Leaf::Angle(value * 0.9),
        "rad" => Leaf::Angle(value.to_degrees()),
        "turn" => Leaf::Angle(value * 360.0),
        "s" => Leaf::Time(value),
        "ms" => Leaf::Time(value / 1000.0),
        _ => return None,
    })
}

/// The arguments of math function `name` (its `Function` token already
/// read).
fn math_function<'i>(name: &str, input: &mut Parser<'i, '_>, depth: usize) -> Parsed<'i, Ast> {
    if depth >= MAX_CALC_DEPTH {
        return invalid(input);
    }
    let name = name.to_ascii_lowercase();
    input.parse_nested_block(|args| match name.as_str() {
        "calc" | "-webkit-calc" | "-moz-calc" => {
            let sum = parse_sum(args, depth + 1)?;
            args.expect_exhausted()?;
            Ok(sum)
        }
        "min" | "max" => {
            let list = args.parse_comma_separated(|arg| parse_sum(arg, depth + 1))?;
            Ok(if name == "min" {
                Ast::Min(list)
            } else {
                Ast::Max(list)
            })
        }
        "clamp" => {
            let low = parse_sum(args, depth + 1)?;
            args.expect_comma()?;
            let value = parse_sum(args, depth + 1)?;
            args.expect_comma()?;
            let high = parse_sum(args, depth + 1)?;
            args.expect_exhausted()?;
            Ok(Ast::Max(vec![low, Ast::Min(vec![value, high])]))
        }
        _ => {
            while args.next().is_ok() {}
            Ok(Ast::Unsupported)
        }
    })
}

/// `a + b - c`: CSS requires whitespace around `+` and `-`.
fn parse_sum<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Parsed<'i, Ast> {
    let mut sum = parse_product(input, depth)?;
    while let Some(op) = sum_operator(input) {
        let term = parse_product(input, depth)?;
        let term = if op == '-' {
            Ast::Mul(Box::new(Ast::Leaf(Leaf::Number(-1.0))), Box::new(term))
        } else {
            term
        };
        sum = Ast::Sum(Box::new(sum), Box::new(term));
    }
    Ok(sum)
}

/// A whitespace-delimited `+` or `-`, else nothing consumed.
fn sum_operator(input: &mut Parser<'_, '_>) -> Option<char> {
    let state = input.state();
    let op = (|| {
        if !matches!(input.next_including_whitespace(), Ok(Token::WhiteSpace(_))) {
            return None;
        }
        let op = match input.next_including_whitespace() {
            Ok(Token::Delim(op @ ('+' | '-'))) => *op,
            _ => return None,
        };
        matches!(input.next_including_whitespace(), Ok(Token::WhiteSpace(_))).then_some(op)
    })();
    if op.is_none() {
        input.reset(&state);
    }
    op
}

fn parse_product<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Parsed<'i, Ast> {
    let mut product = parse_math_value(input, depth)?;
    loop {
        let state = input.state();
        match input.next() {
            Ok(Token::Delim('*')) => {
                product = Ast::Mul(Box::new(product), Box::new(parse_math_value(input, depth)?));
            }
            Ok(Token::Delim('/')) => {
                product = Ast::Div(Box::new(product), Box::new(parse_math_value(input, depth)?));
            }
            _ => {
                input.reset(&state);
                return Ok(product);
            }
        }
    }
}

fn parse_math_value<'i>(input: &mut Parser<'i, '_>, depth: usize) -> Parsed<'i, Ast> {
    let location = input.current_source_location();
    let error = || Err(location.new_custom_error(super::CssError::Invalid));
    let token = input.next()?.clone();
    let leaf = match token {
        Token::Number { value, .. } => Leaf::Number(value),
        Token::Percentage { unit_value, .. } => Leaf::Percent(unit_value * 100.0),
        Token::Dimension { value, unit, .. } => match length_unit(value, &unit) {
            Some((value, unit)) => Leaf::Length(value, unit),
            None => match other_unit(value, &unit) {
                Some(leaf) => leaf,
                None => return error(),
            },
        },
        Token::Ident(ident) => match ident.to_ascii_lowercase().as_str() {
            "e" => Leaf::Number(std::f32::consts::E),
            "pi" => Leaf::Number(std::f32::consts::PI),
            // `infinity`, `-infinity` and `NaN` are refused: no value
            // carries a non-finite number.
            _ => return error(),
        },
        Token::ParenthesisBlock => {
            if depth >= MAX_CALC_DEPTH {
                return error();
            }
            return input.parse_nested_block(|inner| {
                let sum = parse_sum(inner, depth + 1)?;
                inner.expect_exhausted()?;
                Ok(sum)
            });
        }
        Token::Function(name) if is_math(&name) => return math_function(&name, input, depth + 1),
        _ => return error(),
    };
    let value = match leaf {
        Leaf::Number(v)
        | Leaf::Percent(v)
        | Leaf::Length(v, _)
        | Leaf::Angle(v)
        | Leaf::Time(v) => v,
    };
    if !value.is_finite() {
        return error();
    }
    Ok(Ast::Leaf(leaf))
}

/// A math expression's value with its CSS type.
#[derive(Clone, Debug)]
enum Typed {
    Num(f32),
    /// A length-percentage (percentages are lengths in that context).
    Len(CalcExpr),
    Pct(f32),
    Angle(f32),
    Time(f32),
    /// Holds something unsupported: the declaration drops.
    Unsupported,
}

/// Type-checks and folds `ast`. `pct_is_len`: percentages are lengths (a
/// `<length-percentage>` context). `None`: a type error or a non-finite
/// result (the value is invalid).
fn evaluate(ast: &Ast, pct_is_len: bool) -> Option<Typed> {
    let typed = match ast {
        Ast::Unsupported => Typed::Unsupported,
        Ast::Leaf(leaf) => match *leaf {
            Leaf::Number(n) => Typed::Num(n),
            Leaf::Percent(p) if pct_is_len => Typed::Len(CalcExpr::Val(Val::Percent(p))),
            Leaf::Percent(p) => Typed::Pct(p),
            Leaf::Length(v, unit) => match unit_val(v, unit) {
                Some(val) => Typed::Len(CalcExpr::Val(val)),
                None => Typed::Unsupported,
            },
            Leaf::Angle(a) => Typed::Angle(a),
            Leaf::Time(t) => Typed::Time(t),
        },
        Ast::Sum(a, b) => match (evaluate(a, pct_is_len)?, evaluate(b, pct_is_len)?) {
            (Typed::Unsupported, _) | (_, Typed::Unsupported) => Typed::Unsupported,
            (Typed::Num(a), Typed::Num(b)) => Typed::Num(a + b),
            (Typed::Pct(a), Typed::Pct(b)) => Typed::Pct(a + b),
            (Typed::Angle(a), Typed::Angle(b)) => Typed::Angle(a + b),
            (Typed::Time(a), Typed::Time(b)) => Typed::Time(a + b),
            (Typed::Len(a), Typed::Len(b)) => {
                Typed::Len(simplify(CalcExpr::Sum(Box::new(a), Box::new(b))))
            }
            _ => return None,
        },
        Ast::Mul(a, b) => match (evaluate(a, pct_is_len)?, evaluate(b, pct_is_len)?) {
            (Typed::Unsupported, _) | (_, Typed::Unsupported) => Typed::Unsupported,
            (Typed::Num(k), other) | (other, Typed::Num(k)) => scale(other, k),
            _ => return None,
        },
        Ast::Div(a, b) => match (evaluate(a, pct_is_len)?, evaluate(b, pct_is_len)?) {
            (Typed::Unsupported, _) | (_, Typed::Unsupported) => Typed::Unsupported,
            (other, Typed::Num(k)) => scale(other, 1.0 / k),
            _ => return None,
        },
        Ast::Min(args) | Ast::Max(args) => {
            let min = matches!(ast, Ast::Min(_));
            let args: Vec<Typed> = args
                .iter()
                .map(|arg| evaluate(arg, pct_is_len))
                .collect::<Option<_>>()?;
            fold_min_max(args, min)?
        }
    };
    let finite = match &typed {
        Typed::Num(v) | Typed::Pct(v) | Typed::Angle(v) | Typed::Time(v) => v.is_finite(),
        Typed::Len(expr) => expr_is_finite(expr),
        Typed::Unsupported => true,
    };
    finite.then_some(typed)
}

fn scale(typed: Typed, k: f32) -> Typed {
    match typed {
        Typed::Num(v) => Typed::Num(v * k),
        Typed::Pct(v) => Typed::Pct(v * k),
        Typed::Angle(v) => Typed::Angle(v * k),
        Typed::Time(v) => Typed::Time(v * k),
        Typed::Len(expr) => Typed::Len(scale_expr(expr, k)),
        Typed::Unsupported => Typed::Unsupported,
    }
}

/// `k × expr`, distributed over sums and folded into plain values.
fn scale_expr(expr: CalcExpr, k: f32) -> CalcExpr {
    match expr {
        CalcExpr::Val(val) => CalcExpr::Val(scale_val(val, k)),
        CalcExpr::Sum(a, b) => {
            CalcExpr::Sum(Box::new(scale_expr(*a, k)), Box::new(scale_expr(*b, k)))
        }
        CalcExpr::Product(j, inner) => scale_expr(*inner, j * k),
        other => CalcExpr::Product(k, Box::new(other)),
    }
}

fn scale_val(val: Val, k: f32) -> Val {
    match val {
        Val::Px(v) => Val::Px(v * k),
        Val::Percent(v) => Val::Percent(v * k),
        Val::Em(v) => Val::Em(v * k),
        Val::Rem(v) => Val::Rem(v * k),
        Val::Vw(v) => Val::Vw(v * k),
        Val::Vh(v) => Val::Vh(v * k),
        Val::VMin(v) => Val::VMin(v * k),
        Val::VMax(v) => Val::VMax(v * k),
        Val::Auto => Val::Auto,
    }
}

/// An expression folded to one value where its terms share a unit.
fn simplify(expr: CalcExpr) -> CalcExpr {
    match expr.collapse() {
        Some(val) => CalcExpr::Val(val),
        None => expr,
    }
}

fn fold_min_max(args: Vec<Typed>, min: bool) -> Option<Typed> {
    let pick = |a: f32, b: f32| if min { a.min(b) } else { a.max(b) };
    let first = args.first()?.clone();
    if args.iter().any(|arg| matches!(arg, Typed::Unsupported)) {
        return Some(Typed::Unsupported);
    }
    macro_rules! fold {
        ($variant:ident) => {{
            let mut acc = None::<f32>;
            for arg in &args {
                let Typed::$variant(v) = arg else { return None };
                acc = Some(acc.map_or(*v, |a| pick(a, *v)));
            }
            Typed::$variant(acc?)
        }};
    }
    Some(match first {
        Typed::Num(_) => fold!(Num),
        Typed::Pct(_) => fold!(Pct),
        Typed::Angle(_) => fold!(Angle),
        Typed::Time(_) => fold!(Time),
        Typed::Len(_) => {
            let exprs: Vec<CalcExpr> = args
                .into_iter()
                .map(|arg| match arg {
                    Typed::Len(expr) => Some(expr),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            Typed::Len(simplify(if min {
                CalcExpr::Min(exprs)
            } else {
                CalcExpr::Max(exprs)
            }))
        }
        Typed::Unsupported => Typed::Unsupported,
    })
}

fn expr_is_finite(expr: &CalcExpr) -> bool {
    match expr {
        CalcExpr::Val(val) => match val {
            Val::Px(v)
            | Val::Percent(v)
            | Val::Em(v)
            | Val::Rem(v)
            | Val::Vw(v)
            | Val::Vh(v)
            | Val::VMin(v)
            | Val::VMax(v) => v.is_finite(),
            Val::Auto => true,
        },
        CalcExpr::Sum(a, b) => expr_is_finite(a) && expr_is_finite(b),
        CalcExpr::Product(k, inner) => k.is_finite() && expr_is_finite(inner),
        CalcExpr::Min(args) | CalcExpr::Max(args) => args.iter().all(expr_is_finite),
    }
}

#[cfg(test)]
mod tests {
    use bevy::ui::Val;
    use cssparser::{Parser, ParserInput};

    use super::*;

    fn lp(text: &str) -> Option<Lp> {
        let mut input = ParserInput::new(text);
        Parser::new(&mut input).parse_entirely(parse_lp).ok()
    }

    fn numeric(text: &str) -> Option<Numeric> {
        let mut input = ParserInput::new(text);
        Parser::new(&mut input).parse_entirely(parse_numeric).ok()
    }

    /// The largest magnitude anywhere in an expression (products count
    /// their factor).
    fn magnitude(expr: &CalcExpr) -> f32 {
        let val = |val: &Val| match *val {
            Val::Px(v)
            | Val::Percent(v)
            | Val::Em(v)
            | Val::Rem(v)
            | Val::Vw(v)
            | Val::Vh(v)
            | Val::VMin(v)
            | Val::VMax(v) => v.abs(),
            Val::Auto => 0.0,
        };
        match expr {
            CalcExpr::Val(v) => val(v),
            CalcExpr::Sum(a, b) => magnitude(a).max(magnitude(b)),
            CalcExpr::Product(k, inner) => k.abs().max(magnitude(inner)),
            CalcExpr::Min(args) | CalcExpr::Max(args) => {
                args.iter().map(magnitude).fold(0.0, f32::max)
            }
        }
    }

    /// The largest magnitude a parsed length keeps.
    fn lp_magnitude(lp: &Lp) -> f32 {
        match lp {
            Lp::Dim(v, _) | Lp::Percent(v) => v.abs(),
            Lp::Calc(Some(expr)) => magnitude(expr),
            Lp::Calc(None) => 0.0,
        }
    }

    /// Huge but finite values clamp to `MAX_CSS_MAGNITUDE` (Bevy's layout
    /// sums and multiplies them, and two values near `f32::MAX` add up to
    /// infinity, which panics in bevy_ui and parley: UPSTREAM U52, U53).
    #[test]
    fn huge_values_clamp_to_the_bound() {
        assert_eq!(lp("3e38px"), Some(Lp::Dim(MAX_CSS_MAGNITUDE, Unit::Px)));
        assert_eq!(lp("-3e38em"), Some(Lp::Dim(-MAX_CSS_MAGNITUDE, Unit::Em)));
        assert_eq!(lp("3e38"), Some(Lp::Dim(MAX_CSS_MAGNITUDE, Unit::Px)));
        assert_eq!(lp("1e30%"), Some(Lp::Percent(MAX_CSS_MAGNITUDE)));
        assert_eq!(lp("1e30in"), Some(Lp::Dim(MAX_CSS_MAGNITUDE, Unit::Abs)));
        assert_eq!(lp("1e30vw"), Some(Lp::Dim(MAX_CSS_MAGNITUDE, Unit::Vw)));
        assert_eq!(numeric("1e30"), Some(Numeric::Number(MAX_CSS_MAGNITUDE)));
        assert_eq!(
            numeric("-1e30deg"),
            Some(Numeric::Angle(-MAX_CSS_MAGNITUDE))
        );
        assert_eq!(numeric("1e30s"), Some(Numeric::Time(MAX_CSS_MAGNITUDE)));
        assert_eq!(
            numeric("calc(1e20 * 1e10)"),
            Some(Numeric::Number(MAX_CSS_MAGNITUDE))
        );
        // Values inside the bound are kept as written.
        assert_eq!(lp("1234.5px"), Some(Lp::Dim(1234.5, Unit::Px)));
        assert_eq!(lp("-50%"), Some(Lp::Percent(-50.0)));
    }

    /// A mixed-unit `calc()` keeps its terms (they are added up at layout
    /// time), each within the bound; a fold that overflows is invalid.
    #[test]
    fn calc_terms_stay_within_the_bound() {
        let Some(Lp::Calc(Some(expr))) = lp("calc(3e38px + 1em + 3e38px)") else {
            panic!("a mixed-unit calc parses");
        };
        assert!(magnitude(&expr) <= MAX_CSS_MAGNITUDE, "{expr:?}");
        let Some(Lp::Calc(Some(expr))) = lp("calc((1e30px + 1em) * 1e5)") else {
            panic!("a scaled mixed-unit calc parses");
        };
        assert!(magnitude(&expr) <= MAX_CSS_MAGNITUDE, "{expr:?}");
        // Folding overflows f32: refused, never infinite.
        assert_eq!(lp("calc(3e38px + 3e38px)"), None);
        assert_eq!(lp("calc(1e30px * 1e30)"), None);
        assert_eq!(lp("calc((1e30px + 1em) * 1e30)"), None);
        assert_eq!(numeric("calc(1e30 * 1e30)"), None);
    }

    /// Non-finite literals and math constants are refused.
    #[test]
    fn non_finite_values_are_refused() {
        for text in [
            "1e39px",
            "-1e39px",
            "calc(infinity * 1px)",
            "calc(NaN * 1px)",
            "1e39%",
        ] {
            assert_eq!(lp(text), None, "{text}");
        }
        for text in [
            "1e39",
            "calc(infinity)",
            "calc(-infinity)",
            "calc(NaN)",
            "sqrt(-1)",
        ] {
            assert_eq!(numeric(text), None, "{text}");
        }
    }

    proptest::proptest! {
        /// Whatever magnitude, unit and arithmetic a length uses, a parsed
        /// value is finite and within the bound: the whole class of
        /// overflow inputs (bug_0249).
        #[test]
        fn parsed_lengths_are_finite_and_bounded(
            mantissa in -10.0f32..10.0,
            exponent in 0i32..40,
            unit in proptest::sample::select(vec!["px", "em", "rem", "%", "vw", "vh", "in", "pt", ""]),
            factor_exponent in 0i32..40,
            shape in 0usize..6,
        ) {
            let value = format!("{mantissa}e{exponent}{unit}");
            let factor = format!("1e{factor_exponent}");
            let text = match shape {
                0 => value.clone(),
                1 => format!("calc({value} + 1em + {value})"),
                2 => format!("calc({value} * {factor})"),
                3 => format!("calc(({value} - 1px) / {factor})"),
                4 => format!("min({value}, 1em, calc({value} * {factor}))"),
                _ => format!("clamp(1px, calc({value} + 2vw), {value})"),
            };
            if let Some(parsed) = lp(&text) {
                let max = lp_magnitude(&parsed);
                proptest::prop_assert!(max.is_finite() && max <= MAX_CSS_MAGNITUDE, "{text}: {parsed:?}");
            }
        }
    }
}
