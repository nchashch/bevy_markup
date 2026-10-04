//! `.html` files as Tera templates.
//!
//! The loader compiles each file as a Tera template (so syntax errors fail the
//! load). [`HtmlView`] renders a template with its `tera::Context`, then parses
//! the resulting plain HTML with `tl` into [`RenderedHtml`]. A plain `.html`
//! file with no Tera syntax is just a template that renders to itself.

use std::fmt::Write;

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::l10n::LocalizedText;

pub struct HtmlPlugin;

impl Plugin for HtmlPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<HtmlTemplate>()
            .init_asset_loader::<HtmlTemplateLoader>()
            .add_systems(PostUpdate, render_html_views);
    }
}

/// A compiled Tera template loaded from an `.html` file.
#[derive(Asset, TypePath)]
pub struct HtmlTemplate {
    tera: tera::Tera,
    /// Template name inside `tera`: the asset path, so `.html` names get
    /// Tera's HTML autoescaping.
    name: String,
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
}

/// Parsed HTML. The DOM owns its source text.
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

/// Renders `template` with `context` into this entity's [`RenderedHtml`].
/// Re-renders when either field changes or the template reloads.
#[derive(Component)]
#[require(RenderedHtml, LocalizedText)]
pub struct HtmlView {
    pub template: Handle<HtmlTemplate>,
    pub context: tera::Context,
}

/// Output of an [`HtmlView`].
#[derive(Component, Default)]
pub enum RenderedHtml {
    /// Template not loaded yet.
    #[default]
    Pending,
    Ready(HtmlDocument),
    /// Rendering or parsing failed; the message includes the error chain.
    Failed(String),
}

pub(super) fn render_html_views(
    mut events: MessageReader<AssetEvent<HtmlTemplate>>,
    templates: Res<Assets<HtmlTemplate>>,
    mut views: Query<(Ref<HtmlView>, &mut RenderedHtml)>,
) {
    let reloaded: HashSet<AssetId<HtmlTemplate>> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    for (view, mut rendered) in &mut views {
        if !view.is_changed() && !reloaded.contains(&view.template.id()) {
            continue;
        }
        let Some(template) = templates.get(&view.template) else {
            continue;
        };
        *rendered = match template.render(&view.context) {
            Ok(document) => RenderedHtml::Ready(document),
            Err(err) => {
                let message = error_chain(&*err);
                error!("rendering {}: {message}", template.name);
                RenderedHtml::Failed(message)
            }
        };
    }
}

/// `tl` leaves character references as written. Undo the ones Tera's HTML
/// autoescaping produces, so text and attribute values read as authored.
pub fn decode_entities(text: &str) -> String {
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
fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
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
struct HtmlTemplateLoader;

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
