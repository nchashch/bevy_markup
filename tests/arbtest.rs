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

use arbtest::{arbtest, arbitrary};
use arbitrary::Unstructured;
use bevy::prelude::*;
use p23::prelude::*;

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
        let mut ui = TestUi::new("fuzz-html", &[("page.html", &html), ("style.css", BASE_CSS)])
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
                ("locales/en-US/main.ftl.ron", r#"(locale: "en-US", resources: ["ui.ftl"])"#),
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
