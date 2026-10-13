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

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

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
    convert(source, false)
}

/// [`to_html`](Self), for a `.md` *template* (ADR 0020): Tera syntax
/// survives verbatim — a paragraph of only Tera tags passes through
/// unwrapped, inline Tera runs inside text pass raw (unescaped) — so the
/// conversion runs once at load and the per-frame path is Tera-only.
/// Text inside code blocks and code spans is Tera-literal: it stays
/// escaped text, like any Markdown code content.
pub(crate) fn to_html_template(source: &str) -> String {
    convert(&separate_statements(source), true)
}

/// Puts Tera *statements* (`{% … %}`, `{# … #}`, including the `-`
/// whitespace-control variants) on their own line when they have a line
/// to themselves, so they cannot merge with neighbouring Markdown
/// content into one block. A statement *inside* a line (an attribute
/// value, a list item, a heading, a table cell) stays inline — the old
/// pipeline expanded it in place, and the Tera rewrite (ADR 0020) handles
/// inline runs. Statements inside fenced code blocks and inline code
/// spans are left verbatim (they are code). Interpolations (`{{ … }}`)
/// are inline content and stay where they are.
fn separate_statements(source: &str) -> String {
    if !source.contains("{%") && !source.contains("{#") {
        return source.to_owned();
    }
    let mut out = String::with_capacity(source.len() + 16);
    let mut rest = source;
    let mut fence: Option<String> = None;
    let mut at_line_start = true;
    while !rest.is_empty() {
        // Verbatim inside fenced code blocks.
        if fence.is_none() && at_line_start {
            let trimmed = rest.trim_start_matches(' ');
            let indent = rest.len() - trimmed.len();
            if indent <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
                let marker = &trimmed[..3];
                let line_end = rest.find('\n').map_or(rest.len(), |at| at + 1);
                let info = &rest[indent + 3..line_end.min(rest.len())];
                // A closing fence has nothing but whitespace after the
                // marker; an opening one carries the info string.
                if fence.is_none() && !info.trim().is_empty() {
                    fence = Some(marker.to_owned());
                } else if fence.as_deref() == Some(marker) && info.trim().is_empty() {
                    fence = None;
                }
                out.push_str(&rest[..line_end]);
                rest = &rest[line_end..];
                at_line_start = true;
                continue;
            }
        }
        if fence.is_some() {
            let line_end = rest.find('\n').map_or(rest.len(), |at| at + 1);
            out.push_str(&rest[..line_end]);
            rest = &rest[line_end..];
            at_line_start = rest.is_empty() || rest.starts_with('\n');
            continue;
        }
        // The earliest statement opener (a `{#` before a `{%` is a
        // comment, not the tail of a `{%`).
        let if_start = rest.find("{%");
        let comment_start = rest.find("{#");
        let start = match (if_start, comment_start) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let Some(start) = start else {
            out.push_str(rest);
            return out;
        };
        let opener = &rest[start..start + 2];
        let closer = match opener {
            "{%" => "%}",
            _ => "#}",
        };
        // A lone `{%`/`{#` without its closer is text; leave it as is.
        let Some(inner) = rest[start + 2..].find(closer) else {
            out.push_str(rest);
            return out;
        };
        let end = start + 2 + inner + closer.len();
        // Inline code spans protect their content: a statement between
        // backticks stays code. (A pragmatic backtick run: the span's
        // closer is the next run of the same length.) Only a backtick
        // run right before the opener opens a span — a bare `{# … #}`
        // comment is a statement, and must make progress.
        if rest[..start].ends_with('`') {
            let ticks = rest[..start].len() - rest[..start].trim_end_matches('`').len();
            // No closer: the span runs to the statement's end, so the
            // loop always advances (the rest is processed normally).
            let span_end = rest[start..]
                .find(&"`".repeat(ticks))
                .map_or(end, |at| (start + at + ticks).min(end));
            out.push_str(&rest[..span_end]);
            rest = &rest[span_end..];
            at_line_start = false;
            continue;
        }
        // Own line: whitespace-only before the statement and after its
        // closer (on the same line). Inline statements stay where they
        // are.
        let line_start = rest[..start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = rest.find('\n').map_or(rest.len(), |at| at + 1);
        let before = &rest[line_start..start];
        // A multi-line statement's closer is past this line's newline.
        let after = if end < line_end { &rest[end..line_end] } else { "" };
        if before.trim().is_empty() && after.trim().is_empty() {
            out.push_str(&rest[..start]);
            out.push('\n');
            out.push_str(&rest[start..end]);
            out.push('\n');
            rest = &rest[end..];
            at_line_start = rest.starts_with('\n') || rest.is_empty();
        } else {
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            at_line_start = false;
        }
    }
    out
}

