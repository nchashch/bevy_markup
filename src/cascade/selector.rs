//! Selector matching (ADR 0034, 0050). bevy_markup's CSS parser parses
//! each rule's selectors (`css::select`); [`Selector::from_parsed`] keeps
//! the ones the cascade supports (the allowlist) and records what they
//! read; matching runs right to left over [`Node`], a view of the build
//! walk's own structures: the element, its ancestor chain (nearest first)
//! and the shared sibling lists.

use std::collections::BTreeMap;
use std::fmt;

use super::{Ancestor, ElementState, Position, Pseudo, SelectorReads, SiblingList, Siblings};
use std::sync::Arc;

use crate::css::select::{Attribute, AttributeOperator, Combinator, Nth, PseudoClass, Simple};
use crate::html::HtmlElement;

/// A parsed selector.
type Parsed = crate::css::select::Selector;

/// A selector the cascade supports, with what it reads.
#[derive(Clone, Debug)]
pub(super) struct Selector {
    /// Shared with the sheet: keeping it costs a reference count.
    parsed: Arc<Parsed>,
    specificity: (u32, u32, u32),
    /// An interaction pseudo-class anywhere (ADR 0029's pseudo rules).
    pseudo: bool,
    reads: SelectorReads,
    /// `X ~ subject` where `X` is the only other compound and reads only
    /// the sibling itself: its sibling test is answered from a scan cached
    /// on the sibling list ([`SiblingList::any_before`]) instead of a walk
    /// back per element, O(n²) over a list that has no match (review_0005
    /// I03).
    later_sibling: bool,
    /// Identifies the selector in sibling-list caches.
    id: u64,
    /// `:is()`/`:where()`/`:not()` arguments holding a combinator: each
    /// costs depth² per element tested ([`MAX_COMBINATOR_ARGUMENTS`]).
    combinator_arguments: u32,
}

/// The `:is()`/`:where()`/`:not()` arguments holding a combinator a
/// stylesheet may use, in all its rules. An argument is matched by a call
/// of its own, from every ancestor the enclosing selector tries, so each
/// costs depth² per element (≈1.5 ms per build over 95 nested elements);
/// past the cap a rule is skipped (bug_0235: 400 rules took 0.6 s per
/// build where plain ones took 33 ms).
pub(crate) const MAX_COMBINATOR_ARGUMENTS: u32 = 64;

