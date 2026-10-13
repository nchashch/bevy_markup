//! Test vectors: HTML + CSS + Fluent + Tera inputs → the Bevy world they
//! produce, checked headlessly (no window, no renderer).
//!
//! Each test writes its input files into a fresh asset root, runs a minimal
//! app (`MinimalPlugins` + assets + images + `BevyMarkupPlugin`) until every asset
//! is loaded and the UI is built, then compares a text dump of the `HtmlUi`
//! entity's subtree against the expected mapping:
//!
//! ```text
//! <entity>  := <label> [border=t,r,b,l] [padding=…] [margin=…] [gap=N] [bg=#hex] [slice=file t,r,b,l mode]
//!              (then, for `Text` nodes, one indented line per run: "text" face size color)
//! <label>   := tag#id.class… | - (no HtmlElement) | html-ui (the root)
//! face      := serif | serif-bold | serif-italic | serif-bold-italic | mono | default
//! ```

mod common;
use common::*;

use bevy::asset::AssetEvent;
use bevy::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use bevy_markup::prelude::*;

// ---------------------------------------------------------------------------
// Vectors
// ---------------------------------------------------------------------------

/// Tera: variables (autoescaped), loops, conditionals → text; tags → blocks,
/// containers and anonymous loose text; `li` bullets; `pre` keeps whitespace.
#[test]
fn tera_template_structure() {
    let mut ui = TestUi::new(
        "tera",
        &[
            (
                "page.html",
                r#"<h1>Hello {{ name }}</h1>
<ul>{% for item in items %}<li>{{ item }}</li>{% endfor %}</ul>
{% if items | length > 1 %}<p>many</p>{% else %}<p>few</p>{% endif %}
loose   text
<pre>
a
  b</pre>"#,
            ),
            (
                "style.css",
                "html { color: #ffffff; font-family: Spectral; font-size: 20px }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new()
            .with("name", "<Ada>")
            .with("items", &["torch", "rope"]),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  h1
    "Hello <Ada>" serif 20px #ffffff
  ul
    li margin=0,0,0,12
      "• " serif 20px #ffffff
      "torch" serif 20px #ffffff
    li margin=0,0,0,12
      "• " serif 20px #ffffff
      "rope" serif 20px #ffffff
  p
    "many" serif 20px #ffffff
  -
    "loose text" serif 20px #ffffff
  pre padding=8,8,8,8 overflow=Clip,Visible
    "a\n  b" serif 20px #ffffff
"#,
    );
}

/// System font families (any `FontSource` but a handle): one registered
/// source covers every face, and bold/italic are *requested* (TextFont
/// weight/style) for the system to pick. A file family (`Mono`, regular
/// face only) never gets a weight or style: its faces are fixed files.
#[test]
fn system_font_families_request_weight_and_style() {
    let mut ui = TestUi::new(
        "system-fonts",
        &[
            (
                "page.html",
                "<p>plain</p><p><b>strong</b></p><p><i>slanted</i></p><p><b><i>both</i></b></p>\
                 <p class=\"m\"><b><i>mono</i></b></p>",
            ),
            (
                "style.css",
                "html { font-family: Body; color: #ffffff } b { font-weight: bold } \
                 i { font-style: italic } .m { font-family: monospace }",
            ),
        ],
    );
    ui.world_mut()
        .resource_mut::<FontFamilies>()
        .insert("Body", FontFaces::new(FontSource::serif()));
    let mut ui =
        ui.stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "plain" Serif 16px #ffffff
  p
    "strong" Serif 16px #ffffff bold
  p
    "slanted" Serif 16px #ffffff italic
  p
    "both" Serif 16px #ffffff bold italic
  p.m
    "mono" mono 16px #ffffff
"#,
    );
}

/// Inline elements: `background` → a `TextBackgroundColor` on the span,
/// `text-decoration` → `Strikethrough` (+ declared color), and `<img>` → an
/// `InlineBox`/`InlineImage` flowing in the block's text (the fixture image
/// is 32x24; a CSS `width` keeps the aspect ratio). A restyle that drops
/// the background takes the component back in place.
#[test]
fn inline_backgrounds_decorations_and_images() {
    let ui = TestUi::new(
        "inline-runs",
        &[
            (
                "page.html",
                "<p>Plain <span class=\"hl\">highlight</span> and \
                 <img class=\"small\" src=\"frame.png\"> then <del>struck</del>.</p>",
            ),
            (
                "style.css",
                "html { color: #ffffff } .hl { background-color: #ffee00 } \
                 img.small { width: 16px } del { text-decoration: line-through #ff4040 }",
            ),
        ],
    );
    let mut ui =
        ui.stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "Plain " default 16px #ffffff
    "highlight" default 16px #ffffff bg=#ffee00
    " and " default 16px #ffffff
    "frame.png" img 16x12
    " then " default 16px #ffffff
    "struck" default 16px #ffffff line-through=#ff4040
    "." default 16px #ffffff
"#,
    );
    // A restyle without the declarations (the sheet failed): the span
    // components go, the entities stay — and the image, whose CSS width
    // was in the sheet, falls back to its intrinsic size.
    let plain = ui.load::<Stylesheet>("plain.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(plain);
    ui.settle_quiet();
    ui.update(10);
    ui.assert_dump(
        r#"
html-ui
  p
    "Plain highlight and " default 16px #ffffff
    "frame.png" img 32x24
    " then struck." default 16px #ffffff
"#,
    );
}

/// CSS: specificity (id > class > type, compound counts), inheritance into
/// inline elements, font faces from weight/style, missing-face fallback,
/// generic family, em/rem/% sizes, unknown family keeps the inherited one.
#[test]
fn css_cascade_and_fonts() {
    let mut ui = TestUi::from_vector("cascade");
    ui.settle().assert_dump(
        r#"
html-ui
  p#lead.note
    "Lead " serif 30px #00ff00
    "bold " serif-bold 30px #00ff00
    "both" serif-bold-italic 30px #00ff00
  p.note
    "Note " serif 30px #ff0000
    "mono " mono 30px #888888
    "bold?" mono 30px #888888
  p.a.b
    "Two classes" default 20px #222222
  h2
    "Heading " serif-bold 40px #ffffff
    "small" serif-bold 20px #ffffff
"#,
    );
}

/// Turning localization off (`ActiveLocale(None)`) brings back the
/// elements' own content: no stale translation stays on screen.
#[test]
fn turning_localization_off_restores_own_content() {
    let mut ui = TestUi::new(
        "locale-off",
        &[
            ("page.html", r#"<p data-l10n-id="title">Own title</p>"#),
            ("style.css", "html { color: #ffffff; font-size: 20px }"),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            ("locales/en-US/ui.ftl", "title = Translated title"),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle()
        .assert_dump("\nhtml-ui\n  p\n    \"Translated title\" default 20px #ffffff\n");

    ui.world_mut().resource_mut::<ActiveLocale>().0 = None;
    ui.settle()
        .assert_dump("\nhtml-ui\n  p\n    \"Own title\" default 20px #ffffff\n");
}

/// Fluent: `data-l10n-id` replaces content; args (numbers for plurals,
/// strings HTML-escaped); markup in translations is parsed and styled; a
/// missing message falls back to the element's own content.
#[test]
fn fluent_localization() {
    let mut ui = TestUi::new(
        "fluent",
        &[
            (
                "page.html",
                r#"<h1 data-l10n-id="title">fallback</h1>
<p data-l10n-id="greet" data-l10n-args='{"name": "{{ name }}"}'></p>
<p data-l10n-id="coins" data-l10n-args='{"n": {{ n }}}'></p>
<p data-l10n-id="missing">Own <b>content</b></p>"#,
            ),
            (
                "style.css",
                r#"
html { color: #ffffff; font-family: Spectral; font-size: 20px }
b { font-weight: bold; color: #ffcc00 }
em { font-style: italic }
"#,
            ),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            (
                "locales/en-US/ui.ftl",
                r#"
title = Inventory
greet = Welcome, <b>{ $name }</b>!
coins =
    { $n ->
        [one] <em>One</em> coin
       *[other] { $n } coins
    }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn(
        "page.html",
        TemplateContext::new()
            .with("name", "Ada <The Brave>")
            .with("n", &1),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  h1
    "Inventory" serif 20px #ffffff
  p
    "Welcome, " serif 20px #ffffff
    "Ada <The Brave>" serif-bold 20px #ffcc00
    "!" serif 20px #ffffff
  p
    "One" serif-italic 20px #ffffff
    " coin" serif 20px #ffffff
  p
    "Own " serif 20px #ffffff
    "content" serif-bold 20px #ffcc00
"#,
    );
}

/// Deliberate differences from `@fluent/dom` (which the Fluent oracle
/// vectors leave out): translation markup isn't sanitized. Nested elements
/// keep their styling (fluent-dom flattens them to text), `class`/`id` from
/// a translation apply (fluent-dom drops non-localizable attributes), and
/// any element is styled, not only text-level ones (fluent-dom turns `div`
/// into text). String args stay text: see `fluent_localization`.
#[test]
fn fluent_permissive_markup() {
    let mut ui = TestUi::new(
        "fluent-permissive",
        &[
            (
                "page.html",
                r#"<p data-l10n-id="nested"></p>
<p data-l10n-id="class-attr"></p>
<p data-l10n-id="block-element"></p>"#,
            ),
            (
                "style.css",
                r#"
html { color: #ffffff; font-family: Spectral; font-size: 20px }
b { font-weight: bold }
i { font-style: italic }
.hot { color: #ff0000 }
div { color: #00ff00 }
"#,
            ),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            (
                "locales/en-US/ui.ftl",
                r#"
nested = Text <b>bold <i>both</i></b>
class-attr = Plain <span class="hot">hot</span>
block-element = Before <div>block</div> after
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "Text " serif 20px #ffffff
    "bold " serif-bold 20px #ffffff
    "both" serif-bold-italic 20px #ffffff
  p
    "Plain " serif 20px #ffffff
    "hot" serif 20px #ff0000
  p
    "Before " serif 20px #ffffff
    "block" serif 20px #00ff00
    " after" serif 20px #ffffff
"#,
    );
}

/// Box model: `border-image` on the root (`html` rule), on a block (wrapper
/// node + inner `Text`) and on a container; px and % slices (% of the 32×24
/// image: top/bottom of the height, left/right of the width); tiling; borders,
/// padding, background, container `gap`.
#[test]
fn css_box_model() {
    let mut ui = TestUi::from_vector("box_model");
    ui.settle().assert_dump(
        r#"
html-ui border=16,16,16,16 padding=10,20,10,20 slice=frame.png 4,4,4,4 stretch
  p.framed border=6,8,6,8 bg=#102030 slice=frame.png 6,8,6,8 tile
    -
      "Framed" serif 20px #ffffff
  div.panel border=12,12,12,12 padding=4,4,4,4 gap=7 col-gap=7 bg=#333333 slice=frame.png 2,3,4,5 stretch
    p
      "One" serif 20px #ffffff
    p
      "Two" serif 20px #ffffff
"#,
    );
}

/// A template that compiles but fails to *render* (an undefined variable)
/// shows the error as a `failed to render:` paragraph naming the problem,
/// styled like the root, and recovers once the context provides the value.
#[test]
fn template_render_failure_shows_the_error_and_recovers() {
    let mut ui = TestUi::new(
        "render-failure",
        &[
            ("page.html", "<p>Hello {{ player }}</p>"),
            ("style.css", "html { color: #ff0000; font-size: 20px }"),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.update(30);
    {
        let world = ui.world_mut();
        let mut elements = world.query::<(Entity, &bevy_markup::html::HtmlElement)>();
        for (entity, element) in elements.iter(world) {
            eprintln!(
                "probe: tag={} node={}",
                element.tag,
                world.entity(entity).get::<Node>().is_some()
            );
        }
    }
    ui.settle();
    let root = ui.root();
    let failed = match ui.world_mut().get::<RenderedHtml>(root) {
        Some(RenderedHtml::Failed(message)) => message.clone(),
        _ => panic!("expected a render failure"),
    };
    assert!(
        failed.contains("player"),
        "error names the variable: {failed}"
    );
    let dump = ui.dump();
    let mut lines = dump.lines();
    assert_eq!(lines.next(), Some("html-ui"));
    assert_eq!(lines.next(), Some("  -"), "{dump}");
    let run = lines.next().unwrap_or_default();
    assert!(
        run.starts_with("    \"failed to render: ")
            && run.contains("player")
            && run.ends_with("default 20px #ff0000"),
        "{dump}"
    );

    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("player", "Ada");
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "Hello Ada" default 20px #ff0000
"#,
    );
}

/// A stylesheet that fails to load renders the UI unstyled (like a browser)
/// instead of leaving it blank forever.
#[test]
fn failed_stylesheet_still_renders() {
    let mut ui = TestUi::new(
        "failed_css",
        &[("page.html", "<p>Text</p>"), ("style.css", "{ not css")],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "Text" default 16px #ffffff
"#,
    );
}

/// Runtime: mutating `TemplateContext` re-renders, switching `ActiveLocale`
/// re-localizes, swapping `DefaultStylesheet` restyles — and dropping the
/// `html` rule's box properties restores the app's own `Node` values.
#[test]
fn runtime_changes_rebuild() {
    let mut ui = TestUi::new(
        "runtime",
        &[
            (
                "page.html",
                r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>"#,
            ),
            (
                "framed.css",
                r#"html { color: #ffffff; font-size: 20px; border-image: url("frame.png") 4 fill stretch; border-width: 16px; padding: 10px }"#,
            ),
            ("plain.css", "html { color: #00ff00; font-size: 10px }"),
            ("locales/en-US/main.ftl.ron", r#"(locale: "en-US", resources: ["ui.ftl"])"#),
            ("locales/en-US/ui.ftl", "count = { $n } items"),
            ("locales/de/main.ftl.ron", r#"(locale: "de", resources: ["ui.ftl"])"#),
            ("locales/de/ui.ftl", "count = { $n } Dinge"),
        ],
    )
    .stylesheet("framed.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn(
        "page.html",
        TemplateContext::new().with("n", &1),
        // The app's own padding, to be restored when CSS stops overriding it.
        Node {
            padding: UiRect::all(Val::Px(3.0)),
            ..default()
        },
    );
    ui.settle().assert_dump(
        r#"
html-ui border=16,16,16,16 padding=10,10,10,10 slice=frame.png 4,4,4,4 stretch
  p
    "1 items" default 20px #ffffff
"#,
    );

    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &5);
    ui.settle().assert_dump(
        r#"
html-ui border=16,16,16,16 padding=10,10,10,10 slice=frame.png 4,4,4,4 stretch
  p
    "5 items" default 20px #ffffff
"#,
    );

    let de = ui.load::<LocaleBundle>("locales/de/main.ftl.ron");
    ui.world_mut().resource_mut::<ActiveLocale>().set(de);
    ui.settle().assert_dump(
        r#"
html-ui border=16,16,16,16 padding=10,10,10,10 slice=frame.png 4,4,4,4 stretch
  p
    "5 Dinge" default 20px #ffffff
"#,
    );

    let plain = ui.load::<Stylesheet>("plain.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(plain);
    ui.settle().assert_dump(
        r#"
html-ui padding=3,3,3,3
  p
    "5 Dinge" default 10px #00ff00
"#,
    );
}

/// CSS variables end to end: `var(--name)` resolves the UI's
/// `TemplateContext` first (a change re-renders and restyles), then the
/// sheet's own `--` declarations.
#[test]
fn template_context_drives_css_variables() {
    let mut ui = TestUi::new(
        "css_variables",
        &[
            ("page.html", r#"<p>{{ n }}</p>"#),
            (
                "style.css",
                r#"
html { color: var(--text, #ffffff) }
p { color: var(--accent); font-size: var(--px) }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new()
            .with("n", &1)
            .with("--accent", "#ff8800")
            .with("--px", &24),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "1" default 24px #ff8800
"#,
    );

    // A context change with a new value re-resolves the variable.
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("--accent", "#00cc88");
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "1" default 24px #00cc88
"#,
    );
}

/// Transitions end to end: a `:active` restyle of a `transition:` element
/// starts an animation — the transitioned color keeps showing while the
/// same restyle's un-transitioned properties land at once (which proves
/// the restyle itself ran).
#[test]
fn transitions_animate_a_restyle() {
    let mut ui = TestUi::new(
        "transitions",
        &[
            ("page.html", "<p>hi</p>"),
            (
                "style.css",
                r#"
html { color: #ffffff }
p { color: #000000; transition: color 100s linear }
p:active { color: #ffffff; font-size: 30px }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "hi" default 16px #000000
"#,
    );

    // Restyle the p by state (`:active`): the un-transitioned font-size
    // lands immediately; the transitioned color is held at its start.
    let element = ui
        .world_mut()
        .query::<(Entity, &bevy_markup::html::HtmlElement)>()
        .iter(ui.world_mut())
        .find(|(_, element)| element.tag == "p")
        .expect("the p element")
        .0;
    ui.world_mut().entity_mut(element).insert(bevy_markup::signals::PseudoState {
        active: true,
        ..default()
    });
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "hi" default 30px #000000
"#,
    );
}

/// A block with only a background (no border) still gets its box: the
/// background is drawn. A boxed `li` keeps its list indent on the box.
#[test]
fn background_only_block_keeps_its_background() {
    let mut ui = TestUi::new(
        "background-only",
        &[
            ("page.html", r#"<p class="shaded">Shaded</p><ul><li class="shaded">Item</li></ul>"#),
            (
                "style.css",
                "html { color: #ffffff; font-family: Spectral; font-size: 20px } .shaded { background-color: #102030 }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p.shaded bg=#102030
    -
      "Shaded" serif 20px #ffffff
  ul
    li.shaded margin=0,0,0,12 bg=#102030
      -
        "• " serif 20px #ffffff
        "Item" serif 20px #ffffff
"#,
    );
}

/// A stylesheet swap restyles the existing children in place: the same
/// entities — with what the app attached — take the new styles, and
/// `HtmlUiRestyled` fires instead of `HtmlUiBuilt`. When the new styles
/// need another structure (blocks gaining a box need wrapper nodes), the
/// children are rebuilt and `HtmlUiBuilt` fires.
#[test]
fn style_changes_restyle_in_place_unless_the_shape_changes() {
    #[derive(Component)]
    struct Wired;
    #[derive(Resource, Default)]
    struct Events {
        built: usize,
        restyled: usize,
    }
    fn descendants(world: &World, entity: Entity, out: &mut Vec<Entity>) {
        for &child in world.entity(entity).get::<Children>().into_iter().flatten() {
            out.push(child);
            descendants(world, child, out);
        }
    }

    let mut ui = TestUi::new(
        "restyle",
        &[
            (
                "page.html",
                r#"<p id="a">One</p><div class="box"><p>Two</p></div>"#,
            ),
            ("red.css", "html { color: #ff0000; font-size: 20px }"),
            (
                "blue.css",
                "html { color: #0000ff; font-size: 20px } .box { padding: 4px }",
            ),
            (
                "boxed.css",
                "html { color: #0000ff; font-size: 20px } p { background-color: #102030 }",
            ),
        ],
    )
    .stylesheet("red.css");
    let blue = ui.load::<Stylesheet>("blue.css");
    let boxed = ui.load::<Stylesheet>("boxed.css");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    let world = ui.world_mut();
    world.init_resource::<Events>();
    world.add_observer(|_: On<HtmlUiBuilt>, mut events: ResMut<Events>| events.built += 1);
    world.add_observer(|_: On<HtmlUiRestyled>, mut events: ResMut<Events>| events.restyled += 1);
    ui.settle().assert_dump(
        r#"
html-ui
  p#a
    "One" default 20px #ff0000
  div.box
    p
      "Two" default 20px #ff0000
"#,
    );
    let root = ui.root();
    let world = ui.world_mut();
    let mut before = Vec::new();
    descendants(world, root, &mut before);
    let mut elements = world.query::<(Entity, &HtmlElement)>();
    let wired = elements
        .iter(world)
        .find(|(_, element)| element.id.as_deref() == Some("a"))
        .map(|(entity, _)| entity)
        .unwrap();
    world.entity_mut(wired).insert(Wired);

    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(blue);
    ui.settle().assert_dump(
        r#"
html-ui
  p#a
    "One" default 20px #0000ff
  div.box padding=4,4,4,4
    p
      "Two" default 20px #0000ff
"#,
    );
    let world = ui.world_mut();
    let mut after = Vec::new();
    descendants(world, root, &mut after);
    assert_eq!(after, before, "restyle kept every child entity");
    assert!(
        world.entity(wired).contains::<Wired>(),
        "app component survived"
    );
    let events = world.resource::<Events>();
    assert_eq!((events.built, events.restyled), (1, 1));

    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(boxed);
    ui.settle().assert_dump(
        r#"
html-ui
  p#a bg=#102030
    -
      "One" default 20px #0000ff
  div.box
    p bg=#102030
      -
        "Two" default 20px #0000ff
"#,
    );
    let events = ui.world_mut().resource::<Events>();
    assert_eq!(
        (events.built, events.restyled),
        (2, 1),
        "new wrappers: rebuilt"
    );
}

/// Swapping stylesheet `a` → `b` must end exactly where a fresh build with
/// `b` does — also when only a nested node's shape changes, a frame goes
/// away, or text runs merge (the restyle must notice and rebuild).
#[test]
fn restyles_that_change_shape_match_a_fresh_build() {
    let page = r#"<p id="a">One</p><div class="box"><p class="inner">Plain <b>bold</b></p></div>"#;
    let cases = [
        // Only the nested block gains a box (needs a wrapper).
        (
            "html { color: #ffffff }",
            "html { color: #ffffff } .inner { background-color: #102030 }",
        ),
        // The container loses its frame.
        (
            r#"html { color: #ffffff } .box { border-image: url("frame.png") 4 fill stretch; border-width: 4px }"#,
            "html { color: #ffffff }",
        ),
        // `b` stops differing from its block: two runs become one.
        (
            "html { color: #ffffff } b { color: #ff0000 }",
            "html { color: #ffffff }",
        ),
    ];
    for (a, b) in cases {
        let mut swapped = TestUi::new(
            "restyle-shape",
            &[("page.html", page), ("a.css", a), ("b.css", b)],
        )
        .stylesheet("a.css");
        let b_sheet = swapped.load::<Stylesheet>("b.css");
        let mut swapped = swapped.spawn("page.html", TemplateContext::new(), Node::default());
        swapped.settle();
        swapped.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(b_sheet);
        let mut fresh = TestUi::new("restyle-fresh", &[("page.html", page), ("b.css", b)])
            .stylesheet("b.css")
            .spawn("page.html", TemplateContext::new(), Node::default());
        assert_eq!(swapped.settle().dump(), fresh.settle().dump(), "{a} → {b}");
    }
}

/// The `html` rule's box properties leave an `ImageNode` the app put on the
/// `HtmlUi` node alone when the stylesheet has no frame of its own.
#[test]
fn root_box_keeps_the_apps_own_image() {
    let mut ui = TestUi::new(
        "root-image",
        &[
            ("page.html", "<p>Text</p>"),
            ("style.css", "html { padding: 5px }"),
        ],
    )
    .stylesheet("style.css");
    let template = ui.load::<HtmlTemplate>("page.html");
    let backdrop = ui.load::<Image>("frame.png");
    let root = ui
        .world_mut()
        .spawn((HtmlUi::new(template), ImageNode::new(backdrop.clone())))
        .id();
    // `TestUi::spawn` isn't used: the root needs the app's own `ImageNode`.
    ui.settle_quiet();
    ui.update(10);
    let world = ui.world_mut();
    assert_eq!(
        world.get::<Node>(root).unwrap().padding,
        UiRect::all(Val::Px(5.0))
    );
    let image = world
        .get::<ImageNode>(root)
        .expect("the app's ImageNode survived");
    assert_eq!(image.image, backdrop);
}

/// An `ImageNode` the app put on a built element (an icon) is app state, not
/// a CSS frame: a restyle keeps the element and its image instead of
/// rebuilding. Before the frame got its own marker, the image failed the
/// restyle's shape check, so every restyle — including the `PseudoState`
/// insert after each build — rebuilt the UI and the app re-attached the
/// image: a rebuild every frame.
#[test]
fn restyle_keeps_an_apps_image_on_an_element() {
    #[derive(Resource, Default)]
    struct Builds(usize);

    let mut ui = TestUi::new(
        "element-image",
        &[
            ("page.html", r#"<div id="icon"></div><p>Label</p>"#),
            ("red.css", "html { color: #ff0000 }"),
            ("blue.css", "html { color: #0000ff } #icon { width: 16px }"),
        ],
    )
    .stylesheet("red.css");
    let blue = ui.load::<Stylesheet>("blue.css");
    let icon_image = ui.load::<Image>("frame.png");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    ui.world_mut().init_resource::<Builds>();
    ui.world_mut()
        .add_observer(|_: On<HtmlUiBuilt>, mut builds: ResMut<Builds>| builds.0 += 1);
    ui.settle();
    let world = ui.world_mut();
    let mut elements = world.query::<(Entity, &HtmlElement)>();
    let icon = elements
        .iter(world)
        .find(|(_, element)| element.id.as_deref() == Some("icon"))
        .map(|(entity, _)| entity)
        .unwrap();
    world
        .entity_mut(icon)
        .insert(ImageNode::new(icon_image.clone()));
    let builds = world.resource::<Builds>().0;

    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(blue);
    ui.settle_quiet();
    ui.update(10);
    let world = ui.world_mut();
    assert_eq!(
        world.resource::<Builds>().0,
        builds,
        "restyled, not rebuilt"
    );
    let entity = world.entity(icon);
    assert_eq!(
        entity.get::<Node>().unwrap().width,
        Val::Px(16.0),
        "restyle applied"
    );
    assert_eq!(
        entity
            .get::<ImageNode>()
            .expect("the app's image survived")
            .image,
        icon_image
    );
}

/// `position`/insets, `border-radius`, `border-color`, `z-index` and
/// `pointer-events` reach the built nodes; `pointer-events` inherits (the
/// block inside `.overlay` is unpickable too); `static` ignores insets.
/// A stylesheet swap without them takes back only what CSS inserted: an
/// app's own `ZIndex` on another element survives.
#[test]
fn positioning_radius_border_color_z_index_and_pointer_events() {
    let page = r#"<div class="overlay"><p id="tip">Tip</p></div><div id="static"></div><div id="app"></div><div class="pinned"></div><div class="oval"></div>"#;
    let css =
        ".overlay { position: absolute; top: 4px; left: 8px; z-index: 5; pointer-events: none; \
               border-radius: 3px; border-width: 1px; border-color: #ff0000 }
               #static { position: static; top: 9px }
               .pinned { position: fixed; top: 5px; left: 6px }
               .oval { border-radius: 20px 8px / 10px 25% }";
    let mut ui = TestUi::new(
        "positioning",
        &[
            ("page.html", page),
            ("a.css", css),
            ("b.css", "p { color: #ffffff }"),
        ],
    )
    .stylesheet("a.css");
    let plain = ui.load::<Stylesheet>("b.css");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  div.overlay border=1,1,1,1 pos=abs inset=4,auto,auto,8 radius=3,3,3,3 bcolor=#ff0000,#ff0000,#ff0000,#ff0000 z=5 pick=none
    p#tip pick=none
      "Tip" default 16px #ffffff
  div#static
  div#app
  div.pinned pos=fixed inset=5,auto,auto,6
  div.oval radius=20/10,8/Percent(25.0),20/10,8/Percent(25.0)
"#,
    );
    let world = ui.world_mut();
    let mut elements = world.query::<(Entity, &HtmlElement)>();
    let app = elements
        .iter(world)
        .find(|(_, element)| element.id.as_deref() == Some("app"))
        .map(|(entity, _)| entity)
        .unwrap();
    world.entity_mut(app).insert(ZIndex(2));

    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(plain);
    ui.settle_quiet();
    ui.update(10);
    ui.assert_dump(
        r#"
html-ui
  div.overlay
    p#tip
      "Tip" default 16px #ffffff
  div#static
  div#app z=2
  div.pinned
  div.oval
"#,
    );
}

/// The element with this `id` below `root`.
fn element_by_id(world: &mut World, root: Entity, id: &str) -> Entity {
    let mut elements = world.query::<(Entity, &HtmlElement)>();
    let candidates: Vec<Entity> = elements
        .iter(world)
        .filter(|(_, element)| element.id.as_deref() == Some(id))
        .map(|(entity, _)| entity)
        .collect();
    candidates
        .into_iter()
        .find(|&entity| {
            std::iter::successors(Some(entity), |&current| {
                world.get::<ChildOf>(current).map(ChildOf::parent)
            })
            .any(|ancestor| ancestor == root)
        })
        .unwrap_or_else(|| panic!("no #{id} below {root}"))
}

fn focused(world: &World) -> Option<Entity> {
    world.resource::<bevy::input_focus::InputFocus>().get()
}

fn navigable(world: &World, entity: Entity) -> bool {
    world
        .entity(entity)
        .contains::<bevy::ui::auto_directional_navigation::AutoDirectionalNavigation>()
}

/// Browser-style focus: `data-on-click` and `tabindex >= 0` elements are
/// focusable (`tabindex="-1"` opts out), `autofocus` takes the initial
/// focus, `:focus-visible` (with `outline`) applies only while focus is
/// shown, activation emits the click signal, a rebuild keeps focus on the
/// same `id`, a modal root confines focus and an `HtmlNoFocus` root never
/// takes it. `autofocus` comes first on `#b`: a value-less attribute must
/// not eat the next attribute's first character (bug_0019).
#[test]
fn focus_navigation_scope_and_styles() {
    let page = r#"{% if n > 1 %}<section{% else %}<div{% endif %} id="a" data-on-click="pick-a"><p>A {{ n }}</p>{% if n > 1 %}</section>{% else %}</div>{% endif %}
        <div id="b" autofocus data-on-click="pick-b" data-with='{"n": {{ n }}}'><p>B</p></div>
        <div id="c" tabindex="0"><p>C</p></div>
        <div id="d" data-on-click="pick-d" tabindex="-1"><p>D</p></div>"#;
    let css = "div:focus-visible { outline: 2px solid #ff0000; outline-offset: 3px } \
               #c:focus { background-color: #00ff00 }";
    let modal = r#"<div id="ok" data-on-click="ok"><p>OK</p></div>"#;
    let mut ui = TestUi::new(
        "focus",
        &[
            ("page.html", page),
            ("style.css", css),
            ("modal.html", modal),
        ],
    )
    .stylesheet("style.css");
    let modal_template = ui.load::<HtmlTemplate>("modal.html");
    let mut ui = ui.spawn(
        "page.html",
        TemplateContext::new().with("n", &1),
        Node::default(),
    );
    ui.settle();
    ui.update(3);
    let root = ui.root();
    let world = ui.world_mut();
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|id| element_by_id(world, root, id));
    assert_eq!(focused(world), Some(b), "autofocus");
    assert_eq!(
        [a, b, c, d].map(|entity| navigable(world, entity)),
        [true, true, true, false]
    );
    assert!(world.get::<Outline>(b).is_none(), "focus not shown yet");

    // Shown focus: `:focus-visible` restyles in place.
    world
        .resource_mut::<bevy::input_focus::InputFocusVisible>()
        .0 = true;
    ui.update(3);
    let world = ui.world_mut();
    let outline = world.get::<Outline>(b).expect(":focus-visible outline");
    assert_eq!(
        (outline.width, outline.offset),
        (Val::Px(2.0), Val::Px(3.0))
    );

    // Moving focus moves the styles (`:focus` alone styles `#c`).
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(c, bevy::input_focus::FocusCause::Navigated);
    ui.update(3);
    let world = ui.world_mut();
    assert!(world.get::<Outline>(b).is_none(), "outline left b");
    assert_eq!(
        world.get::<BackgroundColor>(c).map(|bg| hex(bg.0)),
        Some("#00ff00".to_owned())
    );

    // Activation emits the element's click signal with its payload and the
    // input the app reported.
    world.trigger(ActivateElement {
        entity: b,
        input: ActivationInput::Key(KeyCode::Enter),
    });
    let signals: Vec<ElementSignal> = world
        .resource::<Messages<ElementSignal>>()
        .iter_current_update_messages()
        .cloned()
        .collect();
    assert_eq!(signals.len(), 1);
    assert_eq!(
        (
            signals[0].name.as_ref(),
            signals[0].payload["n"].as_i64(),
            signals[0].source
        ),
        (
            "pick-b",
            Some(1),
            SignalSource::Activation(ActivationInput::Key(KeyCode::Enter))
        )
    );

    // `a` becomes another element (a `section`): the update replaces it, and
    // focus follows the `id`.
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(a, bevy::input_focus::FocusCause::Navigated);
    ui.update(2);
    let world = ui.world_mut();
    world
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &2);
    ui.settle();
    ui.update(3);
    let world = ui.world_mut();
    let new_a = element_by_id(world, root, "a");
    assert_ne!(new_a, a, "replaced");
    assert_eq!(focused(world), Some(new_a), "focus restored by id");

    // A content update that keeps the structure updates in place: same
    // entities, focus untouched, and `b`'s new `data-with` is what fires.
    let b = element_by_id(world, root, "b");
    world
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &3);
    ui.settle();
    ui.update(3);
    let world = ui.world_mut();
    assert_eq!(element_by_id(world, root, "a"), new_a, "kept in place");
    assert_eq!(focused(world), Some(new_a));
    world.trigger(ActivateElement {
        entity: b,
        input: ActivationInput::Synthetic,
    });
    let payloads: Vec<Option<i64>> = world
        .resource::<Messages<ElementSignal>>()
        .iter_current_update_messages()
        .filter(|signal| signal.name == "pick-b")
        .map(|signal| signal.payload["n"].as_i64())
        .collect();
    assert_eq!(payloads, [Some(3)], "the updated binding, once");

    // A modal root takes focus and confines navigation.
    let dialog = world
        .spawn((HtmlUi::new(modal_template.clone()), HtmlModal))
        .id();
    ui.settle_quiet();
    ui.update(5);
    let world = ui.world_mut();
    let ok = element_by_id(world, dialog, "ok");
    assert_eq!(focused(world), Some(ok), "modal takes focus");
    assert!(navigable(world, ok));
    let page_b = element_by_id(world, root, "b");
    assert!(!navigable(world, page_b), "page outside the modal");

    // Closing it hands focus back to the page (its autofocus element).
    world.entity_mut(dialog).despawn();
    ui.update(3);
    let world = ui.world_mut();
    let new_b = element_by_id(world, root, "b");
    assert_eq!(focused(world), Some(new_b));
    assert!(navigable(world, new_b));

    // An `HtmlNoFocus` root's elements never take part.
    let panel = world.spawn((HtmlUi::new(modal_template), HtmlNoFocus)).id();
    ui.settle_quiet();
    ui.update(5);
    let world = ui.world_mut();
    let panel_ok = element_by_id(world, panel, "ok");
    assert!(!navigable(world, panel_ok));
    assert_eq!(focused(world), Some(new_b));
}

/// `HtmlFocus::navigate` moves focus by layout position and shows it; at an
/// edge it triggers `FocusEdge` instead. `HtmlFocus::activate` emits the
/// focused element's click signal.
#[test]
fn html_focus_navigates_by_layout_and_reports_edges() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::math::CompassOctant;

    #[derive(Resource, Default)]
    struct Edges(Vec<CompassOctant>);

    let page = r#"<div id="top" data-on-click="top" autofocus><p>Top</p></div>
        <div id="bottom" data-on-click="bottom"><p>Bottom</p></div>"#;
    let mut ui = TestUi::with_layout("navigate", &[("page.html", page)], UVec2::new(320, 240))
        .spawn(
            "page.html",
            TemplateContext::new(),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        );
    ui.settle();
    ui.update(3);
    let root = ui.root();
    let world = ui.world_mut();
    world.init_resource::<Edges>();
    world
        .add_observer(|edge: On<FocusEdge>, mut edges: ResMut<Edges>| edges.0.push(edge.direction));
    let [top, bottom] = ["top", "bottom"].map(|id| element_by_id(world, root, id));
    assert_eq!(focused(world), Some(top));

    let moved = world
        .run_system_once(|mut focus: HtmlFocus| focus.navigate(CompassOctant::South))
        .unwrap();
    assert_eq!(moved, Some(bottom));
    assert_eq!(focused(world), Some(bottom));
    assert!(
        world.resource::<bevy::input_focus::InputFocusVisible>().0,
        "navigation shows focus"
    );

    let moved = world
        .run_system_once(|mut focus: HtmlFocus| focus.navigate(CompassOctant::South))
        .unwrap();
    world.flush();
    assert_eq!(moved, None);
    assert_eq!(world.resource::<Edges>().0, [CompassOctant::South]);
    assert_eq!(focused(world), Some(bottom));

    world
        .run_system_once(|mut focus: HtmlFocus| focus.activate(ActivationInput::Synthetic))
        .unwrap();
    world.flush();
    let names: Vec<String> = world
        .resource::<Messages<ElementSignal>>()
        .iter_current_update_messages()
        .map(|signal| signal.name.to_string())
        .collect();
    assert_eq!(names, ["bottom"]);
}

/// Writing a `TemplateContext` whose render is unchanged (same values, or a
/// value the template doesn't print) builds nothing: entities and attached
/// components stay. A change that alters the output rebuilds once.
#[test]
fn identical_render_skips_the_rebuild() {
    #[derive(Component)]
    struct Wired;

    let mut ui = TestUi::new("same-render", &[("page.html", r#"<p id="n">{{ n }}</p>"#)]).spawn(
        "page.html",
        TemplateContext::new().with("n", &1).with("unused", &0),
        Node::default(),
    );
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let paragraph = element_by_id(world, root, "n");
    world.entity_mut(paragraph).insert(Wired);
    let builds = ui.builds();

    for (key, value) in [("n", 1), ("unused", 7)] {
        ui.world_mut()
            .get_mut::<TemplateContext>(root)
            .unwrap()
            .insert(key, &value);
        ui.update(5);
        assert_eq!(ui.builds(), builds, "{key} = {value}: nothing to rebuild");
    }
    let world = ui.world_mut();
    assert!(
        world.entity(paragraph).contains::<Wired>(),
        "entity and its components kept"
    );

    world
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &2);
    ui.settle().assert_dump(
        r#"
html-ui
  p#n
    "2" default 16px #ffffff
"#,
    );
    assert_eq!(ui.builds(), builds + 1, "a changed render rebuilds once");
}

/// `is="…"` runs the app's definition on every spawned element, in document
/// order, with the element's `data-*` dataset and its UI root, before
/// `HtmlUiBuilt` observers run. A rebuild runs it again on the new entities;
/// a restyle keeps the entities and what the definition attached, without
/// running it. An undefined name, and `is` on an element that doesn't become
/// a node (an inline `span`), are ignored.
#[test]
fn custom_elements_run_on_every_spawn() {
    #[derive(Component)]
    struct Icon(String);
    #[derive(Resource, Default)]
    struct Connected(Vec<(Entity, Option<String>, String, Option<String>)>);
    #[derive(Resource, Default)]
    struct IconsAtBuilt(Vec<usize>);

    let mut ui = TestUi::new(
        "custom-elements",
        &[
            (
                "page.html",
                r#"<div id="a" is="icon" data-icon="{{ icon }}" data-size="2">
                     <p id="b" is=" icon " data-icon="inner">x <span is="icon" data-icon="inline">y</span></p>
                   </div>
                   <p id="c" is="undefined">z</p>"#,
            ),
            ("white.css", "p { color: #ffffff }"),
            ("red.css", "p { color: #ff0000 }"),
        ],
    )
    .stylesheet("white.css");
    let app = ui.app_mut();
    app.init_resource::<Connected>()
        .init_resource::<IconsAtBuilt>()
        .define_html_element(
            "icon",
            |connected: In<ElementConnected>,
             elements: Query<&HtmlElement>,
             mut log: ResMut<Connected>,
             mut commands: Commands| {
                let icon = connected.data("icon").unwrap_or_default().to_owned();
                log.0.push((
                    connected.root,
                    elements
                        .get(connected.entity)
                        .ok()
                        .and_then(|e| e.id.clone()),
                    icon.clone(),
                    connected.data("size").map(str::to_owned),
                ));
                commands.entity(connected.entity).insert(Icon(icon));
            },
        )
        .add_observer(
            |_: On<HtmlUiBuilt>, icons: Query<&Icon>, mut seen: ResMut<IconsAtBuilt>| {
                seen.0.push(icons.iter().count());
            },
        );
    let mut ui = ui.spawn(
        "page.html",
        TemplateContext::new().with("icon", &"key_e"),
        Node::default(),
    );
    ui.settle();
    let root = ui.root();
    let log = |ui: &mut TestUi| ui.world_mut().resource::<Connected>().0.clone();
    let id = |id: &str| Some(id.to_owned());
    assert_eq!(
        log(&mut ui),
        [
            (root, id("a"), "key_e".to_owned(), Some("2".to_owned())),
            (root, id("b"), "inner".to_owned(), None),
        ],
        "document order, dataset, root; span and undefined name skipped"
    );
    assert_eq!(
        ui.world_mut().resource::<IconsAtBuilt>().0,
        [2],
        "attached before HtmlUiBuilt"
    );
    let icons = |ui: &mut TestUi| {
        let world = ui.world_mut();
        let mut query = world.query::<(Entity, &Icon)>();
        let mut icons: Vec<_> = query
            .iter(world)
            .map(|(e, icon)| (e, icon.0.clone()))
            .collect();
        icons.sort();
        icons
    };
    let built = icons(&mut ui);

    // Restyle: same entities, components kept, definition not re-run.
    let red = ui.load::<Stylesheet>("red.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(red);
    ui.settle();
    assert!(ui.dump().contains("#ff0000"), "restyled:\n{}", ui.dump());
    assert_eq!(
        icons(&mut ui),
        built,
        "restyle keeps the attached components"
    );
    assert_eq!(log(&mut ui).len(), 2, "restyle doesn't re-run definitions");

    // Rebuild: new entities, definitions re-run with the new data.
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("icon", &"key_r");
    ui.settle();
    let rebuilt = icons(&mut ui);
    assert_eq!(rebuilt.len(), 2);
    assert!(
        rebuilt
            .iter()
            .all(|(entity, _)| built.iter().all(|(old, _)| old != entity))
    );
    assert_eq!(
        log(&mut ui)[2..]
            .iter()
            .map(|row| row.2.as_str())
            .collect::<Vec<_>>(),
        ["key_r", "inner"]
    );
    assert_eq!(ui.world_mut().resource::<IconsAtBuilt>().0, [2, 2]);
}

/// The root rule — `html`, and the document's own `<html id class>` —
/// styles the `HtmlUi` entity itself: layout, position and insets, padding,
/// `border-radius` on its `Node`; `background-color`, `z-index`,
/// `pointer-events` as components. Only what it declares: the app's other
/// fields (moved every frame, say) survive rebuilds, and a stylesheet that
/// stops declaring something gives the app's value back.
#[test]
fn root_rule_styles_the_html_ui_entity() {
    let mut ui = TestUi::new(
        "root-rule",
        &[
            (
                "page.html",
                r#"<html class="hud {{ extra }}" style="height: 30px; opacity: 0.5"><p>x</p></html>"#,
            ),
            (
                "styled.css",
                "html { padding: 3px }
                 .hud { position: absolute; top: 10px; width: 40%; flex-direction: column;
                        background-color: #ff0000; z-index: 5; pointer-events: none;
                        border-radius: 4px }
                 .wide { width: 90% }",
            ),
            ("plain.css", "p { color: #ffffff }"),
        ],
    )
    .stylesheet("styled.css")
    .spawn(
        "page.html",
        TemplateContext::new().with("extra", ""),
        Node {
            left: Val::Px(7.0),
            width: Val::Px(50.0),
            padding: UiRect::all(Val::Px(1.0)),
            ..default()
        },
    );
    let root = ui.root();
    let blue = Color::srgb(0.0, 0.0, 1.0);
    ui.world_mut()
        .entity_mut(root)
        .insert(BackgroundColor(blue));
    ui.settle();

    let world = ui.world_mut();
    let node = world.get::<Node>(root).unwrap().clone();
    assert_eq!(node.position_type, PositionType::Absolute);
    assert_eq!(
        (node.top, node.left),
        (Val::Px(10.0), Val::Px(7.0)),
        "top CSS, left the app's"
    );
    assert_eq!(node.width, Val::Percent(40.0));
    assert_eq!(node.flex_direction, FlexDirection::Column);
    assert_eq!(
        node.padding,
        UiRect::all(Val::Px(3.0)),
        "the `html` rule applies too"
    );
    assert_eq!(
        node.border_radius.top_left,
        (Val::Px(4.0), Val::Px(4.0)).into()
    );
    assert_eq!(
        world.get::<BackgroundColor>(root).unwrap().0,
        Color::srgba(1.0, 0.0, 0.0, 0.5),
        "faded by the root's own opacity"
    );
    assert_eq!(world.get::<ZIndex>(root), Some(&ZIndex(5)));
    assert_eq!(world.get::<Pickable>(root), Some(&Pickable::IGNORE));
    assert_eq!(node.height, Val::Px(30.0), "the root's own `style`");
    let text = |world: &mut World| {
        let mut query = world.query_filtered::<&TextColor, With<Text>>();
        query.single(world).unwrap().0
    };
    assert_eq!(
        text(world).alpha(),
        0.5,
        "the root's opacity fades its subtree"
    );

    // The app moves its UI; a rebuild (new class) keeps that and restyles.
    world.get_mut::<Node>(root).unwrap().left = Val::Px(20.0);
    world
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("extra", "wide");
    ui.settle();
    let node = ui.world_mut().get::<Node>(root).unwrap().clone();
    assert_eq!((node.width, node.left), (Val::Percent(90.0), Val::Px(20.0)));

    // A stylesheet without root rules: everything back to the app's.
    let plain = ui.load::<Stylesheet>("plain.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(plain);
    ui.settle();
    let world = ui.world_mut();
    let node = world.get::<Node>(root).unwrap().clone();
    assert_eq!(node.position_type, PositionType::Relative);
    assert_eq!((node.top, node.left), (Val::Auto, Val::Px(20.0)));
    assert_eq!(node.width, Val::Px(50.0));
    assert_eq!(node.flex_direction, FlexDirection::Row);
    assert_eq!(node.padding, UiRect::all(Val::Px(1.0)));
    assert_eq!(node.border_radius.top_left, CornerRadius::ZERO);
    assert_eq!(world.get::<BackgroundColor>(root).unwrap().0, blue);
    assert_eq!(world.get::<ZIndex>(root), Some(&ZIndex(0)));
    assert_eq!(world.get::<Pickable>(root), None);
    assert_eq!(node.height, Val::Px(30.0), "the template's own style stays");
    assert_eq!(text(world).alpha(), 0.5);
}

/// `HtmlAnchor` keeps an overlay beside its element on the requested side,
/// follows the element when it moves, stays inside the viewport once it has
/// a size, and is despawned with the element.
#[test]
fn anchored_overlay_follows_clamps_and_despawns() {
    let mut ui = TestUi::with_layout(
        "anchor",
        &[
            ("page.html", r#"<div id="a"></div>"#),
            (
                "style.css",
                "html { flex-direction: column; padding: 20px }
                 #a { width: 100px; height: 30px; margin-top: 100px }",
            ),
            (
                "far.css",
                "html { flex-direction: column; padding: 20px }
                 #a { width: 100px; height: 30px; margin-top: 100px; margin-left: 250px }",
            ),
        ],
        UVec2::new(320, 240),
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let a = element_by_id(world, root, "a");
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(60.0),
                height: Val::Px(20.0),
                ..default()
            },
            HtmlAnchor::new(a, AnchorPlacement::Right).with_gap(4.0),
        ))
        .id();
    let rects = |ui: &mut TestUi| {
        ui.update(3);
        let world = ui.world_mut();
        (
            node_rect(world, a).unwrap(),
            node_rect(world, overlay).unwrap(),
        )
    };
    let (element, placed) = rects(&mut ui);
    assert_eq!(element, Rect::new(20.0, 120.0, 120.0, 150.0));
    assert_eq!(
        placed.min,
        Vec2::new(124.0, 120.0),
        "right of it, tops aligned"
    );

    for (placement, min) in [
        (AnchorPlacement::Below, Vec2::new(20.0, 154.0)),
        (AnchorPlacement::Above, Vec2::new(20.0, 96.0)),
        (AnchorPlacement::Left, Vec2::new(-44.0, 120.0)),
    ] {
        ui.world_mut()
            .get_mut::<HtmlAnchor>(overlay)
            .unwrap()
            .placement = placement;
        assert_eq!(rects(&mut ui).1.min, min, "{placement:?}");
    }

    // The element moves to the right edge (a restyle keeps its entity): the
    // overlay follows, clamped to the 320px viewport.
    ui.world_mut()
        .get_mut::<HtmlAnchor>(overlay)
        .unwrap()
        .placement = AnchorPlacement::Right;
    let far = ui.load::<Stylesheet>("far.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(far);
    ui.settle();
    let (element, placed) = rects(&mut ui);
    assert_eq!(element.min.x, 270.0);
    assert_eq!(placed.min, Vec2::new(260.0, 120.0), "clamped to 320 - 60");

    ui.world_mut().entity_mut(root).despawn();
    ui.update(2);
    assert!(
        ui.world_mut().get_entity(overlay).is_err(),
        "despawned with its element"
    );
}

/// An anchored overlay whose element has no camera (no viewport to clamp
/// against) falls back to the overlay's measured size: `Above` sets
/// `bottom` from the element's top minus gap minus the overlay's height,
/// `Left` sets `right` from the element's left minus gap minus its width.
#[test]
fn anchored_overlay_without_a_viewport_uses_the_measured_size() {
    let mut ui = TestUi::with_layout(
        "anchor-no-viewport",
        &[("page.html", "<p>x</p>")],
        UVec2::new(320, 240),
    )
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();
    // A bare "element": laid-out components but no `ComputedUiTargetCamera`.
    let element = ui
        .world_mut()
        .spawn((
            ComputedNode {
                size: Vec2::new(40.0, 30.0),
                ..ComputedNode::DEFAULT
            },
            bevy::ui::UiGlobalTransform::from_translation(Vec2::new(120.0, 115.0)),
        ))
        .id();
    let overlay = ui
        .world_mut()
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(60.0),
                height: Val::Px(20.0),
                ..default()
            },
            HtmlAnchor::new(element, AnchorPlacement::Above).with_gap(4.0),
        ))
        .id();
    let insets = |ui: &mut TestUi| {
        ui.update(2);
        let world = ui.world_mut();
        let node = world.get::<Node>(overlay).unwrap();
        (node.left, node.top, node.right, node.bottom)
    };

    // Above the element, top-anchored by the measured height.
    assert_eq!(
        insets(&mut ui),
        (
            Val::Px(100.0),
            Val::Px(76.0), // min.y - gap - height
            Val::Auto,
            Val::Auto
        )
    );

    ui.world_mut()
        .get_mut::<HtmlAnchor>(overlay)
        .unwrap()
        .placement = AnchorPlacement::Left;
    // Left of the element, left-anchored by the measured width.
    assert_eq!(
        insets(&mut ui),
        (
            Val::Px(36.0), // min.x - gap - width
            Val::Px(100.0),
            Val::Auto,
            Val::Auto
        )
    );
}

/// Template dependencies: `{% include %}`s load with their template
/// (duplicate references load once, and a duplicate include still renders
/// twice), a dependency's reload re-renders the whole set, and a missing or
/// circular dependency fails the template's load.
#[test]
fn template_dependencies_load_reload_and_fail() {
    let mut ui = TestUi::new(
        "template-deps",
        &[
            (
                "pages/page.html",
                r#"{% extends "base.html" %}
{% block body %}{% include "part.html" %}{% include "part.html" %}{% endblock %}"#,
            ),
            (
                "pages/base.html",
                r#"<div id="frame">{% include "part.html" %}{% block body %}{% endblock %}</div>"#,
            ),
            ("pages/part.html", r#"<p id="part">{{ n }}</p>"#),
            ("cyclic.html", r#"{% include "cyclic.html" %}cycle"#),
            ("pages/broken.html", r#"{% include "missing.html" %}"#),
        ],
    )
    .spawn(
        "pages/page.html",
        TemplateContext::new().with("n", &1),
        Node::default(),
    );
    let cyclic = ui.load::<HtmlTemplate>("cyclic.html");
    let broken = ui.load::<HtmlTemplate>("pages/broken.html");
    ui.settle().assert_dump(
        r#"
html-ui
  div#frame
    p#part
      "1" default 16px #ffffff
    p#part
      "1" default 16px #ffffff
    p#part
      "1" default 16px #ffffff
"#,
    );

    // The cyclic include loaded (terminating on the seen set); the missing
    // dependency failed its template's load.
    let server = ui.world_mut().resource::<AssetServer>().clone();
    // A circular include fails its load (`CircularInclude`), like the
    // missing one.
    assert!(server.load_state(cyclic.id()).is_failed());
    assert!(server.load_state(broken.id()).is_failed());

    // Rewriting a dependency and reloading it re-renders the whole set.
    ui.write("pages/part.html", r#"<p id="part">{{ n }}!</p>"#);
    server.reload("pages/page.html");
    ui.settle().assert_dump(
        r#"
html-ui
  div#frame
    p#part
      "1!" default 16px #ffffff
    p#part
      "1!" default 16px #ffffff
    p#part
      "1!" default 16px #ffffff
"#,
    );
}

/// A content update reconciles instead of rebuilding: elements matched by
/// `id` (anywhere among their siblings) or by position (id-less ones) keep
/// their entities and what the app attached, with new text; an inserted
/// element is spawned in document order; a removed one is despawned; a block
/// whose run count changes keeps its entity with new spans.
#[test]
fn content_updates_reconcile_in_place() {
    #[derive(Component)]
    struct Wired;

    let page = r#"<p id="title">Hello {{ name }}</p>
{% if extra %}<p id="extra">Extra</p>{% endif %}
<div id="list">{% for item in items %}<p>{{ item }}</p>{% endfor %}</div>
<p id="rich">{% if bold %}<b>bold</b> {% endif %}tail</p>"#;
    let mut ui = TestUi::new(
        "reconcile",
        &[
            ("page.html", page),
            (
                "style.css",
                "html { color: #ffffff } b { font-weight: bold }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new()
            .with("name", "A")
            .with("extra", &false)
            .with("items", &[1, 2])
            .with("bold", &false),
        Node::default(),
    );
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let [title, list, rich] = ["title", "list", "rich"].map(|id| element_by_id(world, root, id));
    let first_item = world.get::<Children>(list).unwrap()[0];
    for entity in [title, list, rich, first_item] {
        world.entity_mut(entity).insert(Wired);
    }
    let builds = ui.builds();

    let context = &mut *ui.world_mut().get_mut::<TemplateContext>(root).unwrap();
    context.insert("name", "B");
    context.insert("extra", &true);
    context.insert("items", &[1, 2, 3]);
    context.insert("bold", &true);
    ui.settle().assert_dump(
        r#"
html-ui
  p#title
    "Hello B" default 16px #ffffff
  p#extra
    "Extra" default 16px #ffffff
  div#list
    p
      "1" default 16px #ffffff
    p
      "2" default 16px #ffffff
    p
      "3" default 16px #ffffff
  p#rich
    "bold" default 16px #ffffff
    " tail" default 16px #ffffff
"#,
    );
    assert_eq!(ui.builds(), builds + 1, "one update");
    let world = ui.world_mut();
    for (entity, what) in [
        (title, "title"),
        (list, "list"),
        (rich, "rich (new runs)"),
        (first_item, "first list item"),
    ] {
        assert!(world.entity(entity).contains::<Wired>(), "{what} kept");
    }
    assert_eq!(element_by_id(world, root, "rich"), rich);

    // Removing them again despawns the extra element and the third item.
    let extra = element_by_id(world, root, "extra");
    let third = world.get::<Children>(list).unwrap()[2];
    let context = &mut *world.get_mut::<TemplateContext>(root).unwrap();
    context.insert("extra", &false);
    context.insert("items", &[1, 2]);
    ui.settle();
    let world = ui.world_mut();
    assert!(world.get_entity(extra).is_err() && world.get_entity(third).is_err());
    assert_eq!(world.get::<Children>(list).unwrap().len(), 2);
    assert!(world.entity(title).contains::<Wired>() && world.entity(list).contains::<Wired>());
}

/// The `style` attribute cascades as in CSS (over normal rules, under
/// `!important` ones unless itself `!important`), and `opacity` fades the
/// element's subtree — backgrounds and text, nested opacities multiplying.
/// A templated inline value updates the same entity in place.
#[test]
fn style_attribute_and_opacity() {
    let page = r#"<div id="box" style="width: {{ w }}%; background-color: #ff0000; overflow-y: scroll"><p>x</p></div>
<div id="half" class="half"><p id="green" style="color: #00ff00">y</p><div id="quarter" style="opacity: 0.5"><p id="inner">z</p></div></div>
<p id="rule" class="imp" style="color: #0000ff">a</p>
<p id="inline" class="imp" style="color: #0000ff !important">b</p>"#;
    let css = "p { color: #ffffff } #box { width: 10% } \
               .half { opacity: 0.5; background-color: #ffffff } \
               .imp { color: #ff00ff !important }";
    let mut ui = TestUi::new("inline", &[("page.html", page), ("style.css", css)])
        .stylesheet("style.css")
        .spawn(
            "page.html",
            TemplateContext::new().with("w", &40),
            Node::default(),
        );
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let get = |world: &mut World, id: &str| element_by_id(world, root, id);
    let text_color = |world: &mut World, id: &str| {
        let block = get(world, id);
        world.get::<TextColor>(block).unwrap().0
    };
    let boxed = get(world, "box");
    assert_eq!(
        world.get::<Node>(boxed).unwrap().width,
        Val::Percent(40.0),
        "inline over #id"
    );
    assert_eq!(
        world.get::<BackgroundColor>(boxed).unwrap().0,
        Color::srgb(1.0, 0.0, 0.0)
    );
    let half = get(world, "half");
    assert_eq!(
        world.get::<BackgroundColor>(half).unwrap().0,
        Color::srgba(1.0, 1.0, 1.0, 0.5)
    );
    assert_eq!(text_color(world, "green"), Color::srgba(0.0, 1.0, 0.0, 0.5));
    assert_eq!(
        text_color(world, "inner"),
        Color::srgba(1.0, 1.0, 1.0, 0.25),
        "0.5 × 0.5"
    );
    assert_eq!(
        text_color(world, "rule"),
        Color::srgb(1.0, 0.0, 1.0),
        "!important rule wins"
    );
    assert_eq!(
        text_color(world, "inline"),
        Color::srgb(0.0, 0.0, 1.0),
        "inline !important wins"
    );

    world
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("w", &75);
    ui.settle();
    let world = ui.world_mut();
    assert_eq!(get(world, "box"), boxed, "updated in place");
    let node = world.get::<Node>(boxed).unwrap();
    assert_eq!(node.width, Val::Percent(75.0));
    assert_eq!(
        node.overflow.y,
        OverflowAxis::Scroll,
        "CSS overflow survives updates"
    );
}

/// Focus on something that isn't UI — Bevy's input dispatch focuses the
/// primary window at startup — counts as nothing focused: the `autofocus`
/// element takes it (bug_0022). Focus on another UI node the app owns is
/// left alone.
#[test]
fn focus_on_the_window_yields_to_autofocus() {
    let page = r#"<div id="a" data-on-click="a"><p>A</p></div>
<div id="b" autofocus data-on-click="b"><p>B</p></div>"#;
    let mut ui = TestUi::new("focus-window", &[("page.html", page)]).spawn(
        "page.html",
        TemplateContext::new(),
        Node::default(),
    );
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let b = element_by_id(world, root, "b");
    let window = world.spawn(Window::default()).id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(window, bevy::input_focus::FocusCause::Navigated);
    ui.update(3);
    assert_eq!(focused(ui.world_mut()), Some(b), "window focus → autofocus");

    let world = ui.world_mut();
    let app_button = world.spawn(Node::default()).id();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(app_button, bevy::input_focus::FocusCause::Navigated);
    ui.update(3);
    assert_eq!(
        focused(ui.world_mut()),
        Some(app_button),
        "app UI focus kept"
    );
}

/// Templates compose: a page extends a base template, uses a component from a
/// library it includes (Tera 2 components are shared by every template in
/// the set; including a definitions-only file renders nothing), and includes
/// a part — all by paths relative to its own file.
#[test]
fn templates_extend_import_and_include() {
    let mut ui = TestUi::new(
        "template-refs",
        &[
            (
                "pages/page.html",
                r#"{% extends "base.html" %}
{% block body %}{% include "../lib/ui.html" %}{{ <ui.button id="go" label={label} /> }}{% include "part.html" %}{% endblock %}"#,
            ),
            (
                "pages/base.html",
                r#"<div id="frame">{% block body %}{% endblock %}</div>"#,
            ),
            (
                "pages/part.html",
                r#"<p id="part">{{ label }} part</p>"#,
            ),
            (
                "lib/ui.html",
                r#"{% component ui.button(id, label) %}<div id="{{ id }}" class="button"><p>{{ label }}</p></div>{% endcomponent ui.button %}"#,
            ),
        ],
    )
    .spawn(
        "pages/page.html",
        TemplateContext::new().with("label", "Go"),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  div#frame
    div#go.button
      p
        "Go" default 16px #ffffff
    p#part
      "Go part" default 16px #ffffff
"#,
    );
}

/// `HtmlWorldAnchor` puts its root's pivot on the target's projection,
/// hides it behind the camera, off screen or with an invisible target,
/// reports distance and visibility, and goes with its target. (The harness
/// camera keeps an identity projection at the origin: world (x, y) maps to
/// viewport ((x + 1) / 2 × 320, (1 − y) / 2 × 240).)
#[test]
fn world_anchor_projects_hides_and_despawns() {
    let mut ui = TestUi::with_layout("world-anchor", &[], UVec2::new(320, 240));
    let world = ui.world_mut();
    let place = |world: &mut World, target: Entity, at: Vec3| {
        world.entity_mut(target).insert((
            Transform::from_translation(at),
            GlobalTransform::from_translation(at),
        ));
    };
    let target = world.spawn(Visibility::Inherited).id();
    place(world, target, Vec3::new(0.25, 0.5, 0.0));
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(20.0),
                height: Val::Px(10.0),
                ..default()
            },
            HtmlWorldAnchor::new(target).with_offset(Vec3::new(0.25, 0.0, 0.0)),
        ))
        .id();
    let state = |ui: &mut TestUi| {
        ui.update(3);
        let world = ui.world_mut();
        let node = world.get::<Node>(overlay).unwrap();
        (
            (node.left, node.top),
            *world.get::<Visibility>(overlay).unwrap(),
            *world.get::<HtmlWorldAnchorView>(overlay).unwrap(),
        )
    };
    let (position, visibility, view) = state(&mut ui);
    assert_eq!(
        position,
        (Val::Px(230.0), Val::Px(50.0)),
        "(240, 60) minus the bottom-center pivot of 20×10"
    );
    assert_eq!(visibility, Visibility::Inherited);
    assert!(view.on_screen);
    assert!((view.distance - (0.5f32.powi(2) * 2.0).sqrt()).abs() < 1e-5);

    for (at, why) in [
        (Vec3::new(2.0, 0.0, 0.0), "off screen"),
        (Vec3::new(0.0, 0.0, -0.5), "behind the camera"),
    ] {
        place(ui.world_mut(), target, at);
        let (_, visibility, view) = state(&mut ui);
        assert_eq!(visibility, Visibility::Hidden, "{why}");
        assert!(!view.on_screen, "{why}");
    }

    let world = ui.world_mut();
    place(world, target, Vec3::ZERO);
    world.entity_mut(target).insert(Visibility::Hidden);
    let (_, visibility, view) = state(&mut ui);
    assert_eq!(visibility, Visibility::Hidden, "invisible target");
    assert!(view.on_screen);

    ui.world_mut().entity_mut(target).despawn();
    ui.update(2);
    assert!(
        ui.world_mut().get_entity(overlay).is_err(),
        "despawned with its target"
    );
}

/// `HtmlWorldAnchor::with_pivot` offsets the overlay by a fraction of its
/// own size, `with_camera` projects through that camera instead of the
/// default, and with no camera to project through the overlay hides.
#[test]
fn world_anchor_pivot_camera_override_and_no_camera_hides() {
    let mut ui = TestUi::with_layout("world-anchor-pivot", &[], UVec2::new(320, 240));
    let world = ui.world_mut();
    let target = world.spawn(Visibility::Inherited).id();
    world.entity_mut(target).insert((
        Transform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
        GlobalTransform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
    ));
    // A second camera, translated: projecting through it moves the overlay.
    let other = world
        .spawn((
            Camera2d,
            Camera {
                computed: bevy::camera::ComputedCameraValues {
                    target_info: Some(bevy::camera::RenderTargetInfo {
                        physical_size: UVec2::new(320, 240),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            bevy::camera::RenderTarget::None {
                size: UVec2::new(320, 240),
            },
            GlobalTransform::from_translation(Vec3::new(0.5, 0.0, 0.0)),
        ))
        .id();
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(20.0),
                height: Val::Px(10.0),
                ..default()
            },
            HtmlWorldAnchor::new(target)
                .with_pivot(Vec2::new(0.5, 1.0))
                .with_camera(other),
        ))
        .id();
    let placed = |ui: &mut TestUi| {
        ui.update(3);
        let world = ui.world_mut();
        let node = world.get::<Node>(overlay).unwrap();
        (
            node.left,
            node.top,
            *world.get::<Visibility>(overlay).unwrap(),
        )
    };

    // Through `other` at (0.5, 0, 0), world (0.25, 0.5) lands on viewport
    // (120, 60); the pivot (0.5, 1) shifts by a tenth and a half of the
    // overlay's 20×10.
    assert_eq!(
        placed(&mut ui),
        (Val::Px(110.0), Val::Px(50.0), Visibility::Inherited)
    );

    // Despawning the anchor's camera leaves nothing to project through.
    ui.world_mut().despawn(other);
    ui.update(3);
    assert_eq!(
        placed(&mut ui),
        (Val::Px(110.0), Val::Px(50.0), Visibility::Hidden)
    );
}

/// An in-place update writes only what differs: the element whose value
/// changed gets a new `Node`, the others keep theirs untouched (no change
/// detection), so layout and text systems skip them.
#[test]
fn updates_touch_only_changed_components() {
    #[derive(Resource, Default)]
    struct Changed(Vec<String>);

    let page = r#"<div id="a" style="width: {{ w }}px"><p>A</p></div><div id="b" style="width: 10px"><p>B {{ t }}</p></div>"#;
    let mut ui = TestUi::new("touch-changed", &[("page.html", page)]).spawn(
        "page.html",
        TemplateContext::new().with("w", &10).with("t", "x"),
        Node::default(),
    );
    ui.app_mut().init_resource::<Changed>().add_systems(
        Last,
        |nodes: Query<(&HtmlElement, Ref<Node>)>,
         texts: Query<(&ChildOf, Ref<TextSpan>)>,
         elements: Query<&HtmlElement>,
         mut changed: ResMut<Changed>| {
            for (element, node) in &nodes {
                if node.is_changed() && !node.is_added() {
                    changed.0.push(format!(
                        "node {}",
                        element.id.as_deref().unwrap_or(&element.tag)
                    ));
                }
            }
            for (parent, span) in &texts {
                if span.is_changed() && !span.is_added() {
                    let owner = elements
                        .get(parent.parent())
                        .map(|e| e.tag.clone())
                        .unwrap_or_default();
                    changed.0.push(format!("span in {owner}"));
                }
            }
        },
    );
    ui.settle();
    ui.update(2);
    ui.world_mut().resource_mut::<Changed>().0.clear();
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("w", &20);
    ui.settle();
    ui.update(1);
    let changed = std::mem::take(&mut ui.world_mut().resource_mut::<Changed>().0);
    assert_eq!(changed, ["node a"], "only a's Node is written");
}

/// Without an `IsDefaultUiCamera` marker, `HtmlWorldAnchor` projects with
/// Bevy's default UI camera (the window camera), like UI does (bug_0027).
#[test]
fn world_anchor_uses_the_implicit_default_ui_camera() {
    let mut ui = TestUi::with_layout("world-anchor-implicit", &[], UVec2::new(320, 240));
    let world = ui.world_mut();
    let marked: Vec<Entity> = world
        .query_filtered::<Entity, With<bevy::ui::IsDefaultUiCamera>>()
        .iter(world)
        .collect();
    for camera in marked {
        world
            .entity_mut(camera)
            .remove::<bevy::ui::IsDefaultUiCamera>()
            .insert(bevy::camera::RenderTarget::Window(
                bevy::window::WindowRef::Primary,
            ));
    }
    let window = world
        .spawn((
            Window {
                resolution: bevy::window::WindowResolution::new(320, 240),
                ..default()
            },
            bevy::window::PrimaryWindow,
        ))
        .id();
    let _ = window;
    let target = world
        .spawn((
            Visibility::Inherited,
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            HtmlWorldAnchor::new(target),
        ))
        .id();
    ui.update(3);
    let world = ui.world_mut();
    assert!(
        world.get::<HtmlWorldAnchorView>(overlay).unwrap().on_screen,
        "projected with the window's camera"
    );
    assert_eq!(
        *world.get::<Visibility>(overlay).unwrap(),
        Visibility::Inherited
    );
}

/// Untyped loads (folders, `load_untyped`) pick bevy_markup's loaders by file
/// extension: `.css`, `.html`/`.htm`, `.slice.ron`.
#[test]
fn loaders_are_found_by_extension() {
    use std::any::TypeId;

    let mut ui = TestUi::new(
        "extensions",
        &[
            ("a.css", "p { color: red }"),
            ("b.html", "<p>b</p>"),
            ("c.htm", "<p>c</p>"),
            ("e.md", "# md\n"),
            ("f.markdown", "md\n"),
            (
                "d.slice.ron",
                r#"(image: "frame.png", border: (left: 1, right: 1, top: 1, bottom: 1))"#,
            ),
        ],
    );
    let expected = [
        ("a.css", TypeId::of::<Stylesheet>()),
        ("b.html", TypeId::of::<HtmlTemplate>()),
        ("c.htm", TypeId::of::<HtmlTemplate>()),
        ("e.md", TypeId::of::<HtmlTemplate>()),
        ("f.markdown", TypeId::of::<HtmlTemplate>()),
        (
            "d.slice.ron",
            TypeId::of::<bevy_markup::nine_slice::NineSlice>(),
        ),
    ];
    let handles: Vec<_> = expected
        .iter()
        .map(|(path, _)| {
            ui.world_mut()
                .resource::<AssetServer>()
                .load_builder()
                .load_untyped(*path)
        })
        .collect();
    for _ in 0..3000 {
        ui.update(1);
        let loaded = ui
            .world_mut()
            .resource::<Assets<bevy::asset::LoadedUntypedAsset>>();
        if handles.iter().all(|handle| loaded.contains(handle)) {
            break;
        }
    }
    let loaded = ui
        .world_mut()
        .resource::<Assets<bevy::asset::LoadedUntypedAsset>>();
    for ((path, type_id), handle) in expected.iter().zip(&handles) {
        let asset = loaded
            .get(handle)
            .unwrap_or_else(|| panic!("{path} didn't load untyped"));
        assert_eq!(asset.handle.type_id(), *type_id, "{path}");
    }
}

/// Markdown templates (ADR 0014): a `.md` page renders through Tera +
/// pulldown-cmark into the same DOM model — headings, emphasis styled by
/// CSS, lists, an `<img>`, and raw HTML with `data-on-click` passing
/// through unclassed while the constructed elements carry
/// `class="markdown"`. Autoescaping is forced for `.md`: a context
/// value's markup arrives as *text*, not tags.
#[test]
fn markdown_templates_render_and_escape() {
    let ui = TestUi::new(
        "markdown",
        &[
            (
                "page.md",
                "# {{ title }}\n\nHello *world* and {{ value }}.\n\n- first\n- second\n\n![seal](frame.png)\n\n<div data-on-click=\"buy\">Buy</div>\n",
            ),
            (
                "style.css",
                "h1 { color: #ff0000; font-size: 30px }\n\
                 em { color: #00ff00 }\n\
                 li { color: #ffffff }",
            ),
        ],
    );
    let mut ui = ui.stylesheet("style.css").spawn(
        "page.md",
        TemplateContext::new()
            .with("title", "Journal")
            .with("value", "<b>not tags</b>"),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  h1.markdown
    "Journal" default 30px #ff0000
  p.markdown
    "Hello " default 16px #ffffff
    "world" default 16px #00ff00
    " and <b>not tags</b>." default 16px #ffffff
  ul.markdown
    li.markdown margin=0,0,0,12
      "• " default 16px #ffffff
      "first" default 16px #ffffff
    li.markdown margin=0,0,0,12
      "• " default 16px #ffffff
      "second" default 16px #ffffff
  p.markdown
    "frame.png" img 32x24
  div focus on=Click:buy
    -
      "Buy" default 16px #ffffff
"#,
    );
}

/// Markdown in Fluent values (ADR 0014): a bundle whose manifest sets
/// `markdown: true` converts its values (`**bold**` → `<strong>`); without
/// the flag, values stay HTML as written.
#[test]
fn markdown_bundles_opt_in_per_manifest() {
    let mut ui = TestUi::new(
        "markdown-l10n",
        &[
            ("page.html", "<p data-l10n-id=\"m\">fallback</p>"),
            (
                "md.ftl.ron",
                "(locale: \"en-US\", resources: [\"m.ftl\"], markdown: true)",
            ),
            (
                "plain.ftl.ron",
                "(locale: \"en-US\", resources: [\"m.ftl\"])",
            ),
            ("m.ftl", "m = struck **hard** with *flair*.\n"),
            (
                "style.css",
                "html { color: #ffffff; font-family: serif }\n\
                 strong { font-weight: bold }\n\
                 em { font-style: italic }",
            ),
        ],
    )
    .stylesheet("style.css")
    .locale("md.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.world_mut()
        .resource_mut::<FontFamilies>()
        .insert("System Serif", FontFaces::new(FontSource::serif()));
    ui.world_mut()
        .resource_mut::<FontFamilies>()
        .set_generic(GenericFamily::Serif, "System Serif");
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "struck " Serif 16px #ffffff
    "hard" Serif 16px #ffffff bold
    " with " Serif 16px #ffffff
    "flair" Serif 16px #ffffff italic
    "." Serif 16px #ffffff
"#,
    );
    // The same resource without the manifest flag: the value is HTML as
    // written (the `**` are text).
    let plain = ui.load::<LocaleBundle>("plain.ftl.ron");
    ui.world_mut().resource_mut::<ActiveLocale>().0 = Some(plain);
    ui.settle_quiet();
    ui.update(10);
    ui.assert_dump(
        r#"
html-ui
  p
    "struck **hard** with *flair*." Serif 16px #ffffff
"#,
    );
}

/// `<input type="text">` (the `text-input` feature): a block element with
/// Bevy 0.20's editable-text widget — the initial value from the `value`
/// attribute, the text styled by the cascade, the box by CSS. The element
/// is focusable (press-to-focus, navigation). (`maxlength` and
/// `placeholder` are not supported: Bevy 0.20's widget has neither.)
#[cfg(feature = "text-input")]
#[test]
fn text_input_element_maps_to_the_widget() {
    let mut ui = TestUi::new(
        "text-input",
        &[
            (
                "page.html",
                r#"<input type="text" value="hello" maxlength="12" class="field">"#,
            ),
            (
                "style.css",
                "html { color: #ffffff; font-family: serif; font-size: 20px }\n\
                 .field { width: 200px; padding: 4px; background-color: #222222 }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    // `settle()` never stabilizes here: the widget's cursor/edit systems
    // mark the text changed every frame. Bounded updates instead.
    ui.update(30).assert_dump(
        r#"
html-ui
  input.field padding=4,4,4,4 w=200 focus bg=#222222
    "hello" default 20px #ffffff
"#,
    );
}

/// (`--a: var(--brand)`) restyles when it changes: the vars hash used to
/// track only directly referenced names.
#[test]
fn transitive_var_change_restyles() {
    let mut ui = TestUi::new(
        "transitive-var",
        &[
            ("page.html", "<p>colored</p>"),
            (
                "style.css",
                "html { color: #ffffff } p { --a: var(--brand); color: var(--a) }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new().with("--brand", &"#ff0000"),
        Node::default(),
    );
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "colored" default 16px #ff0000
"#,
    );
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("--brand", &"#00ff00");
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "colored" default 16px #00ff00
"#,
    );
}


/// Other `type`s stay unsupported (ADR 0016): only a text (or missing)
/// `type` builds the field; the others are dropped like any unsupported
/// empty inline leaf — no field, no element. The `type` attribute is the
/// real attribute, not a `data-type`.
#[cfg(feature = "text-input")]
#[test]
fn non_text_input_types_are_not_fields() {
    let mut ui = TestUi::new(
        "input-types",
        &[
            (
                "page.html",
                r#"<input type="checkbox"><input type="password"><input>"#,
            ),
            ("style.css", "html { color: #ffffff }"),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.update(20);
    let world = ui.world_mut();
    let mut query = world.query::<(
        &bevy_markup::html::HtmlElement,
        Option<&bevy::text::EditableText>,
        Option<&bevy::ui_widgets::TextInput>,
        Option<&Focusable>,
    )>();
    let inputs: Vec<_> = query
        .iter(world)
        .map(|(element, text, widget, focus)| {
            (
                element.tag.clone(),
                text.is_some(),
                widget.is_some(),
                focus.is_some(),
            )
        })
        .collect();
    assert_eq!(
        inputs,
        vec![("input".to_owned(), true, true, true)],
        "the checkbox and password inputs must not become fields"
    );
}

/// A kept `<input>` is updated in place: the text follows the `value`
/// attribute only when the attribute changed (the typed text is app
/// state), and the cascade's font/color restyle it.
#[cfg(feature = "text-input")]
#[test]
fn kept_input_follows_value_and_styles() {
    let mut ui = TestUi::new(
        "input-update",
        &[
            (
                "page.html",
                r#"<p>{{ n }}</p><input type="text" class="field" value="{{ v }}">"#,
            ),
            (
                "style.css",
                "html { color: #ffffff; font-size: 20px } .field { color: #00ffff }",
            ),
            (
                "b.css",
                "html { color: #ffffff; font-size: 20px } .field { color: #ff00ff; font-size: 30px }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new().with("n", &1).with("v", &"one"),
        Node::default(),
    );
    ui.update(30);
    // The `value` attribute changed: the field's text follows it.
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("v", &"two");
    ui.update(30);
    let entity = {
        let world = ui.world_mut();
        world
            .query::<(Entity, &bevy::ui_widgets::TextInput)>()
            .iter(world)
            .map(|(entity, _)| entity)
            .next()
            .expect("the text field")
    };
    let text = ui
        .world_mut()
        .get::<bevy::text::EditableText>(entity)
        .unwrap()
        .value()
        .to_string();
    assert_eq!(text, "two", "the value attribute change did not land");

    // The attribute is unchanged; typed text survives a re-render.
    ui.world_mut()
        .entity_mut(entity)
        .insert(bevy::text::EditableText::new("typed by the user"));
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &2);
    ui.update(30);
    let text = ui
        .world_mut()
        .get::<bevy::text::EditableText>(entity)
        .unwrap()
        .value()
        .to_string();
    assert_eq!(
        text, "typed by the user",
        "the re-render clobbered the typed text"
    );

    // A restyle updates the field's cascade styling.
    let b_sheet = ui.load::<Stylesheet>("b.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(b_sheet);
    ui.update(30);
    let color = ui
        .world_mut()
        .get::<bevy::prelude::TextColor>(entity)
        .expect("the field kept its TextColor")
        .0;
    assert_eq!(color, bevy::color::Color::srgb(1.0, 0.0, 1.0));
}

/// The text-input branch must style the field from the same box the
/// normal path computed: the `resolve_names` call resolves its grid
/// placement, and dropping it lost a *named* `grid-row`/`-column`
/// (numeric placement maps without the names).
#[cfg(feature = "text-input")]
#[test]
fn text_input_keeps_its_grid_placement() {
    let mut ui = TestUi::new(
        "input-grid",
        &[
            (
                "page.html",
                r#"<div class="grid"><input type="text" style="grid-row: mid"></div>"#,
            ),
            (
                "style.css",
                "html { color: #ffffff } .grid { display: grid; grid-template-rows: [top] auto [mid] auto auto }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.update(30);
    let world = ui.world_mut();
    let rows: Vec<_> = world
        .query::<(&bevy_markup::html::HtmlElement, &Node)>()
        .iter(world)
        .filter(|(element, _)| element.tag == "input")
        .map(|(_, node)| (node.grid_row.get_start(), node.grid_row.get_span()))
        .collect();
    assert_eq!(
        rows,
        vec![(Some(2), None)],
        "the input lost its named grid placement"
    );
}

/// A root `calc()` resolves against the viewport — including on a
/// top-level root, which the resolution pass skipped entirely — and stops
/// resolving once the declaration goes (the `CalcSized` component was
/// never removed, so the old calc kept overwriting the width).
#[test]
fn root_calc_applies_and_stops() {
    let mut ui = TestUi::with_layout(
        "root-calc",
        &[
            ("page.html", "<p>x</p>"),
            ("a.css", "html { width: calc(50% - 20px) }"),
            ("b.css", "html { padding: 5px }"),
        ],
        UVec2::new(640, 480),
    )
    .stylesheet("a.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();
    let root = ui.root();
    let width = ui.world_mut().get::<Node>(root).unwrap().width;
    assert_eq!(width, Val::Px(300.0), "the root's calc() did not resolve");

    // A theme that drops the calc: the width goes back to the app's node.
    let b_sheet = ui.load::<Stylesheet>("b.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(b_sheet);
    ui.settle();
    let width = ui.world_mut().get::<Node>(root).unwrap().width;
    assert_eq!(
        width,
        Val::Auto,
        "the dropped calc() kept overwriting the width"
    );
}

/// A restyle that respawns spans (runs merging under a new sheet) must
/// fire `HtmlUiBuilt`, not `HtmlUiRestyled`: the promise behind the
/// restyled event is that every element was kept, and link spans are new
/// entities the app's `HtmlUiBuilt` wiring has to see.
#[test]
fn restyle_that_respawns_spans_reports_a_build() {
    #[derive(Resource, Default)]
    struct Counts {
        built: usize,
        restyled: usize,
    }

    let mut ui = TestUi::new(
        "span-respawn",
        &[
            ("page.html", "<p>Plain <b>bold</b> text</p>"),
            ("a.css", "html { color: #ffffff } b { color: #ff0000 }"),
            ("b.css", "html { color: #ffffff }"),
        ],
    )
    .stylesheet("a.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.world_mut().insert_resource(Counts::default());
    ui.app_mut().add_observer(
        |_: On<HtmlUiBuilt>, mut counts: ResMut<Counts>| {
            counts.built += 1;
        },
    );
    ui.app_mut().add_observer(
        |_: On<HtmlUiRestyled>, mut counts: ResMut<Counts>| {
            counts.restyled += 1;
        },
    );
    ui.settle();
    let (built, restyled) = {
        let counts = ui.world_mut().resource::<Counts>();
        (counts.built, counts.restyled)
    };

    // The merged runs change the span count: the restyle respawns them.
    let b_sheet = ui.load::<Stylesheet>("b.css");
    ui.world_mut().resource_mut::<DefaultStylesheet>().0 = Some(b_sheet);
    ui.settle();
    let counts = ui.world_mut().resource::<Counts>();
    assert_eq!(counts.built, built + 1, "the respawn was not reported as a build");
    assert_eq!(
        counts.restyled, restyled,
        "a span respawn must not fire HtmlUiRestyled"
    );
}

/// Syncing Bevy's `RemSize` to the root rule's font-size (16px vs Bevy's
/// 20px default) must not queue a restyle: Bevy's layout re-resolves
/// `Val::Rem`s itself, and a spurious `HtmlUiRestyled` broke the "kept"
/// promise right after the first build.
#[test]
fn rem_size_sync_does_not_restyle() {
    #[derive(Resource, Default)]
    struct Counts {
        built: usize,
        restyled: usize,
    }

    let mut ui = TestUi::new(
        "rem-sync",
        &[("page.html", "<p>x</p>"), ("style.css", "html { color: #ffffff }")],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.world_mut().insert_resource(Counts::default());
    ui.app_mut().add_observer(
        |_: On<HtmlUiBuilt>, mut counts: ResMut<Counts>| {
            counts.built += 1;
        },
    );
    ui.app_mut().add_observer(
        |_: On<HtmlUiRestyled>, mut counts: ResMut<Counts>| {
            counts.restyled += 1;
        },
    );
    ui.settle();
    let counts = ui.world_mut().resource::<Counts>();
    assert!(counts.built >= 1, "the UI never built");
    assert_eq!(counts.restyled, 0, "the RemSize sync queued a restyle");
}

/// Pathologically deep user content is capped: ~150 nesting levels
/// overflowed a debug-build stack in the recursive walks. Deeper content
/// is dropped (warned once); the capped prefix builds normally.
#[test]
fn deep_nesting_is_capped() {
    let deep = format!(
        "{}x{}",
        "<div>".repeat(200),
        "</div>".repeat(200)
    );
    let mut ui = TestUi::new(
        "deep-nesting",
        &[("page.html", "<p>before</p>{{ deep | safe }}<p>after</p>")],
    )
    .spawn(
        "page.html",
        TemplateContext::new().with("deep", &deep),
        Node::default(),
    );
    ui.settle();
    let dump = ui.dump();
    let divs = dump.lines().filter(|line| line.trim_start().starts_with("div")).count();
    assert_eq!(divs, 100, "the cap should keep exactly 100 levels:\n{dump}");
    assert!(dump.contains("\"before\""), "content before the deep tree vanished:\n{dump}");
    assert!(dump.contains("\"after\""), "content after the deep tree vanished:\n{dump}");
}

/// `<a href>` links: the link's runs become a picked span entity that
/// emits the `open` signal with the href when clicked; the text styles by
/// the cascade (`a` rules); a link without an href is plain styled text.
#[test]
fn links_carry_their_href_on_span_entities() {
    let mut ui = TestUi::new(
        "links",
        &[
            (
                "page.html",
                r#"<p>See <a href="https://example.com" class="ref">the docs</a>
                   and <a id="plain">not a link</a>.</p>"#,
            ),
            (
                "style.css",
                "html { color: #ffffff; font-family: serif }\n\
                 a { color: #00ffff; text-decoration: underline }",
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "See " default 16px #ffffff
    "the docs" default 16px #00ffff open=https://example.com underline
    " and " default 16px #ffffff
    "not a link" default 16px #00ffff underline
    "." default 16px #ffffff
"#,
    );
}

/// A loaded template is named after its asset path (Tera picks HTML
/// autoescaping from the `.html` name).
#[test]
fn template_is_named_after_its_asset_path() {
    let mut ui = TestUi::new("template-name", &[("ui/page.html", "<p>x</p>")]);
    let handle = ui.load::<HtmlTemplate>("ui/page.html");
    for _ in 0..3000 {
        ui.update(1);
        if ui
            .world_mut()
            .resource::<Assets<HtmlTemplate>>()
            .contains(&handle)
        {
            break;
        }
    }
    let templates = ui.world_mut().resource::<Assets<HtmlTemplate>>();
    assert_eq!(
        templates.get(&handle).expect("loaded").name(),
        "ui/page.html"
    );
}

/// Counts updates (`HtmlUiBuilt` or `HtmlUiRestyled`) per entity.
#[derive(Resource, Default)]
struct UpdatesPer(bevy::platform::collections::HashMap<Entity, usize>);

/// Editing a stylesheet asset in place (hot reload) rebuilds the UIs using
/// it — and only those: a UI whose own stylesheet failed to load isn't
/// rebuilt again because some other sheet reloaded.
#[test]
fn stylesheet_reload_rebuilds_only_its_users() {
    let mut ui = TestUi::new(
        "sheet-reload",
        &[
            ("page.html", "<p>Text</p>"),
            ("good.css", "html { color: #ff0000 }"),
            ("broken.css", "{ not css"),
        ],
    );
    let good = ui.load::<Stylesheet>("good.css");
    let broken = ui.load::<Stylesheet>("broken.css");
    let template = ui.load::<HtmlTemplate>("page.html");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    let user = ui.root();
    let world = ui.world_mut();
    world.init_resource::<UpdatesPer>();
    world.add_observer(|built: On<HtmlUiBuilt>, mut per: ResMut<UpdatesPer>| {
        *per.0.entry(built.entity).or_default() += 1;
    });
    world.add_observer(
        |restyled: On<HtmlUiRestyled>, mut per: ResMut<UpdatesPer>| {
            *per.0.entry(restyled.entity).or_default() += 1;
        },
    );
    world.entity_mut(user).insert(HtmlStylesheet(good.clone()));
    let failed = world
        .spawn((HtmlUi::new(template), HtmlStylesheet(broken)))
        .id();
    ui.settle();
    let counts = |ui: &mut TestUi| {
        let per = &ui.world_mut().resource::<UpdatesPer>().0;
        (
            per.get(&user).copied().unwrap_or(0),
            per.get(&failed).copied().unwrap_or(0),
        )
    };
    let (user_before, failed_before) = counts(&mut ui);
    assert!(
        user_before >= 1 && failed_before >= 1,
        "both built: {user_before}, {failed_before}"
    );

    // In-place edit: `AssetMut` emits `Modified` once dereferenced mutably.
    {
        let mut sheets = ui.world_mut().resource_mut::<Assets<Stylesheet>>();
        let mut sheet = sheets.get_mut(&good).expect("good.css loaded");
        let _: &mut Stylesheet = &mut sheet;
    }
    ui.settle();
    assert_eq!(counts(&mut ui), (user_before + 1, failed_before));
}

/// A frame image that changes (hot reload, or one arriving after the
/// stylesheet) re-slices `%` frames against its new size.
#[test]
fn image_change_reslices_percent_frames() {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    let mut ui = TestUi::new(
        "image-reload",
        &[
            ("page.html", r#"<p class="framed">Framed</p>"#),
            (
                "style.css",
                r#"html { color: #ffffff; font-family: Spectral; font-size: 20px }
.framed { border-image: url("frame.png") 25% fill stretch; border-width: 6px }"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    let frame = ui.load::<Image>("frame.png");
    // 25% of the 32×24 fixture: 6 (top/bottom), 8 (left/right).
    let expected = |slices: &str| {
        format!(
            "\nhtml-ui\n  p.framed border=6,6,6,6 slice=frame.png {slices} stretch\n    -\n      \"Framed\" serif 20px #ffffff\n"
        )
    };
    ui.settle().assert_dump(&expected("6,8,6,8"));

    let bigger = Image::new_fill(
        Extent3d {
            width: 64,
            height: 48,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[255; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    ui.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(&frame, bigger)
        .unwrap();
    ui.settle().assert_dump(&expected("12,16,12,16"));
}

/// The UI isn't built while its stylesheet is still loading (no unstyled
/// flash), and is built, styled, once it arrives.
#[test]
fn loading_stylesheet_defers_the_build() {
    let mut ui = TestUi::new(
        "sheet-loading",
        &[
            ("page.html", "<p>Text</p>"),
            ("style.css", "html { color: #ff0000 }"),
        ],
    );
    // A handle nothing will ever load: the stylesheet stays "loading".
    let pending = ui
        .world_mut()
        .resource::<Assets<Stylesheet>>()
        .reserve_handle();
    ui.world_mut()
        .insert_resource(DefaultStylesheet::new(pending.clone()));
    let real = ui.load::<Stylesheet>("style.css");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    let root = ui.root();
    for _ in 0..3000 {
        ui.update(1);
        let world = ui.world_mut();
        let rendered = matches!(
            world.get::<RenderedHtml>(root),
            Some(RenderedHtml::Ready(_))
        );
        if rendered && world.resource::<Assets<Stylesheet>>().contains(&real) {
            break;
        }
    }
    ui.update(10);
    assert_eq!(ui.builds(), 0, "built while the stylesheet was loading");

    let mut sheets = ui.world_mut().resource_mut::<Assets<Stylesheet>>();
    let sheet = sheets.remove(&real).expect("style.css loaded");
    sheets.insert(&pending, sheet).unwrap();
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "Text" default 16px #ff0000
"#,
    );
}

// ---------------------------------------------------------------------------
// Browser oracle
// ---------------------------------------------------------------------------

/// Every `tests/vectors/*/` with a `browser.json` (written by
/// `scripts/browser_oracle.py` from headless Chromium) is built here and
/// compared against the browser's computed styles: per non-whitespace
/// character (face, size, color) and per block/container element (padding,
/// border widths, border-image, background, gap). Deliberate differences are
/// listed in the comment above `FIXTURE_SIZE`.
#[test]
fn browser_oracle() {
    let vectors = vectors_with("browser.json");
    assert!(!vectors.is_empty(), "no oracle vectors found");

    let mut failures = Vec::new();
    for name in &vectors {
        let json = std::fs::read_to_string(vectors_dir().join(name).join("browser.json")).unwrap();
        let oracle: serde_json::Value = serde_json::from_str(&json).unwrap();
        let mut ui = TestUi::from_vector(name);
        ui.settle();
        for problem in compare_with_browser(&mut ui, &oracle) {
            failures.push(format!("{name}: {problem}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} difference(s) from the browser:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Bevy UI's headless layout of bevy_markup's column model, derived by hand: the
/// default font (FiraMono) advances 0.6em per character and lines are 1.2em;
/// the root's padding insets everything, a container's border and padding
/// inset its children and `gap` separates them, text wraps at the content
/// width (50 characters at 12px), `pre` adds bevy_markup's 8px padding.
#[test]
fn layout_column_stacking() {
    let mut ui = TestUi::from_layout_vector("layout_blocks", UVec2::new(640, 480));
    let actual = ui.settle().layout_dump();
    let expected = "\
html-ui 0,0 640x263
  h1 20,10 600x48
  p 20,58 600x48
  div.panel 20,106 600x83
    p 28,114 584x24
    p.tall 28,145 584x36
  pre 20,189 600x64";
    assert_eq!(actual.trim_end(), expected, "\n--- actual ---\n{actual}");
}

/// Bevy rounds layout to whole pixels; Chromium keeps 1/64px fractions.
const LAYOUT_TOLERANCE: f32 = 1.0;

/// Every `tests/vectors/layout_*/` with a `browser.json` is laid out by Bevy
/// UI headlessly (in the oracle's viewport, under a full-width column root)
/// and each root/block/container border box compared with Chromium's
/// `getBoundingClientRect` under the oracle's bevy_markup layout stylesheet
/// (`BEVY_MARKUP_CSS` in `scripts/browser_oracle.py`), within [`LAYOUT_TOLERANCE`]
/// per value.
#[test]
fn layout_oracle() {
    let vectors: Vec<String> = vectors_with("browser.json")
        .into_iter()
        .filter(|name| name.starts_with("layout_"))
        .collect();
    assert!(!vectors.is_empty(), "no layout oracle vectors found");

    let mut failures = Vec::new();
    for name in &vectors {
        let json = std::fs::read_to_string(vectors_dir().join(name).join("browser.json")).unwrap();
        let oracle: serde_json::Value = serde_json::from_str(&json).unwrap();
        let viewport = &oracle["generator"]["viewport"];
        let viewport = UVec2::new(
            viewport[0].as_u64().unwrap() as u32,
            viewport[1].as_u64().unwrap() as u32,
        );
        let mut ui = TestUi::from_layout_vector(name, viewport);
        ui.settle();
        let root = ui.root();
        let world = ui.world_mut();
        let pairs = match pair_elements(world, root, &oracle) {
            Ok(pairs) => pairs,
            Err(problem) => {
                failures.push(format!("{name}: {problem}"));
                continue;
            }
        };
        for (entity, record, label) in pairs {
            let rect = node_rect(world, entity).expect("laid out");
            let ours = [rect.min.x, rect.min.y, rect.width(), rect.height()];
            let theirs: Vec<f32> = record["rect"]
                .as_array()
                .expect("layout vectors record rects")
                .iter()
                .map(|value| value.as_f64().unwrap() as f32)
                .collect();
            if ours
                .iter()
                .zip(&theirs)
                .any(|(a, b)| (a - b).abs() > LAYOUT_TOLERANCE)
            {
                failures.push(format!(
                    "{name}: {label} [x, y, w, h]: bevy_markup {ours:?} vs browser {theirs:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} layout difference(s) from the browser:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Fluent oracle
// ---------------------------------------------------------------------------

/// Every `tests/vectors/*/` with a `fluent.html` (written by
/// `scripts/fluent_oracle.sh`: Fluent's reference DOM bindings, `@fluent/dom`,
/// translating `page.html` with `messages.ftl`) must build the same world
/// localized by bevy_markup as unlocalized from that reference translation — same
/// nodes, runs, faces, sizes and colors.
#[test]
fn fluent_oracle() {
    let vectors = vectors_with("fluent.html");
    assert!(!vectors.is_empty(), "no Fluent oracle vectors found");

    let mut failures = Vec::new();
    for name in &vectors {
        let mut localized = TestUi::from_vector(name);
        let ours = localized.settle().dump();
        let mut reference = TestUi::vector_page(name, "fluent.html", false);
        let theirs = reference.settle().dump();
        if ours != theirs {
            failures.push(format!(
                "{name} (- @fluent/dom, + bevy_markup):\n{}",
                line_diff(&theirs, &ours)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} vector(s) differ from @fluent/dom:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Lines only in `old` as `- `, only in `new` as `+ `, shared as `  `
/// (longest common subsequence; dumps are small).
fn line_diff(old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, String::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out += &format!("  {}\n", a[i]);
            (i, j) = (i + 1, j + 1);
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out += &format!("+ {}\n", b[j]);
            j += 1;
        } else {
            out += &format!("- {}\n", a[i]);
            i += 1;
        }
    }
    out
}

// Deliberate, documented differences from browsers (skipped by
// `compare_with_browser`):
// - root `background-color`: bevy_markup leaves the `HtmlUi` node's background to the
//   app.
// - `border-style`: bevy_markup ignores it (a `border-width` always applies); vectors
//   set `border-style: solid` so browsers compute the widths.
// - bullets: bevy_markup's `li` prefix `• ` is text on the `Text` root (not compared);
//   browsers draw a marker.

const FIXTURE_SIZE: (f32, f32) = (32.0, 24.0);
const BLOCKS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "p", "li", "pre"];
const CONTAINERS: &[&str] = &[
    "div",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "ul",
    "ol",
    "blockquote",
    "figure",
    "form",
];

/// The face CSS font matching picks for this computed style, with the test
/// harness's registered families (`Spectral` with all four faces, `Mono` with
/// regular only, `monospace` → `Mono`).
fn browser_face(record: &serde_json::Value) -> String {
    let family = record["fontFamily"].as_str().unwrap_or_default();
    let bold = record["fontWeight"].as_f64().unwrap_or(400.0) > 500.0;
    let italic = record["fontStyle"].as_str().unwrap_or("normal") != "normal";
    for name in family.split(',') {
        match name
            .trim()
            .trim_matches(['"', '\''])
            .to_ascii_lowercase()
            .as_str()
        {
            "spectral" => {
                return match (bold, italic) {
                    (false, false) => "serif",
                    (true, false) => "serif-bold",
                    (false, true) => "serif-italic",
                    (true, true) => "serif-bold-italic",
                }
                .to_owned();
            }
            "mono" | "monospace" => return "mono".to_owned(),
            _ => {}
        }
    }
    "default".to_owned()
}

fn browser_color(css: &str) -> Option<String> {
    let inner = css
        .strip_prefix("rgba(")
        .or_else(|| css.strip_prefix("rgb("))?;
    let parts: Vec<f32> = inner
        .trim_end_matches(')')
        .split(',')
        .map(|part| part.trim().parse().unwrap())
        .collect();
    let alpha = parts.get(3).copied().unwrap_or(1.0);
    (alpha > 0.0).then(|| {
        hex(Color::srgba_u8(
            parts[0] as u8,
            parts[1] as u8,
            parts[2] as u8,
            (alpha * 255.0).round() as u8,
        ))
    })
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn compare_with_browser(ui: &mut TestUi, oracle: &serde_json::Value) -> Vec<String> {
    let mut problems = Vec::new();
    let root = ui.root();
    let world = ui.world_mut();

    // Text: one (char, face, size, color) per non-whitespace character.
    let mut ours = Vec::new();
    collect_chars(world, root, &mut ours);
    let theirs: Vec<(char, String, f64, String)> = oracle["runs"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|run| {
            let style = (
                browser_face(run),
                round(run["fontSize"].as_f64().unwrap()),
                browser_color(run["color"].as_str().unwrap()).unwrap_or_default(),
            );
            run["text"]
                .as_str()
                .unwrap()
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(move |c| (c, style.0.clone(), style.1, style.2.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    let text =
        |chars: &[(char, String, f64, String)]| chars.iter().map(|c| c.0).collect::<String>();
    if text(&ours) != text(&theirs) {
        problems.push(format!(
            "text differs: bevy_markup {:?} vs browser {:?}",
            text(&ours),
            text(&theirs)
        ));
    } else if let Some(index) = (0..ours.len()).find(|&i| ours[i] != theirs[i]) {
        let context: String = ours[index.saturating_sub(8)..=index]
            .iter()
            .map(|c| c.0)
            .collect();
        let (_, face, size, color) = &ours[index];
        let (_, b_face, b_size, b_color) = &theirs[index];
        problems.push(format!(
            "text style at …{context:?}: bevy_markup {face} {size}px {color} vs browser {b_face} {b_size}px {b_color}"
        ));
    }

    // Boxes: the root (`html`) and every block/container, in document order.
    let pairs = match pair_elements(world, root, oracle) {
        Ok(pairs) => pairs,
        Err(problem) => {
            problems.push(problem);
            return problems;
        }
    };
    for (entity, theirs, label) in pairs {
        let tag = theirs["tag"].as_str().unwrap();
        let entity_ref = world.entity(entity);
        let node = entity_ref.get::<Node>().cloned().unwrap_or_default();
        let sides = |rect: UiRect| {
            [rect.top, rect.right, rect.bottom, rect.left].map(|v| match v {
                Val::Px(px) => px as f64,
                _ => 0.0,
            })
        };
        let floats = |key: &str| -> [f64; 4] {
            let values: Vec<f64> = theirs[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            [values[0], values[1], values[2], values[3]]
        };
        let mut check = |what: &str, ours: String, browser: String| {
            if ours != browser {
                problems.push(format!(
                    "{label}: {what} bevy_markup {ours} vs browser {browser}"
                ));
            }
        };

        check(
            "padding",
            format!("{:?}", sides(node.padding)),
            format!("{:?}", floats("padding")),
        );
        check(
            "border",
            format!("{:?}", sides(node.border)),
            format!("{:?}", floats("border")),
        );

        let our_slice = entity_ref
            .get::<ImageNode>()
            .and_then(|image| match &image.image_mode {
                NodeImageMode::Sliced(slicer) => {
                    let file = image
                        .image
                        .path()
                        .map(|p| p.path().display().to_string())
                        .unwrap_or_default();
                    let (min, max) = (slicer.border.min_inset, slicer.border.max_inset);
                    let mode = match slicer.sides_scale_mode {
                        SliceScaleMode::Stretch => "stretch",
                        SliceScaleMode::Tile { .. } => "tile",
                    };
                    Some(format!(
                        "{file} {},{},{},{} {mode}",
                        min.y, max.x, max.y, min.x
                    ))
                }
                _ => None,
            });
        check(
            "border-image",
            format!("{our_slice:?}"),
            format!("{:?}", browser_slice(theirs)),
        );

        // Root background: allowed difference (belongs to the app).
        if tag != "html" {
            let our_bg = entity_ref
                .get::<BackgroundColor>()
                .filter(|bg| bg.0 != Color::NONE)
                .map(|bg| hex(bg.0));
            let their_bg = browser_color(theirs["backgroundColor"].as_str().unwrap());
            check("background", format!("{our_bg:?}"), format!("{their_bg:?}"));
        }
        if CONTAINERS.contains(&tag) {
            let our_gap = match node.row_gap {
                Val::Px(px) => px as f64,
                _ => 0.0,
            };
            // `normal` = no CSS gap → bevy_markup uses the root's gap (0 here).
            let their_gap = theirs["rowGap"]
                .as_str()
                .unwrap()
                .trim_end_matches("px")
                .parse()
                .unwrap_or(0.0);
            check("gap", format!("{our_gap}"), format!("{their_gap}"));
        }
    }
    problems
}

/// `file t,r,b,l stretch|tile` from the browser's computed border-image, with
/// `%` slices resolved against the 32×24 fixture.
fn browser_slice(record: &serde_json::Value) -> Option<String> {
    let source = record["borderImageSource"].as_str().unwrap();
    let file = source
        .strip_prefix("url(\"")?
        .trim_end_matches("\")")
        .rsplit('/')
        .next()?
        .to_owned();
    let values: Vec<&str> = record["borderImageSlice"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .filter(|v| *v != "fill")
        .collect();
    let at = |i: usize| match values.len() {
        1 => values[0],
        2 => values[i % 2],
        3 => values[if i == 3 { 1 } else { i }],
        _ => values[i],
    };
    let (width, height) = FIXTURE_SIZE;
    let side = |i: usize| -> f32 {
        let value = at(i);
        match value.strip_suffix('%') {
            Some(percent) => {
                percent.parse::<f32>().unwrap() / 100.0 * if i % 2 == 0 { height } else { width }
            }
            None => value.parse().unwrap(),
        }
    };
    let tile = record["borderImageRepeat"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .any(|r| r != "stretch");
    Some(format!(
        "{file} {},{},{},{} {}",
        side(0),
        side(1),
        side(2),
        side(3),
        if tile { "tile" } else { "stretch" }
    ))
}

/// `(char, face, size, color)` for each non-whitespace character of every
/// `TextSpan` below `entity`, in document order (`Text` roots hold only the
/// `li` bullet, an allowed difference).
fn collect_chars(world: &World, entity: Entity, out: &mut Vec<(char, String, f64, String)>) {
    let entity_ref = world.entity(entity);
    if let (Some(span), Some(font), Some(color)) = (
        entity_ref.get::<TextSpan>(),
        entity_ref.get::<TextFont>(),
        entity_ref.get::<TextColor>(),
    ) {
        let size = match font.font_size {
            FontSize::Px(px) => round(px as f64),
            _ => f64::NAN,
        };
        let face = face_label(&font.font);
        for c in span.0.chars().filter(|c| !c.is_whitespace()) {
            out.push((c, face.clone(), size, hex(color.0)));
        }
    }
    if let Some(children) = entity_ref.get::<Children>() {
        for &child in children {
            collect_chars(world, child, out);
        }
    }
}

/// bevy_markup's root and block/container entities paired, in document order, with
/// the browser's records of the same elements (inline elements have no
/// entity), each labelled `tag.class…`.
fn pair_elements<'o>(
    world: &World,
    root: Entity,
    oracle: &'o serde_json::Value,
) -> Result<Vec<(Entity, &'o serde_json::Value, String)>, String> {
    let mut ours = vec![root];
    collect_elements(world, root, &mut ours);
    let theirs: Vec<&serde_json::Value> = oracle["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            let tag = e["tag"].as_str().unwrap();
            tag == "html" || BLOCKS.contains(&tag) || CONTAINERS.contains(&tag)
        })
        .collect();
    if ours.len() != theirs.len() {
        return Err(format!(
            "element count: bevy_markup {} vs browser {}",
            ours.len(),
            theirs.len()
        ));
    }
    let mut pairs = Vec::new();
    for (entity, record) in ours.into_iter().zip(theirs) {
        let tag = record["tag"].as_str().unwrap();
        let label = match record["classes"].as_array().unwrap().as_slice() {
            [] => tag.to_owned(),
            classes => format!(
                "{tag}.{}",
                classes
                    .iter()
                    .map(|c| c.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
        };
        if let Some(element) = world.entity(entity).get::<HtmlElement>()
            && element.tag != tag
        {
            return Err(format!(
                "{label}: element order differs (bevy_markup has {})",
                element.tag
            ));
        }
        pairs.push((entity, record, label));
    }
    Ok(pairs)
}

/// Entities with an `HtmlElement` below `entity`, in document order.
fn collect_elements(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if let Some(children) = world.entity(entity).get::<Children>() {
        for &child in children {
            if world.entity(child).contains::<HtmlElement>() {
                out.push(child);
            }
            collect_elements(world, child, out);
        }
    }
}

/// An `HtmlUi` nested inside another one's subtree, both updating in the
/// same frame. While the element holding it is kept, the nested UI survives
/// the ancestor's update and gets its own; when the ancestor's update
/// replaces that element, the nested UI is despawned with it — after its own
/// commands (deepest UIs go first), so nothing hits a despawned entity
/// (bug_0016).
#[test]
fn nested_ui_survives_or_goes_with_its_slot_without_panics() {
    let mut ui = TestUi::new(
        "nested-rebuild",
        &[
            (
                "outer.html",
                r#"<div id="{{ slot }}"><p data-l10n-id="outer">Outer</p></div>"#,
            ),
            ("inner.html", r#"<p data-l10n-id="inner">Inner</p>"#),
            (
                "style.css",
                "html { color: #ffffff; font-family: Spectral; font-size: 20px }",
            ),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            ("locales/en-US/ui.ftl", "outer = Outer EN\ninner = Inner EN"),
            (
                "locales/de/main.ftl.ron",
                r#"(locale: "de", resources: ["ui.ftl"])"#,
            ),
            ("locales/de/ui.ftl", "outer = Outer DE\ninner = Inner DE"),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn(
        "outer.html",
        TemplateContext::new().with("slot", "slot"),
        Node::default(),
    );
    ui.settle();

    // Nest a second UI into the outer's slot.
    let slot = {
        let world = ui.world_mut();
        let mut query = world.query::<(Entity, &HtmlElement)>();
        query
            .iter(world)
            .find(|(_, element)| element.id.as_deref() == Some("slot"))
            .map(|(entity, _)| entity)
            .expect("slot")
    };
    let template = ui.world_mut().resource::<AssetServer>().load("inner.html");
    let nested = ui
        .world_mut()
        .spawn((
            HtmlUi::new(template),
            TemplateContext::new(),
            Node::default(),
            ChildOf(slot),
        ))
        .id();
    ui.settle().assert_dump(
        r#"
html-ui
  div#slot
    p
      "Outer EN" serif 20px #ffffff
    -
      p
        "Inner EN" serif 20px #ffffff
"#,
    );

    // Both UIs re-localize in the same frame; the slot is kept, and the
    // nested UI with it.
    let german = ui
        .world_mut()
        .resource::<AssetServer>()
        .load("locales/de/main.ftl.ron");
    ui.world_mut().insert_resource(ActiveLocale::new(german));
    ui.settle().assert_dump(
        r#"
html-ui
  div#slot
    p
      "Outer DE" serif 20px #ffffff
    -
      p
        "Inner DE" serif 20px #ffffff
"#,
    );

    // Both update again in one frame, and the outer's update replaces the
    // slot (a new `id`): the nested UI goes with it.
    let english = ui
        .world_mut()
        .resource::<AssetServer>()
        .load("locales/en-US/main.ftl.ron");
    ui.world_mut().insert_resource(ActiveLocale::new(english));
    let root = ui.root();
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("slot", "other");
    ui.settle().assert_dump(
        r#"
html-ui
  div#other
    p
      "Outer EN" serif 20px #ffffff
"#,
    );
    assert!(
        ui.world_mut().get_entity(nested).is_err(),
        "the nested UI is despawned with its replaced slot"
    );
}

/// `data-on-*` hooks parse into `ElementSignals` on the element entity (the
/// click/press/release observers ride along; hover is tracked by
/// `hover_signals`).
#[test]
fn data_on_hooks_attach_to_elements() {
    let mut ui = TestUi::new(
        "signal-hooks",
        &[
            (
                "page.html",
                r#"<div id="box" data-on-click="buy" data-with='{"n": 3}'><p>Hello</p></div>"#,
            ),
            ("style.css", "html { color: #ffffff; font-size: 20px }"),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();

    let world = ui.world_mut();
    let mut query = world.query::<(Entity, &HtmlElement, &ElementSignals)>();
    let found = query
        .iter(world)
        .find(|(_, element, _)| element.id.as_deref() == Some("box"))
        .map(|(_, _, signals)| signals)
        .expect("box element with signals");
    assert_eq!(found.0.len(), 1);
    assert_eq!(found.0[0].name, "buy");
    assert_eq!(found.0[0].payload["n"], 3);
}

/// `:hover`/`:active` styles apply from the element's [`PseudoState`] and
/// update in place: the state change restyles, it doesn't rebuild (the
/// entity keeps whatever the app attached).
#[test]
fn hover_state_restyles_in_place() {
    let mut ui = TestUi::new(
        "pseudo-hover",
        &[
            (
                "page.html",
                r#"<div id="box" class="card"><p>Hello</p></div>"#,
            ),
            (
                "style.css",
                r#"
html { color: #ffffff; font-size: 20px }
.card { background-color: #111111 }
.card:hover { background-color: #222222; color: #00ff00 }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();

    let box_entity = {
        let world = ui.world_mut();
        let mut query = world.query::<(Entity, &HtmlElement)>();
        query
            .iter(world)
            .find(|(_, element)| element.id.as_deref() == Some("box"))
            .map(|(entity, _)| entity)
            .expect("box")
    };

    // Hover: the state change restyles the card in place.
    ui.world_mut().entity_mut(box_entity).insert(PseudoState {
        hovered: true,
        ..default()
    });
    ui.settle().assert_dump(
        r#"
html-ui
  div#box.card bg=#222222
    p
      "Hello" default 20px #00ff00
"#,
    );

    // Unhover: back to the base styles.
    ui.world_mut()
        .entity_mut(box_entity)
        .insert(PseudoState::default());
    ui.settle().assert_dump(
        r#"
html-ui
  div#box.card bg=#111111
    p
      "Hello" default 20px #ffffff
"#,
    );
}

/// A restyle of an `HtmlUi` whose slots hold app-nested `HtmlUi`s must not
/// fall back to a rebuild: `same_shape` ignores nested-UI children, and the
/// fallback's `despawn_related` killed the nested UIs — panicking when they
/// updated the same frame (bug_0017).
#[test]
fn restyle_with_nested_uis_keeps_them() {
    let mut ui = TestUi::new(
        "nested-restyle",
        &[
            (
                "outer.html",
                r#"<div id="slot"><p class="card">Outer</p></div>"#,
            ),
            ("inner.html", r#"<p class="card">Inner</p>"#),
            (
                "style.css",
                r#"
html { color: #ffffff; font-size: 20px }
.card { background-color: #111111 }
.card:hover { background-color: #222222 }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("outer.html", TemplateContext::new(), Node::default());
    ui.settle();

    let (slot, inner_template) = {
        let world = ui.world_mut();
        let mut elements = world.query::<(Entity, &HtmlElement)>();
        let slot = elements
            .iter(world)
            .find(|(_, element)| element.id.as_deref() == Some("slot"))
            .map(|(entity, _)| entity)
            .expect("slot");
        let template = world.resource::<AssetServer>().load("inner.html");
        (slot, template)
    };
    let inner = ui
        .world_mut()
        .spawn((
            HtmlUi::new(inner_template),
            TemplateContext::new(),
            Node::default(),
            ChildOf(slot),
        ))
        .id();
    ui.settle();

    // Hover both UIs in the same frame: the outer restyles (its slots keep
    // the nested UI), the inner restyles too — nobody despawns anybody.
    let mut cards = ui
        .world_mut()
        .query::<(Entity, &HtmlElement, Option<&PseudoState>)>();
    let cards: Vec<Entity> = cards
        .iter(ui.world_mut())
        .filter(|(_, element, _)| element.has_class("card"))
        .map(|(entity, _, _)| entity)
        .collect();
    assert_eq!(cards.len(), 2);
    for entity in cards {
        ui.world_mut().entity_mut(entity).insert(PseudoState {
            hovered: true,
            ..default()
        });
    }
    ui.settle().assert_dump(
        r#"
html-ui
  div#slot
    p.card bg=#222222
      -
        "Outer" default 20px #ffffff
    -
      p.card bg=#222222
        -
          "Inner" default 20px #ffffff
"#,
    );
    assert!(
        ui.world_mut().get_entity(inner).is_ok(),
        "the nested UI survived the outer restyle"
    );
}

/// An `<img>`'s CSS `width`/`height` sizes the inline box against the
/// containing block's content box at layout time: one dimension scales
/// with the aspect ratio preserved (the fixture is 32×24), both stretch,
/// and padding insets the resolution. Without a declaration the image
/// keeps its intrinsic size.
#[test]
fn layout_image_css_sizes() {
    let page = r#"<div id="root">
  <p id="half"><img class="half" src="frame.png"></p>
  <p id="padded"><img class="full" src="frame.png"></p>
  <p id="stretched"><img class="stretch" src="frame.png"></p>
  <p id="tall"><img class="tall" src="frame.png"></p>
  <p id="intrinsic"><img src="frame.png"></p>
</div>"#;
    let css = "img.half { width: 50% } img.full { width: 100% } \
padded { padding: 10px } p#padded { padding: 10px } img.stretch { width: 100%; height: 50% } img.tall { height: 50% }";
    let mut ui = TestUi::with_layout(
        "image-css-sizes",
        &[("page.html", page), ("style.css", css)],
        UVec2::new(400, 600),
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new(),
        Node {
            width: Val::Px(400.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
    );
    ui.settle();
    ui.update(4);

    // The image box: the span's `InlineBox`, sized by the layout pass.
    let world = ui.world_mut();
    let mut boxes = world.query::<(&bevy::text::InlineBox, &bevy::ui::widget::InlineImage)>();
    let sized: Vec<Vec2> = boxes.iter(world).map(|(box_, _)| box_.size).collect();
    // The paragraphs are root-width blocks (400 content); the padded one
    // has 10px padding per side, so its content box is 380.
    let expected = vec![
        Vec2::new(200.0, 150.0), // half: 50% of 400, aspect 32:24
        Vec2::new(380.0, 285.0), // full: 100% of 380
        Vec2::new(400.0, 300.0), // stretch: both dimensions
        Vec2::new(400.0, 300.0), // tall: 50% of 600, aspect preserved
        Vec2::new(32.0, 24.0),   // intrinsic (no declaration)
    ];
    let mut actual = sized.clone();
    actual.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.total_cmp(&b.y)));
    let mut expected_sorted = expected.clone();
    expected_sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap().then(a.y.total_cmp(&b.y)));
    for (actual, expected) in actual.iter().zip(&expected_sorted) {
        assert!(
            (actual - expected).max_element() <= LAYOUT_TOLERANCE,
            "image box {actual:?} != {expected:?} (all {sized:?})"
        );
    }
}

/// ADR 0021 regression tests: a build that reuses its clean skeleton must
/// equal a build without one. Each test drives one long-lived `reused` UI
/// and a `fresh` comparator that drops its skeleton before every change,
/// so every build there collects from scratch.
mod skeleton_reuse {
    use super::*;
    use bevy_markup::Skeleton;

    fn spawn_two(
        name: &'static str,
        files: &[(&str, &str)],
        context: &TemplateContext,
    ) -> (TestUi, TestUi) {
        let reused = TestUi::new(&format!("{name}-reused"), files)
            .stylesheet("a.css")
            .spawn("page.html", context.clone(), Node::default());
        let fresh = TestUi::new(&format!("{name}-fresh"), files)
            .stylesheet("a.css")
            .spawn("page.html", context.clone(), Node::default());
        (reused, fresh)
    }

    /// Applies `op` to both UIs (the comparator drops its skeleton first,
    /// so its build is fresh), settles both, and compares the dumps.
    /// Both UIs are settled when it returns.
    fn assert_same_after(
        reused: &mut TestUi,
        fresh: &mut TestUi,
        op: impl Fn(&mut TestUi, bool),
    ) {
        reused.settle();
        fresh.settle();
        op(reused, true);
        op(fresh, false);
        assert_eq!(
            reused.settle().dump(),
            fresh.settle().dump(),
            "the reused UI diverged from a fresh build"
        );
    }

    fn set_context(ui: &mut TestUi, key: &'static str, value: &str) {
        let root = ui.root();
        ui.world_mut()
            .get_mut::<TemplateContext>(root)
            .unwrap()
            .insert(key, &value);
    }

    /// Changing the dynamic content must not drop the unchanged
    /// walk-through-tag siblings around it (`span`/`b` in a container
    /// produce no spec of their own; returning early after a failed reuse
    /// lookup lost them).
    #[test]
    fn reused_walkthrough_siblings_keep_their_content() {
        let files = [
            (
                "page.html",
                r#"<div class="panel"><p class="static">Static</p><span>Inline tag</span><b>Bold tag</b><p>Value: {{ n }}</p></div>"#,
            ),
            ("a.css", "html { color: #ffffff; font-size: 20px }"),
        ];
        let (mut reused, mut fresh) =
            spawn_two("walkthrough", &files, &TemplateContext::new().with("n", &1));
        assert_same_after(&mut reused, &mut fresh, |ui, keep| {
            let root = ui.root();
            if !keep {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            set_context(ui, "n", "2");
        });
        let dump = reused.dump();
        assert!(
            dump.contains("\"Inline tag\" default 20px #ffffff")
                && dump.contains("\"Bold tag\" default 20px #ffffff"),
            "walk-through-tag content vanished:\n{dump}"
        );
    }

    /// A stylesheet swap in the same frame as a content change (the
    /// theme-button case) must restyle the unchanged subtrees too: the
    /// cached specs were built from the previous sheet.
    #[test]
    fn stylesheet_swap_with_a_content_change_restyles_reused_subtrees() {
        let files = [
            ("page.html", "<p class=\"static\">Static</p><p>Value: {{ n }}</p>"),
            ("a.css", "html { color: #ffffff } .static { color: #ff0000 }"),
            ("b.css", "html { color: #ffffff } .static { color: #00ff00 }"),
        ];
        let (mut reused, mut fresh) =
            spawn_two("swap", &files, &TemplateContext::new().with("n", &1));
        let b_sheet = reused.load::<Stylesheet>("b.css");
        let b_sheet_fresh = fresh.load::<Stylesheet>("b.css");
        assert_same_after(&mut reused, &mut fresh, move |ui, keep| {
            let root = ui.root();
            if !keep {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            ui.world_mut()
                .entity_mut(root)
                .insert(HtmlStylesheet(if keep {
                    b_sheet.clone()
                } else {
                    b_sheet_fresh.clone()
                }));
            set_context(ui, "n", "2");
        });
        let dump = reused.dump();
        assert!(
            dump.contains("\"Static\" default 16px #00ff00"),
            "the static paragraph kept the previous sheet's color:\n{dump}"
        );
    }

    /// A change that arrives while the replacement sheet is loading is
    /// merged into the build that resolves it (a failed sheet emits no
    /// event): that build looks content-only, but the styles changed —
    /// no reuse. The ready override (blue) is replaced by a failing
    /// sheet; the default sheet (red) must take over.
    #[test]
    fn change_while_a_sheet_loads_restyles_when_it_fails() {
        let files = [
            ("page.html", "<p class=\"static\">Static</p><p>Value: {{ n }}</p>"),
            ("a.css", "html { color: #ffffff } .static { color: #ff0000 }"),
            ("b.css", "html { color: #ffffff } .static { color: #0000ff }"),
        ];
        let mut reused = TestUi::new("failed-sheet-reused", &files)
            .stylesheet("a.css")
            .spawn("page.html", TemplateContext::new().with("n", &1), Node::default());
        let b_sheet = reused.load::<Stylesheet>("b.css");
        let reused_root = reused.root();
        reused
            .world_mut()
            .entity_mut(reused_root)
            .insert(HtmlStylesheet(b_sheet));
        reused.settle();
        assert!(
            reused.dump().contains("#0000ff"),
            "the override should style the paragraph blue:\n{}",
            reused.dump()
        );
        let mut fresh = TestUi::new("failed-sheet-fresh", &files)
            .stylesheet("a.css")
            .spawn("page.html", TemplateContext::new().with("n", &1), Node::default());
        let b_sheet = fresh.load::<Stylesheet>("b.css");
        let fresh_root = fresh.root();
        fresh
            .world_mut()
            .entity_mut(fresh_root)
            .insert(HtmlStylesheet(b_sheet));
        fresh.settle();

        // The swap plus a change arrive while the sheet loads (a `Wait` —
        // the render lands that frame); the sheet then fails without an
        // event, and a further change (the sheet untouched — re-requesting
        // it would flip it to `Loading` for a frame) is the build that
        // resolves the merge.
        for (ui, keep) in [(&mut reused, true), (&mut fresh, false)] {
            let root = ui.root();
            if !keep {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            let missing: Handle<Stylesheet> = ui.load("missing.css");
            ui.world_mut()
                .entity_mut(root)
                .insert(HtmlStylesheet(missing));
            set_context(ui, "n", "2");
        }
        reused.settle();
        fresh.settle();
        set_context(&mut reused, "n", "3");
        set_context(&mut fresh, "n", "3");
        assert_eq!(
            reused.settle().dump(),
            fresh.settle().dump(),
            "the failed sheet's update reused the stale styles"
        );
        assert!(
            reused.dump().contains("\"Static\" default 16px #ff0000"),
            "the static paragraph should fall back to the default sheet:\n{}",
            reused.dump()
        );
    }

    /// A render that changed while the debug outline froze the skeleton
    /// (or while a `Wait` swallowed the build) must not leave the
    /// skeleton's document older than its specs: the next reuse would
    /// pair against the wrong document.
    #[test]
    fn render_change_during_outline_frames_stays_in_step() {
        let files = [(
            "page.html",
            "<p>{{ x }}</p><p class=\"static\">Static</p><p>{{ y }}</p>",
        )];
        let sheet = "html { color: #ffffff }";
        let files = [("page.html", files[0].1), ("a.css", sheet)];
        let (mut reused, mut fresh) = spawn_two(
            "outline-freeze",
            &files,
            &TemplateContext::new().with("x", &"a").with("y", &"b"),
        );
        reused.settle();
        fresh.settle();

        // The render changes while the outline is on (the skeleton is
        // frozen at `x = a`), the outline goes with NO content change (the
        // specs are replaced from the `x = b` render; the stored document
        // must follow), and a change back to `x = a` reuses: paired
        // against the stale document, `x` looks clean and the cached
        // spec's `b` would stick.
        let toggle_outline = |ui: &mut TestUi, outlined: bool, strip: bool| {
            let root = ui.root();
            if strip {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            let mut entity = ui.world_mut().entity_mut(root);
            if outlined {
                entity.insert(HtmlDebugOutline);
            } else {
                entity.remove::<HtmlDebugOutline>();
            }
        };
        for (ui, keep) in [(&mut reused, true), (&mut fresh, false)] {
            toggle_outline(ui, true, !keep);
            set_context(ui, "x", "b");
        }
        reused.settle();
        fresh.settle();
        assert_eq!(reused.dump(), fresh.dump());
        for (ui, keep) in [(&mut reused, true), (&mut fresh, false)] {
            toggle_outline(ui, false, !keep);
        }
        reused.settle();
        fresh.settle();
        assert_eq!(
            reused.dump(),
            fresh.dump(),
            "the skeleton fell out of step with its document"
        );
        set_context(&mut reused, "x", "a");
        set_context(&mut fresh, "x", "a");
        set_context(&mut reused, "y", "d");
        set_context(&mut fresh, "y", "d");
        reused.settle();
        fresh.settle();
        assert_eq!(
            reused.dump(),
            fresh.dump(),
            "the retried pairing paired against the wrong document"
        );
        assert!(
            reused.dump().contains("\"a\" default 16px #ffffff")
                && reused.dump().contains("\"d\" default 16px #ffffff"),
            "wrong text after the outline came off:\n{}",
            reused.dump()
        );
    }

    /// After builds the adaptive gate skipped (reuse scored useless), the
    /// skeleton must still describe the entities: a gate-skipped build
    /// updates them from fresh specs, so its specs must replace the
    /// cached ones. The sequence: enough one-key changes to switch the
    /// gate off, a change the skipped builds apply (`t = 5`), a restyle
    /// (re-styles the entities from the current document), then a change
    /// back (`t = 0`) whose subtree pairs clean against a stale document
    /// and would keep the stale content.
    #[test]
    fn gate_skipped_builds_stay_sound() {
        let rows = (0..9)
            .map(|i| format!("<p>Row {i}: {{{{ v }}}}</p>"))
            .collect::<String>();
        let page = format!("<p class=\"t\">T: {{{{ t }}}}</p>{rows}");
        let files = [
            ("page.html", page.as_str()),
            ("a.css", "html { color: #ffffff }"),
            ("a2.css", "html { color: #ffffff }"),
        ];
        let (mut reused, mut fresh) = spawn_two(
            "gate",
            &files,
            &TemplateContext::new().with("v", &0).with("t", &0),
        );
        reused.settle();
        fresh.settle();

        let toggle = |ui: &mut TestUi, keep: bool, v: u32| {
            let root = ui.root();
            if !keep {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            ui.world_mut()
                .get_mut::<TemplateContext>(root)
                .unwrap()
                .insert("v", &v);
        };
        // One-key changes: the first retry scores useless (1 of 10
        // subtrees), the gate switches off for the next 2^strike builds.
        for step in 1..=33u32 {
            let v = step % 2;
            toggle(&mut reused, true, v);
            toggle(&mut fresh, false, v);
            reused.settle();
            fresh.settle();
            assert_eq!(reused.dump(), fresh.dump(), "toggle {step}");
        }

        // Applied by a gate-skipped build, then a restyle, then back.
        for (ui, keep) in [(&mut reused, true), (&mut fresh, false)] {
            let root = ui.root();
            if !keep {
                ui.world_mut().entity_mut(root).remove::<Skeleton>();
            }
            ui.world_mut()
                .get_mut::<TemplateContext>(root)
                .unwrap()
                .insert("t", &5);
        }
        reused.settle();
        fresh.settle();
        assert_eq!(reused.dump(), fresh.dump());
        let a2 = reused.load::<Stylesheet>("a2.css");
        reused
            .world_mut()
            .resource_mut::<DefaultStylesheet>()
            .0 = Some(a2);
        let a2 = fresh.load::<Stylesheet>("a2.css");
        fresh
            .world_mut()
            .resource_mut::<DefaultStylesheet>()
            .0 = Some(a2);
        reused.settle();
        fresh.settle();
        assert_eq!(reused.dump(), fresh.dump());

        toggle(&mut reused, true, 1);
        toggle(&mut fresh, false, 1);
        {
            let root = reused.root();
            reused
                .world_mut()
                .get_mut::<TemplateContext>(root)
                .unwrap()
                .insert("t", &0);
        }
        {
            let root = fresh.root();
            fresh
                .world_mut()
                .get_mut::<TemplateContext>(root)
                .unwrap()
                .insert("t", &0);
        }
        reused.settle();
        fresh.settle();
        assert_eq!(
            reused.dump(),
            fresh.dump(),
            "the retried pairing served a skeleton the gate-skipped builds left stale"
        );
        assert!(
            reused.dump().contains("\"T: 0\" default 16px #ffffff"),
            "the t row kept its stale content:\n{}",
            reused.dump()
        );
    }

    /// `:hover` keeps working inside a reused subtree: the skip refreshes
    /// the subtree's `DomNode` handles, which the state walk keys on.
    #[test]
    fn hover_applies_inside_reused_subtrees() {
        let files = [
            (
                "page.html",
                // The loop shifts every later node's index between builds:
                // a stale handle then no longer keys the state walk.
                "<p>{% for i in range(end=n) %}<b>r</b>{% endfor %}</p><div class=\"panel\"><p class=\"x\">Hover me</p></div>",
            ),
            (
                "a.css",
                "html { color: #ffffff; font-size: 20px } .x:hover { color: #ff0000 }",
            ),
        ];
        let (mut reused, _fresh) = spawn_two(
            "hover",
            &files,
            &TemplateContext::new().with("n", &1),
        );
        reused.settle();

        // Content changes around the clean panel: its subtree reuses.
        // `n` is a number: it bounds a Tera `range`.
        for n in 2..=4u32 {
            let root = reused.root();
            reused
                .world_mut()
                .get_mut::<TemplateContext>(root)
                .unwrap()
                .insert("n", &n);
            reused.settle();
        }

        let hovered = reused
            .world_mut()
            .query::<(Entity, &bevy_markup::html::HtmlElement)>()
            .iter(reused.world_mut())
            .find(|(_, element)| element.classes.iter().any(|class| class == "x"))
            .expect("the hoverable element")
            .0;
        reused
            .world_mut()
            .entity_mut(hovered)
            .insert(bevy_markup::signals::PseudoState {
                hovered: true,
                ..default()
            });
        let dump = reused.settle().dump();
        assert!(
            dump.contains("\"Hover me\" default 20px #ff0000"),
            ":hover was lost in the reused subtree:\n{dump}"
        );
    }
}

// ---------------------------------------------------------------------------
// Loader regressions
// ---------------------------------------------------------------------------

/// Markdown conversion is chosen per file (bug_0060): an `.html` partial
/// included from a `.md` page stays HTML — its indented HTML is not a
/// Markdown indented code block — and a `.md` partial included from an
/// `.html` page converts. (The blank line ends the raw-HTML block, so
/// converting `part.html` would wrap the indented `<p>` in `pre.markdown`.)
#[test]
fn markdown_conversion_is_per_file() {
    TestUi::new(
        "markdown-partials",
        &[
            (
                "page.md",
                "# Title\n\n{% include \"part.html\" %}\n\nTrailing.\n",
            ),
            (
                "part.html",
                "<div class=\"card\">\n\n    <p id=\"part\">indented</p>\n</div>\n",
            ),
        ],
    )
    .spawn("page.md", TemplateContext::new(), Node::default())
    .settle()
    .assert_dump(
        r#"
html-ui
  h1.markdown
    "Title" default 16px #ffffff
  div.card
    p#part
      "indented" default 16px #ffffff
  p.markdown
    "Trailing." default 16px #ffffff
"#,
    );

    TestUi::new(
        "markdown-partial-from-html",
        &[
            ("page.html", "<h2>Doc</h2>\n{% include \"note.md\" %}\n"),
            ("note.md", "# Note\n\nHello *there*.\n"),
        ],
    )
    .spawn("page.html", TemplateContext::new(), Node::default())
    .settle()
    .assert_dump(
        r#"
html-ui
  h2
    "Doc" default 16px #ffffff
  h1.markdown
    "Note" default 16px #ffffff
  p.markdown
    "Hello there." default 16px #ffffff
"#,
    );
}

/// A render that changes while the active bundle is absent (loading, here:
/// failed) drops the previous document's translations — the elements show
/// their fallback content, not translations keyed on the old DOM's handles
/// (which would land on whatever element now sits at those handles;
/// bug_0064). When a real bundle arrives, the fresh document localizes.
#[test]
fn fresh_render_without_a_bundle_shows_fallback_not_stale_translations() {
    let mut ui = TestUi::new(
        "stale-localize",
        &[
            (
                "page.html",
                "{% if extra %}<div>extra</div>\n{% endif %}<p data-l10n-id=\"m\">fallback</p>",
            ),
            ("a.ftl.ron", "(locale: \"en-US\", resources: [\"a.ftl\"])"),
            ("a.ftl", "m = translated\n"),
            ("bad.ftl.ron", "(locale: \"en-US\", resources: [\"missing.ftl\"])"),
            ("c.ftl.ron", "(locale: \"en-US\", resources: [\"c.ftl\"])"),
            ("c.ftl", "m = localized again\n"),
        ],
    )
    .locale("a.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "translated" default 16px #ffffff
"#,
    );

    // The bundle fails while the context change re-renders in the same
    // frame: the fresh document adds a `<div>` before the `<p>`, shifting
    // the DOM handles the old map keyed on.
    let bad = ui.load::<LocaleBundle>("bad.ftl.ron");
    let root = ui.root();
    ui.world_mut().resource_mut::<ActiveLocale>().0 = Some(bad);
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("extra", &true);
    ui.settle().assert_dump(
        r#"
html-ui
  div
    -
      "extra" default 16px #ffffff
  p
    "fallback" default 16px #ffffff
"#,
    );

    // A real bundle arrives: the fresh document localizes normally.
    let c = ui.load::<LocaleBundle>("c.ftl.ron");
    ui.world_mut().resource_mut::<ActiveLocale>().set(c);
    ui.settle().assert_dump(
        r#"
html-ui
  div
    -
      "extra" default 16px #ffffff
  p
    "localized again" default 16px #ffffff
"#,
    );
}

/// A hot reload delivers `Modified` and `LoadedWithDependencies` in one
/// frame: the event scan must fold over all of them (one re-resolve, one
/// `HtmlUiBuilt`), not stop at the first match and re-serve the second next
/// frame (bug_0068).
#[test]
fn one_frame_bundle_events_build_once() {
    let mut ui = TestUi::new(
        "bundle-events",
        &[
            ("page.html", "<p data-l10n-id=\"m\">fallback</p>"),
            ("b.ftl.ron", "(locale: \"en-US\", resources: [\"b.ftl\"])"),
            ("b.ftl", "m = translated\n"),
        ],
    )
    .locale("b.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump(
        r#"
html-ui
  p
    "translated" default 16px #ffffff
"#,
    );
    let id = ui
        .world_mut()
        .resource::<ActiveLocale>()
        .0
        .as_ref()
        .expect("active bundle")
        .id();
    let builds = ui.builds();
    // Both events of a hot reload, delivered in one frame.
    let mut messages = ui
        .world_mut()
        .resource_mut::<Messages<AssetEvent<LocaleBundle>>>();
    messages.write(AssetEvent::Modified { id });
    messages.write(AssetEvent::LoadedWithDependencies { id });
    ui.update(10);
    assert_eq!(
        ui.builds(),
        builds + 1,
        "both events in one frame re-resolve once"
    );
    ui.assert_dump(
        r#"
html-ui
  p
    "translated" default 16px #ffffff
"#,
    );
}

/// Commented-out and raw-block includes are not references (bug_0067): an
/// include of a deleted file inside them must not fail the load; Tera
/// prints the raw block's tag literally. (The fenced-code/code-span skip is
/// covered by the loader unit test `comments_raw_and_code_are_not_references`:
/// end to end, a fenced `{% include %}` still fails the load at Tera's own
/// add-time include validation, so no vector can assert a literal code
/// sample while the load succeeds.)
#[test]
fn comments_raw_and_code_include_nothing() {
    let mut ui = TestUi::new(
        "skipped-includes",
        &[(
            "page.html",
            "{# {%- include \"gone.html\" -%} #}\n<p>after comment</p>\n\
             {% raw %}{% include \"gone.html\" %}{% endraw %}\n",
        )],
    );
    let handle = ui.load::<HtmlTemplate>("page.html");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    // A failing template never builds (the UI stays `Pending`), so settle
    // quietly and check the load state before the dump.
    ui.settle_quiet();
    assert!(
        !ui.world_mut()
            .resource::<AssetServer>()
            .load_state(handle.id())
            .is_failed(),
        "the commented-out and raw-block includes must not fail the load"
    );
    ui.assert_dump(
        r#"
html-ui
  p
    "after comment" default 16px #ffffff
  -
    "{% include \"gone.html\" %}" default 16px #ffffff
"#,
    );
}

/// Template includes and FTL resource paths may not escape the asset root
/// with `../` paths (bug_0069): the include fails its resolution without
/// reading the outside file, and a manifest naming an outside resource
/// fails its bundle load.
#[test]
fn unapproved_paths_fail_their_loads() {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    let unique = |suffix: &str| {
        format!(
            "bevy_markup-escape-{}-{}.{suffix}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        )
    };

    // A file one level above the (temporary) asset root, which the
    // include names.
    let outside = unique("html");
    std::fs::write(std::env::temp_dir().join(&outside), "<p>outside</p>").unwrap();
    let page = format!("{{% include \"../{outside}\" %}}");
    let mut ui = TestUi::new("unapproved-include", &[("page.html", page.as_str())]);
    let handle = ui.load::<HtmlTemplate>("page.html");
    ui.settle_quiet();
    assert!(
        ui.world_mut()
            .resource::<AssetServer>()
            .load_state(handle.id())
            .is_failed(),
        "an include escaping the asset root must fail the load"
    );
    drop(ui);
    let _ = std::fs::remove_file(std::env::temp_dir().join(&outside));

    let resource = unique("ftl");
    std::fs::write(
        std::env::temp_dir().join(&resource),
        "m = outside\n",
    )
    .unwrap();
    let manifest = format!("(locale: \"en-US\", resources: [\"../{resource}\"])");
    let mut ui = TestUi::new("unapproved-resource", &[("bad.ftl.ron", manifest.as_str())]);
    let bundle = ui.load::<LocaleBundle>("bad.ftl.ron");
    ui.settle_quiet();
    assert!(
        ui.world_mut()
            .resource::<AssetServer>()
            .load_state(bundle.id())
            .is_failed(),
        "a manifest resource escaping the asset root must fail the load"
    );
    drop(ui);
    let _ = std::fs::remove_file(std::env::temp_dir().join(&resource));
}

/// `Above`/`Left` overlays clamp to the viewport once their own size is
/// known (bug_0077): the `bottom`/`right` inset is measured from the
/// viewport's far edge, so staying inside means `inset <= limit` — clamped
/// on the inset alone, not keyed on the overlay's extent. An element flush
/// with the top (left) edge gets `bottom` (right) `= view_h` (`view_w`),
/// not `view_h + gap` (`view_w + gap`, which put the whole overlay past
/// the viewport's edge).
#[test]
fn anchored_overlay_above_left_clamp_to_the_viewport() {
    let mut ui = TestUi::with_layout(
        "anchor-edge",
        &[
            ("page.html", r#"<div id="a"></div>"#),
            (
                "style.css",
                "html { padding: 0 }\n#a { width: 100px; height: 30px }",
            ),
        ],
        UVec2::new(320, 240),
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle();
    let root = ui.root();
    let world = ui.world_mut();
    let a = element_by_id(world, root, "a");
    assert_eq!(
        node_rect(world, a),
        Some(Rect::new(0.0, 0.0, 100.0, 30.0)),
        "the element sits flush with the viewport's top-left corner"
    );
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(60.0),
                height: Val::Px(20.0),
                ..default()
            },
            HtmlAnchor::new(a, AnchorPlacement::Above).with_gap(4.0),
        ))
        .id();
    let placed = |ui: &mut TestUi| {
        ui.update(3);
        let world = ui.world_mut();
        let node = world.get::<Node>(overlay).unwrap();
        (
            node_rect(world, overlay).unwrap(),
            (node.left, node.top, node.right, node.bottom),
        )
    };

    // Above: `bottom` counts up from the viewport's bottom edge, so the
    // unclamped value is `240 - 0 + 4`; clamped it is the viewport height
    // itself, the overlay's near edge touching the viewport's top limit.
    let (rect, insets) = placed(&mut ui);
    assert_eq!(
        insets,
        (Val::Px(0.0), Val::Auto, Val::Auto, Val::Px(240.0)),
        "the bottom inset clamps to the viewport height (unclamped: 240 + gap)"
    );
    assert_eq!(rect.max.y, 0.0, "the overlay's edge reaches the top limit");

    ui.world_mut()
        .get_mut::<HtmlAnchor>(overlay)
        .unwrap()
        .placement = AnchorPlacement::Left;
    let (rect, insets) = placed(&mut ui);
    assert_eq!(
        insets,
        (Val::Auto, Val::Px(0.0), Val::Px(320.0), Val::Auto),
        "the right inset clamps to the viewport width (unclamped: 320 + gap)"
    );
    assert_eq!(rect.max.x, 0.0, "the overlay's edge reaches the left limit");
}

/// `HtmlWorldAnchor` divides the projection by `UiScale` and measures
/// against the viewport, not the render target (bug_0078): with
/// `UiScale(2.0)` the root sits at half the projected window px (not
/// double), and with an offset (split-screen) viewport a point inside the
/// viewport reads on-screen instead of hidden.
#[test]
fn world_anchor_respects_ui_scale_and_viewport_origin() {
    // With `UiScale(2.0)`: the point (0.5, 0.5) projects to window px
    // (240, 60), and the anchor writes UI px — half of that.
    let mut ui = TestUi::with_layout("world-anchor-scale", &[], UVec2::new(320, 240));
    ui.world_mut().insert_resource(UiScale(2.0));
    let world = ui.world_mut();
    let target = world.spawn(Visibility::Inherited).id();
    world.entity_mut(target).insert((
        Transform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
        GlobalTransform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
    ));
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(20.0),
                height: Val::Px(10.0),
                ..default()
            },
            HtmlWorldAnchor::new(target).with_offset(Vec3::new(0.25, 0.0, 0.0)),
        ))
        .id();
    let state = |ui: &mut TestUi, overlay: Entity| {
        ui.update(3);
        let world = ui.world_mut();
        let node = world.get::<Node>(overlay).unwrap();
        (
            (node.left, node.top),
            *world.get::<Visibility>(overlay).unwrap(),
            *world.get::<HtmlWorldAnchorView>(overlay).unwrap(),
        )
    };
    assert_eq!(
        state(&mut ui, overlay),
        (
            (Val::Px(110.0), Val::Px(20.0)),
            Visibility::Inherited,
            HtmlWorldAnchorView {
                distance: (0.5f32.powi(2) * 2.0).sqrt(),
                on_screen: true,
            }
        ),
        "window px (240, 60) divided by the UiScale of 2, minus the pivot"
    );

    // A split-screen camera with an offset viewport: projections come back
    // relative to the render target, so measuring against the viewport
    // (not the target's full size) keeps the point on-screen.
    let mut ui = TestUi::with_layout("world-anchor-split", &[], UVec2::new(320, 240));
    let world = ui.world_mut();
    let target = world.spawn(Visibility::Inherited).id();
    world.entity_mut(target).insert((
        Transform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
        GlobalTransform::from_translation(Vec3::new(0.25, 0.5, 0.0)),
    ));
    let split = world
        .spawn((
            Camera2d,
            Camera {
                viewport: Some(bevy::camera::Viewport {
                    physical_position: UVec2::new(160, 80),
                    physical_size: UVec2::new(160, 120),
                    ..default()
                }),
                computed: bevy::camera::ComputedCameraValues {
                    target_info: Some(bevy::camera::RenderTargetInfo {
                        physical_size: UVec2::new(320, 240),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            bevy::camera::RenderTarget::None {
                size: UVec2::new(320, 240),
            },
            GlobalTransform::from_translation(Vec3::ZERO),
        ))
        .id();
    let overlay = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(20.0),
                height: Val::Px(10.0),
                ..default()
            },
            HtmlWorldAnchor::new(target)
                .with_offset(Vec3::new(0.25, 0.0, 0.0))
                .with_camera(split),
        ))
        .id();
    // The point (0.5, 0.5) projects to window px (280, 110) — (160, 80)
    // into the viewport, well inside its 160×120.
    let ((left, top), visibility, view) = state(&mut ui, overlay);
    assert_eq!(
        (left, top),
        (Val::Px(110.0), Val::Px(20.0)),
        "(280, 110) relative to the viewport's (160, 80) origin, minus the pivot"
    );
    assert_eq!(visibility, Visibility::Inherited, "the point is on screen");
    assert!(view.on_screen);
}
