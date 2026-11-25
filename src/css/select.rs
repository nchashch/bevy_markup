//! Selectors (ADR 0050): bevy_markup's own parser on cssparser's tokens,
//! into its own types. It accepts the Selectors 4 grammar parcel_selectors
//! accepted (which it replaces), pseudo-classes and pseudo-elements it
//! doesn't support included: those parse to [`Simple::Unsupported`], and
//! the cascade's allowlist (`cascade/selector.rs`) skips their rule rather
//! than failing the sheet. A selector that is invalid CSS fails its list,
//! and so the sheet, as before.

use cssparser::{Parser as CssParser, Token};

use super::{CssError, Error};

/// A rule's selectors, each shared (the cascade keeps them without a
/// copy).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SelectorList(pub(crate) Vec<std::sync::Arc<Selector>>);

/// A complex selector in one allocation, in match order: the subject's
/// compound first, then each [`Simple::Combinator`] followed by the
/// compound to its left in the source (parcel_selectors' layout).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Selector {
    pub(crate) parts: Vec<Simple>,
}

impl Selector {
    /// The compounds, subject first.
    pub(crate) fn compounds(&self) -> impl Iterator<Item = &[Simple]> {
        self.parts
            .split(|part| matches!(part, Simple::Combinator(_)))
    }

    /// The combinators, nearest the subject first.
    pub(crate) fn combinators(&self) -> impl Iterator<Item = Combinator> + '_ {
        self.parts.iter().filter_map(|part| match part {
            Simple::Combinator(combinator) => Some(*combinator),
            _ => None,
        })
    }
}

impl std::fmt::Display for Selector {
    /// Close to the source (the cascade's log names skipped selectors):
    /// supported parts as written, unsupported ones as `:…`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let compounds: Vec<&[Simple]> = self.compounds().collect();
        let combinators: Vec<Combinator> = self.combinators().collect();
        for (index, compound) in compounds.iter().enumerate().rev() {
            for part in *compound {
                part.fmt(f)?;
            }
            if let Some(combinator) = index.checked_sub(1).map(|i| combinators[i]) {
                f.write_str(match combinator {
                    Combinator::Descendant => " ",
                    Combinator::Child => " > ",
                    Combinator::NextSibling => " + ",
                    Combinator::LaterSibling => " ~ ",
                })?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for Simple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |f: &mut std::fmt::Formatter<'_>, name: &str, list: &[Selector]| {
            write!(f, ":{name}(")?;
            for (i, selector) in list.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{selector}")?;
            }
            f.write_str(")")
        };
        match self {
            Simple::Type(name) => f.write_str(name),
            Simple::Universal => f.write_str("*"),
            Simple::Id(id) => write!(f, "#{id}"),
            Simple::Class(class) => write!(f, ".{class}"),
            Simple::Attribute(Attribute { name, test: None }) => write!(f, "[{name}]"),
            Simple::Attribute(Attribute {
                name,
                test: Some(test),
            }) => write!(
                f,
                "[{name}{}{:?}{}]",
                match test.operator {
                    AttributeOperator::Equal => "=",
                    AttributeOperator::Includes => "~=",
                    AttributeOperator::DashMatch => "|=",
                    AttributeOperator::Prefix => "^=",
                    AttributeOperator::Substring => "*=",
                    AttributeOperator::Suffix => "$=",
                },
                test.value,
                if test.case_insensitive { " i" } else { "" }
            ),
            Simple::Pseudo(class) => write!(f, ":{class:?}"),
            Simple::Nth(nth) => write!(f, ":nth({}n+{})", nth.a, nth.b),
            Simple::Empty => f.write_str(":empty"),
            Simple::Not(selectors) => list(f, "not", selectors),
            Simple::Is(selectors) => list(f, "is", selectors),
            Simple::Where(selectors) => list(f, "where", selectors),
            Simple::Unsupported { .. } => f.write_str(":…"),
            Simple::Combinator(_) => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Combinator {
    /// Whitespace.
    Descendant,
    /// `>`.
    Child,
    /// `+`.
    NextSibling,
    /// `~`.
    LaterSibling,
}

/// One simple selector.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Simple {
    /// A type selector, ASCII-lowercased (HTML tags match case-insensitively).
    Type(String),
    /// `*`.
    Universal,
    Id(String),
    Class(String),
    /// An attribute selector with a lowercase name in no namespace.
    Attribute(Attribute),
    Pseudo(PseudoClass),
    /// `:nth-child()` and friends, `:first-child` & co. as their `An+B`.
    Nth(Nth),
    Empty,
    Not(Vec<Selector>),
    Is(Vec<Selector>),
    Where(Vec<Selector>),
    /// Parsed, never matched: the allowlist skips its rule. Pseudo-elements,
    /// unknown pseudo-classes, `:has()`, `:nth-child(… of S)`, `:root`,
    /// `:scope`, `:host`, `::part()`, `::slotted()`, `&`, namespaced or
    /// uppercase-named attributes and namespaced type selectors. `specificity`
    /// is what a browser would count, so lists stay comparable.
    Unsupported {
        specificity: Specificity,
    },
    /// Between two compounds of a [`Selector`] (never inside a compound).
    Combinator(Combinator),
}

/// An attribute selector.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Attribute {
    pub(crate) name: String,
    pub(crate) test: Option<AttributeTest>,
}

