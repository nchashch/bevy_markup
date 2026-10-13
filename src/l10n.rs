//! Fluent localization of HTML via `data-l10n-id` / `data-l10n-args`, the
//! attribute convention from Fluent's DOM bindings (`fluent-dom`):
//!
//! ```html
//! <h1 data-l10n-id="inventory-title">Inventory</h1>
//! <p data-l10n-id="hp-status" data-l10n-args='{"hp": 7, "max": 10}'></p>
//! ```
//!
//! Any element with `data-l10n-id` (block, container or inline) gets the
//! message's formatted value as its content, replacing its children; its own
//! content is the fallback when the message is missing or no [`ActiveLocale`]
//! is set. `data-l10n-args` is a JSON object of Fluent variables (numbers stay
//! numbers, so plural selectors work); a whole map can come from one Tera
//! variable: `data-l10n-args='{{ args }}'`.
//!
//! Translations are markup, like fluent-dom's "DOM overlays": a value may
//! contain inline elements (`Press <kbd>Ctrl</kbd>…`), styled by the CSS like
//! document elements. In `.ftl` values write a literal `<`/`&` as
//! `&lt;`/`&amp;` and a literal `{`/`}` as `{"{"}`/`{"}"}`.
//!
//! Named elements keep the source's attributes while translators own the
//! text and word order: a translation's `<b data-l10n-name="who">Ada</b>` is
//! styled as the translated element's own `data-l10n-name="who"` descendant
//! (its tag, `id` and classes). Each source element can be used once; a name
//! with no unused source element of the same tag is plain text.
//!
//! ```html
//! <p data-l10n-id="meet">Meet <b data-l10n-name="who" class="hero"></b>.</p>
//! ```
//!
//! Deliberate differences from fluent-dom (verified against it by the
//! `fluent_oracle` test vectors): translation markup isn't sanitized —
//! nested elements, `class`/`id` and non-text-level elements in a translation
//! are kept and styled (Bevy UI has no scripts or links to protect); string
//! args are HTML-escaped before formatting, so values like `Ada <The Brave>`
//! stay text; and numbers format as fluent-rs does (`1234.5`, no locale
//! grouping like `1,234.5`).
//!
//! # Locale bundles
//!
//! Messages come from a `*.ftl.ron` bundle ([`LocaleBundle`]): a manifest
//! naming the locale and its FTL resources, registered by
//! [`BevyMarkupPlugin`](crate::BevyMarkupPlugin):
//!
//! ```ron
//! (
//!     locale: "en-US",
//!     resources: ["ui.ftl"],
//! )
//! ```
//!
//! Resource paths resolve relative to the manifest (like a URL: `../ui.ftl`
//! from the asset root, `/ui/ui.ftl` from the asset root) and are read as
//! dependencies, so Bevy's file watcher reloads every bundle that uses an
//! edited `.ftl` file (a manual `AssetServer::reload` must name the bundle).
//! A resource with syntax errors still loads — Fluent keeps its valid
//! messages — with the errors logged.
//!
//! # Markdown values
//!
//! The manifest's `markdown: bool` opts the bundle's message values into
//! Markdown (ADR 0014): each message's pattern converts to HTML once, at
//! bundle load (ADR 0020's per-message cache — the formatted arguments
//! substitute into the cached HTML; messages with select expressions or
//! adjacent placeables convert per frame), CommonMark +
//! tables/strikethrough/task lists/footnotes. Off by default — HTML-authored
//! bundles must not flip it (Markdown would read their literal `+`/`-`/`#`
//! line starts as syntax). Note the interplay: string args are HTML-escaped
//! (they can't inject markup) but their characters are still Markdown, so a
//! value like `{{ name }} is here` would emphasize a name that starts with
//! `*`. FTL continuation lines are indented (FTL syntax), so a Markdown
//! list starts at two spaces.
//!
//! ```ron
//! (
//!     locale: "en-US",
//!     resources: ["journal.ftl"],
//!     markdown: true,
//! )
//! ```

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use fluent::{FluentArgs, FluentResource, bundle::FluentBundle, memoizer::MemoizerKind};
use serde::Deserialize;
use std::{borrow::Borrow, ops::Deref, sync::Arc};
use unic_langid::LanguageIdentifier;

use crate::html::RenderedHtml;
use crate::md;
use crate::template::decode_entities;