/// The shared conversion: `tera` selects whether Tera syntax passes
/// through verbatim (ADR 0020) or converts as Markdown text.
fn convert(source: &str, tera: bool) -> String {
    let mut html = String::with_capacity(source.len() * 2);
    let parser = Parser::new_ext(
        source,
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_FOOTNOTES,
    );
    let marked = Marked {
        parser,
        pending: VecDeque::new(),
    };
    let events: Vec<Event<'_>> = marked.collect();
    let events = if tera { rewrite(events) } else { events };
    pulldown_cmark::html::push_html(&mut html, &mut events.into_iter());
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

/// The Tera syntax runs of `text`: `(tera, text)` pairs in order — every
/// `{{ … }}`, `{% … %}` and `{# … #}` (quoted strings inside a tag skip
/// their own closing pair). `tera` also swallows a trailing unclosed tag
/// (the source is mid-template-expansion).
fn tera_runs(text: &str) -> Vec<(bool, &str)> {
    let bytes = text.as_bytes();
    let mut runs: Vec<(bool, &str)> = Vec::new();
    let mut start = 0;
    let mut scan = 0;
    while scan < bytes.len() {
        if bytes[scan] == b'{' && scan + 1 < bytes.len() {
            let opener = bytes[scan + 1];
            if opener == b'{' || opener == b'%' || opener == b'#' {
                let closer = match opener {
                    b'{' => b'}',
                    b'%' => b'%',
                    _ => b'#',
                };
                // Scan to the closer, skipping quoted strings.
                let mut cursor = scan + 2;
                let mut end = None;
                while cursor < bytes.len() {
                    if bytes[cursor] == b'"' || bytes[cursor] == b'\'' {
                        let quote = bytes[cursor];
                        cursor += 1;
                        while cursor < bytes.len() && bytes[cursor] != quote {
                            cursor += if bytes[cursor] == b'\\' { 2 } else { 1 };
                        }
                        cursor += 1;
                        continue;
                    }
                    if bytes[cursor] == closer
                        && cursor + 1 < bytes.len()
                        && bytes[cursor + 1] == b'}'
                    {
                        end = Some(cursor + 2);
                        break;
                    }
                    cursor += 1;
                }
                let end = end.unwrap_or(bytes.len());
                if scan > start {
                    runs.push((false, &text[start..scan]));
                }
                runs.push((true, &text[scan..end]));
                scan = end;
                start = end;
                continue;
            }
        }
        scan += 1;
    }
    if start < text.len() {
        runs.push((false, &text[start..]));
    }
    if runs.is_empty() {
        runs.push((false, text));
    }
    runs
}

/// The Tera-aware rewrite (ADR 0020): text splits into plain and Tera
/// runs — Tera as raw `Html`, which `push_html` writes unescaped — and a
/// paragraph whose text is only Tera passes through without its `<p>`
/// wrapper and marker (no element is constructed). Code blocks and code
/// spans are Tera-literal: their text stays escaped, like any Markdown
/// code content.
fn rewrite(events: Vec<Event<'_>>) -> Vec<Event<'_>> {

    let mut out = Vec::with_capacity(events.len());
    let mut code_block = false;
    let mut index = 0;
    while index < events.len() {
        match &events[index] {
            // The marker precedes a constructed start tag. A paragraph
            // start begins a pure-Tera inspection: the paragraph's text up
            // to its end, its marker and its wrapper dropped when every
            // text run in it is Tera.
            Event::Html(text) if text.as_ref() == MARKER => {
                let starts_paragraph =
                    matches!(events.get(index + 1), Some(Event::Start(Tag::Paragraph)));
                if starts_paragraph {
                    let mut cursor = index + 2;
                    let mut pure = true;
                    while cursor < events.len()
                        && !matches!(events[cursor], Event::End(TagEnd::Paragraph))
                    {
                        match &events[cursor] {
                            Event::Text(text) => {
                                pure &= tera_runs(text).iter().all(|(tera, _)| *tera);
                            }
                            Event::SoftBreak => {}
                            _ => pure = false,
                        }
                        cursor += 1;
                    }
                    let ends = cursor < events.len();
                    if pure && ends {
                        for event in &events[index + 2..cursor] {
                            if let Event::Text(text) = event {
                                for (tera, part) in tera_runs(text) {
                                    debug_assert!(tera, "a pure paragraph has only Tera runs");
                                    out.push(Event::Html(CowStr::Boxed(part.to_owned().into_boxed_str())));
                                }
                            }
                        }
                        index = cursor + 1;
                        continue;
                    }
                }
                out.push(Event::Html(CowStr::from(MARKER)));
                index += 1;
            }
            Event::Start(Tag::CodeBlock(_)) => {
                code_block = true;
                out.push(events[index].clone());
                index += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                code_block = false;
                out.push(events[index].clone());
                index += 1;
            }
            // Inline code: literal, escaped by `push_html`.
            Event::Code(code) => {
                out.push(Event::Code(code.clone()));
                index += 1;
            }
            Event::Text(text) if code_block => {
                out.push(Event::Text(text.clone()));
                index += 1;
            }
            Event::Text(text) => {
                for (tera, part) in tera_runs(text) {
                    if tera {
                        out.push(Event::Html(CowStr::Boxed(part.to_owned().into_boxed_str())));
                    } else {
                        out.push(Event::Text(CowStr::Boxed(part.to_owned().into_boxed_str())));
                    }
                }
                index += 1;
            }
            event => {
                out.push(event.clone());
                index += 1;
            }
        }
    }
    out
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

    /// A `{# … #}` comment with no backtick after it used to spin
    /// forever: the code-span branch consumed nothing. Every shape
    /// terminates, and the comment/text survive verbatim.
    #[test]
    fn a_comment_without_a_backtick_terminates() {
        let out = separate_statements("{# note #}\nHello\n");
        assert!(out.contains("{# note #}\n"), "{out}");
        assert!(out.contains("Hello"), "{out}");

        let out = separate_statements("Hi {# c #} there");
        assert!(out.contains("Hi {# c #} there"), "{out}");

        let out = separate_statements("{##}");
        assert!(out.contains("{##}"), "{out}");
    }

    /// A statement inside a code span still stays verbatim, whether or
    /// not the span closes after it (an unterminated span runs to the
    /// statement's end so the rest is processed normally).
    #[test]
    fn a_statement_between_backticks_stays_verbatim() {
        let out = separate_statements("use `{# c #}` here");
        assert!(out.contains("use `{# c #}` here"), "{out}");

        let out = separate_statements("use `{# c #} tail");
        assert!(out.contains("use `{# c #}"), "{out}");
        assert!(out.contains("tail"), "{out}");
    }

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

    /// ADR 0020: a template's Tera syntax survives verbatim — a
    /// pure-Tera paragraph passes unwrapped (no `<p>`, no class), inline
    /// Tera inside text passes raw while the surrounding Markdown still
    /// constructs and classes, and code is Tera-literal. Tera statements
    /// sit on their own lines (like after a Tera expansion), so they never
    /// shape Markdown structure.
    #[test]
    fn template_mode_passes_tera_through() {
        // A pure-Tera paragraph (Tera lines and the whitespace between
        // them): no wrapper, no class, raw syntax. Plain text between
        // statements becomes its own paragraph, as after a Tera expansion.
        let html = to_html_template("{% if score > 3 %}\n{% endif %}\n");
        assert!(!html.contains("<p"), "PURE = {html:?}");
        assert!(html.contains("{% if score > 3 %}"), "{html}");
        assert!(html.contains("{% endif %}"), "{html}");
        let html = to_html_template("{% if score > 3 %}\nshow it\n{% endif %}\n");
        assert!(html.contains("{% if score > 3 %}"), "MIXED = {html:?}");
        assert!(html.contains("<p class=\"markdown\">show it</p>"), "MIXED = {html:?}");
        assert!(html.contains("{% endif %}"), "MIXED = {html:?}");

        // A block-level loop: the wrapper-free Tera lines bracket the
        // constructed Markdown.
        let html = to_html_template(
            "{% for i in range(end=2) %}\n## Section {{ i }}\n{% endfor %}\n",
        );
        assert!(html.contains("{% for i in range(end=2) %}"), "{html}");
        assert!(html.contains("{% endfor %}"), "{html}");
        assert!(
            html.contains("<h2 class=\"markdown\">Section {{ i }}</h2>"),
            "{html}"
        );

        // Inline Tera inside text: raw alongside constructed emphasis.
        let html = to_html_template("hello {{ name }}, *slanted*\n");
        assert!(
            html.contains("hello {{ name }}, <em class=\"markdown\">slanted</em>"),
            "INLINE = {html:?}"
        );

        // Code stays Tera-literal: a code span's `{{ x }}` is escaped
        // text, not passed through.
        let html = to_html_template("a `{{ x }}` span\n");
        assert!(html.contains("<code>"), "{html}");
        assert!(!html.contains("<p class=\"markdown\">{{ x }}</p>"), "{html}");

        // The `to_html` path (values, not templates) converts Tera
        // looking text as text, as before.
        let html = to_html("hello {{ name }}\n");
        assert!(html.contains("hello {{ name }}"), "{html}");
        assert!(html.contains("<p class=\"markdown\">"), "{html}");
    }
}

    /// Tera in a template (ADR 0020): a paragraph of only statements
    /// passes through unwrapped; inline statements (inside a line, an
    /// attribute value, a list item, a heading) stay inline and render
    /// where they are; a code span/block is Tera-literal; and a raw
    /// `{% raw %}` example in text doesn't break the template.
    #[test]
    fn tera_templates_survive_the_conversion() {
        // A pure-Tera paragraph: unwrapped.
        let html = to_html_template("{% if hd %}<p>hd</p>{% endif %}\n");
        assert!(html.contains("<p>hd</p>"), "{html}");
        // An inline statement inside an attribute value: no newlines.
        let html = to_html_template(
            "<img src=\"{% if hd %}hd{% else %}sd{% endif %}.png\">\n",
        );
        assert!(
            html.contains("src=\"{% if hd %}hd{% else %}sd{% endif %}.png\""),
            "{html}"
        );
        // A statement inside a list item, a heading and a table cell.
        let html = to_html_template("# Hello {% if a %}world{% endif %}\n");
        assert!(
            html.contains("<h1 class=\"markdown\">Hello {% if a %}world{% endif %}</h1>"),
            "{html}"
        );
        let html = to_html_template("* {% if a %}one{% endif %} item\n");
        assert!(
            html.contains("<li class=\"markdown\">{% if a %}one{% endif %} item</li>"),
            "{html}"
        );
        let html = to_html_template(
            "| a | b |\n|---|---|\n| {% if a %}1{% endif %} | 2 |\n",
        );
        assert!(html.contains("{% if a %}1{% endif %}"), "{html}");
        // A code span and a fenced block are Tera-literal.
        let html = to_html_template("a `{{ x }}` span\n");
        assert!(html.contains("<code>{{ x }}</code>"), "{html}");
        let html = to_html_template("```\nlet s = \"{%\"; // %}\n```\n");
        assert!(
            html.contains("let s = \"{%\"; // %}"),
            "the fenced code is verbatim: {html}"
        );
        // A literal `{% raw %}` in text: Tera-literal via the code span...
        let html = to_html_template("text `{% raw %}` tail\n");
        assert!(html.contains("<code>{% raw %}</code>"), "{html}");
    }