/// Selector ids for [`Selector::id`].
static NEXT_SELECTOR_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Selector {
    /// `None` for a selector outside the supported grammar: its rule is
    /// skipped (logged by the caller).
    pub(super) fn from_parsed(parsed: &Arc<Parsed>) -> Option<Self> {
        let mut reads = SelectorReads::default();
        let mut pseudo = false;
        let mut combinator_arguments = 0;
        let mut x_reads_context = false;
        // Which compound (0: the subject) the parts belong to.
        let mut compound = 0;
        for part in &parsed.parts {
            if let Simple::Combinator(combinator) = part {
                compound += 1;
                match combinator {
                    Combinator::Descendant | Combinator::Child => {}
                    // ADR 0028's limit: a sibling combinator only next to
                    // the subject.
                    Combinator::NextSibling if compound == 1 => reads.adjacent_siblings = true,
                    Combinator::LaterSibling if compound == 1 => reads.general_siblings = true,
                    Combinator::NextSibling | Combinator::LaterSibling => return None,
                }
                continue;
            }
            let subject = compound == 0;
            let part = compound_part(part, false, 0)?;
            reads.dataset |= part.attributes;
            if part.structural {
                if subject {
                    reads.subject_position = true;
                } else {
                    reads.context_position = true;
                }
            }
            reads.context_position |= part.context_structural;
            x_reads_context |= !subject && part.context_structural;
            pseudo |= part.pseudo;
            combinator_arguments += part.combinator_arguments;
        }
        let mut combinators = parsed.combinators();
        let later_sibling = combinators.next() == Some(Combinator::LaterSibling)
            && combinators.next().is_none()
            && !x_reads_context;
        Some(Self {
            parsed: parsed.clone(),
            specificity: parsed.specificity(),
            pseudo,
            reads,
            later_sibling,
            id: NEXT_SELECTOR_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            combinator_arguments,
        })
    }

    /// See [`MAX_COMBINATOR_ARGUMENTS`].
    pub(super) fn combinator_arguments(&self) -> u32 {
        self.combinator_arguments
    }

    /// Parses `text` as one selector (tests).
    #[cfg(test)]
    pub(super) fn parse(text: &str) -> Option<Self> {
        let css = format!("{text} {{}}");
        let sheet = crate::css::sheet::parse_sheet(&css).ok()?;
        match sheet.rules.first()?.selectors.0.as_slice() {
            [only] => Self::from_parsed(only),
            _ => None,
        }
    }

    pub(super) fn specificity(&self) -> (u32, u32, u32) {
        self.specificity
    }

    pub(super) fn has_pseudo(&self) -> bool {
        self.pseudo
    }

    pub(super) fn reads(&self) -> SelectorReads {
        self.reads
    }

    /// `element` (with its `pseudo` and structural state) and its
    /// `ancestors` (nearest first) against the selector.
    pub(super) fn matches(
        &self,
        element: &HtmlElement,
        pseudo: Pseudo,
        ancestors: &[Ancestor],
        state: &ElementState,
    ) -> bool {
        self.run(element, pseudo, ancestors, state, false)
    }

    /// Whether the selector could match if every interaction test went
    /// either way (ADR 0029's reactivity record). An interaction
    /// pseudo-class answers whatever helps the match: true outside a
    /// `:not()`, false inside one. The allowlist admits no `:not()` inside
    /// another (through `:is()` neither), so that is exact.
    pub(super) fn matches_ignoring_pseudo(
        &self,
        element: &HtmlElement,
        ancestors: &[Ancestor],
        state: &ElementState,
    ) -> bool {
        self.run(element, Pseudo::default(), ancestors, state, true)
    }

    fn run(
        &self,
        element: &HtmlElement,
        pseudo: Pseudo,
        ancestors: &[Ancestor],
        state: &ElementState,
        wild: bool,
    ) -> bool {
        let scope = Scope {
            element,
            pseudo,
            ancestors,
            state,
            wild,
        };
        let node = Node {
            scope: &scope,
            kind: Kind::Subject,
        };
        if let (true, Some(siblings)) = (self.later_sibling, &state.previous_siblings) {
            // The subject's own compound, then "some preceding sibling
            // matches `X`" from the list's cached scan.
            let mut compounds = self.parsed.compounds();
            let (Some(subject), Some(x)) = (compounds.next(), compounds.next()) else {
                return false;
            };
            if !compound_matches(subject, &node, false) {
                return false;
            }
            let list = &siblings.list;
            return list.any_before(siblings.count, (self.id, wild), |index| {
                let sibling = node.with(Kind::Listed {
                    list,
                    index,
                    level: 0,
                });
                compound_matches(x, &sibling, false)
            });
        }
        matches_selector(&self.parsed.parts, &node, false)
    }
}

/// What one compound part reads.
#[derive(Clone, Copy, Default)]
struct PartReads {
    attributes: bool,
    /// A position (or `:focus-within`) of the element the part tests.
    structural: bool,
    /// An interaction pseudo-class (ADR 0029's pseudo rules).
    pseudo: bool,
    /// A position of an ancestor: a part after a combinator inside a
    /// `:is()`/`:where()`/`:not()` argument.
    context_structural: bool,
    /// Arguments holding a combinator ([`MAX_COMBINATOR_ARGUMENTS`]).
    combinator_arguments: u32,
}

impl PartReads {
    fn or(self, other: Self) -> Self {
        Self {
            attributes: self.attributes | other.attributes,
            structural: self.structural | other.structural,
            pseudo: self.pseudo | other.pseudo,
            context_structural: self.context_structural | other.context_structural,
            combinator_arguments: self.combinator_arguments + other.combinator_arguments,
        }
    }
}

