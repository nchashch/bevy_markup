//! Test vectors: HTML + CSS + Fluent + Tera inputs → the Bevy world they
//! produce, checked headlessly (no window, no renderer).
//!
//! Each test writes its input files into a fresh asset root, runs a minimal
//! app (`MinimalPlugins` + assets + images + `HtmlUiPlugin`) until every asset
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

use bevy::prelude::*;
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
            ("locales/en-US/main.ftl.ron", r#"(locale: "en-US", resources: ["ui.ftl"])"#),
            ("locales/en-US/ui.ftl", "title = Translated title"),
        ],
    )
    .stylesheet("style.css")
    .locale("locales/en-US/main.ftl.ron")
    .spawn("page.html", TemplateContext::new(), Node::default());
    ui.settle().assert_dump("\nhtml-ui\n  p\n    \"Translated title\" default 20px #ffffff\n");

    ui.world_mut().resource_mut::<ActiveLocale>().0 = None;
    ui.settle().assert_dump("\nhtml-ui\n  p\n    \"Own title\" default 20px #ffffff\n");
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
        TemplateContext::new().with("name", "Ada <The Brave>").with("n", &1),
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
  div.panel border=12,12,12,12 padding=4,4,4,4 gap=7 bg=#333333 slice=frame.png 2,3,4,5 stretch
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
    ui.settle();
    let root = ui.root();
    let failed = match ui.world_mut().get::<RenderedHtml>(root) {
        Some(RenderedHtml::Failed(message)) => message.clone(),
        _ => panic!("expected a render failure"),
    };
    assert!(failed.contains("player"), "error names the variable: {failed}");
    let dump = ui.dump();
    let mut lines = dump.lines();
    assert_eq!(lines.next(), Some("html-ui"));
    assert_eq!(lines.next(), Some("  -"), "{dump}");
    let run = lines.next().unwrap_or_default();
    assert!(
        run.starts_with("    \"failed to render: ") && run.contains("player") && run.ends_with("default 20px #ff0000"),
        "{dump}"
    );

    ui.world_mut().get_mut::<TemplateContext>(root).unwrap().insert("player", "Ada");
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

    let de = ui.load::<BundleAsset>("locales/de/main.ftl.ron");
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
            ("page.html", r#"<p id="a">One</p><div class="box"><p>Two</p></div>"#),
            ("red.css", "html { color: #ff0000; font-size: 20px }"),
            ("blue.css", "html { color: #0000ff; font-size: 20px } .box { padding: 4px }"),
            ("boxed.css", "html { color: #0000ff; font-size: 20px } p { background-color: #102030 }"),
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
    assert!(world.entity(wired).contains::<Wired>(), "app component survived");
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
    assert_eq!((events.built, events.restyled), (2, 1), "new wrappers: rebuilt");
}

/// Swapping stylesheet `a` → `b` must end exactly where a fresh build with
/// `b` does — also when only a nested node's shape changes, a frame goes
/// away, or text runs merge (the restyle must notice and rebuild).
#[test]
fn restyles_that_change_shape_match_a_fresh_build() {
    let page = r#"<p id="a">One</p><div class="box"><p class="inner">Plain <b>bold</b></p></div>"#;
    let cases = [
        // Only the nested block gains a box (needs a wrapper).
        ("html { color: #ffffff }", "html { color: #ffffff } .inner { background-color: #102030 }"),
        // The container loses its frame.
        (
            r#"html { color: #ffffff } .box { border-image: url("frame.png") 4 fill stretch; border-width: 4px }"#,
            "html { color: #ffffff }",
        ),
        // `b` stops differing from its block: two runs become one.
        ("html { color: #ffffff } b { color: #ff0000 }", "html { color: #ffffff }"),
    ];
    for (a, b) in cases {
        let mut swapped = TestUi::new("restyle-shape", &[("page.html", page), ("a.css", a), ("b.css", b)])
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
        &[("page.html", "<p>Text</p>"), ("style.css", "html { padding: 5px }")],
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
    assert_eq!(world.get::<Node>(root).unwrap().padding, UiRect::all(Val::Px(5.0)));
    let image = world.get::<ImageNode>(root).expect("the app's ImageNode survived");
    assert_eq!(image.image, backdrop);
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
        ("d.slice.ron", TypeId::of::<bevy_markup::nine_slice::NineSlice>()),
    ];
    let handles: Vec<_> = expected
        .iter()
        .map(|(path, _)| ui.world_mut().resource::<AssetServer>().load_builder().load_untyped(*path))
        .collect();
    for _ in 0..3000 {
        ui.update(1);
        let loaded = ui.world_mut().resource::<Assets<bevy::asset::LoadedUntypedAsset>>();
        if handles.iter().all(|handle| loaded.contains(handle)) {
            break;
        }
    }
    let loaded = ui.world_mut().resource::<Assets<bevy::asset::LoadedUntypedAsset>>();
    for ((path, type_id), handle) in expected.iter().zip(&handles) {
        let asset = loaded.get(handle).unwrap_or_else(|| panic!("{path} didn't load untyped"));
        assert_eq!(asset.handle.type_id(), *type_id, "{path}");
    }
}