/// The Fluent resources of one locale, loaded from a `*.ftl.ron` manifest
/// (see the [locale bundles](self#locale-bundles) section). Use it via
/// [`ActiveLocale`]. The manifest's `markdown: bool` (default off) opts the
/// bundle's message values into Markdown: the formatted value converts to
/// HTML (`md::to_html`) before the walk parses it as markup — cached per
/// message (ADR 0020 Decision 3), so the conversion runs once per load.
#[derive(Asset, Clone, TypePath)]
pub struct LocaleBundle(Arc<BundleState>);

/// A [`FluentBundle`] over shared resources: an asset must be `Send + Sync`,
/// so the concurrent memoizer.
type ConcurrentBundle = fluent::concurrent::FluentBundle<Arc<FluentResource>>;

impl Deref for LocaleBundle {
    type Target = ConcurrentBundle;

    fn deref(&self) -> &Self::Target {
        &self.0.bundle
    }
}

/// ADR 0020 Decision 3: `markdown: true` messages' patterns pre-convert at
/// bundle load — each placeable becomes a `\u{2}index\u{3}` marker, the
/// pattern converts to HTML once, and every frame substitutes the formatted
/// arguments into the cached HTML. Messages with selectors, or with two
/// placeables and no text between them (their formatted values can't be
/// split apart textually), convert per frame instead.
struct BundleState {
    bundle: ConcurrentBundle,
    markdown: Option<MarkdownCache>,
}

#[derive(Default)]
struct MarkdownCache {
    messages: HashMap<String, MarkdownEntry>,
}

struct MarkdownEntry {
    /// The converted HTML, with `\u{2}index\u{3}` markers at the
    /// placeables.
    html: String,
    /// The pattern's literal text runs: `parts[0]` before the first
    /// placeable, `parts[k]` between placeables, `parts[n]` after the last.
    parts: Vec<String>,
}

/// The marker that stands for placeable `index` in a cached pattern.
fn marker(index: usize) -> String {
    format!("\u{2}{index}\u{3}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocaleBundleDescriptor {
    locale: LanguageIdentifier,
    resources: Vec<String>,
    /// Message values are Markdown (ADR 0014). Default: plain HTML markup.
    #[serde(default)]
    markdown: bool,
}

#[derive(Default, TypePath)]
pub(crate) struct LocaleBundleLoader;

impl AssetLoader for LocaleBundleLoader {
    type Asset = LocaleBundle;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<LocaleBundle, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let manifest: LocaleBundleDescriptor = ron::de::from_bytes(&bytes)?;

        let mut bundle = ConcurrentBundle::new_concurrent(vec![manifest.locale]);
        let mut sources = Vec::new();
        for name in manifest.resources {
            let path = load_context.path().resolve_embed_str(&name)?;
            // `read_asset_bytes` skips Bevy's own unapproved-path check:
            // enforce it, matching `AssetServer::load`'s default.
            if path.is_unapproved() {
                return Err(format!("resource escapes the asset root: {name}").into());
            }
            // Reading through the load context makes each `.ftl` a
            // dependency of the bundle: with Bevy's file watcher, editing one
            // reloads every bundle that uses it.
            let bytes = load_context.read_asset_bytes(path.clone()).await?;
            let source = String::from_utf8(bytes)?;
            // A resource with syntax errors still loads: Fluent keeps its
            // valid messages and the errors name the broken ones.
            let resource = match FluentResource::try_new(source.clone()) {
                Ok(resource) => resource,
                Err((resource, errors)) => {
                    for error in errors {
                        error!("parsing `{path}`: {error}");
                    }
                    resource
                }
            };
            sources.push(source);
            // Duplicate message ids across resources: the earlier one wins.
            if let Err(errors) = bundle.add_resource(Arc::new(resource)) {
                for error in errors {
                    warn!("adding `{path}`: {error}");
                }
            }
        }
        Ok(LocaleBundle(Arc::new(BundleState {
            bundle,
            markdown: manifest
                .markdown
                .then(|| markdown_cache(&sources)),
        })))
    }

    fn extensions(&self) -> &[&str] {
        &["ftl.ron"]
    }
}

/// The [`LocaleBundle`] that `data-l10n-id` attributes resolve against.
/// `None` (the default): no localization, elements show their own content.
/// Set a different handle to switch language; every `HtmlUi` re-localizes
/// once the bundle is loaded, so preload bundles you'll switch to.
#[derive(Resource, Default, Clone, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct ActiveLocale(pub Option<Handle<LocaleBundle>>);

