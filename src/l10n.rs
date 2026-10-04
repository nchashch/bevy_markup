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

#[cfg(test)]
mod tests {
    use super::*;
    use fluent::FluentBundle;
    use proptest::prelude::*;

    /// An en-US bundle with Fluent's default bidi isolation on (as
    /// bevy_fluent's bundles have it).
    fn bundle(ftl: &str) -> FluentBundle<FluentResource> {
        // The element type is inferred: `unic-langid` is only a direct
        // dependency with the `fuzzing` feature.
        let mut bundle = FluentBundle::new(vec!["en-US".parse().unwrap()]);
        bundle.add_resource(FluentResource::try_new(ftl.to_owned()).unwrap()).unwrap();
        bundle
    }

    const ITEMS: &str = "items = { $n ->\n    [one] one item\n   *[other] { $n } items\n}\n";

    /// Text dense in markup characters and entity look-alikes (`&lt;`
    /// typed as text), plus arbitrary characters other than the bidi
    /// isolation marks `translate` strips.
    const MARKUP_TEXT: &str = "(&lt;|&gt;|&amp;|&quot;|&#39;|[<>&\"';# a-z]|[^\u{2068}\u{2069}]){0,24}";

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
    }

    /// Float JSON numbers are numbers too: `1.0` is `one` (as `JSON.parse`
    /// gives fluent-dom the number 1), `1.5` is `other` and keeps its
    /// fraction. Catches stringifying numbers (`"1.0"` → `other`).
    #[test]
    fn float_args_are_numbers() {
        let bundle = bundle(ITEMS);
        assert_eq!(translate(&bundle, "items", Some(r#"{"n": 1.0}"#)), Ok("one item".to_owned()));
        assert_eq!(translate(&bundle, "items", Some(r#"{"n": 1.5}"#)), Ok("1.5 items".to_owned()));
        // CLDR plural operand `n` is the absolute value: -1 is `one` too.
        assert_eq!(translate(&bundle, "items", Some(r#"{"n": -1}"#)), Ok("one item".to_owned()));
        assert_eq!(translate(&bundle, "items", Some(r#"{"n": -2}"#)), Ok("-2 items".to_owned()));
        // A *string* "1" is not the number 1: it matches only a `[1]` key.
        assert_eq!(translate(&bundle, "items", Some(r#"{"n": "1"}"#)), Ok("1 items".to_owned()));
    }

    /// Bools become the strings `true`/`false`, usable as selector keys.
    #[test]
    fn bool_args_are_strings() {
        let bundle = bundle("m = { $on ->\n    [true] on\n   *[false] off\n}\nraw = { $on }\n");
        assert_eq!(translate(&bundle, "m", Some(r#"{"on": true}"#)), Ok("on".to_owned()));
        assert_eq!(translate(&bundle, "m", Some(r#"{"on": false}"#)), Ok("off".to_owned()));
        assert_eq!(translate(&bundle, "raw", Some(r#"{"on": true}"#)), Ok("true".to_owned()));
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
