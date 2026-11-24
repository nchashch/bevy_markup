//! The selector types parcel_selectors parses into: bevy_markup's own
//! `SelectorImpl` (ADR 0048), where lightningcss's used to be. Every name
//! is an owned `String`, so a parsed list outlives its source text.
//!
//! The parser accepts what a browser parses, pseudo-classes and
//! pseudo-elements it doesn't know included: an unsupported selector skips
//! its rule in the cascade's allowlist (`cascade/selector.rs`), it doesn't
//! fail the sheet.

use std::fmt;

use cssparser::{CowRcStr, ParseError, Parser as CssParser, SourceLocation, ToCss};
use parcel_selectors::parser::{self, NestingRequirement, ParseErrorRecovery};
use static_self::IntoOwned;

use super::{CssError, Error};

/// An identifier: a tag, class, id, attribute or namespace name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Name(pub(crate) String);

impl<'i> From<CowRcStr<'i>> for Name {
    fn from(name: CowRcStr<'i>) -> Self {
        Name(name.as_ref().to_owned())
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ToCss for Name {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

/// An attribute selector's value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Value(pub(crate) String);

impl<'i> From<CowRcStr<'i>> for Value {
    fn from(value: CowRcStr<'i>) -> Self {
        Value(value.as_ref().to_owned())
    }
}

impl AsRef<str> for Value {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ToCss for Value {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_string(&self.0, dest)
    }
}

/// A pseudo-class that isn't tree-structural (parcel parses those itself).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PseudoClass {
    Hover,
    Active,
    Focus,
    FocusVisible,
    FocusWithin,
    /// Any other name, functional ones included: parsed, never matched.
    Other(String),
}

impl ToCss for PseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(match self {
            PseudoClass::Hover => ":hover",
            PseudoClass::Active => ":active",
            PseudoClass::Focus => ":focus",
            PseudoClass::FocusVisible => ":focus-visible",
            PseudoClass::FocusWithin => ":focus-within",
            PseudoClass::Other(name) => {
                dest.write_char(':')?;
                return cssparser::serialize_identifier(name, dest);
            }
        })
    }
}

impl<'i> parser::NonTSPseudoClass<'i> for PseudoClass {
    type Impl = Impl;

    fn is_active_or_hover(&self) -> bool {
        matches!(self, PseudoClass::Active | PseudoClass::Hover)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            PseudoClass::Active
                | PseudoClass::Hover
                | PseudoClass::Focus
                | PseudoClass::FocusVisible
                | PseudoClass::FocusWithin
        )
    }
}

/// A pseudo-element: parsed, never matched (the allowlist skips its rule).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PseudoElement(pub(crate) String);

impl ToCss for PseudoElement {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str("::")?;
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl<'i> parser::PseudoElement<'i> for PseudoElement {
    type Impl = Impl;
}

/// No vendor prefixes (`:-webkit-any()` isn't parsed).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NoPrefix;

impl ToCss for NoPrefix {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        Ok(())
    }
}

/// bevy_markup's selector implementation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Impl;

impl<'i> parser::SelectorImpl<'i> for Impl {
    type ExtraMatchingData = ();
    type AttrValue = Value;
    type Identifier = Name;
    type LocalName = Name;
    type NamespaceUrl = Name;
    type NamespacePrefix = Name;
    type BorrowedNamespaceUrl = Name;
    type BorrowedLocalName = Name;
    type NonTSPseudoClass = PseudoClass;
    type VendorPrefix = NoPrefix;
    type PseudoElement = PseudoElement;
}

/// Every type is owned already: `into_owned` only changes the lifetime the
/// parsed selector names.
macro_rules! owned {
    ($($ty:ty),*) => {$(
        impl<'any> IntoOwned<'any> for $ty {
            type Owned = Self;
            fn into_owned(self) -> Self {
                self
            }
        }
    )*};
}
owned!(Impl, Name, Value, PseudoClass, PseudoElement, NoPrefix);

/// A parsed selector.
pub(crate) type Selector = parser::Selector<'static, Impl>;
/// A rule's parsed selectors.
pub(crate) type SelectorList = parcel_selectors::SelectorList<'static, Impl>;
/// A selector component (the allowlist walks them).
pub(crate) type Component = parser::Component<'static, Impl>;

/// What parcel_selectors parses for bevy_markup.
struct SelectorParser;

impl<'i> parser::Parser<'i> for SelectorParser {
    type Impl = Impl;
    type Error = CssError<'i>;

    fn parse_slotted(&self) -> bool {
        true
    }

    fn parse_part(&self) -> bool {
        true
    }

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_host(&self) -> bool {
        true
    }

    fn is_nesting_allowed(&self) -> bool {
        true
    }

    fn parse_non_ts_pseudo_class(
        &self,
        _location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<PseudoClass, ParseError<'i, CssError<'i>>> {
        Ok(pseudo_class(&name))
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: CowRcStr<'i>,
        arguments: &mut CssParser<'i, 't>,
    ) -> Result<PseudoClass, ParseError<'i, CssError<'i>>> {
        while arguments.next().is_ok() {}
        Ok(PseudoClass::Other(name.as_ref().to_ascii_lowercase()))
    }

    fn parse_pseudo_element(
        &self,
        _location: SourceLocation,
        name: CowRcStr<'i>,
    ) -> Result<PseudoElement, ParseError<'i, CssError<'i>>> {
        Ok(PseudoElement(name.as_ref().to_ascii_lowercase()))
    }

    fn parse_functional_pseudo_element<'t>(
        &self,
        name: CowRcStr<'i>,
        arguments: &mut CssParser<'i, 't>,
    ) -> Result<PseudoElement, ParseError<'i, CssError<'i>>> {
        while arguments.next().is_ok() {}
        Ok(PseudoElement(name.as_ref().to_ascii_lowercase()))
    }
}

/// The interaction pseudo-classes the cascade matches, by name (ASCII
/// case-insensitive, as CSS names are).
fn pseudo_class(name: &str) -> PseudoClass {
    match name.to_ascii_lowercase().as_str() {
        "hover" => PseudoClass::Hover,
        "active" => PseudoClass::Active,
        "focus" => PseudoClass::Focus,
        "focus-visible" => PseudoClass::FocusVisible,
        "focus-within" => PseudoClass::FocusWithin,
        other => PseudoClass::Other(other.to_owned()),
    }
}

/// A rule's selector list (the prelude of a style rule). An invalid
/// selector fails the list, as in CSS.
pub(crate) fn parse_list<'i>(input: &mut CssParser<'i, '_>) -> Result<SelectorList, Error<'i>> {
    let list = parcel_selectors::SelectorList::parse(
        &SelectorParser,
        input,
        ParseErrorRecovery::DiscardList,
        NestingRequirement::None,
    )?;
    Ok(list.into_owned())
}
