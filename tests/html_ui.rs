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

use std::fmt::Write as _;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::asset::uuid::Uuid;
use bevy::asset::{AssetMetaCheck, UntypedHandle};
use bevy::image::{CompressedImageFormats, ImageLoader};
use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use p23::prelude::*;

const FRAME_PNG: &[u8] = include_bytes!("fixtures/frame.png"); // 32×24

/// Fake font handles: identity is all the mapping needs (no font files).
const fn font(id: u128) -> Handle<Font> {
    Handle::Uuid(Uuid::from_u128(id), PhantomData)
}
const SERIF: [Handle<Font>; 4] = [font(1), font(2), font(3), font(4)];
const MONO: Handle<Font> = font(5);

fn face_label(source: &FontSource) -> String {
    let FontSource::Handle(handle) = source else {
        return format!("{source:?}");
    };
    let labels = [
        (&SERIF[0], "serif"),
        (&SERIF[1], "serif-bold"),
        (&SERIF[2], "serif-italic"),
        (&SERIF[3], "serif-bold-italic"),
        (&MONO, "mono"),
    ];
    labels
        .iter()
        .find(|(font, _)| font.id() == handle.id())
        .map_or("default", |(_, label)| label)
        .to_owned()
}

#[derive(Resource, Default)]
struct Builds(usize);

struct TestUi {
    app: App,
    dir: PathBuf,
    root: Option<Entity>,
    /// Assets that must be loaded before the UI counts as settled.
    tracked: Vec<UntypedHandle>,
    builds_seen: usize,
}

impl TestUi {
    /// A headless app whose asset root holds `files` plus `frame.png`.
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "p23-test-{name}-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("frame.png"), FRAME_PNG).unwrap();
        for (path, contents) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: dir.to_string_lossy().into_owned(),
                meta_check: AssetMetaCheck::Never,
                ..default()
            },
            ImagePlugin::default(),
            HtmlUiPlugin,
        ))
        // bevy_render normally registers the image loader.
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()))
        .init_resource::<Builds>()
        .add_observer(|_: On<HtmlUiBuilt>, mut builds: ResMut<Builds>| builds.0 += 1);

        let mut fonts = app.world_mut().resource_mut::<FontFamilies>();
        fonts
            .insert(
                "Spectral",
                FontFaces::new(SERIF[0].clone())
                    .with_bold(SERIF[1].clone())
                    .with_italic(SERIF[2].clone())
                    .with_bold_italic(SERIF[3].clone()),
            )
            // Regular only: bold/italic requests fall back to it.
            .insert("Mono", FontFaces::new(MONO.clone()))
            .set_generic(GenericFamily::Monospace, "Mono");

        Self {
            app,
            dir,
            root: None,
            tracked: Vec::new(),
            builds_seen: 0,
        }
    }

    fn load<A: Asset>(&mut self, path: &str) -> Handle<A> {
        let handle: Handle<A> = self.app.world().resource::<AssetServer>().load(path.to_owned());
        self.tracked.push(handle.clone().untyped());
        handle
    }

    fn stylesheet(mut self, path: &str) -> Self {
        let sheet = self.load(path);
        self.app.insert_resource(DefaultStylesheet::new(sheet));
        self
    }

    fn locale(mut self, path: &str) -> Self {
        let bundle = self.load(path);
        self.app.insert_resource(ActiveLocale::new(bundle));
        self
    }

    fn spawn(mut self, template: &str, context: TemplateContext, node: Node) -> Self {
        let template = self.load(template);
        let root = self
            .app
            .world_mut()
            .spawn((HtmlUi::new(template), context, node))
            .id();
        self.root = Some(root);
        self
    }

    /// Updates until every tracked asset is loaded (or failed), at least one
    /// new build happened, and nothing changed for a few frames.
    fn settle(&mut self) -> &mut Self {
        let mut stable = 0;
        let mut last_builds = usize::MAX;
        for _ in 0..3000 {
            self.app.update();
            let server = self.app.world().resource::<AssetServer>();
            let loaded = self.tracked.iter().all(|handle| {
                server.is_loaded_with_dependencies(handle.id())
                    || server.load_state(handle.id()).is_failed()
            });
            let builds = self.app.world().resource::<Builds>().0;
            if loaded && builds > self.builds_seen && builds == last_builds {
                stable += 1;
                if stable >= 5 {
                    self.builds_seen = builds;
                    return self;
                }
            } else {
                stable = 0;
            }
            last_builds = builds;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("UI never settled (assets loading or no rebuild); dump:\n{}", self.dump());
    }

    fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    fn root(&self) -> Entity {
        self.root.expect("spawned")
    }

    fn dump(&mut self) -> String {
        let world = self.app.world_mut();
        let mut out = String::new();
        dump_entity(world, self.root.expect("spawned"), 0, &mut out);
        out
    }

    fn assert_dump(&mut self, expected: &str) {
        let actual = self.dump();
        let expected = expected.trim_start_matches('\n');
        assert_eq!(
            actual.trim_end(),
            expected.trim_end(),
            "\n--- actual ---\n{actual}\n--- expected ---\n{expected}"
        );
    }
}

