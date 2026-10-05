//! Robustness properties (arbtest): arbitrary bytes fed through every
//! pipeline input — HTML template, CSS, Fluent bundle, `data-l10n-args` —
//! must never panic, must always settle, and must always leave some output
//! (the `html-ui` root, with content or the `failed to render:` paragraph).
//!
//! arbtest re-runs the property with fresh random bytes until the time
//! budget runs out; on failure it prints a seed for deterministic replay
//! (`arbtest(property).seed(0x…)`) and minimizes it. `TestUi` (see
//! `common`) is the harness.

mod common;
use common::TestUi;

use arbitrary::Unstructured;
use arbtest::{arbitrary, arbtest};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_markup::prelude::*;

const BASE_CSS: &str = "html { color: #ffffff; font-size: 20px }";
const SIZED_PAGE: &str = r#"<div class="outer"><p id="lead" class="note">Text</p><section><p>Deep</p></section><ul><li>One</li></ul></div>"#;

/// Arbitrary printable-ish text: raw bytes lossily decoded to UTF-8.
fn text(u: &mut Unstructured) -> arbitrary::Result<String> {
    let raw: Vec<u8> = u.arbitrary()?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// The pipeline settled and the `HtmlUi` root survived with some output.
fn settled_with_output(ui: &mut TestUi) {
    let dump = ui.settle().dump();
    assert!(
        dump.starts_with("html-ui"),
        "no root output for input\n--- dump ---\n{dump}"
    );
}

/// Arbitrary HTML (Tera source → tl DOM): may render, may fail with the
/// error paragraph, but never panics or hangs.
#[test]
fn arbitrary_html_never_panics() {
    arbtest(|u| {
        let html = text(u)?;
        let mut ui = TestUi::new(
            "fuzz-html",
            &[("page.html", &html), ("style.css", BASE_CSS)],
        )
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
        settled_with_output(&mut ui);
        Ok(())
    })
    .budget_ms(400)
    .size_max(1 << 14);
}

/// Arbitrary CSS: lightningcss parsing, the cascade and the box properties
/// all consume attacker-controlled stylesheets without panicking.
#[test]
fn arbitrary_css_never_panics() {
    arbtest(|u| {
        let css = text(u)?;
        let mut ui = TestUi::new(
            "fuzz-css",
            &[("page.html", SIZED_PAGE), ("style.css", &css)],
        )
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
        settled_with_output(&mut ui);
        Ok(())
    })
    .budget_ms(400)
    .size_max(1 << 14);
}

/// Arbitrary Fluent: a bundle body of random bytes plus random
/// `data-l10n-args` JSON. Parse failures fall back to element content; they
/// must not panic the localize system.
#[test]
fn arbitrary_fluent_never_panics() {
    arbtest(|u| {
        let ftl = text(u)?;
        let args = text(u)?;
        let page = format!(
            "<h1 data-l10n-id=\"greet\">fallback</h1>\
             <p data-l10n-id=\"count\" data-l10n-args='{args}'>raw</p>"
        );
        let mut ui = TestUi::new(
            "fuzz-fluent",
            &[
                ("page.html", page.as_str()),
                ("style.css", BASE_CSS),
                (
                    "locales/en-US/main.ftl.ron",
                    r#"(locale: "en-US", resources: ["ui.ftl"])"#,
                ),
                ("locales/en-US/ui.ftl", &ftl),
            ],
        )
        .stylesheet("style.css")
        .locale("locales/en-US/main.ftl.ron")
        .spawn("page.html", TemplateContext::new(), Node::default());
        settled_with_output(&mut ui);
        Ok(())
    })
    .budget_ms(400)
    .size_max(1 << 14);
}

// ---------------------------------------------------------------------------
// Structure-aware: random well-formed documents vs an HTML model
// ---------------------------------------------------------------------------

const CONTAINER_TAGS: &[&str] = &["div", "section", "ul"];
const BLOCK_TAGS: &[&str] = &["p", "h1", "li"];
const INLINE_TAGS: &[&str] = &["b", "i", "span"];
/// Visible text pieces: words, characters that need escaping, non-ASCII
/// spaces HTML does *not* collapse (NBSP, ideographic space), and ASCII
/// whitespace that it does. No `{` (Tera syntax).
const TOKENS: &[&str] = &[
    "a", "bc", "Déf", "x1", "日本", "&", "<", ">", "\"", "'", "\u{a0}", "\u{3000}", " ", "  ",
    "\n", "\t", " \r\n ", "\u{c}",
];

#[derive(Debug)]
enum Html {
    Text(String),
    Element {
        tag: &'static str,
        id: Option<&'static str>,
        class: Option<&'static str>,
        children: Vec<Html>,
    },
}

fn gen_nodes(u: &mut Unstructured, depth: usize) -> arbitrary::Result<Vec<Html>> {
    let n = u.int_in_range(0..=if depth >= 3 { 2 } else { 4 })?;
    (0..n).map(|_| gen_node(u, depth)).collect()
}

fn gen_node(u: &mut Unstructured, depth: usize) -> arbitrary::Result<Html> {
    if depth >= 4 || u.ratio(1, 3)? {
        let n = u.int_in_range(1..=4)?;
        let mut text = String::new();
        for _ in 0..n {
            text.push_str(u.choose(TOKENS)?);
        }
        return Ok(Html::Text(text));
    }
    let tags = *u.choose(&[CONTAINER_TAGS, BLOCK_TAGS, INLINE_TAGS])?;
    Ok(Html::Element {
        tag: u.choose(tags)?,
        id: *u.choose(&[None, None, Some("x"), Some("y")])?,
        class: *u.choose(&[None, Some("a"), Some("b")])?,
        children: gen_nodes(u, depth + 1)?,
    })
}

fn serialize(nodes: &[Html], out: &mut String) {
    for node in nodes {
        match node {
            Html::Text(text) => out.push_str(
                &text
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            ),
            Html::Element {
                tag,
                id,
                class,
                children,
            } => {
                out.push('<');
                out.push_str(tag);
                if let Some(id) = id {
                    out.push_str(&format!(" id=\"{id}\""));
                }
                if let Some(class) = class {
                    out.push_str(&format!(" class=\"{class}\""));
                }
                out.push('>');
                serialize(children, out);
                out.push_str(&format!("</{tag}>"));
            }
        }
    }
}

/// HTML white-space collapsing (CSS `white-space: normal`): runs of ASCII
/// whitespace become one space, leading/trailing ones go. Other spaces
/// (NBSP, U+3000) are ordinary characters.
fn collapse(text: &str) -> String {
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r');
    text.split(is_space)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn visible_text(nodes: &[Html], out: &mut String) {
    for node in nodes {
        match node {
            Html::Text(text) => out.push_str(text),
            Html::Element { children, .. } => visible_text(children, out),
        }
    }
}

/// One expected node: depth below the root, `tag#id.class` (`-` for
/// anonymous text), and its text (`None` for containers).
type Expected = (usize, String, Option<String>);

/// The nodes bevy_markup must build for `nodes` at container level: containers
/// nest, blocks take all their descendants' text as one collapsed line
/// (`li` with a bullet), inline elements outside a block are walked
/// through, and each run of loose text becomes an anonymous block.
fn model(nodes: &[Html], depth: usize, out: &mut Vec<Expected>) {
    let mut loose = String::new();
    let flush = |loose: &mut String, out: &mut Vec<Expected>| {
        let text = collapse(loose);
        if !text.is_empty() {
            out.push((depth, "-".to_owned(), Some(text)));
        }
        loose.clear();
    };
    for node in nodes {
        let (tag, id, class, children) = match node {
            Html::Text(text) => {
                loose.push_str(text);
                continue;
            }
            Html::Element {
                tag,
                id,
                class,
                children,
            } => (tag, id, class, children),
        };
        flush(&mut loose, out);
        let mut label = (*tag).to_owned();
        if let Some(id) = id {
            label.push_str(&format!("#{id}"));
        }
        if let Some(class) = class {
            label.push_str(&format!(".{class}"));
        }
        if CONTAINER_TAGS.contains(tag) {
            out.push((depth, label, None));
            model(children, depth + 1, out);
        } else if BLOCK_TAGS.contains(tag) {
            let mut text = String::new();
            visible_text(children, &mut text);
            let bullet = if *tag == "li" { "• " } else { "" };
            out.push((depth, label, Some(format!("{bullet}{}", collapse(&text)))));
        } else {
            model(children, depth, out);
        }
    }
    flush(&mut loose, out);
}

/// What bevy_markup built below `entity`, in the model's shape, plus the
/// `HtmlElement` entities in depth-first order.
fn observe(
    world: &World,
    entity: Entity,
    depth: usize,
    out: &mut Vec<Expected>,
    elements: &mut Vec<Entity>,
) {
    for &child in world.entity(entity).get::<Children>().into_iter().flatten() {
        let child_ref = world.entity(child);
        if child_ref.contains::<TextSpan>() {
            continue;
        }
        let label = child_ref
            .get::<HtmlElement>()
            .map_or("-".to_owned(), common::element_label);
        if child_ref.contains::<HtmlElement>() {
            elements.push(child);
        }
        let text = child_ref.get::<Text>().map(|text| {
            let spans = child_ref.get::<Children>().into_iter().flatten();
            let spans = spans.filter_map(|&span| world.entity(span).get::<TextSpan>());
            spans.fold(text.0.clone(), |text, span| text + &span.0)
        });
        // Containers hold child nodes; a block's children are its spans.
        let container = text.is_none();
        out.push((depth, label, text));
        if container {
            observe(world, child, depth + 1, out, elements);
        }
    }
}

/// Random well-formed documents (containers, blocks, inline elements,
/// loose text with entities, NBSP and every kind of ASCII whitespace)
/// build exactly the node tree HTML semantics predict: one node per
/// container/block in document order and nesting, one anonymous block per
/// piece of loose text, each block's text = its visible text after
/// white-space collapsing — and `HtmlElements::iter` walks those element
/// nodes in the same (document) order. Catches dropped or reordered
/// nodes, inline content leaking out of blocks, whitespace collapsing that
/// eats non-collapsible spaces or misses a whitespace kind, entity
/// decoding gaps, and breadth-first element lookups.
#[test]
fn generated_documents_build_the_html_model() {
    arbtest(|u| {
        let nodes = gen_nodes(u, 0)?;
        let mut page = String::new();
        serialize(&nodes, &mut page);
        let mut expected = Vec::new();
        model(&nodes, 0, &mut expected);

        let mut ui = TestUi::new(
            "structured-html",
            &[("page.html", &page), ("style.css", BASE_CSS)],
        )
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
        ui.settle();
        let root = ui.root();
        let (mut actual, mut elements) = (Vec::new(), Vec::new());
        observe(ui.world_mut(), root, 0, &mut actual, &mut elements);
        assert_eq!(actual, expected, "page: {page:?}");

        let iterated = ui
            .world_mut()
            .run_system_once(move |html: HtmlElements| {
                html.iter(root)
                    .map(|(entity, _)| entity)
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(
            iterated, elements,
            "HtmlElements::iter order, page: {page:?}"
        );
        Ok(())
    })
    .budget_ms(600)
    .size_max(1 << 10);
}