/// `[name op value flags]`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AttributeTest {
    pub(crate) operator: AttributeOperator,
    pub(crate) value: String,
    /// The `i` flag.
    pub(crate) case_insensitive: bool,
    /// The value can't match: an empty `^=`/`$=`/`*=` value, or an empty or
    /// whitespace-holding `~=` value (Selectors 4 §6.1, 6.2).
    pub(crate) never: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttributeOperator {
    /// `=`.
    Equal,
    /// `~=`.
    Includes,
    /// `|=`.
    DashMatch,
    /// `^=`.
    Prefix,
    /// `*=`.
    Substring,
    /// `$=`.
    Suffix,
}

/// The interaction pseudo-classes the cascade matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PseudoClass {
    Hover,
    Active,
    Focus,
    FocusVisible,
    FocusWithin,
}

/// A structural pseudo-class: an `An+B` over the element's index counted
/// from the start or the end, among all siblings or those of its type.
/// `only` (`:only-child`/`:only-of-type`) is first and last at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Nth {
    pub(crate) a: i32,
    pub(crate) b: i32,
    pub(crate) of_type: bool,
    pub(crate) from_end: bool,
    pub(crate) only: bool,
}

/// `(ids, classes, types)`, each capped at 1023 as parcel_selectors packed
/// them (a selector with more than a thousand of one kind is absurd; the cap
/// keeps old and new orders equal).
pub(crate) type Specificity = (u32, u32, u32);

const SPECIFICITY_CAP: u32 = 0x3ff;

impl Selector {
    /// Selectors 4 §17, capped per field.
    pub(crate) fn specificity(&self) -> Specificity {
        let raw = self
            .parts
            .iter()
            .map(Simple::specificity)
            .fold((0, 0, 0), add);
        (
            raw.0.min(SPECIFICITY_CAP),
            raw.1.min(SPECIFICITY_CAP),
            raw.2.min(SPECIFICITY_CAP),
        )
    }
}

fn add(a: Specificity, b: Specificity) -> Specificity {
    (
        a.0.saturating_add(b.0),
        a.1.saturating_add(b.1),
        a.2.saturating_add(b.2),
    )
}

fn most_specific(list: &[Selector]) -> Specificity {
    list.iter()
        .map(Selector::specificity)
        .max()
        .unwrap_or_default()
}

impl Simple {
    fn specificity(&self) -> Specificity {
        match self {
            Simple::Type(_) => (0, 0, 1),
            Simple::Universal | Simple::Where(_) | Simple::Combinator(_) => (0, 0, 0),
            Simple::Id(_) => (1, 0, 0),
            Simple::Class(_)
            | Simple::Attribute(_)
            | Simple::Pseudo(_)
            | Simple::Nth(_)
            | Simple::Empty => (0, 1, 0),
            Simple::Not(list) | Simple::Is(list) => most_specific(list),
            Simple::Unsupported { specificity } => *specificity,
        }
    }
}

