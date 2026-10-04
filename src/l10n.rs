//! Fluent localization of HTML via `data-l10n-id` / `data-l10n-args`, the
//! attribute convention from Fluent's DOM bindings (`fluent-dom`):
//!
//! ```html
//! <h1 data-l10n-id="inventory-title">Inventory</h1>
//! <p data-l10n-id="hp-status" data-l10n-args='{"hp": 7, "max": 10}'></p>
//! ```
//!
//! An element with `data-l10n-id` gets the message's formatted value as its
//! content, replacing its children; its own content is the fallback when the
//! message is missing or no [`ActiveLocale`] is set. `data-l10n-args` is a JSON
//! object of Fluent variables (numbers stay numbers, so plural selectors work);
//! a whole map can come from one Tera variable: `data-l10n-args='{{ args }}'`.
//!
//! Translations are markup, like fluent-dom's "DOM overlays": a value may
//! contain inline elements (`Press <kbd>Ctrl</kbd>…`), styled by the CSS like
//! document elements. In `.ftl` values write a literal `<`/`&` as
//! `&lt;`/`&amp;` and a literal `{`/`}` as `{"{"}`/`{"}"}`. String args are
//! HTML-escaped before formatting, so values like `Ada <The Brave>` stay text.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_fluent::BundleAsset;
use fluent::{FluentArgs, FluentResource, bundle::FluentBundle, memoizer::MemoizerKind};
use std::borrow::Borrow;

use crate::html::RenderedHtml;
use crate::template::decode_entities;

/// The Fluent bundle (`*.ftl.ron`, loaded via bevy_fluent) that
/// `data-l10n-id` attributes resolve against. `None` (the default): no
/// localization, elements show their own content. Set a different handle to
/// switch language; every `HtmlUi` re-localizes once the bundle is loaded, so
/// preload bundles you'll switch to.
#[derive(Resource, Default, Clone, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct ActiveLocale(pub Option<Handle<BundleAsset>>);

impl ActiveLocale {
    pub fn new(bundle: Handle<BundleAsset>) -> Self {
        Self(Some(bundle))
    }

    /// Switches to `bundle`.
    pub fn set(&mut self, bundle: Handle<BundleAsset>) {
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
    mut events: MessageReader<AssetEvent<BundleAsset>>,
    locale: Res<ActiveLocale>,
    bundles: Res<Assets<BundleAsset>>,
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
    let bundle_ready = events.read().any(|event| match event {
        AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => {
            *id == handle.id()
        }
        _ => false,
    });
    // Not loaded yet: its load event triggers localization.
    let Some(bundle) = bundles.get(handle) else {
        return;
    };
    for (rendered, mut localized) in &mut views {
        if !bundle_ready && !rendered.is_changed() && !locale.is_changed() {
            continue;
        }
        let RenderedHtml::Ready(document) = &*rendered else {
            continue;
        };
        localized.0 = resolve_all(document.dom(), &**bundle);
    }
}

/// Resolves every `data-l10n-id` element in `dom` against `bundle`.
fn resolve_all<R, M>(
    dom: &tl::VDom,
    bundle: &FluentBundle<R, M>,
) -> HashMap<tl::NodeHandle, Result<String, String>>
where
    R: Borrow<FluentResource>,
    M: MemoizerKind,
{
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
                out.insert(handle, translate(bundle, &id, args.as_deref()));
            }
            // Translated elements replace their children, so only descend
            // into untranslated ones.
            None => stack.extend(tag.children().top().iter().copied()),
        }
    }
    out
}

fn translate<R, M>(
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
    Ok(text.chars().filter(|c| !matches!(c, '\u{2068}' | '\u{2069}')).collect())
}

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
            _ => return Err(format!("`{key}`: only numbers, strings and bools are supported")),
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