/// One simple selector against the allowlist: `None` when unsupported.
/// `negated`: inside a `:not()`, which may not hold another (ADR 0029's
/// reactivity record answers by negation parity, exact at one level).
/// `depth`: how many `:is()`/`:where()`/`:not()` arguments hold it (0 at
/// the selector's top level).
fn compound_part(part: &Simple, negated: bool, depth: u32) -> Option<PartReads> {
    let reads = |attributes, structural, pseudo| PartReads {
        attributes,
        structural,
        pseudo,
        ..PartReads::default()
    };
    Some(match part {
        Simple::Type(_) | Simple::Universal | Simple::Id(_) | Simple::Class(_) => {
            PartReads::default()
        }
        // Any operator and case flag on a `data-*` attribute (the element
        // model carries only those; other attributes skip the rule).
        Simple::Attribute(Attribute { name, .. }) => {
            data_attribute(name)?;
            reads(true, false, false)
        }
        Simple::Pseudo(PseudoClass::FocusWithin) => reads(false, true, false),
        Simple::Pseudo(_) => reads(false, false, true),
        Simple::Nth(_) | Simple::Empty => reads(false, true, false),
        Simple::Not(list) if !negated => nested(list, true, depth + 1)?,
        Simple::Not(_) => return None,
        Simple::Is(list) | Simple::Where(list) => nested(list, negated, depth + 1)?,
        Simple::Unsupported { .. } | Simple::Combinator(_) => return None,
    })
}

/// The selectors of a `:not()`/`:is()`/`:where()` argument: descendant and
/// child combinators only (a sibling combinator would need invalidation
/// the cascade doesn't have, ADR 0034), and only in an argument at the
/// first level (`depth` 1). An argument is matched by a call of its own
/// whose failure is local, so the enclosing walk calls it again at every
/// ancestor: an argument with a combinator inside another costs
/// depth^levels (bug_0213: six levels of `:is(… div) div`, 104 bytes of
/// CSS over 95 nested `<div>`s, took two minutes). Nested lists of
/// compounds (`:is(.a, :not(.b))`) stay free.
fn nested(list: &[Parsed], negated: bool, depth: u32) -> Option<PartReads> {
    let mut reads = PartReads::default();
    for selector in list {
        let mut combinators = selector.combinators().peekable();
        if combinators.peek().is_some() {
            if depth != 1
                || combinators.any(|c| !matches!(c, Combinator::Descendant | Combinator::Child))
            {
                return None;
            }
            reads.combinator_arguments += 1;
        }
        // The argument's own subject compound tests the same element as
        // the compound holding it; parts past a combinator, its ancestors.
        let mut subject = true;
        for part in &selector.parts {
            if let Simple::Combinator(_) = part {
                subject = false;
                continue;
            }
            let mut part = compound_part(part, negated, depth)?;
            if !subject {
                part.context_structural |= part.structural;
                part.structural = false;
            }
            reads = reads.or(part);
        }
    }
    Some(reads)
}

/// The dataset key of a `data-*` attribute name (the element model
/// carries only those).
fn data_attribute(name: &str) -> Option<&str> {
    name.strip_prefix("data-").filter(|key| !key.is_empty())
}

/// How a match attempt from one element failed (Servo's
/// `SelectorMatchingResult`): where the walk may retry. Without it, a
/// selector with several descendant combinators backtracks over every
/// combination of ancestors.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Matched,
    /// Try the next candidate of the nearest `~` to the right.
    RestartFromLaterSibling,
    /// Try the next candidate of the nearest descendant combinator.
    RestartFromDescendant,
    /// No candidate further up can match: stop.
    NotMatchedGlobally,
}

/// A selector's parts (match order) against `node`. `negated`: inside a
/// `:not()`.
fn matches_selector(parts: &[Simple], node: &Node<'_>, negated: bool) -> bool {
    matches_from(parts, node, negated) == Outcome::Matched
}

/// `parts` (a selector's tail, from a compound on) with its first compound
/// at `node`.
fn matches_from(parts: &[Simple], node: &Node<'_>, negated: bool) -> Outcome {
    let end = parts
        .iter()
        .position(|part| matches!(part, Simple::Combinator(_)))
        .unwrap_or(parts.len());
    if !compound_matches(&parts[..end], node, negated) {
        return Outcome::RestartFromLaterSibling;
    }
    let Some(Simple::Combinator(combinator)) = parts.get(end) else {
        return Outcome::Matched;
    };
    let (combinator, rest) = (*combinator, &parts[end + 1..]);
    fn next<'a>(node: &Node<'a>, combinator: Combinator) -> Option<Node<'a>> {
        match combinator {
            Combinator::Descendant | Combinator::Child => node.parent(),
            Combinator::NextSibling | Combinator::LaterSibling => node.sibling(true),
        }
    }
    let none_left = match combinator {
        Combinator::NextSibling | Combinator::LaterSibling => Outcome::RestartFromDescendant,
        Combinator::Descendant | Combinator::Child => Outcome::NotMatchedGlobally,
    };
    let mut candidate = next(node, combinator);
    loop {
        let Some(element) = candidate else {
            return none_left;
        };
        let outcome = matches_from(rest, &element, negated);
        match (outcome, combinator) {
            (Outcome::Matched | Outcome::NotMatchedGlobally, _) | (_, Combinator::NextSibling) => {
                return outcome;
            }
            (_, Combinator::Child) => return Outcome::RestartFromDescendant,
            (Outcome::RestartFromDescendant, Combinator::LaterSibling) => return outcome,
            _ => {}
        }
        candidate = next(&element, combinator);
    }
}

