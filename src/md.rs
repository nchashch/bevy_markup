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

use std::collections::VecDeque;

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag};

/// The class injected into every element Markdown compilation constructs
/// (paragraphs, headings, images, links, emphasis, tables, …), so a
/// document styles its Markdown content apart from the raw-HTML `<img>`
/// tags it embeds — which pass through untouched, as in any HTML template,
/// and can add the class by hand if they want it.
pub(crate) const MARKDOWN_CLASS: &str = "markdown";

/// Emitted before every constructed start tag (see [`MARKDOWN_CLASS`]);
/// `to_html` rewrites `marker + <tag>` into the classed tag and drops the
/// marker before the HTML reaches `tl`.
const MARKER: &str = "\u{1}bevy-markup\u{1}";

/// Converts Markdown `source` to the HTML `tl` parses. Not a round trip:
/// the output is one `<p>` per paragraph etc., which the walk reads
/// transparently (a `<p>` inside a translated block's fragment becomes
/// plain runs).
pub(crate) fn to_html(source: &str) -> String {
    let mut html = String::with_capacity(source.len() * 2);
    let parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
    );
    let mut marked = Marked {
        parser,
        pending: VecDeque::new(),
    };
    pulldown_cmark::html::push_html(&mut html, &mut marked);
    inject_class(&html)
}

/// An event stream that emits [`MARKER`] before every constructed start
/// tag. Raw HTML (`Html`/`InlineHtml` events) is passed through without
/// one — that is the whole distinction: pulldown-constructed versus
/// handwritten. Raw HTML *blocks* skip the marker too: pulldown writes
/// their content verbatim, so a marker there would class the author's own
/// first tag.
struct Marked<'a> {
    parser: Parser<'a>,
    /// The start tags waiting behind their markers.
    pending: VecDeque<Event<'a>>,
}

impl<'a> Iterator for Marked<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Event<'a>> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        match self.parser.next()? {
            // Not an element of its own: content only.
            Event::Start(Tag::HtmlBlock) => Some(Event::Start(Tag::HtmlBlock)),
            Event::Start(tag) => {
                // The marker must not be treated as text; it is an `Html`
                // event because `push_html` writes those verbatim.
                self.pending.push_back(Event::Start(tag));
                Some(Event::Html(CowStr::from(MARKER)))
            }
            event => Some(event),
        }
    }
}