impl ActiveLocale {
    pub fn new(bundle: Handle<LocaleBundle>) -> Self {
        Self(Some(bundle))
    }

    /// Switches to `bundle`.
    pub fn set(&mut self, bundle: Handle<LocaleBundle>) {
        self.0 = Some(bundle);
    }
}

/// Translations for an `HtmlUi`'s `data-l10n-id` elements (`Err` holds why a
/// lookup failed). Maintained by
/// [`HtmlUiSystems::Localize`](crate::HtmlUiSystems::Localize); see
/// [`HtmlDocument::outline`](crate::template::HtmlDocument::outline) to inspect it.
#[derive(Component, Default)]
pub struct LocalizedText(pub(crate) HashMap<tl::NodeHandle, Result<String, String>>);

pub(crate) fn localize(
    mut events: MessageReader<AssetEvent<LocaleBundle>>,
    locale: Res<ActiveLocale>,
    bundles: Res<Assets<LocaleBundle>>,
    mut views: Query<(Ref<RenderedHtml>, &mut LocalizedText)>,
) {
    let Some(handle) = &locale.0 else {
        events.clear();
        if locale.is_changed() {
            for (_, mut localized) in &mut views {
                if !localized.0.is_empty() {
                    localized.0.clear();
                }
            }
        }
        return;
    };
    // `.fold`, not `.any`: the iterator consumes events one at a time —
    // `.any` stops at the first match, leaving a second matching event
    // (Modified then LoadedWithDependencies on a hot reload) to re-serve
    // next frame with an extra re-resolve and rebuild.
    let bundle_ready = events.read().fold(false, |ready, event| {
        ready
            || match event {
                AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => {
                    *id == handle.id()
                }
                _ => false,
            }
    });
    // Not loaded (yet, or ever — a failed asset emits no event): its load
    // event triggers localization. A render that changed meanwhile must
    // not inherit the previous document's translations (they key on the
    // old DOM's handles and would land on the wrong elements): drop them,
    // the elements show their fallback content until the bundle arrives.
    let Some(bundle) = bundles.get(handle) else {
        for (rendered, mut localized) in &mut views {
            if rendered.is_changed() {
                localized.0.clear();
            }
        }
        return;
    };
    for (rendered, mut localized) in &mut views {
        if !bundle_ready && !rendered.is_changed() && !locale.is_changed() {
            continue;
        }
        let RenderedHtml::Ready(document) = &*rendered else {
            continue;
        };
        localized.0 = resolve_all(
            document.dom(),
            &bundle.0.bundle,
            bundle.0.markdown.as_ref(),
        );
    }
}

/// Resolves every `data-l10n-id` element in `dom` against `bundle`. With
/// the Markdown cache (ADR 0020 Decision 3), each value substitutes its
/// formatted arguments into the message's pre-converted HTML; without it,
/// each formatted value converts to HTML (`md::to_html`).
fn resolve_all(
    dom: &tl::VDom,
    bundle: &ConcurrentBundle,
    markdown: Option<&MarkdownCache>,
) -> HashMap<tl::NodeHandle, Result<String, String>> {
    let parser = dom.parser();
    let mut out = HashMap::default();
    let mut stack: Vec<tl::NodeHandle> = dom.children().to_vec();
    while let Some(handle) = stack.pop() {
        let Some(tl::Node::Tag(tag)) = handle.get(parser) else {
            continue;
        };
        let attributes = tag.attributes();
        match attributes.get("data-l10n-id").flatten() {
            Some(id) => {
                let args = attributes.get("data-l10n-args").flatten();
                let id = decode_entities(&id.as_utf8_str());
                let args = args.map(|args| decode_entities(&args.as_utf8_str()));
                let text = translate(bundle, &id, args.as_deref());
                out.insert(
                    handle,
                    match markdown.and_then(|cache| cache.messages.get(&id)) {
                        Some(entry) => {
                            // The cache is an accelerator: a text that
                            // doesn't split (an argument swallowing a
                            // literal part) falls back to converting the
                            // formatted text directly.
                            text.map(|text| {
                                substitute(entry, &text)
                                    .unwrap_or_else(|| md::to_html(&text))
                            })
                        }
                        None if markdown.is_some() => {
                            text.map(|text| md::to_html(&text))
                        }
                        None => text,
                    },
                );
            }
            // Translated elements replace their children, so only descend
            // into untranslated ones.
            None => stack.extend(tag.children().top().iter().copied()),
        }
    }
    out
}

