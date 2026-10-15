//! Selector matching vs a reference model (review_0005 I13).
//!
//! Random small documents (`div`/`p` elements with classes `a`/`b` and an
//! optional `data-m`) × one random rule from the supported selector grammar
//! (ADR 0028): type/`*`, a class, a `data-m` attribute test, `:hover` (nothing
//! is hovered, so it never matches), `:not(:hover)`/`:not(.a)`, the structural
//! pseudo-classes on the subject, and the four combinators with one compound
//! on their left. The rule paints a background; each element's built
//! background must equal the model's "does the selector match" answer.
//!
//! Within the documented limits only: structural pseudo-classes and `:not`
//! appear on the subject, and the sibling combinators sit next to it (ADR
//! 0028). The elements carry no ids and repeat their classes, so elements that
//! differ only in what a selector reads stand side by side (the style cache's
//! key, I06). Found by this property at `e3646ae`: I06, I07, I08.

mod common;
use common::*;

use bevy::prelude::*;
use bevy_markup::prelude::*;
use proptest::prelude::*;

/// One generated element.
#[derive(Clone, Debug)]
struct El {
    tag: &'static str,
    classes: Vec<&'static str>,
    data_m: Option<&'static str>,
    children: Vec<El>,
}

/// A small deterministic generator (xorshift), so a failing seed reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> usize {
        (self.next() % n) as usize
    }
}

fn element(rng: &mut Rng, depth: usize) -> El {
    let tag = if depth >= 2 || rng.below(3) == 0 { "p" } else { "div" };
    let classes = match rng.below(4) {
        0 => vec![],
        1 => vec!["a"],
        2 => vec!["b"],
        _ => vec!["a", "b"],
    };
    let data_m = match rng.below(3) {
        0 => None,
        1 => Some("dark"),
        _ => Some("light"),
    };
    let children = if tag == "div" {
        (0..1 + rng.below(3)).map(|_| element(rng, depth + 1)).collect()
    } else {
        Vec::new()
    };
    El { tag, classes, data_m, children }
}

/// A compound selector's parts.
#[derive(Clone, Debug, Default)]
struct Compound {
    tag: Option<&'static str>,
    class: Option<&'static str>,
    /// `[data-m]` (None value) or `[data-m="…"]`.
    attr: Option<Option<&'static str>>,
    hover: bool,
    not_hover: bool,
    not_class: Option<&'static str>,
    /// `first-child`, `last-child` or `nth-child(2)` (subject only).
    structural: Option<&'static str>,
}

impl Compound {
    fn random(rng: &mut Rng, subject: bool) -> Self {
        let mut compound = Compound {
            tag: [None, Some("div"), Some("p")][rng.below(3)],
            class: [None, Some("a"), Some("b")][rng.below(3)],
            attr: [None, Some(None), Some(Some("dark"))][rng.below(3)],
            hover: rng.below(5) == 0,
            ..Default::default()
        };
        if subject {
            match rng.below(5) {
                0 => compound.not_hover = true,
                1 => compound.not_class = Some("a"),
                _ => {}
            }
            compound.structural = [None, None, Some("first-child"), Some("last-child"), Some("nth-child(2)")]
                [rng.below(5)];
        }
        compound
    }

    fn css(&self) -> String {
        let mut out = self.tag.unwrap_or("").to_owned();
        if let Some(class) = self.class {
            out += &format!(".{class}");
        }
        match self.attr {
            Some(None) => out += "[data-m]",
            Some(Some(value)) => out += &format!("[data-m=\"{value}\"]"),
            None => {}
        }
        if self.hover {
            out += ":hover";
        }
        if self.not_hover {
            out += ":not(:hover)";
        }
        if let Some(class) = self.not_class {
            out += &format!(":not(.{class})");
        }
        if let Some(structural) = self.structural {
            out += &format!(":{structural}");
        }
        if out.is_empty() { "*".to_owned() } else { out }
    }

    /// `index`/`count`: the 1-based position among the parent's elements.
    fn matches(&self, el: &El, index: usize, count: usize) -> bool {
        self.tag.is_none_or(|tag| tag == el.tag)
            && self.class.is_none_or(|class| el.classes.contains(&class))
            && match self.attr {
                None => true,
                Some(None) => el.data_m.is_some(),
                Some(Some(value)) => el.data_m == Some(value),
            }
            && !self.hover
            && self.not_class.is_none_or(|class| !el.classes.contains(&class))
            && match self.structural {
                None => true,
                Some("first-child") => index == 1,
                Some("last-child") => index == count,
                Some(_) => index == 2,
            }
    }
}

#[derive(Clone, Copy, Debug)]
enum Combinator {
    Descendant,
    Child,
    Adjacent,
    General,
}

