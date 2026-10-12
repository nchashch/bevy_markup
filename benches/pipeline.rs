//! Pipeline benchmarks (ADR 0018): where a content change spends its
//! time. Criterion measures three things:
//!
//! - **render**: Tera rendering the synthetic document (the crate's
//!   `HtmlTemplate::render` is a pass-through over this for `.html` —
//!   render, then the Markdown branch skipped — so `tera` is measured
//!   directly; `HtmlTemplate` has no public constructor);
//! - **parse**: `HtmlDocument::parse` (the `tl` stage);
//! - **frame** workloads: one `app.update()` per iteration over a headless
//!   app (the `tests/common` layout composition, without schedule
//!   shuffling):
//!   - `idle` — the render is identical: skipped (0009's "nothing
//!     changing" column);
//!   - `changing` — one templated value and one templated color differ:
//!     the whole render/parse/cascade runs, the diff is one paragraph, and
//!     the color change keeps a transition entry advancing every frame;
//!   - `rebuilding` — every paragraph templated: the whole document
//!     re-collected;
//!   - `localized` — every paragraph is a `data-l10n-id` message whose
//!     arg changes per frame (the Fluent stage);
//!   - `markdown` — a `.md` template with Markdown `markdown: true` Fluent
//!     values (the pulldown-cmark stage, per frame).
//!
//! The markup and stylesheet are a kitchen sink (fonts via registered
//! families, a grid, a flex row, `var()` + `calc()`, a transition,
//! positioning, a `border-image` frame) so the cascade's mapping arms run.
//!
//! These are 0009's throwaway-benchmark method, formalized — the numbers
//! 0017 has to beat.
//!
//! Run: `cargo bench` (or `cargo bench --bench pipeline`). Criterion saves
//! per-benchmark baselines under `target/criterion/` (gitignored) and
//! flags regressions on the next run.

use bevy::asset::AssetMetaCheck;
use bevy::asset::uuid::Uuid;
use bevy::camera::{ComputedCameraValues, RenderTarget, RenderTargetInfo};
use bevy::image::{CompressedImageFormats, ImageLoader};
use bevy::prelude::*;
use bevy::text::DEFAULT_FONT_DATA;
use bevy_markup::html::{HtmlUi, TemplateContext};
use bevy_markup::prelude::*;
use bevy_markup::template::HtmlDocument;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::marker::PhantomData;
use std::sync::LazyLock;

/// The document sizes 0009's method used.
const SIZES: [usize; 2] = [120, 600];

/// Viewport for the headless layout (the test harness's size).
const VIEWPORT: UVec2 = UVec2::new(1280, 800);