/// Rewrites each `MARKER + <tag …>` into `<tag … class="markdown">`
/// (appending to a tag that already has a class) and drops the markers.
fn inject_class(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(index) = rest.find(MARKER) {
        out.push_str(&rest[..index]);
        rest = &rest[index + MARKER.len()..];
        // The constructed start tag follows (pulldown may put a newline
        // between, since the marker leaves the output mid-line).
        let whitespace = rest.len() - rest.trim_start().len();
        out.push_str(&rest[..whitespace]);
        rest = &rest[whitespace..];
        let Some(tag) = rest.strip_prefix('<') else {
            // No tag after the marker (nothing to class): drop the marker.
            continue;
        };
        let Some(close) = tag.find('>') else {
            break;
        };
        let (inside, after) = tag.split_at(close);
        let name_end = inside
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(inside.len());
        let (name, attrs) = inside.split_at(name_end);
        let classed = match attrs.find(" class=\"") {
            // Append inside the existing class list.
            Some(at) => {
                let value_at = at + " class=\"".len();
                let value_end = value_at + attrs[value_at..].find('"').unwrap_or(0);
                format!(
                    "{name}{before} {MARKDOWN_CLASS}\"{after_tail}",
                    before = &attrs[..value_end],
                    after_tail = &attrs[value_end + 1..],
                )
            }
            None => format!("{name} class=\"{MARKDOWN_CLASS}\"{attrs}"),
        };
        out.push('<');
        out.push_str(&classed);
        out.push('>');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
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
    /// Every constructed element carries [`MARKDOWN_CLASS`].
    #[test]
    fn converts_the_document_basics() {
        let html = to_html("# Title\n\nplain *slanted* and **strong** text\n");
        assert!(html.contains("<h1 class=\"markdown\">Title</h1>"), "{html}");
        assert!(
            html.contains(
                "<p class=\"markdown\">plain <em class=\"markdown\">slanted</em> \
                 and <strong class=\"markdown\">strong</strong> text</p>"
            ),
            "{html}"
        );
        let html = to_html("- one\n- two\n");
        assert!(html.contains("<li class=\"markdown\">one</li>"), "{html}");
        let html = to_html("```\ncode\n```\n");
        assert!(
            html.contains("<pre class=\"markdown\"><code>code\n</code></pre>"),
            "{html}"
        );
    }

    /// The enabled GFM extensions: tables and strikethrough.
    #[test]
    fn converts_tables_and_strikethrough() {
        let html = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<table class=\"markdown\">"), "{html}");
        assert!(html.contains("<td class=\"markdown\">2</td>"), "{html}");
        let html = to_html("~~gone~~\n");
        assert!(
            html.contains("<del class=\"markdown\">gone</del>"),
            "{html}"
        );
    }

    /// Raw HTML passes through, inline and (with blank lines around it)
    /// block — a Markdown page may embed real UI, unclassed. (The blank
    /// line ends the raw block, so the following Markdown paragraph is
    /// constructed and classed.)
    #[test]
    fn passes_raw_html_through() {
        let html = to_html("before <span class=\"hl\">marked</span> after\n");
        assert!(
            html.contains("before <span class=\"hl\">marked</span> after"),
            "{html}"
        );
        let html =
            to_html("text\n\n<div data-on-click=\"buy\" class=\"action\">Buy</div>\n\ntail\n");
        assert!(
            html.contains("<div data-on-click=\"buy\" class=\"action\">Buy</div>"),
            "{html}"
        );
        assert!(html.contains("<p class=\"markdown\">tail</p>"), "{html}");
    }

    /// Entities survive: escaped text is not re-interpreted as markup
    /// (`&lt;` stays an entity for `tl`/`decode_entities`).
    #[test]
    fn keeps_entities_as_entities() {
        let html = to_html("a &lt;b&gt; c\n");
        assert!(
            html.contains("<p class=\"markdown\">a &lt;b&gt; c</p>"),
            "{html}"
        );
    }

    /// A one-line value gains a `<p>` wrapper: the walk reads fragments
    /// through their content, so this is transparent.
    #[test]
    fn wraps_a_paragraph() {
        // The marker leaves the output mid-line, so pulldown prefixes the
        // first block with a newline.
        assert!(to_html("Hello").contains("<p class=\"markdown\">Hello</p>"));
    }

    /// Every constructed element carries [`MARKDOWN_CLASS`] — including
    /// images (`img.markdown`) and links — while a raw-HTML `<img>` in the
    /// same document passes through unclassed.
    #[test]
    fn constructed_elements_carry_the_class() {
        let html = to_html("![cat](../ui/cat.png)");
        assert!(
            html.contains("<img class=\"markdown\" src=\"../ui/cat.png\" alt=\"cat\" />"),
            "{html}"
        );

        let html = to_html("[text](page.md)");
        assert!(
            html.contains("<a class=\"markdown\" href=\"page.md\">text</a>"),
            "{html}"
        );

        let html = to_html("text\n\n<img class=\"seal\" src=\"cat.png\">\n");
        assert!(
            html.contains("<img class=\"seal\" src=\"cat.png\">"),
            "{html}"
        );
        assert!(
            !html.contains("class=\"markdown markdown\""),
            "no double class: {html}"
        );
    }

    /// A footnote definition already carries a class: `markdown` is
    /// appended to the list, not a second attribute.
    #[test]
    fn appends_to_an_existing_class() {
        let html = to_html("text[^1]\n\n[^1]: the note\n");
        assert!(
            html.contains("class=\"footnote-definition markdown\""),
            "{html}"
        );
    }
}
