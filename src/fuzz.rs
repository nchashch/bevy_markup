//! Fuzzing harnesses, behind the `fuzzing` cargo feature (used by the
//! `fuzz/`, `honggfuzz/`, `fuzzcheck/` and `test-fuzz/` drivers).
//! `#[doc(hidden)]` and not covered by semver: these call internal glue
//! directly, because the public API reaches it only through a full Bevy app,
//! which is far too slow per fuzz iteration.
//!
//! Contract for every function: arbitrary `&str` inputs may produce `Err` or
//! arbitrary output, but must never panic, hang, or abort.

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::prelude::*;

use crate::build::{BoxStyle, Styler, root_style};
use crate::cascade::{HtmlStyles, image_urls};
use crate::fonts::FontFamilies;
use crate::html::HtmlElement;
use crate::l10n::LocalizedText;
use crate::style::Stylesheet;
use crate::template::HtmlTemplate;

/// A stand-in `frame.png`: 32×24 RGBA, so `%` slices resolve to the same
/// arithmetic as the test fixture.
fn fake_frame() -> Image {
    Image::new_fill(
        wgpu_types::Extent3d {
            width: 32,
            height: 24,
            depth_or_array_layers: 1,
        },
        wgpu_types::TextureDimension::D2,
        &[255; 4],
        wgpu_types::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Tera compile + render with a JSON context, then `tl` parse + outline +
/// entity decoding: the whole Render stage on arbitrary sources.
pub fn render_html(source: &str, context_json: &str) -> Result<String, String> {
    let name = "fuzz/template.html";
    let mut tera = tera::Tera::new();
    tera.add_raw_template(name, source).map_err(|err| err.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(context_json).map_err(|err| err.to_string())?;
    let context = tera::Context::from_serialize(&value).map_err(|err| err.to_string())?;
    let template = HtmlTemplate { tera, name: name.to_owned() };
    let document = template.render(&context).map_err(|err| err.to_string())?;
    Ok(document.outline(&LocalizedText::default()))
}

/// lightningcss parse + cascade over synthetic elements + the `html` root
/// box, with every `url()` bound to the fake frame image (exercises slice
/// arithmetic and the style cache). Errors are values, not crashes.
pub fn cascade(css: &str) -> Result<String, String> {
    use lightningcss::stylesheet::{ParserOptions, StyleSheet};
    use lightningcss::traits::IntoOwned;

    let parsed = StyleSheet::parse(css, ParserOptions::default()).map_err(|err| err.to_string())?;
    let sheet = parsed.into_owned();
    let urls = image_urls(&sheet);
    let mut assets = Assets::<Image>::default();
    let handles: Vec<_> = (0..urls.len()).map(|_| assets.add(fake_frame())).collect();
    let stylesheet = Stylesheet { sheet, image_urls: urls, images: handles };

    let styles = HtmlStyles::from_sheet(&stylesheet.sheet);
    let fonts = FontFamilies::default();
    let root = root_style(&styles, &fonts, &assets);
    let styler = Styler {
        styles: &styles,
        fonts: &fonts,
        root_size: root.size,
        sheet: Some(&stylesheet),
        images: &assets,
    };

    let elements = [
        HtmlElement { tag: "html".into(), id: None, classes: vec![] },
        HtmlElement { tag: "p".into(), id: None, classes: vec![] },
        HtmlElement { tag: "p".into(), id: Some("lead".into()), classes: vec!["note".into()] },
        HtmlElement { tag: "div".into(), id: None, classes: vec!["panel".into(), "wide".into()] },
        HtmlElement { tag: "li".into(), id: None, classes: vec![] },
        HtmlElement { tag: "pre".into(), id: None, classes: vec![] },
    ];
    let mut out = String::new();
    // One hit per element, then a repeat: exercises the cascade's match cache.
    for round in 0..2 {
        for element in &elements {
            let style = styler.style_of(element, root);
            let boxed: BoxStyle = styler.box_of(element);
            std::fmt::write(
                &mut out,
                format_args!(
                    "round {round} {}#{}{:?}: color={:?} family={:?} size={} bold={} italic={} \
                     border={:?} padding={:?} bg={:?} image={} gap={:?}\n",
                    element.tag,
                    element.id.as_deref().unwrap_or(""),
                    element.classes,
                    style.color,
                    style.family,
                    style.size,
                    style.bold,
                    style.italic,
                    boxed.border,
                    boxed.padding,
                    boxed.background,
                    boxed.image.as_ref().map_or(0, |_| 1),
                    boxed.row_gap,
                ),
            )
            .ok();
        }
    }
    Ok(out)
}

/// Fluent bundle compilation + `translate` with arbitrary message ids and
/// `data-l10n-args` JSON: parse failures and missing messages are values.
pub fn translate(ftl: &str, id: &str, args_json: &str) -> Result<String, String> {
    // Unparsable FTL is skipped: this fluent version drops the partial
    // resource, so there is no recovery path to fuzz.
    let resource = fluent::FluentResource::try_new(ftl.to_owned())
        .map_err(|errors| format!("{errors:?}"))?;
    let locale: unic_langid::LanguageIdentifier =
        "en".parse().map_err(|err| format!("{err:?}"))?;
    let mut bundle = fluent::concurrent::FluentBundle::new_concurrent(vec![locale]);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("{errors:?}"))?;
    crate::l10n::translate(&bundle, id, Some(args_json))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Found by honggfuzz (`ftl` target): fluent-syntax 0.11.1 slices source
    /// text at byte ranges that can land inside a multi-byte character —
    /// here a broken `\U` escape after one. The vendored
    /// `vendor/fluent-syntax` patch clamps to char boundaries; this must
    /// never panic.
    #[test]
    fn broken_unicode_escape_after_multibyte_char_does_not_panic() {
        let ftl = "u={\"\\U\u{fffd}";
        let _ = translate(ftl, "", "");
    }

    /// Same class of bug in the other direction (byte index 100 inside a
    /// multi-byte character); also from honggfuzz.
    #[test]
    fn long_multibyte_error_context_does_not_panic() {
        let ftl = format!("x = {}{{{}", "é".repeat(48), "é");
        let _ = translate(&ftl, "", "");
    }

    /// bug_0015: placeables nested far beyond the vendored parser's limit
    /// (`MAX_PLACEABLE_DEPTH`, 100) are a parse error, not a stack overflow
    /// — even on a 2 MB thread, where an unbounded debug build overflowed
    /// between 200 and 400 levels.
    #[test]
    fn deeply_nested_placeables_are_an_error_not_a_stack_overflow() {
        let nested = |depth: usize| format!("x = {}\"a\"{}\n", "{".repeat(depth), "}".repeat(depth));
        let deep = nested(100_000);
        let result = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || translate(&deep, "x", "{}"))
            .unwrap()
            .join()
            .expect("no stack overflow");
        assert!(result.is_err(), "{result:?}");
    }

    /// Nesting up to the limit still parses and formats, and the limit
    /// leaves later entries alone (the deep entry becomes Junk).
    #[test]
    fn nesting_up_to_the_limit_still_works() {
        let nested = |depth: usize| format!("{}\"a\"{}", "{".repeat(depth), "}".repeat(depth));
        assert_eq!(translate(&format!("x = {}\n", nested(100)), "x", "{}"), Ok("a".to_owned()));
        let resource = fluent::FluentResource::try_new(format!("deep = {}\nok = fine\n", nested(101)));
        let (resource, errors) = resource.expect_err("depth 101 is an error");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let ids: Vec<&str> = resource
            .entries()
            .filter_map(|entry| match entry {
                fluent_syntax::ast::Entry::Message(message) => Some(message.id.name),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["ok"], "the entry after the deep one survives");
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// The cascade's per-element style cache is transparent: the second
        /// round over the same elements (all cache hits) reports exactly
        /// what the first computed, for stylesheets built from selectors
        /// matching the harness elements and every mapped property. Catches
        /// a cache key missing a part (id/classes) or a cached value that
        /// depends on the first caller's inherited style.
        #[test]
        fn style_cache_is_transparent(
            rules in proptest::collection::vec(
                (
                    proptest::sample::select(&["html", "p", "p#lead", ".note", "div.panel.wide", ".wide", "*", "li", "pre"][..]),
                    proptest::sample::select(&[
                        "color: #123456", "font-size: 1.5em", "font-size: 2rem", "font-size: 12px",
                        "font-weight: bold", "font-style: italic", "padding: 3px 4px", "border-width: 2px",
                        "background-color: red", "gap: 5px", "font-family: serif",
                        r#"border-image: url("f.png") 25% fill round"#,
                    ][..]),
                    proptest::bool::ANY,
                ),
                1..8,
            )
        ) {
            let css: String = rules
                .iter()
                .map(|(selector, declaration, important)| {
                    format!("{selector} {{ {declaration}{} }}\n", if *important { " !important" } else { "" })
                })
                .collect();
            let out = cascade(&css).unwrap();
            let (first, second): (Vec<&str>, Vec<&str>) = out.lines().partition(|line| line.starts_with("round 0 "));
            proptest::prop_assert_eq!(first.len(), second.len());
            for (a, b) in first.iter().zip(&second) {
                proptest::prop_assert_eq!(&a["round 0 ".len()..], &b["round 1 ".len()..], "css:\n{}", css);
            }
        }
    }
}
