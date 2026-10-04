//! Property-based tests (proptest) over the same headless pipeline as the
//! vectors: relationships that must hold for *any* generated input
//! (metamorphic properties), not just fixed examples.
//!
//! Like the vectors, these check components, not layout: the dump is the
//! observation, `TestUi` (see `common`) is the harness.

mod common;
use common::TestUi;

use bevy::prelude::*;
use proptest::prelude::*;
use p23::bevy_fluent::BundleAsset;
use p23::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// `padding: T R` builds the same world as the four longhands.
    #[test]
    fn padding_shorthand_equals_longhands(t in 0u8..40, r in 0u8..40) {
        let page = "<p>Text</p>";
        let base = "html { color: #ffffff; font-size: 20px } p { ";
        let shorthand = format!("{base}padding: {t}px {r}px }}");
        let longhands = format!(
            "{base}padding-top: {t}px; padding-right: {r}px; \
             padding-bottom: {t}px; padding-left: {r}px }}"
        );
        let mut a = TestUi::new("prop-pad-shorthand", &[("page.html", page), ("style.css", &shorthand)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        let mut b = TestUi::new("prop-pad-longhands", &[("page.html", page), ("style.css", &longhands)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        prop_assert_eq!(a.settle().dump(), b.settle().dump());
    }

    /// `border-image: …` (shorthand) draws the same frame as its
    /// `-source`/`-slice`/`-repeat` longhands.
    #[test]
    fn border_image_shorthand_equals_longhands(slice in 1u8..12u8, tile in any::<bool>()) {
        let page = "<p>Text</p>";
        let repeat = if tile { "repeat" } else { "stretch" };
        let shorthand = format!(
            r#"html {{ color: #ffffff; font-size: 20px }} p {{ border-image: url("frame.png") {slice} fill {repeat} }}"#
        );
        let longhands = format!(
            r#"html {{ color: #ffffff; font-size: 20px }} p {{
                border-image-source: url("frame.png");
                border-image-slice: {slice} fill;
                border-image-repeat: {repeat};
            }}"#
        );
        let files = [
            ("page.html", page),
            ("style.css", shorthand.as_str()),
        ];
        let mut a = TestUi::new("prop-bi-shorthand", &files)
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        let long_files = [
            ("page.html", page),
            ("style.css", longhands.as_str()),
        ];
        let mut b = TestUi::new("prop-bi-longhands", &long_files)
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        prop_assert_eq!(a.settle().dump(), b.settle().dump());
    }

    /// Rules that match nothing change nothing, no matter what they declare —
    /// even with `!important`.
    #[test]
    fn unmatched_rules_change_nothing(
        ids in proptest::collection::vec("[a-z][a-z0-9]{0,7}", 1..4),
        classes in proptest::collection::vec("[a-z][a-z0-9]{0,7}", 1..4),
        sizes in proptest::collection::vec(8u8..40, 4),
    ) {
        let page = "<p class=\"note\">Text</p><div><p>Inner</p></div>";
        let base = "html { color: #ffffff; font-size: 20px } .note { color: #00ff00 }";
        // Every generated selector names an id or class no element carries.
        let mut junk = String::new();
        for ((id, class), size) in ids.iter().zip(&classes).zip(&sizes) {
            junk.push_str(&format!(
                "\n#{id} {{ font-size: {size}px }}\n.{class} {{ color: #123456 !important }}\n"
            ));
        }
        let (with_junk, without) = (format!("{base}{junk}"), base.to_owned());
        let mut a = TestUi::new("prop-unmatched-junk", &[("page.html", page), ("style.css", &with_junk)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        let mut b = TestUi::new("prop-unmatched-base", &[("page.html", page), ("style.css", &without)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        prop_assert_eq!(a.settle().dump(), b.settle().dump());
    }

    /// Specificity monotonicity: a class rule always beats a type rule, in
    /// either source order.
    #[test]
    fn class_beats_type_selector_regardless_of_order(
        type_color in "[0-9a-f]{6}",
        class_color in "[0-9a-f]{6}",
    ) {
        prop_assume!(type_color != class_color);
        let page = "<p class=\"note\">Text</p>";
        let head = "html { color: #ffffff; font-size: 20px } ";
        let type_rule = format!("p {{ color: #{type_color} }} ");
        let class_rule = format!(".note {{ color: #{class_color} }} ");
        let (type_first, class_first) = (format!("{head}{type_rule}{class_rule}"), format!("{head}{class_rule}{type_rule}"));
        let mut a = TestUi::new("prop-spec-type-first", &[("page.html", page), ("style.css", &type_first)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        let mut b = TestUi::new("prop-spec-class-first", &[("page.html", page), ("style.css", &class_first)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        let dump = a.settle().dump();
        prop_assert!(dump.contains(&format!("#{class_color}")), "class rule must win:\n{dump}");
        prop_assert_eq!(dump, b.settle().dump());
    }

    /// Idempotence: rebuilding with no input change produces the identical
    /// subtree (one generation of children, no leftovers).
    #[test]
    fn rebuild_with_same_context_is_idempotent(n in 0i64..1000) {
        let mut ui = TestUi::new(
            "prop-idempotent",
            &[
                ("page.html", "<p>{{ n }}</p>"),
                ("style.css", "html { color: #ffffff; font-size: 20px }"),
            ],
        )
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new().with("n", &n), Node::default());
        let before = ui.settle().dump();

        let root = ui.root();
        ui.world_mut()
            .get_mut::<TemplateContext>(root)
            .unwrap()
            .insert("n", &n); // same value; only the mutation is detected
        let after = ui.settle().dump();
        prop_assert_eq!(before, after);
    }

    /// Round trip: theme X → Y → X ends where it started, including the
    /// `html` rule's box properties (dropped and re-applied).
    #[test]
    fn theme_round_trip(padding in 0u8..20) {
        let swap_css = format!("html {{ color: #ffffff; font-size: 20px; padding: {padding}px }}");
        let files = [
            ("page.html", "<p>Text</p>"),
            (
                "framed.css",
                r#"html { color: #ffffff; font-size: 20px; border-image: url("frame.png") 4 fill stretch; border-width: 16px; padding: 10px }"#,
            ),
            (
                "plain.css",
                "html { color: #00ff00; font-size: 12px; padding: 0px }",
            ),
            (
                "swap.css",
                swap_css.as_str(),
            ),
        ];
        let mut ui = TestUi::new("prop-theme-round-trip", &files)
        .stylesheet("framed.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
        let start = ui.settle().dump();

        let server = ui.world_mut().resource::<AssetServer>().clone();
        let plain: Handle<Stylesheet> = server.load("plain.css");
        ui.world_mut().insert_resource(DefaultStylesheet::new(plain));
        ui.settle();
        let swapped: Handle<Stylesheet> = server.load("framed.css");
        ui.world_mut().insert_resource(DefaultStylesheet::new(swapped));
        prop_assert_eq!(start, ui.settle().dump());
    }

    /// Round trip: locale A → B → A and context v → w → v end where they
    /// started.
    #[test]
    fn locale_and_context_round_trip(v in 0i64..100, w in 0i64..100) {
        prop_assume!(v != w);
        let mut ui = TestUi::new(
            "prop-locale-round-trip",
            &[
                ("page.html", r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>"#),
                ("style.css", "html { color: #ffffff; font-size: 20px }"),
                ("locales/en-US/main.ftl.ron", r#"(locale: "en-US", resources: ["ui.ftl"])"#),
                ("locales/en-US/ui.ftl", "count = { $n } items"),
                ("locales/de/main.ftl.ron", r#"(locale: "de", resources: ["ui.ftl"])"#),
                ("locales/de/ui.ftl", "count = { $n } Dinge"),
            ],
        )
        .stylesheet("style.css")
        .locale("locales/en-US/main.ftl.ron")
        .spawn("page.html", TemplateContext::new().with("n", &v), Node::default());
        let start = ui.settle().dump();

        let server = ui.world_mut().resource::<AssetServer>().clone();
        let de: Handle<BundleAsset> = server.load("locales/de/main.ftl.ron");
        ui.world_mut().insert_resource(ActiveLocale::new(de));
        let root = ui.root();
        ui.world_mut().get_mut::<TemplateContext>(root).unwrap().insert("n", &w);
        ui.settle();

        let en: Handle<BundleAsset> = server.load("locales/en-US/main.ftl.ron");
        ui.world_mut().insert_resource(ActiveLocale::new(en));
        ui.world_mut().get_mut::<TemplateContext>(root).unwrap().insert("n", &v);
        prop_assert_eq!(start, ui.settle().dump());
    }
}