/// A loaded template is named after its asset path (Tera picks HTML
/// autoescaping from the `.html` name).
#[test]
fn template_is_named_after_its_asset_path() {
    let mut ui = TestUi::new("template-name", &[("ui/page.html", "<p>x</p>")]);
    let handle = ui.load::<HtmlTemplate>("ui/page.html");
    for _ in 0..3000 {
        ui.update(1);
        if ui.world_mut().resource::<Assets<HtmlTemplate>>().contains(&handle) {
            break;
        }
    }
    let templates = ui.world_mut().resource::<Assets<HtmlTemplate>>();
    assert_eq!(templates.get(&handle).expect("loaded").name(), "ui/page.html");
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
    world.add_observer(|restyled: On<HtmlUiRestyled>, mut per: ResMut<UpdatesPer>| {
        *per.0.entry(restyled.entity).or_default() += 1;
    });
    world.entity_mut(user).insert(HtmlStylesheet(good.clone()));
    let failed = world.spawn((HtmlUi::new(template), HtmlStylesheet(broken))).id();
    ui.settle();
    let counts = |ui: &mut TestUi| {
        let per = &ui.world_mut().resource::<UpdatesPer>().0;
        (per.get(&user).copied().unwrap_or(0), per.get(&failed).copied().unwrap_or(0))
    };
    let (user_before, failed_before) = counts(&mut ui);
    assert!(user_before >= 1 && failed_before >= 1, "both built: {user_before}, {failed_before}");

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
        Extent3d { width: 64, height: 48, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[255; 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    ui.world_mut().resource_mut::<Assets<Image>>().insert(&frame, bigger).unwrap();
    ui.settle().assert_dump(&expected("12,16,12,16"));
}

/// The UI isn't built while its stylesheet is still loading (no unstyled
/// flash), and is built, styled, once it arrives.
#[test]
fn loading_stylesheet_defers_the_build() {
    let mut ui = TestUi::new(
        "sheet-loading",
        &[("page.html", "<p>Text</p>"), ("style.css", "html { color: #ff0000 }")],
    );
    // A handle nothing will ever load: the stylesheet stays "loading".
    let pending = ui.world_mut().resource::<Assets<Stylesheet>>().reserve_handle();
    ui.world_mut().insert_resource(DefaultStylesheet::new(pending.clone()));
    let real = ui.load::<Stylesheet>("style.css");
    let mut ui = ui.spawn("page.html", TemplateContext::new(), Node::default());
    let root = ui.root();
    for _ in 0..3000 {
        ui.update(1);
        let world = ui.world_mut();
        let rendered = matches!(world.get::<RenderedHtml>(root), Some(RenderedHtml::Ready(_)));
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
        let viewport = UVec2::new(viewport[0].as_u64().unwrap() as u32, viewport[1].as_u64().unwrap() as u32);
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
            if ours.iter().zip(&theirs).any(|(a, b)| (a - b).abs() > LAYOUT_TOLERANCE) {
                failures.push(format!("{name}: {label} [x, y, w, h]: bevy_markup {ours:?} vs browser {theirs:?}"));
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
            failures.push(format!("{name} (- @fluent/dom, + bevy_markup):\n{}", line_diff(&theirs, &ours)));
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
    "div", "section", "article", "header", "footer", "main", "nav", "aside", "ul", "ol",
    "blockquote", "figure", "form",
];

/// The face CSS font matching picks for this computed style, with the test
/// harness's registered families (`Spectral` with all four faces, `Mono` with
/// regular only, `monospace` → `Mono`).
fn browser_face(record: &serde_json::Value) -> String {
    let family = record["fontFamily"].as_str().unwrap_or_default();
    let bold = record["fontWeight"].as_f64().unwrap_or(400.0) > 500.0;
    let italic = record["fontStyle"].as_str().unwrap_or("normal") != "normal";
    for name in family.split(',') {
        match name.trim().trim_matches(['"', '\'']).to_ascii_lowercase().as_str() {
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
    let inner = css.strip_prefix("rgba(").or_else(|| css.strip_prefix("rgb("))?;
    let parts: Vec<f32> = inner
        .trim_end_matches(')')
        .split(',')
        .map(|part| part.trim().parse().unwrap())
        .collect();
    let alpha = parts.get(3).copied().unwrap_or(1.0);
    (alpha > 0.0).then(|| hex(Color::srgba_u8(parts[0] as u8, parts[1] as u8, parts[2] as u8, (alpha * 255.0).round() as u8)))
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
    let text = |chars: &[(char, String, f64, String)]| chars.iter().map(|c| c.0).collect::<String>();
    if text(&ours) != text(&theirs) {
        problems.push(format!("text differs: bevy_markup {:?} vs browser {:?}", text(&ours), text(&theirs)));
    } else if let Some(index) = (0..ours.len()).find(|&i| ours[i] != theirs[i]) {
        let context: String = ours[index.saturating_sub(8)..=index].iter().map(|c| c.0).collect();
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
            let values: Vec<f64> = theirs[key].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            [values[0], values[1], values[2], values[3]]
        };
        let mut check = |what: &str, ours: String, browser: String| {
            if ours != browser {
                problems.push(format!("{label}: {what} bevy_markup {ours} vs browser {browser}"));
            }
        };

        check("padding", format!("{:?}", sides(node.padding)), format!("{:?}", floats("padding")));
        check("border", format!("{:?}", sides(node.border)), format!("{:?}", floats("border")));

        let our_slice = entity_ref.get::<ImageNode>().and_then(|image| match &image.image_mode {
            NodeImageMode::Sliced(slicer) => {
                let file = image.image.path().map(|p| p.path().display().to_string()).unwrap_or_default();
                let (min, max) = (slicer.border.min_inset, slicer.border.max_inset);
                let mode = match slicer.sides_scale_mode {
                    SliceScaleMode::Stretch => "stretch",
                    SliceScaleMode::Tile { .. } => "tile",
                };
                Some(format!("{file} {},{},{},{} {mode}", min.y, max.x, max.y, min.x))
            }
            _ => None,
        });
        check("border-image", format!("{our_slice:?}"), format!("{:?}", browser_slice(theirs)));

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
            let their_gap = theirs["rowGap"].as_str().unwrap().trim_end_matches("px").parse().unwrap_or(0.0);
            check("gap", format!("{our_gap}"), format!("{their_gap}"));
        }
    }
    problems
}

/// `file t,r,b,l stretch|tile` from the browser's computed border-image, with
/// `%` slices resolved against the 32×24 fixture.
fn browser_slice(record: &serde_json::Value) -> Option<String> {
    let source = record["borderImageSource"].as_str().unwrap();
    let file = source.strip_prefix("url(\"")?.trim_end_matches("\")").rsplit('/').next()?.to_owned();
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
            Some(percent) => percent.parse::<f32>().unwrap() / 100.0 * if i % 2 == 0 { height } else { width },
            None => value.parse().unwrap(),
        }
    };
    let tile = record["borderImageRepeat"].as_str().unwrap().split_whitespace().any(|r| r != "stretch");
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
        return Err(format!("element count: bevy_markup {} vs browser {}", ours.len(), theirs.len()));
    }
    let mut pairs = Vec::new();
    for (entity, record) in ours.into_iter().zip(theirs) {
        let tag = record["tag"].as_str().unwrap();
        let label = match record["classes"].as_array().unwrap().as_slice() {
            [] => tag.to_owned(),
            classes => format!(
                "{tag}.{}",
                classes.iter().map(|c| c.as_str().unwrap()).collect::<Vec<_>>().join(".")
            ),
        };
        if let Some(element) = world.entity(entity).get::<HtmlElement>()
            && element.tag != tag
        {
            return Err(format!("{label}: element order differs (bevy_markup has {})", element.tag));
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