pub(crate) fn translate<R, M>(
    bundle: &FluentBundle<R, M>,
    id: &str,
    args_json: Option<&str>,
) -> Result<String, String>
where
    R: Borrow<FluentResource>,
    M: MemoizerKind,
{
    let message = bundle
        .get_message(id)
        .ok_or_else(|| format!("missing message `{id}`"))?;
    let pattern = message
        .value()
        .ok_or_else(|| format!("message `{id}` has no value"))?;

    let args = args_json
        .map(|json| parse_args(json).map_err(|err| format!("`{id}` data-l10n-args: {err}")))
        .transpose()?;

    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args.as_ref(), &mut errors);
    if let Some(err) = errors.first() {
        return Err(format!("formatting `{id}`: {err}"));
    }
    // Drop the Unicode bidi isolation marks Fluent wraps placeables in; the
    // bundle is shared behind an `Arc`, so `set_use_isolating` isn't reachable.
    Ok(text
        .chars()
        .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
        .collect())
}

/// Fuzzing entry (ADR 0020's `markdown: true` path, exercised by
/// `fuzz::translate`): the cache is built from the same source, and the
/// formatted value substitutes into it — never panics, errors are values.
#[cfg(feature = "fuzzing")]
pub(crate) fn markdown_fuzz(ftl: &str, id: &str, args_json: &str) -> Option<String> {
    let cache = markdown_cache(&[ftl.to_owned()]);
    let entry = cache.messages.get(id)?;
    let text = translate_markdown_value(ftl, id, args_json)?;
    substitute(entry, &text)
}

/// The `markdown: true` per-frame path, split out for the fuzz harness:
/// format the message against a plain (non-markdown) bundle, then
/// substitute into the cache — the same text the real bundle produces.
#[cfg(feature = "fuzzing")]
fn translate_markdown_value(ftl: &str, id: &str, args_json: &str) -> Option<String> {
    let resource = fluent::FluentResource::try_new(ftl.to_owned()).ok()?;
    let locale: unic_langid::LanguageIdentifier = "en".parse().ok()?;
    let mut bundle = fluent::concurrent::FluentBundle::new_concurrent(vec![locale]);
    bundle.add_resource(resource).ok()?;
    translate(&bundle, id, Some(args_json)).ok()
}

/// Substitutes the formatted arguments into a cached pattern's HTML. The
/// cached `parts` are the pattern's `n + 1` literal runs (empty at the
/// edges when the pattern starts or ends with a placeable; multi-line
/// runs joined): `text` must be `parts[0]`, then each placeable's
/// formatted value (bidi isolation marks stripped), then the next part,
/// and so on — the first part anchors the text's start, the last its
/// end. Each gap replaces its `\u{2}index\u{3}` marker verbatim (the
/// values were escaped by `parse_args`; ADR 0020's Decision 4). `None`:
/// the text doesn't split (a part moved or vanished) — the caller
/// converts the formatted text directly instead.
fn substitute(entry: &MarkdownEntry, text: &str) -> Option<String> {
    const ISOLATES: [char; 2] = ['\u{2068}', '\u{2069}'];
    let last = entry.parts.len() - 1;
    let first = entry.parts.first()?;
    if !text.starts_with(first.as_str()) || !text.ends_with(entry.parts[last].as_str()) {
        return None;
    }
    if last == 0 {
        // No placeables: the text must be the pattern verbatim.
        return Some(entry.html.clone());
    }
    // One value per marker: value k runs from the end of `parts[k]` to
    // the start of `parts[k + 1]`.
    let mut bounds = Vec::with_capacity(last);
    let mut cursor = first.len();
    let limit = text.len() - entry.parts[last].len();
    for part in &entry.parts[1..last] {
        if cursor > limit {
            return None;
        }
        let start = cursor + text[cursor..limit].find(part.as_str())?;
        bounds.push((cursor, start));
        cursor = start + part.len();
    }
    if cursor > limit {
        return None;
    }
    bounds.push((cursor, limit));
    let mut out = entry.html.clone();
    for (index, (start, end)) in bounds.into_iter().enumerate().rev() {
        let value: String = text[start..end]
            .chars()
            .filter(|c| !ISOLATES.contains(c))
            .collect();
        out = out.replace(&marker(index), &value);
    }
    Some(out)
}

