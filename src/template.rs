//! `.html` files as Tera templates, and the parsed result.
//!
//! The loader compiles each file as a Tera 2 template, so syntax errors fail the
//! load. The template's name is its asset path, so `.html` files get Tera's
//! HTML autoescaping of `{{ }}` values. A plain `.html` file with no Tera
//! syntax is a template that renders to itself.

use std::fmt::Write;

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::l10n::LocalizedText;

/// A compiled Tera template loaded from an `.html`/`.htm` file. Use it via
/// [`HtmlUi`](crate::html::HtmlUi).
#[derive(Asset, TypePath)]
pub struct HtmlTemplate {
    pub(crate) tera: tera::Tera,
    /// Template name inside `tera`: the asset path.
    pub(crate) name: String,
}

impl HtmlTemplate {
    /// Renders with `context`, then parses the output into a DOM.
    pub fn render(
        &self,
        context: &tera::Context,
    ) -> Result<HtmlDocument, Box<dyn std::error::Error + Send + Sync>> {
        let html = self.tera.render(&self.name, context)?;
        Ok(HtmlDocument::parse(html)?)
    }

    /// The template's name (its asset path).
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Parsed HTML. Owns its source text.
pub struct HtmlDocument {
    dom: tl::VDomGuard,
}

impl HtmlDocument {
    pub fn parse(html: String) -> Result<Self, tl::ParseError> {
        // SAFETY: `parse_owned` leaks `html` and frees it when the returned
        // `VDomGuard` drops; `VDomGuard::get_ref` ties borrows to the guard.
        let dom = unsafe { tl::parse_owned(html, tl::ParserOptions::default()) }?;
        Ok(Self { dom })
    }

    /// The parsed DOM.
    pub fn dom(&self) -> &tl::VDom<'_> {
        self.dom.get_ref()
    }

    /// Indented outline of the DOM: one line per element (with attributes),
    /// text node, and comment. Whitespace-only text is skipped. Elements in
    /// `localized` show their translation (`l10n "…"`) instead of children.
    pub fn outline(&self, localized: &LocalizedText) -> String {
        let dom = self.dom();
        let parser = dom.parser();
        let mut out = String::new();
        for handle in dom.children() {
            write_node(&mut out, parser, &localized.0, *handle, 0);
        }
        out
    }
}

/// `tl` leaves character references as written. Undo the ones Tera's HTML
/// autoescaping produces, so text and attribute values read as authored.
pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// `err: source: source…` — Tera puts the useful detail in the sources.
pub(crate) fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(err) = source {
        let _ = write!(message, ": {err}");
        source = err.source();
    }
    message
}

fn write_node(
    out: &mut String,
    parser: &tl::Parser,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    handle: tl::NodeHandle,
    depth: usize,
) {
    let Some(node) = handle.get(parser) else {
        return;
    };
    let indent = "  ".repeat(depth);
    match node {
        tl::Node::Tag(tag) => {
            let _ = write!(out, "{indent}{}", tag.name().as_utf8_str());
            for (key, value) in tag.attributes().iter() {
                match value {
                    Some(value) => {
                        let _ = write!(out, " {key}=\"{value}\"");
                    }
                    None => {
                        let _ = write!(out, " {key}");
                    }
                }
            }
            out.push('\n');
            let child_indent = "  ".repeat(depth + 1);
            match localized.get(&handle) {
                Some(Ok(text)) => {
                    let _ = writeln!(out, "{child_indent}l10n {text:?}");
                }
                Some(Err(err)) => {
                    let _ = writeln!(out, "{child_indent}l10n error: {err}");
                }
                None => {
                    for child in tag.children().top().iter() {
                        write_node(out, parser, localized, *child, depth + 1);
                    }
                }
            }
        }
        tl::Node::Raw(text) => {
            let text = text.as_utf8_str();
            let text = text.trim();
            if !text.is_empty() {
                let _ = writeln!(out, "{indent}{text:?}");
            }
        }
        tl::Node::Comment(text) => {
            let _ = writeln!(out, "{indent}{}", text.as_utf8_str());
        }
    }
}

#[derive(Default, TypePath)]
pub(crate) struct HtmlTemplateLoader;

impl AssetLoader for HtmlTemplateLoader {
    type Asset = HtmlTemplate;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<HtmlTemplate, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;

