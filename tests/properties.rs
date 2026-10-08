//! Property-based tests over the same headless pipeline as the vectors:
//! relationships that must hold for *any* generated input (metamorphic
//! properties), not just fixed examples.
//!
//! Written with `test_strategy`'s `#[proptest]` (strategies live on the
//! parameters, so the bodies stay rustfmt-able).
//!
//! Like the vectors, these check components, not layout: the dump is the
//! observation, `TestUi` (see `common`) is the harness.

mod common;
use common::TestUi;

use bevy::prelude::*;
use bevy_markup::prelude::*;
use proptest::strategy::Strategy;
use proptest::{prop_assert, prop_assert_eq};
use test_strategy::proptest;

/// Regex strategy for CSS color hex digits.
fn hex6() -> proptest::strategy::BoxedStrategy<String> {
    proptest::string::string_regex("[0-9a-f]{6}")
        .unwrap()
        .boxed()
}

/// Regex strategy for an unmatched class/id name.
fn name() -> proptest::strategy::BoxedStrategy<String> {
    proptest::string::string_regex("[a-z][a-z0-9]{0,7}")
        .unwrap()
        .boxed()
}

/// A generated stylesheet over a fixed set of selectors and declarations,
/// including ones that change structure (box properties need wrappers) and
/// layout.
fn generated_sheet() -> proptest::strategy::BoxedStrategy<String> {
    const SELECTORS: [&str; 6] = ["html", "p", ".box", "b", "li", "pre"];
    const DECLARATIONS: [&str; 10] = [
        "color: #ff0000",
        "color: #00ff00",
        "font-size: 30px",
        "font-family: Spectral",
        "font-weight: bold",
        "padding: 3px",
        "background-color: #102030",
        "border-width: 2px",
        "display: none",
        "flex-direction: row",
    ];
    proptest::collection::vec((0..SELECTORS.len(), 0..DECLARATIONS.len()), 0..6)
        .prop_map(|rules| {
            rules
                .into_iter()
                .map(|(selector, declaration)| {
                    format!(
                        "{} {{ {} }}\n",
                        SELECTORS[selector], DECLARATIONS[declaration]
                    )
                })
                .collect()
        })
        .boxed()
}