/// Builds the Markdown cache from a resource's source (ADR 0020
/// Decision 3): per message without selectors, adjacent placeables, or
/// term/message references, the pattern's `n + 1` literal runs anchor
/// `substitute` (empty at the edges when the pattern starts or ends with
/// a placeable), and the pattern — its placeables replaced by markers —
/// converts to HTML once. Duplicate message ids keep the first (the
/// bundle's own choice). Messages whose conversion leaves a marker in a
/// Markdown link destination don't cache: CommonMark rejects control
/// characters there, so the link is lost.
fn markdown_cache(sources: &[String]) -> MarkdownCache {
    use fluent_syntax::ast::Expression;
    use fluent_syntax::ast::InlineExpression;
    use fluent_syntax::ast::PatternElement;
    let mut cache = MarkdownCache::default();
    for source in sources {
        // Re-parsed here: Fluent's own `FluentResource` keeps its AST
        // private. A message fluent rejected isn't in this resource either.
        let Ok(resource) = fluent_syntax::parser::parse(source.as_str()) else {
            continue;
        };
        for entry in resource.body {
            let fluent_syntax::ast::Entry::Message(message) = entry else {
                continue;
            };
            let Some(id) = message.id.name.split('.').next().map(str::to_owned) else {
                continue;
            };
            let Some(pattern) = &message.value else {
                continue;
            };
            let mut parts: Vec<String> = vec![String::new()];
            let mut html = String::new();
            let mut index = 0;
            let mut placeable_before = false;
            let mut cached = true;
            for element in &pattern.elements {
                match element {
                    PatternElement::TextElement { value } => {
                        placeable_before = false;
                        parts.last_mut().unwrap().push_str(value);
                        html.push_str(value);
                    }
                    PatternElement::Placeable { expression } => {
                        if placeable_before {
                            // Two placeables, no text between: their
                            // formatted values can't be split apart
                            // textually.
                            cached = false;
                            break;
                        }
                        let uncachable = match expression {
                            Expression::Select { .. } => true,
                            Expression::Inline(inline) => matches!(
                                inline,
                                InlineExpression::MessageReference { .. }
                                    | InlineExpression::TermReference { .. }
                            ),
                        };
                        if uncachable {
                            // A select picks per arguments; a term or
                            // message reference expands to markup (its
                            // Markdown must convert), not literal text.
                            cached = false;
                            break;
                        }
                        placeable_before = true;
                        html.push_str(&marker(index));
                        parts.push(String::new());
                        index += 1;
                    }
                }
            }
            if cached {
                let html = md::to_html(&html);
                if html.contains("](\u{2}") {
                    // A marker in a Markdown link destination: CommonMark
                    // rejects control characters there and the link is
                    // lost — convert per value instead.
                    continue;
                }
                cache
                    .messages
                    .entry(id)
                    .or_insert(MarkdownEntry { html, parts });
            }
        }
    }
    cache
}

#[allow(clippy::needless_continue)]
fn parse_args(json: &str) -> Result<FluentArgs<'static>, String> {
    let serde_json::Value::Object(map) =
        serde_json::from_str(json).map_err(|err| err.to_string())?
    else {
        return Err("expected a JSON object".to_owned());
    };
    let mut args = FluentArgs::new();
    for (key, value) in map {
        match value {
            serde_json::Value::Number(number) => match number.as_f64() {
                Some(number) => args.set(key, number),
                None => return Err(format!("`{key}`: number out of range")),
            },
            // Translations are parsed as markup; keep values as text.
            serde_json::Value::String(string) => args.set(key, escape_html(&string)),
            serde_json::Value::Bool(flag) => args.set(key, flag.to_string()),
            _ => {
                return Err(format!(
                    "`{key}`: only numbers, strings and bools are supported"
                ));
            }
        }
    }
    Ok(args)
}