/// Why a selector failed to parse (the sheet loader's message names it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorError {
    /// Nothing where a compound selector should be.
    Empty,
    /// A combinator with nothing after it.
    DanglingCombinator,
    /// A token that can't start or continue a selector here.
    Unexpected,
    /// A namespace prefix (bevy_markup declares no namespaces).
    UnknownNamespace,
    /// Something not allowed after a pseudo-element, `::part()` or
    /// `::slotted()`, or a pseudo-element inside `:is()`/`:not()` & co.
    InvalidState,
    /// A malformed attribute selector.
    BadAttribute,
    /// A malformed `An+B`.
    BadNth,
}

type Parsed<'i, T> = Result<T, Error<'i>>;

fn fail<'i, T>(input: &CssParser<'i, '_>, error: SelectorError) -> Parsed<'i, T> {
    Err(input.new_custom_error(CssError::Selector(error)))
}

/// Where in a selector the parser is (parcel_selectors'
/// `SelectorParsingState`, for the states bevy_markup's grammar reaches).
#[derive(Clone, Copy, Debug, Default)]
struct State {
    after_part: bool,
    after_slotted: bool,
    /// After a pseudo-element (every one bevy_markup parses is
    /// "non-stateful": no pseudo-class may follow it).
    after_pseudo_element: bool,
    disallow_pseudos: bool,
    disallow_combinators: bool,
}

impl State {
    fn after_pseudo(self) -> bool {
        self.after_part || self.after_slotted || self.after_pseudo_element
    }

    fn allows_pseudos(self) -> bool {
        !(self.after_pseudo_element || self.disallow_pseudos)
    }

    fn allows_part_or_slotted(self) -> bool {
        !(self.after_pseudo() || self.disallow_pseudos)
    }

    fn allows_non_functional_pseudo_classes(self) -> bool {
        !(self.after_slotted || self.after_pseudo_element)
    }

    fn allows_tree_structural(self) -> bool {
        !self.after_pseudo()
    }

    /// The state an argument list (`:is()`, `:not()`, …) starts in.
    fn argument(self) -> Self {
        Self {
            disallow_pseudos: true,
            ..self
        }
    }
}

/// A rule's selector list (the prelude of a style rule). An invalid
/// selector fails the list, as in CSS.
pub(crate) fn parse_list<'i>(input: &mut CssParser<'i, '_>) -> Parsed<'i, SelectorList> {
    let selectors = list(input, State::default(), false, false)?;
    Ok(SelectorList(
        selectors.into_iter().map(std::sync::Arc::new).collect(),
    ))
}

/// Comma-separated selectors. `forgiving`: an invalid selector is dropped
/// instead of failing the list (`:is()`, `:where()`, `:has()`, `of S`).
/// `relative`: each may start with a combinator (`:has()`).
fn list<'i>(
    input: &mut CssParser<'i, '_>,
    state: State,
    forgiving: bool,
    relative: bool,
) -> Parsed<'i, Vec<Selector>> {
    let mut selectors = Vec::new();
    loop {
        let selector = input.parse_until_before(cssparser::Delimiter::Comma, |input| {
            if relative {
                let start = input.state();
                match input.next() {
                    Ok(Token::Delim('>' | '+' | '~')) => {}
                    _ => input.reset(&start),
                }
            }
            complex(input, state)
        });
        match selector {
            Ok(selector) => selectors.push(selector),
            Err(error) if !forgiving => return Err(error),
            Err(_) => {}
        }
        // `parse_until_before` stops before the comma (the rest was consumed
        // or, on an error, is skipped here).
        loop {
            match input.next() {
                Err(_) => return Ok(selectors),
                Ok(Token::Comma) => break,
                Ok(_) => {}
            }
        }
    }
}

