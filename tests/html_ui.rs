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

    /// A plain-HTML vector from `tests/vectors/<name>/` (`page.html` +
    /// `style.css`), spawned with an empty context.
    fn from_vector(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors").join(name);
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
        let (page, css) = (read("page.html"), read("style.css"));
        Self::new(name, &[("page.html", &page), ("style.css", &css)])
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), Node::default())
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

// ---------------------------------------------------------------------------
// Browser oracle
// ---------------------------------------------------------------------------

/// Every `tests/vectors/*/` with a `browser.json` (written by
/// `scripts/browser_oracle.py` from headless Chromium) is built here and
/// compared against the browser's computed styles: per non-whitespace
/// character (face, size, color) and per block/container element (padding,
/// border widths, border-image, background, gap). Deliberate differences are
/// listed in [`ALLOWED_DIFFERENCES`].
#[test]
fn browser_oracle() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors");
    let mut vectors: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.join("browser.json").exists().then(|| path.file_name()?.to_str().map(str::to_owned))?
        })
        .collect();
    vectors.sort();
    assert!(!vectors.is_empty(), "no oracle vectors found");

    let mut failures = Vec::new();
    for name in &vectors {
        let json = std::fs::read_to_string(root.join(name).join("browser.json")).unwrap();
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

// Deliberate, documented differences from browsers (skipped by
// `compare_with_browser`):
// - `pre`: browsers' `pre` has no padding under `* { all: unset }`; p23 gives
//   it a fixed 8px (see AGENTS.md limits).
// - root `background-color`: p23 leaves the `HtmlUi` node's background to the
//   app.
// - `border-style`: p23 ignores it (a `border-width` always applies); vectors
//   set `border-style: solid` so browsers compute the widths.
// - bullets: p23's `li` prefix `• ` is text on the `Text` root (not compared);
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
        problems.push(format!("text differs: p23 {:?} vs browser {:?}", text(&ours), text(&theirs)));
    } else if let Some(index) = (0..ours.len()).find(|&i| ours[i] != theirs[i]) {
        let context: String = ours[index.saturating_sub(8)..=index].iter().map(|c| c.0).collect();
        let (_, face, size, color) = &ours[index];
        let (_, b_face, b_size, b_color) = &theirs[index];
        problems.push(format!(
            "text style at …{context:?}: p23 {face} {size}px {color} vs browser {b_face} {b_size}px {b_color}"
        ));
    }

    // Boxes: the root (`html`) and every block/container, in document order.
    let mut our_elements = vec![root];
    collect_elements(world, root, &mut our_elements);
    let their_elements: Vec<&serde_json::Value> = oracle["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            let tag = e["tag"].as_str().unwrap();
            tag == "html" || BLOCKS.contains(&tag) || CONTAINERS.contains(&tag)
        })
        .collect();
    if our_elements.len() != their_elements.len() {
        problems.push(format!(
            "element count: p23 {} vs browser {}",
            our_elements.len(),
            their_elements.len()
        ));
        return problems;
    }
    for (entity, theirs) in our_elements.into_iter().zip(their_elements) {
        let tag = theirs["tag"].as_str().unwrap();
        let label = match theirs["classes"].as_array().unwrap().as_slice() {
            [] => tag.to_owned(),
            classes => format!(
                "{tag}.{}",
                classes.iter().map(|c| c.as_str().unwrap()).collect::<Vec<_>>().join(".")
            ),
        };
        let entity_ref = world.entity(entity);
        if let Some(element) = entity_ref.get::<HtmlElement>()
            && element.tag != tag
        {
            problems.push(format!("{label}: element order differs (p23 has {})", element.tag));
            continue;
        }
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
                problems.push(format!("{label}: {what} p23 {ours} vs browser {browser}"));
            }
        };

        // `pre` padding: allowed difference.
        if tag != "pre" {
            check("padding", format!("{:?}", sides(node.padding)), format!("{:?}", floats("padding")));
        }
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
            // `normal` = no CSS gap → p23 uses the root's gap (0 here).
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