/// The frame bench's stylesheet: a kitchen sink of the CSS subset's
/// mapping arms (fonts, grid, flex, `var()`/`calc()`, a transition,
/// positioning, a `border-image` frame over the `frame.png` fixture) — the
/// cascade exercises them on every re-collect.
const CSS: &str = r#"
html {
  color: #ffffff;
  font-family: Spectral;
  font-size: 16px;
  border-image: url("frame.png") 4 fill stretch;
  border-width: 4px;
  padding: 10px;
}
header { display: flex; flex-direction: row; gap: 8px; padding: var(--pad) }
header > .chip { background-color: #1a1a1a; flex-grow: 1 }
.grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 4px;
}
.grid > .cell { background-color: #101010; padding: 2px }
.counter {
  color: #000000;
  font-size: calc(var(--base) + 4px);
  transition: color 0.2s linear;
  position: relative;
  top: calc(var(--pad) * 2);
}
.counter.active { color: #ffffff }
p.row { margin: 2px 0 }
"#;

/// The markup prelude for the `frame` workloads: a kitchen-sink shell —
/// a flex header, a grid, and a counter whose class and color are
/// templated (per-frame change with a transition attached: the advance
/// system's active path).
const SHELL_PRELUDE: &str = r#"<header><span class="chip">a</span><span class="chip">b</span></header>
<div class="grid">{% for i in range(end=8) %}<span class="cell">cell {{ i }}</span>{% endfor %}</div>
<p class="counter {% if n % 2 == 1 %}active{% endif %}" style="color: rgb({{ n % 255 }}, 0, 0)">{{ n }}</p>
"#;

/// The markup for the `frame` workloads: the shell around N paragraphs.
/// `counter_only`: the paragraphs are static (the `changing` workload —
/// one templated value in the shell); otherwise every paragraph templates
/// the context (the `rebuilding` workload).
fn shell_template(elements: usize, counter_only: bool) -> String {
    let line = if counter_only {
        "item {{ i }} of the list"
    } else {
        "item {{ i }} of {{ n }}"
    };
    format!(
        r#"{prelude}{{% for i in range(end={elements}) %}}<p class="row">{line}</p>
{{% endfor %}}"#,
        prelude = SHELL_PRELUDE
    )
}

/// The `localized` workload's markup: every paragraph localized, args
/// changing per frame.
fn localized_template(elements: usize) -> String {
    format!(
        "{{% for i in range(end={elements}) %}}\
         <p class=\"row\" data-l10n-id=\"row\" data-l10n-args='{{\"i\": {{{{ i }}}}, \"n\": {{{{ n }}}}}}'>fallback</p>\n\
         {{% endfor %}}"
    )
}

/// The `markdown` workload's template: a `.md` document (the Markdown
/// loader by extension) whose Fluent values are Markdown too.
fn markdown_template(elements: usize) -> String {
    format!(
        "{{% for i in range(end={elements}) %}}\
         ## Section {{{{ i }}}}\n\
         A *markdown* paragraph with `code`, some **emphasis** and a data-l10n-id line:\n\
         <p class=\"row\" data-l10n-id=\"row\" data-l10n-args='{{\"i\": {{{{ i }}}}, \"n\": {{{{ n }}}}}}'>fallback</p>\n\n\
         {{% endfor %}}"
    )
}

/// The locale manifest + messages for the localized and markdown
/// workloads; `markdown` opts the values into Markdown conversion.
fn locale_files(markdown: bool) -> Vec<(String, String)> {
    let markdown_line = if markdown { "\n    markdown: true," } else { "" };
    let rows = (0..600)
        .map(|_| {
            format!(
                "row-n = Item {{ $i }} of **{{ $n }}** things.\nrow = Item {{ $i }} of the list.\n"
            )
        })
        .collect::<Vec<_>>()
        .concat();
    vec![
        (
            "locales/en-US/main.ftl.ron".into(),
            format!(
                "(\n    locale: \"en-US\",\n    resources: [\"ui.ftl\"],{markdown_line}\n)\n"
            ),
        ),
        ("locales/en-US/ui.ftl".into(), rows),
    ]
}

/// The context: the one value the workloads change.
fn context(n: i64) -> tera::Context {
    tera::Context::from_serialize(&serde_json::json!({ "n": n })).unwrap()
}

/// The rendered HTML for the parse benchmarks (Tera one-off).
fn rendered(source: &str) -> String {
    tera::Tera::one_off(source, &context(0), true).unwrap()
}

/// Fake font ids, served from Bevy's embedded font (the test harness's
/// trick): the cascade resolves families and weights against them, and
/// text measurement has real glyphs.
fn font(id: u128) -> Handle<Font> {
    Handle::Uuid(Uuid::from_u128(id), PhantomData)
}

static SPECTRAL: LazyLock<[Handle<Font>; 4]> = LazyLock::new(|| [font(1), font(2), font(3), font(4)]);

/// The frame bench's fixture frame image (the tests' 32×24 PNG), for the
/// `border-image`.
static FRAME_PNG: &[u8] = include_bytes!("../tests/fixtures/frame.png");

/// A headless app over a temp asset root with `files`, laid out against a
/// fixed-size camera — the `tests/common` layout composition, without
/// schedule shuffling (a bench wants a pinned order) or the build
/// counters. Returns the app and the `HtmlUi` root.
fn bench_app(files: &[(&str, &str)]) -> (App, Entity) {
    bench_app_loading(files, "page.html")
}

/// [`bench_app`](Self), loading `template` as the UI's template (the
/// Markdown workload's is a `.md` file).
fn bench_app_loading(files: &[(&str, &str)], template: &str) -> (App, Entity) {
    let template = template.to_owned();
    let dir = std::env::temp_dir().join(format!(
        "bevy_markup-bench-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
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
        BevyMarkupPlugin,
        // The layout harness's composition: `UiPlugin` lays roots out
        // against their camera's hand-set target size; its picking and
        // focus systems need the input, window, picking and texture-atlas
        // resources.
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
    // bevy_render normally registers the image loader.
    app.register_asset_loader(ImageLoader::new(CompressedImageFormats::empty()));
    // The CSS's `font-family: Spectral` resolves to the fake faces, served
    // from the embedded font (real glyphs for measurement).
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    for handle in SPECTRAL.iter() {
        fonts
            .insert(handle.id(), Font::from_bytes(DEFAULT_FONT_DATA.to_vec()))
            .expect("fresh id");
    }
    drop(fonts);
    app.world_mut().resource_mut::<FontFamilies>().insert(
        "Spectral",
        FontFaces::new(SPECTRAL[0].clone())
            .with_bold(SPECTRAL[1].clone())
            .with_italic(SPECTRAL[2].clone())
            .with_bold_italic(SPECTRAL[3].clone()),
    );

    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: ComputedCameraValues {
                target_info: Some(RenderTargetInfo {
                    physical_size: VIEWPORT,
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
        RenderTarget::None { size: VIEWPORT },
        bevy::ui::IsDefaultUiCamera,
    ));

    // Spawn the UI; run to a steady state (the assets load and the first
    // build lands; a few frames past it for the layout to settle).
    let server = app.world().resource::<AssetServer>();
    let template = server.load(&template);
    let sheet = server.load("style.css");
    let locale = server.load("locales/en-US/main.ftl.ron");
    let root = app
        .world_mut()
        .spawn((
            HtmlUi::new(template),
            HtmlStylesheet(sheet),
            TemplateContext::new()
                .with("n", &0)
                .with("--pad", "8px")
                .with("--base", "12px"),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
    app.world_mut().resource_mut::<ActiveLocale>().set(locale);
    for _ in 0..60 {
        app.update();
    }
    (app, root)
}

/// The context mutation the frame workloads do per iteration.
fn set_n(app: &mut App, root: Entity, n: i64) {
    app.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &n);
}

fn render_and_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");
    for size in SIZES {
        let source = shell_template(size, false);
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| black_box(tera::Tera::one_off(&source, &context(0), true).unwrap()))
        });
    }
    group.finish();

    let mut group = c.benchmark_group("parse");
    for size in SIZES {
        let html = rendered(&shell_template(size, false));
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            // `parse` takes the string; the clone is excluded with a batch.
            b.iter_batched(
                || html.clone(),
                |html| black_box(HtmlDocument::parse(html)),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame");

    // The kitchen-sink workloads at both sizes; the specialized ones at
    // 600 (the largest document, where per-frame cost matters).
    for size in SIZES {
        let page = shell_template(size, false);
        let files = [
            ("page.html", page.as_str()),
            ("style.css", CSS),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            ("locales/en-US/ui.ftl", "row = Item {{ $i }} of the list.\n"),
        ];

        // Idle: nothing changes — the render is identical, everything
        // skipped (0009's "nothing changing" column).
        let (mut app, _root) = bench_app(&files);
        group.throughput(Throughput::Elements(size as u64));
        group.bench_function(BenchmarkId::new("idle", size), |b| {
            b.iter(|| black_box(app.update()))
        });

        // Changing: one value and one templated color differ — full
        // render/parse/cascade, a tiny diff, a transition entry advancing
        // every frame (0009's method: "one templated value changing per
        // frame", plus the advance system's active path).
        let changing = shell_template(size, true);
        let files = files_with(&files, "page.html", &changing);
        let (mut app, root) = bench_app(&files);
        let mut n = 0_i64;
        group.bench_function(BenchmarkId::new("changing", size), |b| {
            b.iter(|| {
                n += 1;
                set_n(&mut app, root, n);
                black_box(app.update())
            })
        });

        // Rebuilding: every paragraph templated — a whole-document diff.
        let rebuilding = shell_template(size, false);
        let files = files_with(&files, "page.html", &rebuilding);
        let (mut app, root) = bench_app(&files);
        let mut n = 0_i64;
        group.bench_function(BenchmarkId::new("rebuilding", size), |b| {
            b.iter(|| {
                n += 1;
                set_n(&mut app, root, n);
                black_box(app.update())
            })
        });
    }

    // Localized: every paragraph a data-l10n-id message whose arg changes
    // per frame — the Fluent stage over the whole document.
    let localized = localized_template(600);
    let mut files = vec![
        ("page.html".to_owned(), localized),
        ("style.css".to_owned(), CSS.to_owned()),
    ];
    files.extend(locale_files(false));
    let files = borrowed(&files);
    let (mut app, root) = bench_app(&files);
    let mut n = 0_i64;
    group.bench_function(BenchmarkId::new("localized", 600), |b| {
        b.iter(|| {
            n += 1;
            set_n(&mut app, root, n);
            black_box(app.update())
        })
    });

    // Markdown: a `.md` template (converted at render) whose Fluent
    // values are Markdown too (converted at localize, `markdown: true`).
    let markdown = markdown_template(600);
    let mut files = vec![
        ("page.md".to_owned(), markdown),
        ("style.css".to_owned(), CSS.to_owned()),
    ];
    files.extend(locale_files(true));
    let files = borrowed(&files);
    let (mut app, root) = bench_app_loading(&files, "page.md");
    let mut n = 0_i64;
    group.bench_function(BenchmarkId::new("markdown", 600), |b| {
        b.iter(|| {
            n += 1;
            set_n(&mut app, root, n);
            black_box(app.update())
        })
    });

    group.finish();
}

/// Owned (String, String) pairs as borrowed (path, contents).
fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(path, contents)| (path.as_str(), contents.as_str()))
        .collect()
}

/// The file list with `contents` as `path`'s file.
fn files_with<'a>(
    files: &'a [(&'a str, &'a str)],
    path: &'a str,
    contents: &'a str,
) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .filter(|(existing, _)| *existing != path)
        .chain(std::iter::once(&(path, contents)))
        .copied()
        .collect()
}

criterion_group!(benches, render_and_parse, frames);
criterion_main!(benches);