/// One complex selector.
fn complex<'i>(input: &mut CssParser<'i, '_>, state: State) -> Parsed<'i, Selector> {
    let mut state = state;
    // Source order, separators between compounds; reordered at the end.
    let mut parts = Vec::new();
    let mut combinators = 0;
    loop {
        if !compound(input, &mut state, &mut parts)? {
            return fail(
                input,
                if combinators == 0 {
                    SelectorError::Empty
                } else {
                    SelectorError::DanglingCombinator
                },
            );
        }
        if state.after_pseudo() {
            // Nothing may follow a pseudo-element, `::part()` or
            // `::slotted()`, a combinator included (whitespace may).
            if input.next().is_ok() {
                return fail(input, SelectorError::InvalidState);
            }
            break;
        }
        let Some(combinator) = next_combinator(input) else {
            break;
        };
        if state.disallow_combinators {
            return fail(input, SelectorError::InvalidState);
        }
        parts.push(Simple::Combinator(combinator));
        combinators += 1;
    }
    // Match order: reversing the whole list reverses the compounds (and
    // each compound's parts, which are put back).
    parts.reverse();
    let mut start = 0;
    for end in 0..=parts.len() {
        if end == parts.len() || matches!(parts[end], Simple::Combinator(_)) {
            parts[start..end].reverse();
            start = end + 1;
        }
    }
    Ok(Selector { parts })
}

/// The combinator after a compound, if one follows (whitespace alone is
/// the descendant combinator when another compound follows it).
fn next_combinator(input: &mut CssParser<'_, '_>) -> Option<Combinator> {
    let mut whitespace = false;
    loop {
        let before = input.state();
        match input.next_including_whitespace() {
            Err(_) => return None,
            Ok(Token::WhiteSpace(_)) => whitespace = true,
            Ok(Token::Delim('>')) => return Some(Combinator::Child),
            Ok(Token::Delim('+')) => return Some(Combinator::NextSibling),
            Ok(Token::Delim('~')) => return Some(Combinator::LaterSibling),
            Ok(_) => {
                input.reset(&before);
                return whitespace.then_some(Combinator::Descendant);
            }
        }
    }
}

/// A compound selector, appended to `parts`; false when nothing
/// selector-like is next.
fn compound<'i>(
    input: &mut CssParser<'i, '_>,
    state: &mut State,
    parts: &mut Vec<Simple>,
) -> Parsed<'i, bool> {
    input.skip_whitespace();
    let before = parts.len();
    if input.try_parse(|input| input.expect_delim('&')).is_ok() {
        parts.push(unsupported((0, 0, 0)));
    }
    if let Some(part) = type_selector(input)? {
        // An argument list inherits the state: no type selector after a
        // pseudo-element, even inside `:not()`.
        if state.after_pseudo() {
            return fail(input, SelectorError::InvalidState);
        }
        parts.push(part);
    }
    while let Some(part) = simple(input, state)? {
        parts.push(part);
    }
    Ok(parts.len() > before)
}

fn unsupported(specificity: Specificity) -> Simple {
    Simple::Unsupported { specificity }
}

/// A type or universal selector, with an optional namespace prefix.
fn type_selector<'i>(input: &mut CssParser<'i, '_>) -> Parsed<'i, Option<Simple>> {
    let start = input.state();
    let token = match input.next_including_whitespace() {
        Ok(token) => token.clone(),
        Err(_) => {
            input.reset(&start);
            return Ok(None);
        }
    };
    match token {
        Token::Ident(name) => {
            let after = input.state();
            if let Ok(Token::Delim('|')) = input.next_including_whitespace() {
                return fail(input, SelectorError::UnknownNamespace);
            }
            input.reset(&after);
            Ok(Some(Simple::Type(name.to_ascii_lowercase())))
        }
        Token::Delim('*') => {
            let after = input.state();
            if let Ok(Token::Delim('|')) = input.next_including_whitespace() {
                // `*|name` / `*|*`: any namespace.
                return namespaced_name(input);
            }
            input.reset(&after);
            Ok(Some(Simple::Universal))
        }
        // `|name` / `|*`: no namespace.
        Token::Delim('|') => namespaced_name(input),
        _ => {
            input.reset(&start);
            Ok(None)
        }
    }
}