/// Escapes text for inclusion in markup (inverse of `decode_entities`).
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent::FluentBundle;
    use proptest::prelude::*;

    /// ADR 0020 Decision 3: the cached pattern's HTML substitutes the
    /// formatted arguments at the markers — escaped, the bidi isolation
    /// marks stripped — and falls back (`None`) when the pattern's text
    /// parts no longer anchor the split.
    #[test]
    fn markdown_substitution() {
        // Build the cache the way the loader does: the pattern's text runs
        // anchor, the placeables become markers.
        let source = "row = Item { $i } of **{ $n }** things.\ngreet = Hello { $name }.\n";
        let cache = markdown_cache(&[source.to_owned()]);
        let entry = cache.messages.get("row").expect("row cached");

        // The formatted text (isolation marks included, as fluent's).
        let text = "Item \u{2068}3\u{2069} of **\u{2068}7\u{2069}** things.";
        let html = substitute(entry, text).expect("substituted");
        // The isolation marks are stripped from the values (the plain path
        // strips them from the whole text).
        assert!(
            html.contains("Item 3 of <strong class=\"markdown\">7</strong> things."),
            "{html}"
        );
        // The substituted values land verbatim: they were escaped by
        // `parse_args` before fluent formatted them (escaping again here
        // double-escaped `O'Brien & Co`).
        let entry = cache.messages.get("greet").expect("greet cached");
        let html = substitute(entry, "Hello \u{2068}&lt;em&gt;*&lt;/em&gt;\u{2069}.").expect("substituted");
        assert!(
            html.contains("&lt;em&gt;*&lt;/em&gt;"),
            "the pre-escaped value lands as-is: {html}"
        );

        // A text part moved or vanished: the split fails, the caller falls
        // back to converting the formatted value.
        assert_eq!(substitute(entry, "Completely different"), None);
    }

    /// An en-US bundle with Fluent's default bidi isolation on (as
    /// [`LocaleBundle`]s have it).
    fn bundle(ftl: &str) -> FluentBundle<FluentResource> {
        // The element type is inferred.
        let mut bundle = FluentBundle::new(vec!["en-US".parse().unwrap()]);
        bundle
            .add_resource(FluentResource::try_new(ftl.to_owned()).unwrap())
            .unwrap();
        bundle
    }

    const ITEMS: &str = "items = { $n ->\n    [one] one item\n   *[other] { $n } items\n}\n";

    /// Text dense in markup characters and entity look-alikes (`&lt;`
    /// typed as text), plus arbitrary characters other than the bidi
    /// isolation marks `translate` strips.
    const MARKUP_TEXT: &str =
        "(&lt;|&gt;|&amp;|&quot;|&#39;|[<>&\"';# a-z]|[^\u{2068}\u{2069}]){0,24}";

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// `escape_html` is undone exactly by `decode_entities` and leaves no
        /// markup-significant character behind. Catches an escape the
        /// decoder doesn't know (entities shown in the UI) or a missing one
        /// (string args injecting markup into translations).
        #[test]
        fn escape_then_decode_is_identity(s in MARKUP_TEXT) {
            let escaped = escape_html(&s);
            prop_assert!(!escaped.contains(['<', '>', '"', '\'']), "{escaped:?}");
            prop_assert_eq!(decode_entities(&escaped), s);
        }

        /// JSON numbers stay Fluent numbers, so English plural selection
        /// follows CLDR (`one` for exactly 1, `other` otherwise) and numbers
        /// print as fluent-rs formats them (no grouping). Catches args
        /// passed as strings (always `other`, `1 items`).
        #[test]
        fn integer_args_select_english_plurals(n in prop_oneof![0u32..4, 0u32..100_000]) {
            let expected = if n == 1 { "one item".to_owned() } else { format!("{n} items") };
            let json = format!(r#"{{"n": {n}}}"#);
            prop_assert_eq!(translate(&bundle(ITEMS), "items", Some(&json)), Ok(expected));
        }

        /// A string arg comes out as markup that decodes back to the
        /// string: escaped for the translation parser, and only the bidi
        /// marks Fluent adds are removed.
        #[test]
        fn string_args_survive_as_escaped_text(s in MARKUP_TEXT) {
            let json = serde_json::json!({ "s": s }).to_string();
            let text = translate(&bundle("m = [{ $s }]"), "m", Some(&json)).unwrap();
            prop_assert!(!text.contains('<'), "{text:?}");
            prop_assert_eq!(decode_entities(&text), format!("[{s}]"));
        }

        /// Every cached entry substitutes exactly what the per-value
        /// conversion of the same formatted text produces — over patterns
        /// of arbitrary shape: leading/trailing placeables (the old
        /// parts/marker alignment lost those), several placeables, and
        /// multi-line patterns (the old cache split parts per line). The
        /// cached entry exists exactly when the message is cacheable
        /// (no select, no adjacent placeables, no term/message
        /// references).
        #[test]
        fn cached_markdown_matches_per_value_conversion(
            lead in prop::bool::ANY,
            trail in prop::bool::ANY,
            second in prop::bool::ANY,
            multiline in prop::bool::ANY,
            n in 0u32..100,
        ) {
            let mid = if second { " and { $n }/2" } else { "" };
            let body = format!("say {n}{mid}");
            let pattern = match (lead, multiline) {
                (true, false) => format!("{{ $n }} {body}"),
                (false, true) => format!("{body}\n\n  and {n} again"),
                (true, true) => format!("{{ $n }} {body}\n\n  and {n} again"),
                (false, false) => body,
            };
            let id = "m";
            let source = format!("{id} = {pattern}\n");
            let cache = markdown_cache(&[source.clone()]);
            if second {
                // Two placeables separated by text are cacheable; the
                // empty-part alignment must still hold.
                prop_assert!(cache.messages.contains_key(id));
            }
            let bundle = bundle(&source);
            let json = serde_json::json!({ "n": n }).to_string();
            let text = translate(&bundle, id, Some(&json)).unwrap();
            let expected = md::to_html(&text);
            match cache.messages.get(id) {
                Some(entry) => {
                    let substituted = substitute(entry, &text);
                    prop_assert_eq!(
                        substituted.as_ref(),
                        Some(&expected),
                        "pattern {:?} text {:?}",
                        pattern,
                        text
                    );
                }
                None => prop_assert_eq!(md::to_html(&text), expected),
            }
        }
    }

    /// Uncacheable patterns: a select, adjacent placeables, a term or
    /// message reference (markup expands per value), and a pattern whose
    /// placeable lands in a Markdown link destination (CommonMark rejects
    /// control characters there and the link is lost). Duplicate ids keep
    /// the first entry (the bundle's choice).
    #[test]
    fn markdown_cache_skips_uncacheable_patterns() {
        let cases = [
            "sel = { $n ->\n   *[other] { $n } items\n}",
            "adjacent = { $a }{ $b }",
            "term = Use { -brand } now",
            "reference = See { other } now",
            "link = See [the docs]({ $url }) now",
        ];
        for source in cases {
            assert!(
                markdown_cache(&[source.to_owned()]).messages.is_empty(),
                "{source:?} should not cache"
            );
        }
        // A duplicate id keeps the first message's pattern.
        let cache = markdown_cache(&[
            "dup = First { $x } one\n".to_owned(),
            "dup = Second { $x } two\n".to_owned(),
        ]);
        let entry = cache.messages.get("dup").expect("dup cached");
        assert!(entry.html.contains("First"), "{:?}", entry.html);
    }

    /// Cached substitution never shows double-escaped text: the values
    /// were escaped once by `parse_args`; a second escape in `substitute`
    /// showed `O&#39;Brien &amp; Co`.
    #[test]
    fn string_args_escape_once_through_the_cache() {
        let source = "apos = Hello { $name }!\n";
        let cache = markdown_cache(&[source.to_owned()]);
        let json = serde_json::json!({ "name": "O'Brien & Co" }).to_string();
        let text = translate(&bundle(source), "apos", Some(&json)).unwrap();
        let html = substitute(cache.messages.get("apos").unwrap(), &text).unwrap();
        assert!(
            html.contains("Hello O&#39;Brien &amp; Co!"),
            "double-escaped? {html}"
        );
        assert!(!html.contains("&amp;#39;"), "double-escaped: {html}");
    }

    /// A message without placeables is cached too: the conversion runs
    /// once at load, not on every localize.
    #[test]
    fn placeable_free_messages_are_cached() {
        let cache = markdown_cache(&["plain = Just **text**.\n".to_owned()]);
        let entry = cache.messages.get("plain").expect("plain cached");
        assert!(entry.html.contains("<strong"));
        assert_eq!(
            substitute(entry, "Just **text**.").as_deref(),
            Some(entry.html.as_str()),
            "the whole text matches the pattern: the cached HTML comes back"
        );
    }

    /// An argument whose text swallows a later literal part: the split
    /// fails (or the parts no longer match) and the caller falls back to
    /// converting the formatted text — the output is still right.
    #[test]
    fn args_containing_literal_parts_fall_back() {
        let source = "pair = A { $x } B { $y } C\n";
        let cache = markdown_cache(&[source.to_owned()]);
        let json = serde_json::json!({ "x": "1 B 2", "y": "3" }).to_string();
        let text = translate(&bundle(source), "pair", Some(&json)).unwrap();
        let expected = md::to_html(&text);
        let entry = cache.messages.get("pair").unwrap();
        match substitute(entry, &text) {
            Some(html) => assert_eq!(html, expected, "the mis-split substituted"),
            None => assert_eq!(md::to_html(&text), expected),
        }
    }

    /// Float JSON numbers are numbers too: `1.0` is `one` (as `JSON.parse`
    /// gives fluent-dom the number 1), `1.5` is `other` and keeps its
    /// fraction. Catches stringifying numbers (`"1.0"` → `other`).
    #[test]
    fn float_args_are_numbers() {
        let bundle = bundle(ITEMS);
        assert_eq!(
            translate(&bundle, "items", Some(r#"{"n": 1.0}"#)),
            Ok("one item".to_owned())
        );
        assert_eq!(
            translate(&bundle, "items", Some(r#"{"n": 1.5}"#)),
            Ok("1.5 items".to_owned())
        );
        // CLDR plural operand `n` is the absolute value: -1 is `one` too.
        assert_eq!(
            translate(&bundle, "items", Some(r#"{"n": -1}"#)),
            Ok("one item".to_owned())
        );
        assert_eq!(
            translate(&bundle, "items", Some(r#"{"n": -2}"#)),
            Ok("-2 items".to_owned())
        );
        // A *string* "1" is not the number 1: it matches only a `[1]` key.
        assert_eq!(
            translate(&bundle, "items", Some(r#"{"n": "1"}"#)),
            Ok("1 items".to_owned())
        );
    }

    /// Bools become the strings `true`/`false`, usable as selector keys.
    #[test]
    fn bool_args_are_strings() {
        let bundle = bundle("m = { $on ->\n    [true] on\n   *[false] off\n}\nraw = { $on }\n");
        assert_eq!(
            translate(&bundle, "m", Some(r#"{"on": true}"#)),
            Ok("on".to_owned())
        );
        assert_eq!(
            translate(&bundle, "m", Some(r#"{"on": false}"#)),
            Ok("off".to_owned())
        );
        assert_eq!(
            translate(&bundle, "raw", Some(r#"{"on": true}"#)),
            Ok("true".to_owned())
        );
    }

    /// Arg names are taken verbatim, including the `-`/`_` Fluent
    /// identifiers allow.
    #[test]
    fn arg_names_with_dashes_and_underscores() {
        let bundle = bundle("m = { $a-b }/{ $c_d }");
        assert_eq!(
            translate(&bundle, "m", Some(r#"{"c_d": "y", "a-b": 2}"#)),
            Ok("2/y".to_owned())
        );
    }

    /// Unsupported or malformed `data-l10n-args` are errors naming the
    /// message (the element falls back to its own content), never a
    /// half-formatted translation with a missing variable.
    #[test]
    fn bad_args_are_errors() {
        let bundle = bundle(ITEMS);
        for json in [
            r#"{"n": null}"#,
            r#"{"n": [1]}"#,
            r#"{"n": {"v": 1}}"#,
            "[1]",
            "3",
            r#""n""#,
            "{n: 1}",
            "",
        ] {
            let result = translate(&bundle, "items", Some(json));
            assert!(
                result.as_ref().is_err_and(|err| err.contains("`items`")),
                "{json:?} gave {result:?}"
            );
        }
    }

    /// Missing messages, value-less messages and unresolved variables are
    /// errors, not text (which would replace the fallback content).
    #[test]
    fn unresolvable_messages_are_errors() {
        let bundle = bundle("attrs-only =\n    .title = T\nneeds = { $x }\n");
        assert!(translate(&bundle, "absent", None).is_err());
        assert!(translate(&bundle, "attrs-only", None).is_err());
        assert!(translate(&bundle, "needs", None).is_err());
        assert!(translate(&bundle, "needs", Some("{}")).is_err());
    }

    /// Only the FSI/PDI marks Fluent wraps placeables in are stripped;
    /// other bidi controls authored in the message stay. Catches a strip
    /// that's too broad (RTL text loses its marks) or missing (invisible
    /// characters in every placeable).
    #[test]
    fn strips_only_placeable_isolation_marks() {
        let bundle = bundle("m = \u{200f}a { $x } \u{200e}b");
        assert_eq!(
            translate(&bundle, "m", Some(r#"{"x": "y"}"#)),
            Ok("\u{200f}a y \u{200e}b".to_owned())
        );
    }
}