/// Every simple selector of a compound at `node`.
fn compound_matches(compound: &[Simple], node: &Node<'_>, negated: bool) -> bool {
    compound
        .iter()
        .all(|part| simple_matches(part, node, negated))
}

fn simple_matches(part: &Simple, node: &Node<'_>, negated: bool) -> bool {
    // One field per test (a whole `View` per simple selector was a copy
    // the optimizer kept: +6% on plain sheets).
    let state = |actual: bool| {
        if node.scope.wild { !negated } else { actual }
    };
    match part {
        Simple::Type(name) => node.tag() == name,
        Simple::Universal => true,
        Simple::Id(id) => node.id() == Some(id.as_str()),
        Simple::Class(class) => node.classes().iter().any(|own| own == class),
        Simple::Attribute(attribute) => attribute_matches(attribute, node.dataset()),
        Simple::Pseudo(PseudoClass::Hover) => state(node.pseudo().hover),
        Simple::Pseudo(PseudoClass::Active) => state(node.pseudo().active),
        Simple::Pseudo(PseudoClass::Focus) => state(node.pseudo().focus),
        Simple::Pseudo(PseudoClass::FocusVisible) => state(node.pseudo().focus_visible),
        Simple::Pseudo(PseudoClass::FocusWithin) => node.position().focus_within,
        Simple::Nth(nth) => nth_matches(nth, node),
        Simple::Empty => node.position().empty,
        Simple::Not(list) => !list
            .iter()
            .any(|selector| matches_selector(&selector.parts, node, true)),
        Simple::Is(list) | Simple::Where(list) => list
            .iter()
            .any(|selector| matches_selector(&selector.parts, node, negated)),
        Simple::Unsupported { .. } | Simple::Combinator(_) => false,
    }
}

fn attribute_matches(attribute: &Attribute, dataset: Option<&BTreeMap<String, String>>) -> bool {
    let Some(value) = data_attribute(&attribute.name)
        .and_then(|key| dataset.and_then(|dataset| dataset.get(key)))
    else {
        return false;
    };
    let Some(test) = &attribute.test else {
        return true;
    };
    if test.never {
        return false;
    }
    let expected = test.value.as_str();
    let fold = test.case_insensitive;
    let eq = |a: &str, b: &str| {
        if fold {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let starts = |text: &str, prefix: &str| {
        text.len() >= prefix.len()
            && text.is_char_boundary(prefix.len())
            && eq(&text[..prefix.len()], prefix)
    };
    let ends = |text: &str, suffix: &str| {
        let at = text.len().wrapping_sub(suffix.len());
        text.len() >= suffix.len() && text.is_char_boundary(at) && eq(&text[at..], suffix)
    };
    match test.operator {
        AttributeOperator::Equal => eq(value, expected),
        AttributeOperator::Includes => value
            .split([' ', '\t', '\n', '\r', '\x0C'])
            .any(|word| eq(word, expected)),
        AttributeOperator::DashMatch => {
            eq(value, expected)
                || (starts(value, expected) && value.as_bytes().get(expected.len()) == Some(&b'-'))
        }
        AttributeOperator::Prefix => starts(value, expected),
        AttributeOperator::Suffix => ends(value, expected),
        AttributeOperator::Substring => {
            if fold {
                value
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase())
            } else {
                value.contains(expected)
            }
        }
    }
}

/// `An+B` at `node`'s 1-based index among its siblings (or those of its
/// type), counted from the start or the end. The arithmetic is exact
/// (`i64`): parcel_selectors' checked `i32` answered "no" past `i32`'s
/// range (`:nth-child(n-2147483647)` matched nothing; CSS: everything).
fn nth_matches(nth: &Nth, node: &Node<'_>) -> bool {
    let index = |from_end| node.nth_index(nth.of_type, from_end);
    if nth.only {
        return index(false) == 1 && index(true) == 1;
    }
    let index = index(nth.from_end) as i64;
    let (a, b) = (i64::from(nth.a), i64::from(nth.b));
    if a == 0 {
        return index == b;
    }
    let steps = index - b;
    steps % a == 0 && steps / a >= 0
}

/// What one match runs against.
struct Scope<'a> {
    element: &'a HtmlElement,
    pseudo: Pseudo,
    ancestors: &'a [Ancestor],
    state: &'a ElementState,
    wild: bool,
}