/// The name after an explicit namespace prefix: parsed, unsupported. At
/// the end of the input there is no type selector (the prefix stays
/// consumed), as parcel_selectors read it: `&|` is the nesting selector.
fn namespaced_name<'i>(input: &mut CssParser<'i, '_>) -> Parsed<'i, Option<Simple>> {
    match input.next_including_whitespace() {
        Ok(Token::Delim('*')) => Ok(Some(unsupported((0, 0, 0)))),
        Ok(Token::Ident(_)) => Ok(Some(unsupported((0, 0, 1)))),
        Err(_) => Ok(None),
        _ => fail(input, SelectorError::UnknownNamespace),
    }
}

/// One simple selector other than a type selector, or `None` (nothing
/// consumed) when the compound ends here.
fn simple<'i>(input: &mut CssParser<'i, '_>, state: &mut State) -> Parsed<'i, Option<Simple>> {
    let start = input.state();
    let token = match input.next_including_whitespace() {
        Ok(token) => token.clone(),
        Err(_) => {
            input.reset(&start);
            return Ok(None);
        }
    };
    let part = match token {
        Token::IDHash(id) => {
            if state.after_pseudo() {
                return fail(input, SelectorError::InvalidState);
            }
            Simple::Id(id.as_ref().to_owned())
        }
        Token::Delim('.') => {
            if state.after_pseudo() {
                return fail(input, SelectorError::InvalidState);
            }
            match input.next_including_whitespace() {
                Ok(Token::Ident(class)) => Simple::Class(class.as_ref().to_owned()),
                _ => return fail(input, SelectorError::Unexpected),
            }
        }
        Token::SquareBracketBlock => {
            if state.after_pseudo() {
                return fail(input, SelectorError::InvalidState);
            }
            input.parse_nested_block(attribute)?
        }
        Token::Colon => pseudo(input, state)?,
        Token::Delim('&') => unsupported((0, 0, 0)),
        _ => {
            input.reset(&start);
            return Ok(None);
        }
    };
    Ok(Some(part))
}

/// `[name]`, `[name op value]`, `[name op value i|s]`.
fn attribute<'i>(input: &mut CssParser<'i, '_>) -> Parsed<'i, Simple> {
    input.skip_whitespace();
    // The name, with an optional namespace prefix.
    let (name, namespaced) = match input.next_including_whitespace()?.clone() {
        Token::Ident(name) => {
            let after = input.state();
            if let Ok(Token::Delim('|')) = input.next_including_whitespace() {
                return fail(input, SelectorError::UnknownNamespace);
            }
            input.reset(&after);
            (name, false)
        }
        Token::Delim('*') => match input.next_including_whitespace() {
            Ok(Token::Delim('|')) => (attribute_name_after_bar(input)?, true),
            _ => return fail(input, SelectorError::BadAttribute),
        },
        Token::Delim('|') => (attribute_name_after_bar(input)?, false),
        _ => return fail(input, SelectorError::BadAttribute),
    };
    // A namespace, or an uppercase name with a value test: parsed, never
    // matched (parcel_selectors kept those apart; the element model
    // carries lowercase `data-*` attributes only).
    let supported = !namespaced && !name.bytes().any(|byte| byte.is_ascii_uppercase());
    let operator = match input.next() {
        // `[name]`: the allowlist reads the name as written (a `data-`
        // prefix), matching reads it lowercased, as parcel did.
        Err(_) if namespaced => return Ok(unsupported((0, 1, 0))),
        Err(_) => {
            let name = if name.starts_with("data-") {
                name.to_ascii_lowercase()
            } else {
                name.as_ref().to_owned()
            };
            return Ok(Simple::Attribute(Attribute { name, test: None }));
        }
        Ok(Token::Delim('=')) => AttributeOperator::Equal,
        Ok(Token::IncludeMatch) => AttributeOperator::Includes,
        Ok(Token::DashMatch) => AttributeOperator::DashMatch,
        Ok(Token::PrefixMatch) => AttributeOperator::Prefix,
        Ok(Token::SubstringMatch) => AttributeOperator::Substring,
        Ok(Token::SuffixMatch) => AttributeOperator::Suffix,
        Ok(_) => return fail(input, SelectorError::BadAttribute),
    };
    let value = match input.next() {
        Ok(Token::Ident(value) | Token::QuotedString(value)) => value.as_ref().to_owned(),
        _ => return fail(input, SelectorError::BadAttribute),
    };
    let case_insensitive = match input.next() {
        Err(_) => false,
        Ok(Token::Ident(flag)) if flag.eq_ignore_ascii_case("i") => true,
        Ok(Token::Ident(flag)) if flag.eq_ignore_ascii_case("s") => false,
        Ok(_) => return fail(input, SelectorError::BadAttribute),
    };
    if !supported {
        return Ok(unsupported((0, 1, 0)));
    }
    let never = match operator {
        AttributeOperator::Equal | AttributeOperator::DashMatch => false,
        AttributeOperator::Includes => {
            value.is_empty() || value.contains([' ', '\t', '\n', '\r', '\x0C'])
        }
        AttributeOperator::Prefix | AttributeOperator::Substring | AttributeOperator::Suffix => {
            value.is_empty()
        }
    };
    Ok(Simple::Attribute(Attribute {
        name: name.as_ref().to_owned(),
        test: Some(AttributeTest {
            operator,
            value,
            case_insensitive,
            never,
        }),
    }))
}