        let name = load_context.path().to_string();
        let mut tera = tera::Tera::new();
        tera.add_raw_template(&name, &source)?;
        Ok(HtmlTemplate { tera, name })
    }

    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Text dense in markup characters and entity look-alikes (`&lt;`
    /// typed as text), plus arbitrary characters.
    const MARKUP_TEXT: &str = "(?s)(&lt;|&gt;|&amp;|&quot;|&#39;|[<>&\"';# a-z]|.){0,24}";

    /// Each of the five references Tera's autoescaping emits decodes to its
    /// character. Catches a dropped or misspelled entry (e.g. `&#x27;`
    /// instead of `&#39;`), which would show raw entities in the UI.
    #[test]
    fn decodes_the_five_escapes() {
        assert_eq!(decode_entities("&lt;b&gt; &quot;x&quot; &#39;y&#39; &amp;"), "<b> \"x\" 'y' &");
    }

    /// Decoding undoes exactly one level of escaping: an escaped entity
    /// (`&amp;lt;`, what Tera emits for a literal `&lt;`) becomes the entity
    /// text, not the character. Catches decoding `&amp;` before the others
    /// (or decoding repeatedly), which turns authored `&lt;` into `<`.
    #[test]
    fn decodes_one_level_only() {
        assert_eq!(decode_entities("&amp;lt;"), "&lt;");
        assert_eq!(decode_entities("&amp;amp;"), "&amp;");
        assert_eq!(decode_entities("&amp;quot;&amp;#39;"), "&quot;&#39;");
    }

    /// References outside the five (named, numeric, malformed) and bare `&`
    /// are left as written (documented limit), never mangled into a
    /// partial decode such as `&amp;nbsp;` → `&nbsp;` → something else.
    #[test]
    fn other_references_are_left_alone() {
        for text in ["&nbsp;", "&#169;", "&#x3C;", "&copy;", "a & b", "&lt", "&;", "&#39"] {
            assert_eq!(decode_entities(text), text);
        }
    }

    /// Concatenated text of every raw node in `document`, decoded.
    fn decoded_text(document: &HtmlDocument) -> String {
        let dom = document.dom();
        dom.nodes()
            .iter()
            .filter_map(|node| match node {
                tl::Node::Raw(text) => Some(decode_entities(&text.as_utf8_str())),
                _ => None,
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Any string from a Tera variable (`.html` name → autoescaping)
        /// reads back verbatim after `tl` parsing and `decode_entities`:
        /// Tera's escape set and ours must match exactly (a Tera upgrade
        /// emitting `&#x27;`/`&apos;`, or a missing decode, shows entities
        /// in the UI; a missing escape lets data inject markup).
        #[test]
        fn rendered_variables_round_trip(s in MARKUP_TEXT) {
            let mut tera = tera::Tera::new();
            tera.add_raw_template("t.html", "<p>{{ s }}</p>").unwrap();
            let template = HtmlTemplate { tera, name: "t.html".to_owned() };
            let mut context = tera::Context::new();
            context.insert("s", &s);
            let document = template.render(&context).unwrap();
            prop_assert_eq!(document.dom().query_selector("p").unwrap().count(), 1);
            prop_assert_eq!(decoded_text(&document), s);
        }
    }

    /// The DOM outline indents each level by two spaces, skips blank text
    /// (formatting whitespace between tags), quotes text, and shows a
    /// translated element's translation (or error) instead of its children.
    #[test]
    fn outline_indents_levels_and_shows_translations() {
        let document = HtmlDocument::parse(
            "<div class=\"a\">\n  <p>Hi <b>there</b></p>\n  <p data-l10n-id=\"t\">x</p>\n  <p data-l10n-id=\"e\">y</p>\n</div>".to_owned(),
        )
        .unwrap();
        let paragraphs: Vec<tl::NodeHandle> = document
            .dom()
            .query_selector("p")
            .unwrap()
            .collect();
        let mut localized = LocalizedText::default();
        localized.0.insert(paragraphs[1], Ok("Translated".to_owned()));
        localized.0.insert(paragraphs[2], Err("missing".to_owned()));
        let expected = "\
div class=\"a\"
  p
    \"Hi\"
    b
      \"there\"
  p data-l10n-id=\"t\"
    l10n \"Translated\"
  p data-l10n-id=\"e\"
    l10n error: missing
";
        assert_eq!(document.outline(&localized), expected);
    }

    /// Error messages include every `source()` in the chain (Tera puts the
    /// useful detail — the missing variable — in a source).
    #[test]
    fn error_chain_includes_all_sources() {
        #[derive(Debug)]
        struct Layer(&'static str, Option<Box<Layer>>);
        impl std::fmt::Display for Layer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        impl std::error::Error for Layer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.1.as_deref().map(|layer| layer as _)
            }
        }
        let error = Layer("render failed", Some(Box::new(Layer("in t.html", Some(Box::new(Layer("unknown variable `x`", None)))))));
        assert_eq!(error_chain(&error), "render failed: in t.html: unknown variable `x`");
    }
}