impl Drop for TestUi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn hex(color: Color) -> String {
    let [r, g, b, a] = color.to_srgba().to_u8_array();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

fn val(value: Val) -> String {
    match value {
        Val::Px(px) => format!("{px}"),
        Val::Auto => "auto".to_owned(),
        other => format!("{other:?}"),
    }
}

/// `t,r,b,l`, or `None` when all zero/auto.
fn rect(rect: UiRect) -> Option<String> {
    let sides = [rect.top, rect.right, rect.bottom, rect.left];
    sides
        .iter()
        .any(|side| !matches!(side, Val::Px(0.0) | Val::Auto))
        .then(|| sides.map(val).join(","))
}

fn style_label(font: &TextFont, color: &TextColor) -> String {
    let size = match font.font_size {
        FontSize::Px(px) => format!("{px}px"),
        other => format!("{other:?}"),
    };
    format!("{} {size} {}", face_label(&font.font), hex(color.0))
}

fn dump_entity(world: &mut World, entity: Entity, depth: usize, out: &mut String) {
    let entity_ref = world.entity(entity);
    let indent = "  ".repeat(depth);
    let mut line = match (depth, entity_ref.get::<HtmlElement>()) {
        (0, _) => "html-ui".to_owned(),
        (_, Some(element)) => {
            let mut label = element.tag.clone();
            if let Some(id) = &element.id {
                write!(label, "#{id}").unwrap();
            }
            for class in &element.classes {
                write!(label, ".{class}").unwrap();
            }
            label
        }
        (_, None) => "-".to_owned(),
    };
    if let Some(node) = entity_ref.get::<Node>() {
        for (name, value) in [
            ("border", node.border),
            ("padding", node.padding),
            ("margin", node.margin),
        ] {
            if let Some(value) = rect(value) {
                write!(line, " {name}={value}").unwrap();
            }
        }
        if let Val::Px(gap) = node.row_gap
            && gap != 0.0
        {
            write!(line, " gap={gap}").unwrap();
        }
    }
    if let Some(background) = entity_ref.get::<BackgroundColor>()
        && background.0 != Color::NONE
    {
        write!(line, " bg={}", hex(background.0)).unwrap();
    }
    if let Some(image) = entity_ref.get::<ImageNode>()
        && let NodeImageMode::Sliced(slicer) = &image.image_mode
    {
        let file = image
            .image
            .path()
            .map(|path| path.path().display().to_string())
            .unwrap_or_default();
        let (min, max) = (slicer.border.min_inset, slicer.border.max_inset);
        let mode = match slicer.sides_scale_mode {
            SliceScaleMode::Stretch => "stretch",
            SliceScaleMode::Tile { .. } => "tile",
        };
        write!(line, " slice={file} {},{},{},{} {mode}", min.y, max.x, max.y, min.x).unwrap();
    }
    writeln!(out, "{indent}{line}").unwrap();

    let children: Vec<Entity> = entity_ref
        .get::<Children>()
        .map(|children| children.to_vec())
        .unwrap_or_default();
    if let (Some(text), Some(font), Some(color)) = (
        entity_ref.get::<Text>(),
        entity_ref.get::<TextFont>(),
        entity_ref.get::<TextColor>(),
    ) {
        if !text.0.is_empty() {
            writeln!(out, "{indent}  {:?} {}", text.0, style_label(font, color)).unwrap();
        }
        for &child in &children {
            let child = world.entity(child);
            if let (Some(span), Some(font), Some(color)) = (
                child.get::<TextSpan>(),
                child.get::<TextFont>(),
                child.get::<TextColor>(),
            ) {
                writeln!(out, "{indent}  {:?} {}", span.0, style_label(font, color)).unwrap();
            }
        }
    }
    for child in children {
        if world.entity(child).contains::<TextSpan>() {
            continue;
        }
        dump_entity(world, child, depth + 1, out);
    }
}

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
  pre padding=8,8,8,8
    "a\n  b" serif 20px #ffffff
"#,
    );
}