/// Restyling never disagrees with building: swapping from any stylesheet A
/// to B gives exactly the world a fresh build with B gives (whether the
/// swap restyled in place or had to rebuild for a new structure).
#[proptest(cases = 24)]
fn restyle_matches_a_fresh_build(
    #[strategy(generated_sheet())] a: String,
    #[strategy(generated_sheet())] b: String,
) {
    let page = r#"<p>Plain <b>bold</b></p><div class="box"><p>Inside</p></div><ul><li>Item</li></ul><pre>pre</pre>"#;
    let mut swapped = TestUi::new(
        "prop-restyle",
        &[("page.html", page), ("a.css", &a), ("b.css", &b)],
    )
    .stylesheet("a.css");
    let b_sheet = swapped.load::<Stylesheet>("b.css");
    let mut swapped = swapped.spawn("page.html", TemplateContext::new(), Node::default());
    swapped.settle();
    swapped.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(b_sheet);
    let mut fresh = TestUi::new("prop-fresh", &[("page.html", page), ("b.css", &b)])
        .stylesheet("b.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
    prop_assert_eq!(swapped.settle().dump(), fresh.settle().dump());
}

/// Fragments for [`content_update_matches_a_fresh_build`]: keyed and
/// unkeyed elements, an `id` reused with another tag, nested containers,
/// rich text, signals and inline styles.
const FRAGMENTS: &[&str] = &[
    r#"<p id="a">A</p>"#,
    r#"<p id="a">A changed</p>"#,
    r#"<section id="a"><p>A as a section</p></section>"#,
    "<p>loose</p>",
    "<p>Plain <b>bold</b> tail</p>",
    r#"<div id="d"><p>one</p><p>two</p></div>"#,
    r#"<div id="d"><p>two</p></div>"#,
    r#"<div class="box"><p>boxed</p></div>"#,
    "<ul><li>i</li><li>j</li></ul>",
    r#"<div id="btn" data-on-click="go" data-with='{"n": 1}'><p>Go</p></div>"#,
    r#"<p style="color: #ff0000; opacity: 0.5">styled</p>"#,
];

/// Updating the content in place never disagrees with building: rendering
/// document A and then B gives exactly the world a fresh build of B gives,
/// whatever was kept, replaced, inserted or removed on the way.
#[proptest(cases = 48)]
fn content_update_matches_a_fresh_build(
    #[strategy(proptest::collection::vec(0..FRAGMENTS.len(), 0..6))] a: Vec<usize>,
    #[strategy(proptest::collection::vec(0..FRAGMENTS.len(), 0..6))] b: Vec<usize>,
) {
    let doc = |picks: &[usize]| picks.iter().map(|&i| FRAGMENTS[i]).collect::<String>();
    let page = "{{ doc | safe }}";
    let css = "html { color: #ffffff } .box { padding: 4px; background-color: #102030 } \
               b { color: #00ff00 } #a { color: #0000ff }";
    let mut updated = TestUi::new("prop-update", &[("page.html", page), ("style.css", css)])
        .stylesheet("style.css")
        .spawn(
            "page.html",
            TemplateContext::new().with("doc", &doc(&a)),
            Node::default(),
        );
    updated.settle();
    let root = updated.root();
    updated
        .world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("doc", &doc(&b));
    let mut fresh = TestUi::new(
        "prop-update-fresh",
        &[("page.html", page), ("style.css", css)],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new().with("doc", &doc(&b)),
        Node::default(),
    );
    let expected = fresh.settle().dump();
    // An identical render builds nothing, so only settle on a change.
    let actual = if doc(&a) == doc(&b) {
        updated.settle_quiet().dump()
    } else {
        updated.settle().dump()
    };
    prop_assert_eq!(actual, expected);
}

/// `padding: T R` builds the same world as the four longhands.
#[proptest(cases = 24)]
fn padding_shorthand_equals_longhands(#[strategy(0u8..40)] t: u8, #[strategy(0u8..40)] r: u8) {
    let page = "<p>Text</p>";
    let base = "html { color: #ffffff; font-size: 20px } p { ";
    let shorthand = format!("{base}padding: {t}px {r}px }}");
    let longhands = format!(
        "{base}padding-top: {t}px; padding-right: {r}px; \
         padding-bottom: {t}px; padding-left: {r}px }}"
    );
    let mut a = TestUi::new(
        "prop-pad-shorthand",
        &[("page.html", page), ("style.css", &shorthand)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let mut b = TestUi::new(
        "prop-pad-longhands",
        &[("page.html", page), ("style.css", &longhands)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    prop_assert_eq!(a.settle().dump(), b.settle().dump());
}

/// `border-image: …` (shorthand) draws the same frame as its
/// `-source`/`-slice`/`-repeat` longhands.
#[proptest(cases = 24)]
fn border_image_shorthand_equals_longhands(#[strategy(1u8..12)] slice: u8, #[any] tile: bool) {
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
    let mut a = TestUi::new(
        "prop-bi-shorthand",
        &[("page.html", page), ("style.css", &shorthand)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let mut b = TestUi::new(
        "prop-bi-longhands",
        &[("page.html", page), ("style.css", &longhands)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    prop_assert_eq!(a.settle().dump(), b.settle().dump());
}

/// Rules that match nothing change nothing, no matter what they declare —
/// even with `!important`.
#[proptest(cases = 24)]
fn unmatched_rules_change_nothing(
    #[strategy(proptest::collection::vec(name(), 1..4))] ids: Vec<String>,
    #[strategy(proptest::collection::vec(name(), 1..4))] classes: Vec<String>,
    #[strategy(proptest::collection::vec(8u8..40, 4))] sizes: Vec<u8>,
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
    let mut a = TestUi::new(
        "prop-unmatched-junk",
        &[("page.html", page), ("style.css", &with_junk)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let mut b = TestUi::new(
        "prop-unmatched-base",
        &[("page.html", page), ("style.css", &without)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    prop_assert_eq!(a.settle().dump(), b.settle().dump());
}

/// Specificity monotonicity: a class rule always beats a type rule, in
/// either source order.
#[proptest(cases = 24)]
fn class_beats_type_selector_regardless_of_order(
    #[strategy(hex6())] type_color: String,
    #[strategy(hex6())]
    #[filter(#type_color != #class_color)]
    class_color: String,
) {
    let page = "<p class=\"note\">Text</p>";
    let head = "html { color: #ffffff; font-size: 20px } ";
    let type_rule = format!("p {{ color: #{type_color} }} ");
    let class_rule = format!(".note {{ color: #{class_color} }} ");
    let (type_first, class_first) = (
        format!("{head}{type_rule}{class_rule}"),
        format!("{head}{class_rule}{type_rule}"),
    );
    let mut a = TestUi::new(
        "prop-spec-type-first",
        &[("page.html", page), ("style.css", &type_first)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let mut b = TestUi::new(
        "prop-spec-class-first",
        &[("page.html", page), ("style.css", &class_first)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let dump = a.settle().dump();
    prop_assert!(
        dump.contains(&format!("#{class_color}")),
        "class rule must win:\n{dump}"
    );
    prop_assert_eq!(dump, b.settle().dump());
}

/// Idempotence: re-setting the same context value leaves the subtree as it
/// was — no rebuild at all (the render is identical), same dump.
#[proptest(cases = 24)]
fn rebuild_with_same_context_is_idempotent(#[strategy(0i64..1000)] n: i64) {
    let mut ui = TestUi::new(
        "prop-idempotent",
        &[
            ("page.html", "<p>{{ n }}</p>"),
            ("style.css", "html { color: #ffffff; font-size: 20px }"),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new().with("n", &n),
        Node::default(),
    );
    let before = ui.settle().dump();

    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &n); // same value; only the mutation is detected
    let builds = ui.builds();
    let after = ui.settle_quiet().dump();
    prop_assert_eq!(ui.builds(), builds, "an identical render must not rebuild");
    prop_assert_eq!(before, after);
}

/// Round trip: theme X → Y → X ends where it started, including the
/// `html` rule's box properties (dropped and re-applied).
#[proptest(cases = 24)]
fn theme_round_trip(#[strategy(0u8..20)] padding: u8) {
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
        ("swap.css", swap_css.as_str()),
    ];
    let mut ui = TestUi::new("prop-theme-round-trip", &files)
        .stylesheet("framed.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
    let start = ui.settle().dump();

    let server = ui.world_mut().resource::<AssetServer>().clone();
    let plain: Handle<Stylesheet> = server.load("plain.css");
    ui.world_mut()
        .insert_resource(DefaultStylesheet::new(plain));
    ui.settle();
    let framed: Handle<Stylesheet> = server.load("framed.css");
    ui.world_mut()
        .insert_resource(DefaultStylesheet::new(framed));
    prop_assert_eq!(start, ui.settle().dump());
}

/// Round trip: locale A → B → A and context v → w → v end where they
/// started.
#[proptest(cases = 24)]
fn locale_and_context_round_trip(
    #[strategy(0i64..100)] v: i64,
    #[strategy(0i64..100)]
    #[filter(#v != #w)]
    w: i64,
) {
    let mut ui = TestUi::new(
        "prop-locale-round-trip",
        &[
            (
                "page.html",
                r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>"#,
            ),
            ("style.css", "html { color: #ffffff; font-size: 20px }"),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            ("locales/en-US/ui.ftl", "count = { $n } items"),
            (
                "locales/de/main.ftl.ron",
                r#"(locale: "de", resources: ["ui.ftl"])"#,
            ),
            ("locales/de/ui.ftl", "count = { $n } Dinge"),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn(
        "page.html",
        TemplateContext::new().with("n", &v),
        Node::default(),
    );
    let start = ui.settle().dump();

    let server = ui.world_mut().resource::<AssetServer>().clone();
    let de: Handle<LocaleBundle> = server.load("locales/de/main.ftl.ron");
    ui.world_mut().insert_resource(ActiveLocale::new(de));
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &w);
    ui.settle();

    let en: Handle<LocaleBundle> = server.load("locales/en-US/main.ftl.ron");
    ui.world_mut().insert_resource(ActiveLocale::new(en));
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &v);
    prop_assert_eq!(start, ui.settle().dump());
}

/// Selectors for the generated stylesheets below: each matches some
/// element of [`LIST_PAGE`] or nothing (`#ghost`), and `div p` is a valid
/// but unsupported (combinator) selector.
const SELECTORS: &[&str] = &[
    "p", ".a", "#x", "p.a", "*", "div", ".b", "p#x.a", "#ghost", "div p",
];
const LIST_PAGE: &str = r#"<p id="x" class="a">One</p><p class="b">Two <b class="a">bold</b></p><div class="a"><p>Three</p></div>"#;

/// A rule: selector list, color, `!important`.
type ListRule = (Vec<&'static str>, String, bool);

fn list_rules() -> impl Strategy<Value = Vec<ListRule>> {
    let selectors = proptest::collection::vec(proptest::sample::select(SELECTORS), 1..4);
    proptest::collection::vec((selectors, hex6(), proptest::bool::weighted(0.2)), 1..6)
}

fn declaration(color: &str, important: bool) -> String {
    format!(
        "color: #{color}{}",
        if important { " !important" } else { "" }
    )
}

/// Metamorphic CSS identities over generated stylesheets, all of which
/// must build the same world:
/// - a selector list `a, b { X }` ≡ `a { X } b { X }` in its place (each
///   selector matches with its own specificity — catches a list taking its
///   first or highest selector's specificity, or dropping the whole list
///   when one selector is unsupported);
/// - comments, line breaks and spacing between tokens are insignificant,
///   and repeating the whole sheet changes nothing (catches source-order
///   or cache bookkeeping that depends on rule count or formatting).
#[proptest(cases = 16)]
fn selector_lists_formatting_and_duplicates_change_nothing(
    #[strategy(list_rules())] rules: Vec<ListRule>,
) {
    let base = "html { color: #ffffff; font-size: 20px }\n";
    let mut lists = base.to_owned();
    let mut expanded = base.to_owned();
    let mut noisy = String::new();
    for (selectors, color, important) in &rules {
        let declaration = declaration(color, *important);
        lists.push_str(&format!("{} {{ {declaration} }}\n", selectors.join(",")));
        for selector in selectors {
            expanded.push_str(&format!("{selector} {{ {declaration} }}\n"));
        }
        noisy.push_str(&format!(
            "/* rule */\n{}\n/* before block */{{\n\t{} /* end */ ;\n}}\n\n",
            selectors.join(" ,\n  "),
            declaration.replace(": ", " :\n   ")
        ));
    }
    let noisy = format!("/* top */ {base}{noisy}{noisy}");
    let mut dumps = Vec::new();
    for (name, css) in [
        ("prop-lists", &lists),
        ("prop-expanded", &expanded),
        ("prop-noisy", &noisy),
    ] {
        let mut ui = TestUi::new(name, &[("page.html", LIST_PAGE), ("style.css", css)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        dumps.push(ui.settle().dump());
    }
    prop_assert_eq!(
        &dumps[0],
        &dumps[1],
        "selector list vs expanded:\n{}\n---\n{}",
        lists,
        expanded
    );
    prop_assert_eq!(
        &dumps[0],
        &dumps[2],
        "compact vs noisy+duplicated:\n{}\n---\n{}",
        lists,
        noisy
    );
}

/// Every sRGB notation of one color lands on the same color: `#rrggbb`,
/// upper-case hex, `#rrggbbaa` with opaque alpha, `rgb()` in comma and
/// space syntax, `rgba(…, 1)`, `#rgb` when the channels allow it. The
/// expected hex is computed from the channels, so a channel swap or a
/// dropped notation shows up as a wrong or inherited (white) color.
#[proptest(cases = 16)]
fn color_notations_agree(
    #[strategy(proptest::array::uniform3(0u8..=255))] rgb: [u8; 3],
    #[strategy(proptest::array::uniform3(0u8..16))] short: [u8; 3],
) {
    let [r, g, b] = rgb;
    let notations = [
        format!("#{r:02x}{g:02x}{b:02x}"),
        format!("#{r:02X}{g:02X}{b:02X}"),
        format!("#{r:02x}{g:02x}{b:02x}ff"),
        format!("rgb({r}, {g}, {b})"),
        format!("rgb({r} {g} {b})"),
        format!("rgba({r}, {g}, {b}, 1)"),
    ];
    // `#rgb` doubles each digit: channel = digit × 17.
    let [sr, sg, sb] = short;
    let short_hex = format!("#{sr:x}{sg:x}{sb:x}");
    let short_expected = format!("#{:02x}{:02x}{:02x}", sr * 17, sg * 17, sb * 17);

    let mut page = String::new();
    // Not white, so an ignored declaration (inherited white) can't pass.
    let mut css = "html { color: #010203; font-size: 20px }\n".to_owned();
    for (i, notation) in notations.iter().chain([&short_hex]).enumerate() {
        page.push_str(&format!("<p class=\"c{i}\">c{i}</p>"));
        css.push_str(&format!(".c{i} {{ color: {notation} }}\n"));
    }
    let mut ui = TestUi::new("prop-colors", &[("page.html", &page), ("style.css", &css)])
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
    let dump = ui.settle().dump();
    let expected = format!("#{r:02x}{g:02x}{b:02x}");
    for (i, notation) in notations.iter().enumerate() {
        let run = format!("\"c{i}\" default 20px {expected}");
        prop_assert!(
            dump.contains(&run),
            "{notation} should be {expected}:\n{dump}"
        );
    }
    let run = format!("\"c{}\" default 20px {short_expected}", notations.len());
    prop_assert!(
        dump.contains(&run),
        "{short_hex} should be {short_expected}:\n{dump}"
    );
}

/// CSS named colors are the sRGB values the spec lists (checked against a
/// hand-copied table), including mixed case.
#[proptest(cases = 4)]
fn named_colors_match_the_spec(#[any] upper: bool) {
    const NAMED: &[(&str, &str)] = &[
        ("red", "#ff0000"),
        ("Lime", "#00ff00"),
        ("navy", "#000080"),
        ("teal", "#008080"),
        ("rebeccapurple", "#663399"),
        ("GoldenRod", "#daa520"),
        ("gray", "#808080"),
        ("grey", "#808080"),
    ];
    let mut page = String::new();
    let mut css = "html { color: #010203; font-size: 20px }\n".to_owned();
    for (i, (name, _)) in NAMED.iter().enumerate() {
        let name = if upper {
            name.to_ascii_uppercase()
        } else {
            (*name).to_owned()
        };
        page.push_str(&format!("<p class=\"c{i}\">c{i}</p>"));
        css.push_str(&format!(".c{i} {{ color: {name} }}\n"));
    }
    let mut ui = TestUi::new("prop-named", &[("page.html", &page), ("style.css", &css)])
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), Node::default());
    let dump = ui.settle().dump();
    for (i, (name, hex)) in NAMED.iter().enumerate() {
        prop_assert!(
            dump.contains(&format!("\"c{i}\" default 20px {hex}")),
            "{name} → {hex}:\n{dump}"
        );
    }
}

/// Relative font sizes resolve against the right base: `Nem` and `N×100%`
/// against the parent's size, `Nrem` against the root's, nested `em` on
/// an inline element against the enclosing block's computed size. Bases
/// and factors are multiples of 1/4, so every expected size is exact.
/// Catches `rem` resolved against the parent (or `em` against the root),
/// `%` not divided by 100, and inline `em` compounding twice.
#[proptest(cases = 16)]
fn relative_font_sizes_resolve_against_their_base(
    #[strategy(8u16..40)] root: u16,
    #[strategy(8u16..40)] parent: u16,
    #[strategy(1u16..16)] quarters: u16,
) {
    let k = f32::from(quarters) / 4.0;
    let percent = u32::from(quarters) * 25;
    let css = format!(
        "html {{ color: #ffffff; font-size: {root}px }}\n\
         div {{ font-size: {parent}px }}\n\
         .em {{ font-size: {k}em }}\n\
         .pct {{ font-size: {percent}% }}\n\
         .rem {{ font-size: {k}rem }}\n\
         .half {{ font-size: 0.5em }}\n"
    );
    let page = r#"<div><p class="em">em <b class="half">half</b></p><p class="pct">pct</p><p class="rem">rem</p><p>inherit</p></div>"#;
    let mut ui = TestUi::new(
        "prop-font-size",
        &[("page.html", page), ("style.css", &css)],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let dump = ui.settle().dump();
    let (root, parent) = (f32::from(root), f32::from(parent));
    for (text, size) in [
        ("em ", parent * k),
        ("half", parent * k * 0.5),
        ("pct", parent * k),
        ("rem", root * k),
        ("inherit", parent),
    ] {
        let run = format!("{text:?} default {size}px #ffffff");
        prop_assert!(dump.contains(&run), "missing {run:?}\n{css}\n{dump}");
    }
}

/// A `*.slice.ron` manifest reaches the frame node side by side: `left`
/// and `top` are the min insets, `right` and `bottom` the max insets,
/// `sides`/`center` the scale modes, drawn over the border box. Catches
/// swapped or mirrored sides (invisible for symmetric frames).
#[proptest(cases = 8)]
fn nine_slice_manifest_maps_each_side(
    #[strategy(proptest::array::uniform4(0u8..12))] sides: [u8; 4],
    #[any] tile_sides: bool,
    #[any] tile_center: bool,
) {
    let [left, right, top, bottom] = sides;
    let mode = |tile: bool| if tile { "Tile(2.0)" } else { "Stretch" };
    let manifest = format!(
        r#"(image: "frame.png", border: (left: {left}, right: {right}, top: {top}, bottom: {bottom}),
            sides: {}, center: {})"#,
        mode(tile_sides),
        mode(tile_center)
    );
    let mut ui = TestUi::new(
        "prop-nine-slice",
        &[("page.html", "<p>x</p>"), ("frame.slice.ron", &manifest)],
    )
    .spawn("page.html", TemplateContext::new(), Node::default());
    let slice: Handle<NineSlice> = ui.load("frame.slice.ron");
    let frame = ui.world_mut().spawn(NineSliceFrame(slice)).id();
    ui.settle();

    let image = ui
        .world_mut()
        .get::<ImageNode>(frame)
        .expect("frame got an ImageNode");
    prop_assert_eq!(image.visual_box, VisualBox::BorderBox);
    prop_assert_eq!(
        image
            .image
            .path()
            .map(|path| path.path().display().to_string()),
        Some("frame.png".to_owned())
    );
    let NodeImageMode::Sliced(slicer) = &image.image_mode else {
        panic!("not sliced: {:?}", image.image_mode);
    };
    let border = slicer.border;
    prop_assert_eq!(
        [
            border.min_inset.x,
            border.max_inset.x,
            border.min_inset.y,
            border.max_inset.y
        ],
        sides.map(f32::from)
    );
    let tiles = |mode: &SliceScaleMode| matches!(mode, SliceScaleMode::Tile { stretch_value } if *stretch_value == 2.0);
    prop_assert_eq!(tiles(&slicer.sides_scale_mode), tile_sides);
    prop_assert_eq!(tiles(&slicer.center_scale_mode), tile_center);
}
