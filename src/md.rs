//! Markdown as a source syntax ([ADR 0014]): `.md` templates and Markdown
//! Fluent values convert here into the HTML the rest of the pipeline
//! already parses — Markdown is sugar for documents, never a second DOM
//! model.
//!
//! Dialect: CommonMark plus the GFM extensions (tables, strikethrough,
//! task lists) and footnotes. Raw inline/block HTML passes through
//! unchanged, so a Markdown page can embed real UI — `data-on-click`
//! buttons, containers — like an HTML template can.
//!
//! The conversion is small enough to run per re-render: streaming events
//! into `push_html`, no AST, no allocation beyond the output string.
//!
//! [ADR 0014]: docs/agents/adr/0014-markdown-via-pulldown-cmark.md

use pulldown_cmark::{Options, Parser};

/// Converts Markdown `source` to the HTML `tl` parses. Not a round trip:
/// the output is one `<p>` per paragraph etc., which the walk reads
/// transparently (a `<p>` inside a translated block's fragment becomes
/// plain runs).
pub(crate) fn to_html(source: &str) -> String {
    let mut html = String::with_capacity(source.len() * 2);
    let mut parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
    );
    pulldown_cmark::html::push_html(&mut html, &mut parser);
    html
}

/// Whether an asset path is a Markdown template (see the
/// `MarkdownTemplateLoader`).
pub(crate) fn is_markdown(name: &str) -> bool {
    name.ends_with(".md") || name.ends_with(".markdown")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CommonMark basics: headings, emphasis, lists; code blocks; images.
    #[test]
    fn converts_the_document_basics() {
        let html = to_html("# Title\n\nplain *slanted* and **strong** text\n");
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<p>plain <em>slanted</em> and <strong>strong</strong> text</p>"), "{html}");
        let html = to_html("- one\n- two\n");
        assert!(html.contains("<li>one</li>"), "{html}");
        let html = to_html("```\ncode\n```\n");
        assert!(html.contains("<pre><code>code\n</code></pre>"), "{html}");
    }

    /// The enabled GFM extensions: tables and strikethrough.
    #[test]
    fn converts_tables_and_strikethrough() {
        let html = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<td>2</td>"), "{html}");
        let html = to_html("~~gone~~\n");
        assert!(html.contains("<del>gone</del>"), "{html}");
    }

    /// Raw HTML passes through, inline and (with blank lines around it)
    /// block — a Markdown page may embed real UI.
    #[test]
    fn passes_raw_html_through() {
        let html = to_html("before <span class=\"hl\">marked</span> after\n");
        assert!(html.contains("before <span class=\"hl\">marked</span> after"), "{html}");
        let html = to_html(
            "text\n\n<div data-on-click=\"buy\" class=\"action\">Buy</div>\n\ntail\n",
        );
        assert!(html.contains("<div data-on-click=\"buy\" class=\"action\">Buy</div>"), "{html}");
    }

    /// Entities survive: escaped text is not re-interpreted as markup
    /// (`&lt;` stays an entity for `tl`/`decode_entities`).
    #[test]
    fn keeps_entities_as_entities() {
        let html = to_html("a &lt;b&gt; c\n");
        assert!(html.contains("<p>a &lt;b&gt; c</p>"), "{html}");
    }

    /// A one-line value gains a `<p>` wrapper: the walk reads fragments
    /// through their content, so this is transparent.
    #[test]
    fn wraps_a_paragraph() {
        assert_eq!(to_html("Hello"), "<p>Hello</p>\n");
    }
}