/// An element as parcel's matcher walks it.
#[derive(Clone, Copy)]
pub(super) struct Node<'a> {
    scope: &'a Scope<'a>,
    kind: Kind<'a>,
}

/// Where a [`Node`] sits. `level`: the node's parent is
/// `ancestors[level]` (none past the chain).
#[derive(Clone, Copy, Debug)]
enum Kind<'a> {
    /// The element being styled.
    Subject,
    /// `ancestors[i]`.
    Chain(usize),
    /// An entry of a shared sibling list.
    Listed {
        list: &'a SiblingList,
        index: usize,
        level: usize,
    },
    /// A stand-in sibling where no list is threaded (tests, image runs):
    /// the counts come from the element's [`Position`], so positional
    /// matches agree with it. `same`: of the element's type. `rest` /
    /// `rest_same`: how many more stand-ins follow in this direction.
    Phantom {
        level: usize,
        tag: &'a str,
        same: bool,
        backward: bool,
        rest: usize,
        rest_same: usize,
    },
}

/// A stand-in's tag when it isn't of the element's type: matches no type
/// selector and no real tag.
const OTHER_TYPE: &str = "\0";

impl<'a> Node<'a> {
    fn with(&self, kind: Kind<'a>) -> Self {
        Self {
            scope: self.scope,
            kind,
        }
    }

    /// For the subject and chain nodes: the threaded sibling view (the
    /// list and the node's index in it), its position, its level.
    fn placement(&self) -> Option<(Option<&'a Siblings>, Position, usize)> {
        let scope = self.scope;
        match self.kind {
            Kind::Subject => Some((
                scope.state.previous_siblings.as_ref(),
                scope.state.position(),
                0,
            )),
            Kind::Chain(index) => {
                let ancestor = &scope.ancestors[index];
                Some((ancestor.link.0.as_ref(), ancestor.position, index + 1))
            }
            _ => None,
        }
    }

    fn phantom(&self, level: usize, backward: bool, total: usize, same: usize) -> Option<Self> {
        if total == 0 {
            return None;
        }
        let is_same = same > 0;
        Some(self.with(Kind::Phantom {
            level,
            tag: self.tag(),
            same: is_same,
            backward,
            rest: total - 1,
            rest_same: same - usize::from(is_same),
        }))
    }

    fn sibling(&self, backward: bool) -> Option<Self> {
        if let Some((siblings, position, level)) = self.placement() {
            return match siblings {
                Some(view) if backward => {
                    let index = view.count.checked_sub(1)?;
                    Some(self.with(Kind::Listed {
                        list: &view.list,
                        index,
                        level,
                    }))
                }
                // The list holds the node itself: the next entry follows it.
                Some(view) if view.list.list.len() > view.count => {
                    let index = view.count + 1;
                    (index < view.list.list.len()).then(|| {
                        self.with(Kind::Listed {
                            list: &view.list,
                            index,
                            level,
                        })
                    })
                }
                _ if backward => self.phantom(
                    level,
                    true,
                    position.tag_index.saturating_sub(1),
                    position.type_index.saturating_sub(1),
                ),
                _ => self.phantom(
                    level,
                    false,
                    position.tag_count.saturating_sub(position.tag_index),
                    position.type_count.saturating_sub(position.type_index),
                ),
            };
        }
        match self.kind {
            Kind::Listed { list, index, level } => {
                let index = if backward {
                    index.checked_sub(1)?
                } else {
                    index + 1
                };
                (index < list.list.len()).then(|| self.with(Kind::Listed { list, index, level }))
            }
            Kind::Phantom {
                level,
                tag,
                backward: direction,
                rest,
                rest_same,
                ..
            } if direction == backward => {
                if rest == 0 {
                    return None;
                }
                let same = rest_same > 0;
                Some(self.with(Kind::Phantom {
                    level,
                    tag,
                    same,
                    backward,
                    rest: rest - 1,
                    rest_same: rest_same - usize::from(same),
                }))
            }
            _ => None,
        }
    }