#[derive(Clone, Debug)]
struct Selector {
    left: Option<(Compound, Combinator)>,
    subject: Compound,
}

impl Selector {
    fn css(&self) -> String {
        match &self.left {
            None => self.subject.css(),
            Some((left, combinator)) => {
                let joiner = match combinator {
                    Combinator::Descendant => " ",
                    Combinator::Child => " > ",
                    Combinator::Adjacent => " + ",
                    Combinator::General => " ~ ",
                };
                format!("{}{joiner}{}", left.css(), self.subject.css())
            }
        }
    }
}

/// The model's answer per element, in document order (pre-order).
fn expected(root: &El, selector: &Selector) -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    // (element, index, count, ancestors nearest last, previous siblings)
    fn walk<'a>(
        el: &'a El,
        index: usize,
        count: usize,
        ancestors: &mut Vec<(&'a El, usize, usize)>,
        previous: &[&'a El],
        selector: &Selector,
        out: &mut Vec<(&'static str, bool)>,
    ) {
        let subject = selector.subject.matches(el, index, count);
        let matched = subject
            && match &selector.left {
                None => true,
                Some((left, Combinator::Descendant)) => ancestors
                    .iter()
                    .any(|(ancestor, i, n)| left.matches(ancestor, *i, *n)),
                Some((left, Combinator::Child)) => ancestors
                    .last()
                    .is_some_and(|(parent, i, n)| left.matches(parent, *i, *n)),
                // Siblings carry no structural part (the left compound has
                // none), so their positions don't matter here.
                Some((left, Combinator::Adjacent)) => {
                    previous.last().is_some_and(|sibling| left.matches(sibling, 0, 0))
                }
                Some((left, Combinator::General)) => {
                    previous.iter().any(|sibling| left.matches(sibling, 0, 0))
                }
            };
        out.push((el.tag, matched));
        ancestors.push((el, index, count));
        let count = el.children.len();
        for (position, child) in el.children.iter().enumerate() {
            let previous: Vec<&El> = el.children[..position].iter().collect();
            walk(child, position + 1, count, ancestors, &previous, selector, out);
        }
        ancestors.pop();
    }
    walk(root, 1, 1, &mut Vec::new(), &[], selector, &mut out);
    out
}

fn html(el: &El, texts: &mut usize) -> String {
    let mut out = format!("<{}", el.tag);
    if !el.classes.is_empty() {
        out += &format!(" class=\"{}\"", el.classes.join(" "));
    }
    if let Some(value) = el.data_m {
        out += &format!(" data-m=\"{value}\"");
    }
    out += ">";
    if el.tag == "p" {
        *texts += 1;
        out += &format!("t{texts}");
    }
    for child in &el.children {
        out += &html(child, texts);
    }
    out + &format!("</{}>", el.tag)
}

/// The built elements in document order: (tag, painted).
fn built(dump: &str) -> Vec<(String, bool)> {
    dump.lines()
        .skip(1) // html-ui
        .map(str::trim_start)
        .filter(|line| !line.starts_with('"') && !line.starts_with('-'))
        .map(|line| {
            let tag = line.split(['.', '#', ' ']).next().unwrap_or("").to_owned();
            (tag, line.contains("bg=#ff0000"))
        })
        .collect()
}

fn case(seed: u64) {
    let mut rng = Rng(seed | 1);
    // The root is a div with 1–3 children, each of which may nest once.
    let root = El {
        tag: "div",
        classes: vec![],
        data_m: None,
        children: (0..1 + rng.below(3)).map(|_| element(&mut rng, 1)).collect(),
    };
    let subject = Compound::random(&mut rng, true);
    let left = match rng.below(5) {
        0 => None,
        n => Some((
            Compound::random(&mut rng, false),
            [Combinator::Descendant, Combinator::Child, Combinator::Adjacent, Combinator::General][n - 1],
        )),
    };
    let selector = Selector { left, subject };
    let page = html(&root, &mut 0);
    let css = format!("html {{ color: #ffffff }} {} {{ background-color: #ff0000 }}", selector.css());
    let dump = TestUi::new(&format!("selector-model-{seed}"), &[("page.html", page.as_str()), ("a.css", css.as_str())])
        .stylesheet("a.css")
        .spawn("page.html", TemplateContext::new(), Node::default())
        .settle()
        .dump();
    let model = expected(&root, &selector);
    let actual = built(&dump);
    let model: Vec<(String, bool)> = model.into_iter().map(|(tag, painted)| (tag.to_owned(), painted)).collect();
    assert_eq!(
        actual, model,
        "seed {seed}\nselector: {}\npage: {page}\ndump:\n{dump}",
        selector.css()
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    #[test]
    fn selectors_match_the_reference_model(seed in any::<u64>()) {
        case(seed);
    }
}
