//! bevy_markup's CSS parser (ADR 0048): stylesheets, `style` attributes and
//! `var()`-substituted values, parsed on `cssparser`'s tokenizer into the
//! declarations the cascade applies. It parses only what bevy_markup maps to
//! Bevy UI; everything else is dropped like an invalid declaration.
//!
//! Every input is untrusted (ADR 0044). The parsers recurse only on nested
//! blocks, whose depth `cascade::nesting_over_limit` bounds before any text
//! reaches them; `calc()` keeps a depth counter of its own, and every
//! number is checked finite, so no value carries NaN or infinity into the
//! cascade.

pub(crate) mod color;
pub(crate) mod grid;
pub(crate) mod property;
pub(crate) mod select;
pub(crate) mod sheet;
pub(crate) mod value;

use cssparser::{BasicParseErrorKind, ParseError, ParseErrorKind, Parser};
use parcel_selectors::parser::SelectorParseErrorKind;

/// Why a parse failed: a selector error from parcel_selectors, or a value
/// outside the grammar bevy_markup parses.
#[derive(Clone, Debug)]
pub(crate) enum CssError<'i> {
    Selector(SelectorParseErrorKind<'i>),
    Invalid,
}

impl<'i> From<SelectorParseErrorKind<'i>> for CssError<'i> {
    fn from(kind: SelectorParseErrorKind<'i>) -> Self {
        CssError::Selector(kind)
    }
}

/// A parse error in any of the CSS parsers.
pub(crate) type Error<'i> = ParseError<'i, CssError<'i>>;

/// A parse result.
pub(crate) type Parsed<'i, T> = Result<T, Error<'i>>;

/// An invalid-value error at the parser's position.
pub(crate) fn invalid<'i, T>(input: &Parser<'i, '_>) -> Parsed<'i, T> {
    Err(input.new_custom_error(CssError::Invalid))
}

/// A readable message for a parse error, with its position (the sheet
/// loader's error text).
pub(crate) fn describe(error: &Error<'_>) -> String {
    let what = match &error.kind {
        ParseErrorKind::Basic(BasicParseErrorKind::UnexpectedToken(token)) => {
            format!("unexpected token {token:?}")
        }
        ParseErrorKind::Basic(BasicParseErrorKind::EndOfInput) => "unexpected end of input".into(),
        ParseErrorKind::Basic(BasicParseErrorKind::AtRuleInvalid(name)) => {
            format!("invalid at-rule @{name}")
        }
        ParseErrorKind::Basic(BasicParseErrorKind::AtRuleBodyInvalid) => {
            "invalid at-rule body".into()
        }
        ParseErrorKind::Basic(BasicParseErrorKind::QualifiedRuleInvalid) => "invalid rule".into(),
        ParseErrorKind::Custom(CssError::Selector(kind)) => format!("invalid selector ({kind:?})"),
        ParseErrorKind::Custom(CssError::Invalid) => "invalid value".into(),
    };
    format!(
        "{what} at {}:{}",
        error.location.line + 1,
        error.location.column
    )
}