fn attribute_name_after_bar<'i>(
    input: &mut CssParser<'i, '_>,
) -> Parsed<'i, cssparser::CowRcStr<'i>> {
    match input.next_including_whitespace() {
        Ok(Token::Ident(name)) => Ok(name.clone()),
        _ => fail(input, SelectorError::BadAttribute),
    }
}

/// CSS 2's pseudo-elements, which a single colon also introduces.
fn is_css2_pseudo_element(name: &str) -> bool {
    ["before", "after", "first-line", "first-letter"]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
}

/// After a `:`: a pseudo-class or pseudo-element.
fn pseudo<'i>(input: &mut CssParser<'i, '_>, state: &mut State) -> Parsed<'i, Simple> {
    let (double, token) = match input.next_including_whitespace()?.clone() {
        Token::Colon => (true, input.next_including_whitespace()?.clone()),
        token => (false, token),
    };
    let (name, functional) = match token {
        Token::Ident(name) => (name, false),
        Token::Function(name) => (name, true),
        _ => return fail(input, SelectorError::Unexpected),
    };
    if double || is_css2_pseudo_element(&name) {
        return pseudo_element(input, state, &name, functional);
    }
    if functional {
        return input.parse_nested_block(|input| functional_pseudo_class(input, state, &name));
    }
    if !state.allows_non_functional_pseudo_classes() {
        return fail(input, SelectorError::InvalidState);
    }
    let lower = name.to_ascii_lowercase();
    if state.allows_tree_structural() {
        let nth = |of_type, from_end, only| {
            Simple::Nth(Nth {
                a: 0,
                b: 1,
                of_type,
                from_end,
                only,
            })
        };
        match lower.as_str() {
            "first-child" => return Ok(nth(false, false, false)),
            "last-child" => return Ok(nth(false, true, false)),
            "only-child" => return Ok(nth(false, false, true)),
            "first-of-type" => return Ok(nth(true, false, false)),
            "last-of-type" => return Ok(nth(true, true, false)),
            "only-of-type" => return Ok(nth(true, false, true)),
            "empty" => return Ok(Simple::Empty),
            "root" => return Ok(unsupported((0, 1, 0))),
            "scope" | "host" => return Ok(unsupported((0, 0, 0))),
            _ => {}
        }
    }
    Ok(match lower.as_str() {
        "hover" => Simple::Pseudo(PseudoClass::Hover),
        "active" => Simple::Pseudo(PseudoClass::Active),
        "focus" => Simple::Pseudo(PseudoClass::Focus),
        "focus-visible" => Simple::Pseudo(PseudoClass::FocusVisible),
        "focus-within" => Simple::Pseudo(PseudoClass::FocusWithin),
        _ => unsupported((0, 1, 0)),
    })
}