/// CSS: specificity (id > class > type, compound counts), inheritance into
/// inline elements, font faces from weight/style, missing-face fallback,
/// generic family, em/rem/% sizes, unknown family keeps the inherited one.
#[test]
fn css_cascade_and_fonts() {
    let mut ui = TestUi::new(
        "cascade",
        &[
            (
                "page.html",
                r#"<p id="lead" class="note">Lead <b>bold <i>both</i></b></p>
<p class="note">Note <code>mono <b>bold?</b></code></p>
<p class="a b">Two classes</p>
<h2>Heading <span class="small">small</span></h2>"#,
            ),
            (
                "style.css",
                r#"
html { color: #ffffff; font-family: Spectral; font-size: 20px }
#lead { color: #00ff00 }
.note { color: #ff0000; font-size: 1.5em }
p { color: #0000ff }
b { font-weight: 700 }
i { font-style: italic }
code { font-family: Nonexistent, monospace; color: #888888 }
.a { color: #111111 }
.b { color: #222222 }
p.a { font-family: Nonexistent }
h2 { font-size: 2rem; font-weight: bold }
.small { font-size: 50% }
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
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
    "Two classes" serif 20px #222222
  h2
    "Heading " serif-bold 40px #ffffff
    "small" serif-bold 20px #ffffff
"#,
    );
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

/// Box model: `border-image` on the root (`html` rule), on a block (wrapper
/// node + inner `Text`) and on a container; px and % slices (% of the 32×24
/// image: top/bottom of the height, left/right of the width); tiling; borders,
/// padding, background, container `gap`.
#[test]
fn css_box_model() {
    let mut ui = TestUi::new(
        "box",
        &[
            (
                "page.html",
                r#"<p class="framed">Framed</p>
<div class="panel"><p>One</p><p>Two</p></div>"#,
            ),
            (
                "style.css",
                r#"
html {
  color: #ffffff; font-family: Spectral; font-size: 20px;
  border-image: url("frame.png") 4 fill stretch;
  border-width: 16px; padding: 10px 20px;
}
.framed {
  border-image-source: url("frame.png");
  border-image-slice: 25% fill;
  border-image-repeat: repeat;
  border-width: 6px 8px;
  background-color: #102030;
}
.panel {
  border-image: url("frame.png") 2 3 4 5 fill stretch;
  border-width: 12px; padding: 4px; gap: 7px; background-color: #333333;
}
"#,
            ),
        ],
    )
    .stylesheet("style.css")
    .spawn("page.html", TemplateContext::new(), Node::default());
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