    fn level(&self) -> usize {
        match self.kind {
            Kind::Subject => 0,
            Kind::Chain(index) => index + 1,
            Kind::Listed { level, .. } | Kind::Phantom { level, .. } => level,
        }
    }
}

impl fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({:?})", self.kind)
    }
}

impl<'a> Node<'a> {
    /// The element, ancestor or sibling list entry a non-stand-in node is.
    #[inline]
    fn entry(&self) -> Option<&'a Ancestor> {
        match self.kind {
            Kind::Subject | Kind::Phantom { .. } => None,
            Kind::Chain(index) => Some(&self.scope.ancestors[index]),
            Kind::Listed { list, index, .. } => Some(&list.list[index]),
        }
    }

    #[inline]
    fn tag(&self) -> &'a str {
        match self.kind {
            Kind::Subject => &self.scope.element.tag,
            Kind::Phantom { tag, same, .. } => {
                if same {
                    tag
                } else {
                    OTHER_TYPE
                }
            }
            Kind::Chain(index) => &self.scope.ancestors[index].tag,
            Kind::Listed { list, index, .. } => &list.list[index].tag,
        }
    }

    #[inline]
    fn id(&self) -> Option<&'a str> {
        match self.kind {
            Kind::Subject => self.scope.element.id.as_deref(),
            Kind::Phantom { .. } => None,
            _ => self.entry().and_then(|entry| entry.id.as_deref()),
        }
    }

    #[inline]
    fn classes(&self) -> &'a [String] {
        match self.kind {
            Kind::Subject => &self.scope.element.classes,
            Kind::Phantom { .. } => &[],
            _ => self.entry().map_or(&[], |entry| &entry.classes),
        }
    }

    #[inline]
    fn pseudo(&self) -> Pseudo {
        match self.kind {
            Kind::Subject => self.scope.pseudo,
            Kind::Phantom { .. } => Pseudo::default(),
            _ => self
                .entry()
                .map_or_else(Pseudo::default, |entry| entry.pseudo),
        }
    }

    #[inline]
    fn dataset(&self) -> Option<&'a BTreeMap<String, String>> {
        match self.kind {
            Kind::Subject => Some(&self.scope.element.dataset),
            Kind::Phantom { .. } => None,
            _ => self.entry().and_then(|entry| entry.dataset.as_deref()),
        }
    }

    #[inline]
    fn position(&self) -> Position {
        match self.kind {
            Kind::Subject => self.scope.state.position(),
            Kind::Phantom { .. } => Position::default(),
            _ => self
                .entry()
                .map_or_else(Position::default, |entry| entry.position),
        }
    }

    fn parent(&self) -> Option<Self> {
        let level = self.level();
        (level < self.scope.ancestors.len()).then(|| self.with(Kind::Chain(level)))
    }

    /// The node's 1-based index among its siblings (`of_type`: those of
    /// its tag), from the start or the end: from its [`Position`] where it
    /// has one, else by walking its stand-in siblings (as parcel did).
    fn nth_index(&self, of_type: bool, from_end: bool) -> usize {
        if !matches!(self.kind, Kind::Phantom { .. }) {
            let position = self.position();
            // As a walk would count: an unset position (0) is the first.
            return match (of_type, from_end) {
                (false, false) => position.tag_index.max(1),
                (false, true) => {
                    position.tag_count + 1 - position.tag_index.min(position.tag_count)
                }
                (true, false) => position.type_index.max(1),
                (true, true) => {
                    position.type_count + 1 - position.type_index.min(position.type_count)
                }
            };
        }
        let tag = self.tag();
        let mut index = 1;
        let mut current = *self;
        while let Some(sibling) = current.sibling(!from_end) {
            if !of_type || sibling.tag() == tag {
                index += 1;
            }
            current = sibling;
        }
        index
    }
}