fn pseudo_element<'i>(
    input: &mut CssParser<'i, '_>,
    state: &mut State,
    name: &str,
    functional: bool,
) -> Parsed<'i, Simple> {
    if !state.allows_pseudos() {
        return fail(input, SelectorError::InvalidState);
    }
    if functional && name.eq_ignore_ascii_case("part") {
        if !state.allows_part_or_slotted() {
            return fail(input, SelectorError::InvalidState);
        }
        input.parse_nested_block(|input| {
            input.expect_ident()?;
            while !input.is_exhausted() {
                input.expect_ident()?;
            }
            Ok::<_, Error<'i>>(())
        })?;
        state.after_part = true;
        return Ok(unsupported((0, 0, 1)));
    }
    if functional && name.eq_ignore_ascii_case("slotted") {
        if !state.allows_part_or_slotted() {
            return fail(input, SelectorError::InvalidState);
        }
        let inner = State {
            disallow_pseudos: true,
            disallow_combinators: true,
            ..*state
        };
        let selector = input.parse_nested_block(|input| complex(input, inner))?;
        state.after_slotted = true;
        return Ok(unsupported(add((0, 0, 1), selector.specificity())));
    }
    if functional {
        input.parse_nested_block(|input| {
            while input.next().is_ok() {}
            Ok::<_, Error<'i>>(())
        })?;
    }
    if state.after_slotted {
        // No pseudo-element bevy_markup parses is valid after `::slotted()`.
        return fail(input, SelectorError::InvalidState);
    }
    state.after_pseudo_element = true;
    Ok(unsupported((0, 0, 1)))
}

fn functional_pseudo_class<'i>(
    input: &mut CssParser<'i, '_>,
    state: &mut State,
    name: &str,
) -> Parsed<'i, Simple> {
    let lower = name.to_ascii_lowercase();
    let nth = |of_type, from_end| (of_type, from_end);
    let kind = match lower.as_str() {
        "nth-child" => Some(nth(false, false)),
        "nth-last-child" => Some(nth(false, true)),
        "nth-of-type" => Some(nth(true, false)),
        "nth-last-of-type" => Some(nth(true, true)),
        _ => None,
    };
    if let Some((of_type, from_end)) = kind {
        if !state.allows_tree_structural() {
            return fail(input, SelectorError::InvalidState);
        }
        let (a, b) = cssparser::parse_nth(input)
            .map_err(|_| input.new_custom_error(CssError::Selector(SelectorError::BadNth)))?;
        if !of_type
            && input
                .try_parse(|input| input.expect_ident_matching("of"))
                .is_ok()
        {
            // `:nth-child(An+B of S)`: parsed, unsupported.
            let selectors = list(input, state.argument(), true, false)?;
            return Ok(unsupported(add((0, 1, 0), most_specific(&selectors))));
        }
        return Ok(Simple::Nth(Nth {
            a,
            b,
            of_type,
            from_end,
            only: false,
        }));
    }
    match lower.as_str() {
        "nth-col" | "nth-last-col" => {
            if !state.allows_tree_structural() {
                return fail(input, SelectorError::InvalidState);
            }
            cssparser::parse_nth(input)
                .map_err(|_| input.new_custom_error(CssError::Selector(SelectorError::BadNth)))?;
            Ok(unsupported((0, 1, 0)))
        }
        "is" => Ok(Simple::Is(list(input, state.argument(), true, false)?)),
        "where" => Ok(Simple::Where(list(input, state.argument(), true, false)?)),
        "not" => Ok(Simple::Not(list(input, state.argument(), false, false)?)),
        "has" => {
            let selectors = list(input, *state, true, true)?;
            Ok(unsupported(most_specific(&selectors)))
        }
        "host" => {
            if !state.allows_tree_structural() {
                return fail(input, SelectorError::InvalidState);
            }
            let inner = State {
                disallow_pseudos: true,
                disallow_combinators: true,
                ..*state
            };
            let selector = complex(input, inner)?;
            Ok(unsupported(add((0, 1, 0), selector.specificity())))
        }
        _ => {
            if state.after_pseudo() {
                return fail(input, SelectorError::InvalidState);
            }
            while input.next().is_ok() {}
            Ok(unsupported((0, 1, 0)))
        }
    }
}
