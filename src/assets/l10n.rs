//! Fluent localization of parsed HTML via `data-l10n-id` / `data-l10n-args`,
//! the attribute convention from Fluent's DOM bindings (`fluent-dom`):
//!
//! ```html
//! <h1 data-l10n-id="inventory-title"></h1>
//! <p data-l10n-id="hp-status" data-l10n-args='{"hp": 7, "max": 10}'></p>
//! ```
//!
//! An element with `data-l10n-id` gets the message's formatted value as its
//! text, replacing its children. `data-l10n-args` is a JSON object of Fluent
//! variables (numbers stay numbers, so plural selectors work).
//!
//! The DOM is not mutated (`tl::VDomGuard` only hands out shared borrows);
//! translations live beside it in [`LocalizedText`], keyed by node.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_fluent::prelude::*;
use fluent::{FluentArgs, FluentResource, bundle::FluentBundle, memoizer::MemoizerKind};
use std::borrow::Borrow;

use super::html::{RenderedHtml, decode_entities};

const LOCALE_PATH: &str = "locales/en-US/main.ftl.ron";

pub struct L10nPlugin;

impl Plugin for L10nPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FluentPlugin)
            .add_systems(Startup, load_locale)
            .add_systems(PostUpdate, localize_html_views.after(super::html::render_html_views));
    }
}

/// The Fluent bundle `data-l10n-id` keys resolve against. Swap the handle to
/// change language; every view re-localizes once the new bundle loads.
#[derive(Resource)]
pub struct ActiveLocale(pub Handle<BundleAsset>);

/// Translated text per element carrying `data-l10n-id`. `Err` holds why the
/// lookup failed (missing message, bad args), for display/debugging.
#[derive(Component, Default)]
pub struct LocalizedText(pub HashMap<tl::NodeHandle, Result<String, String>>);

fn load_locale(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(ActiveLocale(asset_server.load(LOCALE_PATH)));
}

fn localize_html_views(
    mut events: MessageReader<AssetEvent<BundleAsset>>,
    locale: Res<ActiveLocale>,
    bundles: Res<Assets<BundleAsset>>,
    mut views: Query<(Ref<RenderedHtml>, &mut LocalizedText)>,
) {
    let bundle_ready = events.read().any(|event| match event {
        AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => {
            *id == locale.0.id()
        }
        _ => false,
    });
    let Some(bundle) = bundles.get(&locale.0) else {
        return;
    };
    for (rendered, mut localized) in &mut views {
        if !bundle_ready && !rendered.is_changed() && !locale.is_changed() {
            continue;
        }
        let RenderedHtml::Ready(document) = &*rendered else {
            continue;
        };
        localized.0 = localize(document.dom(), &**bundle);
    }
}

/// Resolves every `data-l10n-id` element in `dom` against `bundle`.
pub fn localize<R, M>(
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
            serde_json::Value::String(string) => args.set(key, string),
            serde_json::Value::Bool(flag) => args.set(key, flag.to_string()),
            _ => return Err(format!("`{key}`: only numbers, strings and bools are supported")),
        }
    }
    Ok(args)
}
