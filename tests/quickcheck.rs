//! Property tests over structured input (`quickcheck`'s strength): derive
//! the input shape by hand, let quickcheck generate/shrink it.
//!
//! Unlike `properties.rs` (metamorphic, proptest), these check the pipeline
//! against an independent reference model of CSS precedence, and against a
//! round trip of arbitrary text through Tera's autoescaping.
//!
//! Observation is the usual dump; `TestUi` (see `common`) is the harness.

mod common;
use common::TestUi;

use bevy::prelude::*;
use p23::prelude::*;
use quickcheck::{Arbitrary, Gen};
use quickcheck_macros::quickcheck;

const BASE_CSS: &str = "html { color: #ffffff; font-size: 20px }";

// ---------------------------------------------------------------------------
// Cascade winner vs a reference precedence model
// ---------------------------------------------------------------------------

/// A `<p>` target element: optional `id="the-id"`, classes `c0`… (`n_classes`).
#[derive(Clone, Debug)]
struct Element {
    has_id: bool,
    n_classes: u8,
}

/// One CSS rule that may or may not match the element.
///
/// `ids`: 0 = no id part, 1 = `#the-id` (matches iff the element has the id),
/// 2 = `#ghost` (never matches). Class parts are `.c0`/`.c1`/`.c2`; the
/// element never has `c2`, so those rules never match either.
#[derive(Clone, Debug)]
struct Rule {
    tag: bool,
    ids: u8,
    classes: Vec<u8>,
    important: bool,
    color: [u8; 3],
}

#[derive(Clone, Debug)]
struct CascadeCase {
    element: Element,
    rules: Vec<Rule>,
}

impl Arbitrary for Rule {
    fn arbitrary(g: &mut Gen) -> Self {
        // `Gen`'s RNG is private; build everything from `arbitrary` + `choose`.
        let ids = *g.choose(&[0u8, 1, 2]).unwrap();
        let n_classes = *g.choose(&[0usize, 1, 2]).unwrap();
        Self {
            tag: bool::arbitrary(g),
            ids,
            classes: (0..n_classes).map(|_| *g.choose(&[0u8, 1, 2]).unwrap()).collect(),
            important: bool::arbitrary(g),
            color: [u8::arbitrary(g), u8::arbitrary(g), u8::arbitrary(g)],
        }
    }
}

impl Arbitrary for CascadeCase {
    fn arbitrary(g: &mut Gen) -> Self {
        let n_rules = *g.choose(&[1usize, 2, 3, 4, 5, 6]).unwrap();
        Self {
            element: Element {
                has_id: bool::arbitrary(g),
                n_classes: *g.choose(&[0u8, 1, 2]).unwrap(),
            },
            rules: (0..n_rules).map(|_| Rule::arbitrary(g)).collect(),
        }
    }
}

fn rule_matches(rule: &Rule, element: &Element) -> bool {
    let id_ok = match rule.ids {
        0 => true,
        1 => element.has_id,
        _ => false,
    };
    id_ok && rule.classes.iter().all(|&class| class < element.n_classes)
}

/// (ids, classes, type), the CSS specificity tuple.
fn specificity(rule: &Rule) -> (u8, u8, u8) {
    ((rule.ids > 0) as u8, rule.classes.len() as u8, rule.tag as u8)
}

/// The winning color per CSS precedence: importance, then specificity, then
/// source order; no match = inherited `html` color.
fn expected_color(case: &CascadeCase) -> [u8; 3] {
    case.rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| rule_matches(rule, &case.element))
        .max_by_key(|(index, rule)| (rule.important, specificity(rule), *index))
        .map_or([0xff, 0xff, 0xff], |(_, rule)| rule.color)
}

fn rule_css(rule: &Rule) -> String {
    let mut selector = String::new();
    if rule.tag {
        selector.push_str("p");
    } else {
        selector.push('*');
    }
    if rule.ids > 0 {
        selector.push_str(if rule.ids == 1 { "#the-id" } else { "#ghost" });
    }
    for class in &rule.classes {
        selector.push_str(&format!(".c{class}"));
    }
    let bang = if rule.important { " !important" } else { "" };
    format!("{} {{ color: #{:02x}{:02x}{:02x}{} }} ", selector, rule.color[0], rule.color[1], rule.color[2], bang)
}

/// For any stylesheet of compound rules, the color p23 puts on the target
/// `<p>` is the one CSS precedence picks.
#[quickcheck]
fn cascade_winner_matches_precedence_model(case: CascadeCase) {
    let element = &case.element;
    let id_attr = if element.has_id { " id=\"the-id\"" } else { "" };
    let classes: Vec<String> = (0..element.n_classes).map(|c| format!("c{c}")).collect();
    let page = format!("<p{id_attr} class=\"{}\">Text</p>", classes.join(" "));
    let css = format!("{BASE_CSS} {}", case.rules.iter().map(rule_css).collect::<String>());

    let mut ui = TestUi::new("qc-cascade", &[("page.html", &page), ("style.css", &css)])
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
    let color = expected_color(&case);
    let expected = format!("\"Text\" default 20px #{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let dump = ui.settle().dump();
    assert!(
        dump.contains(&expected),
        "expected run {expected:?} (winner of {case:#?})\nCSS:\n{css}\n--- dump ---\n{dump}"
    );
}

// ---------------------------------------------------------------------------
// Arbitrary text round trips through the template
// ---------------------------------------------------------------------------

/// Printable, non-whitespace ASCII — enough to hit every escape Tera emits
/// (`<`, `>`, `&`, `"`, `'`) without whitespace collapsing changing the text.
#[derive(Clone, Debug)]
struct SafeText(String);

const SAFE: &[u8] = b"abcXYZ0123456789.,:;!?()[]{}<>+-*/=#%&\"'\\_|~^$@`";

impl Arbitrary for SafeText {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = *g.choose(&[1usize, 5, 10, 20, 40]).unwrap();
        let chars: String = (0..len)
            .map(|_| *g.choose(SAFE).unwrap() as char)
            .collect();
        Self(chars)
    }
}

/// Whatever a context variable holds, `{{ s }}` renders it back verbatim:
/// Tera's autoescaping and `decode_entities` must cancel exactly.
#[quickcheck]
fn context_text_round_trips_through_template(text: SafeText) {
    let SafeText(s) = &text;
    let page = "<p>{{ s }}</p>";
    let mut ui = TestUi::new("qc-text", &[("page.html", page), ("style.css", BASE_CSS)])
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new().with("s", s), Node::default());
    let expected = format!("{s:?} default 20px #ffffff");
    let dump = ui.settle().dump();
    assert!(
        dump.contains(&expected),
        "text {s:?} did not round trip\n--- dump ---\n{dump}"
    );
}
