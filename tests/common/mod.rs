//! Shared headless test harness: `TestUi` builds a minimal app in a temp
//! asset root, settles it, and dumps the `HtmlUi` subtree as text.

use std::fmt::Write as _;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::asset::uuid::Uuid;
use bevy::asset::{AssetMetaCheck, UntypedHandle};
use bevy::camera::{ComputedCameraValues, RenderTarget, RenderTargetInfo};
use bevy::image::{CompressedImageFormats, ImageLoader};
use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};
use p23::prelude::*;

pub const FRAME_PNG: &[u8] = include_bytes!("../fixtures/frame.png"); // 32×24

/// Fake font handles: identity is all the mapping needs (no font files).
pub const fn font(id: u128) -> Handle<Font> {
    Handle::Uuid(Uuid::from_u128(id), PhantomData)
}
pub const SERIF: [Handle<Font>; 4] = [font(1), font(2), font(3), font(4)];
pub const MONO: Handle<Font> = font(5);

pub fn face_label(source: &FontSource) -> String {
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
pub struct Builds(pub usize);

/// `tests/vectors/`.
#[allow(dead_code)] // not every test binary exercises every helper
pub fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors")
}

/// Names of the vectors holding `file`, sorted.
#[allow(dead_code)] // not every test binary exercises every helper
pub fn vectors_with(file: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(vectors_dir())
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.join(file).exists().then(|| path.file_name()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    names
}

pub struct TestUi {
    app: App,
    dir: PathBuf,
    root: Option<Entity>,
    /// Assets that must be loaded before the UI counts as settled.
    tracked: Vec<UntypedHandle>,
    builds_seen: usize,
}

impl TestUi {
    /// A headless app whose asset root holds `files` plus `frame.png`.
    pub fn new(name: &str, files: &[(&str, &str)]) -> Self {
        Self::build(name, files, None)
    }

    /// Like [`new`](Self::new), plus Bevy UI layout and text measurement
    /// against a `viewport`-sized camera — still no window or renderer. Text
    /// uses Bevy's embedded default font (FiraMono, printable ASCII only): no
    /// fake font families are registered, since layout needs real glyphs.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn with_layout(name: &str, files: &[(&str, &str)], viewport: UVec2) -> Self {
        Self::build(name, files, Some(viewport))
    }

    fn build(name: &str, files: &[(&str, &str)], viewport: Option<UVec2>) -> Self {
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

        match viewport {
            Some(size) => {
                // `UiPlugin` lays roots out against their camera's target size,
                // which bevy_render's `camera_system` would compute; set it by
                // hand on a size-only target. Its picking and focus systems need
                // the input, window, picking and texture-atlas resources.
                app.add_plugins((
                    bevy::input::InputPlugin,
                    bevy::window::WindowPlugin {
                        primary_window: None,
                        exit_condition: bevy::window::ExitCondition::DontExit,
                        ..default()
                    },
                    bevy::text::TextPlugin::default(),
                    bevy::ui::UiPlugin::default(),
                    bevy::picking::DefaultPickingPlugins,
                    bevy::image::TextureAtlasPlugin,
                ));
                app.world_mut().spawn((
                    Camera2d,
                    Camera {
                        computed: ComputedCameraValues {
                            target_info: Some(RenderTargetInfo {
                                physical_size: size,
                                scale_factor: 1.0,
                            }),
                            ..default()
                        },
                        ..default()
                    },
                    RenderTarget::None { size },
                    bevy::ui::IsDefaultUiCamera,
                ));
            }
            None => {
                app.world_mut()
                    .resource_mut::<FontFamilies>()
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
            }
        }

        Self {
            app,
            dir,
            root: None,
            tracked: Vec::new(),
            builds_seen: 0,
        }
    }

    /// A layout vector from `tests/vectors/<name>/` (`page.html` +
    /// `style.css`, see [`with_layout`](Self::with_layout)) under a
    /// full-width column root, the way p23 apps set up their `HtmlUi` node.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn from_layout_vector(name: &str, viewport: UVec2) -> Self {
        let dir = vectors_dir().join(name);
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
        let (page, css) = (read("page.html"), read("style.css"));
        let root = Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        };
        Self::with_layout(name, &[("page.html", &page), ("style.css", &css)], viewport)
            .stylesheet("style.css")
            .spawn("page.html", TemplateContext::new(), root)
    }

    /// A plain-HTML vector from `tests/vectors/<name>/`: `page.html` +
    /// `style.css`, spawned with an empty context, and localized with the
    /// vector's `messages.ftl` (as the en-US bundle) if it has one.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn from_vector(name: &str) -> Self {
        Self::vector_page(name, "page.html", true)
    }

    /// `page` from vector `name` with its `style.css`; `localized` makes the
    /// vector's `messages.ftl` (if any) the active en-US bundle.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn vector_page(name: &str, page: &str, localized: bool) -> Self {
        const BUNDLE: &str = "locales/en-US/main.ftl.ron";
        let dir = vectors_dir().join(name);
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
        let (html, css) = (read(page), read("style.css"));
        let messages = dir.join("messages.ftl");
        let messages = (localized && messages.exists()).then(|| read("messages.ftl"));
        let mut files = vec![(page, html.as_str()), ("style.css", css.as_str())];
        if let Some(messages) = &messages {
            files.push((BUNDLE, r#"(locale: "en-US", resources: ["messages.ftl"])"#));
            files.push(("locales/en-US/messages.ftl", messages));
        }
        let ui = Self::new(name, &files).stylesheet("style.css");
        let ui = if messages.is_some() { ui.locale(BUNDLE) } else { ui };
        ui.spawn(page, TemplateContext::new(), Node::default())
    }

    pub fn load<A: Asset>(&mut self, path: &str) -> Handle<A> {
        let handle: Handle<A> = self.app.world().resource::<AssetServer>().load(path.to_owned());
        self.tracked.push(handle.clone().untyped());
        handle
    }

    pub fn stylesheet(mut self, path: &str) -> Self {
        let sheet = self.load(path);
        self.app.insert_resource(DefaultStylesheet::new(sheet));
        self
    }

    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn locale(mut self, path: &str) -> Self {
        let bundle = self.load(path);
        self.app.insert_resource(ActiveLocale::new(bundle));
        self
    }

    pub fn spawn(mut self, template: &str, context: TemplateContext, node: Node) -> Self {
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
    pub fn settle(&mut self) -> &mut Self {
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

    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn root(&self) -> Entity {
        self.root.expect("spawned")
    }

    pub fn dump(&mut self) -> String {
        let world = self.app.world_mut();
        let mut out = String::new();
        dump_entity(world, self.root.expect("spawned"), 0, &mut out);
        out
    }

    /// The laid-out `HtmlUi` subtree (needs [`with_layout`](Self::with_layout)):
    /// one line per node, `label x,y wxh` in logical px from the viewport's
    /// top-left (border boxes; `TextSpan`s are part of their `Text` node).
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn layout_dump(&mut self) -> String {
        fn walk(world: &World, entity: Entity, depth: usize, out: &mut String) {
            let label = match (depth, world.entity(entity).get::<HtmlElement>()) {
                (0, _) => "html-ui".to_owned(),
                (_, Some(element)) => element_label(element),
                (_, None) => "-".to_owned(),
            };
            let rect = node_rect(world, entity).expect("laid out (use TestUi::with_layout)");
            writeln!(
                out,
                "{}{label} {},{} {}x{}",
                "  ".repeat(depth),
                rect.min.x,
                rect.min.y,
                rect.width(),
                rect.height()
            )
            .unwrap();
            for &child in world.entity(entity).get::<Children>().into_iter().flatten() {
                if !world.entity(child).contains::<TextSpan>() {
                    walk(world, child, depth + 1, out);
                }
            }
        }
        let mut out = String::new();
        walk(self.app.world(), self.root(), 0, &mut out);
        out
    }

    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn assert_dump(&mut self, expected: &str) {
        let actual = self.dump();
        let expected = expected.trim_start_matches('\n');
        assert_eq!(
            actual.trim_end(),
            expected.trim_end(),
            "\n--- actual ---\n{actual}\n--- expected ---\n{expected}"
        );
    }

    /// Adds `contents` at `path` in the asset root — binary-safe, for files
    /// [`new`](Self::new) can't take as `&str` (images). Call before anything
    /// loads `path`.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn with_file(self, path: &str, contents: &[u8]) -> Self {
        let path = self.dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
        self
    }

    /// [`settle`](Self::settle) for a change that legitimately rebuilds
    /// nothing (e.g. swapping `DefaultStylesheet` under an entity with its
    /// own `HtmlStylesheet`): updates until every tracked asset is loaded
    /// (or failed) and the build count stayed put for a few frames. A
    /// rebuild is allowed but not required; either way it counts as seen.
    #[allow(dead_code)] // not every test binary exercises every helper
    pub fn settle_quiet(&mut self) -> &mut Self {
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
            stable = if loaded && builds == last_builds { stable + 1 } else { 0 };
            if stable >= 5 {
                self.builds_seen = builds;
                return self;
            }
            last_builds = builds;
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("UI never settled (assets loading or builds churning); dump:\n{}", self.dump());
    }
}

impl Drop for TestUi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn hex(color: Color) -> String {
    let [r, g, b, a] = color.to_srgba().to_u8_array();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

pub fn val(value: Val) -> String {
    match value {
        Val::Px(px) => format!("{px}"),
        Val::Auto => "auto".to_owned(),
        other => format!("{other:?}"),
    }
}

/// `t,r,b,l`, or `None` when all zero/auto.
pub fn rect(rect: UiRect) -> Option<String> {
    let sides = [rect.top, rect.right, rect.bottom, rect.left];
    sides
        .iter()
        .any(|side| !matches!(side, Val::Px(0.0) | Val::Auto))
        .then(|| sides.map(val).join(","))
}

pub fn style_label(font: &TextFont, color: &TextColor) -> String {
    let size = match font.font_size {
        FontSize::Px(px) => format!("{px}px"),
        other => format!("{other:?}"),
    };
    format!("{} {size} {}", face_label(&font.font), hex(color.0))
}

/// `tag#id.class…`.
pub fn element_label(element: &HtmlElement) -> String {
    let mut label = element.tag.clone();
    if let Some(id) = &element.id {
        write!(label, "#{id}").unwrap();
    }
    for class in &element.classes {
        write!(label, ".{class}").unwrap();
    }
    label
}

/// `entity`'s laid-out border box in logical px from the viewport's top-left
/// (`None` before layout ran). The harness camera has scale factor 1.
#[allow(dead_code)] // not every test binary exercises every helper
pub fn node_rect(world: &World, entity: Entity) -> Option<Rect> {
    let entity_ref = world.entity(entity);
    let size = entity_ref.get::<ComputedNode>()?.size;
    let center = entity_ref.get::<UiGlobalTransform>()?.translation;
    Some(Rect::from_center_size(center, size))
}

pub fn dump_entity(world: &mut World, entity: Entity, depth: usize, out: &mut String) {
    let entity_ref = world.entity(entity);
    let indent = "  ".repeat(depth);
    let mut line = match (depth, entity_ref.get::<HtmlElement>()) {
        (0, _) => "html-ui".to_owned(),
        (_, Some(element)) => element_label(element),
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
