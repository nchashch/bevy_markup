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
//!   shuffling) — `idle` (the render is identical: skipped), `changing`
//!   (one templated value differs: the whole render/parse/cascade runs,
//!   the diff is one paragraph) and `rebuilding` (every paragraph
//!   templated: the whole document re-collected). These are 0009's
//!   throwaway-benchmark method, formalized — the numbers 0017 has to
//!   beat.
//!
//! Run: `cargo bench` (or `cargo bench --bench pipeline`). Criterion
//! saves per-benchmark baselines under `target/criterion/` (gitignored)
//! and flags regressions on the next run.

use bevy::asset::AssetMetaCheck;
use bevy::camera::{ComputedCameraValues, RenderTarget, RenderTargetInfo};
use bevy::image::{CompressedImageFormats, ImageLoader};
use bevy::prelude::*;
use bevy_markup::html::{HtmlUi, TemplateContext};
use bevy_markup::prelude::*;
use bevy_markup::template::HtmlDocument;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

/// The document sizes 0009's method used.
const SIZES: [usize; 2] = [120, 600];

/// Viewport for the headless layout (the test harness's size).
const VIEWPORT: UVec2 = UVec2::new(1280, 800);

/// The template's paragraph, with every paragraph templating the context's
/// `n` (the `rebuilding` workload): a whole-document diff per frame.
fn rebuilding_template(elements: usize) -> String {
    format!(
        "<h1>bench</h1>\n\
         {{% for i in range(end={elements}) %}}\
         <p class=\"row-{{{{ i }}}}\">item {{{{ i }}}} of {{{{ n }}}}</p>\n\
         {{% endfor %}}"
    )
}

/// The template with exactly one templated value (the `changing`
/// workload): the output differs per frame in one paragraph.
fn changing_template(elements: usize) -> String {
    format!(
        "<h1>bench</h1>\n\
         {{% for i in range(end={elements}) %}}\
         <p class=\"row-{{{{ i }}}}\">item {{{{ i }}}} of the list</p>\n\
         {{% endfor %}}\
         <p id=\"counter\">{{{{ n }}}}</p>"
    )
}

/// The context: the one value the workloads change.
fn context(n: i64) -> tera::Context {
    tera::Context::from_serialize(&serde_json::json!({ "n": n })).unwrap()
}

/// The rendered HTML for the parse benchmarks (Tera one-off).
fn rendered(source: &str) -> String {
    tera::Tera::one_off(source, &context(0), true).unwrap()
}

/// A headless app over a temp asset root with `files`, laid out against a
/// fixed-size camera — the `tests/common` layout composition, without
/// schedule shuffling (a bench wants a pinned order) or the build
/// counters.
fn bench_app(files: &[(&str, &str)]) -> (App, Entity) {
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
    let template = server.load("page.html");
    let sheet = server.load("style.css");
    let root = app
        .world_mut()
        .spawn((
            HtmlUi::new(template),
            HtmlStylesheet(sheet),
            TemplateContext::new().with("n", &0),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
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
        let source = rebuilding_template(size);
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| black_box(tera::Tera::one_off(&source, &context(0), true).unwrap()))
        });
    }
    group.finish();

    let mut group = c.benchmark_group("parse");
    for size in SIZES {
        let html = rendered(&rebuilding_template(size));
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
    let css = "html { color: #ffffff; font-size: 16px } p { color: #eeeeee }";

    let mut group = c.benchmark_group("frame");
    for size in SIZES {
        let rebuilding = rebuilding_template(size);
        let changing = changing_template(size);
        // `changing.html` is listed only so the changing workload's app
        // shows the same file inventory; `page.html` is what loads.
        let files = [
            ("page.html", rebuilding.as_str()),
            ("changing.html", changing.as_str()),
            ("style.css", css),
        ];

        // Idle: nothing changes — the render is identical, everything
        // skipped (0009's "nothing changing" column).
        let (mut app, _root) = bench_app(&files_with(&files, "page.html", &rebuilding));
        group.throughput(Throughput::Elements(size as u64));
        group.bench_function(BenchmarkId::new("idle", size), |b| {
            b.iter(|| black_box(app.update()))
        });

        // Changing: one value differs — full render/parse/cascade, a tiny
        // diff (0009's method: "one templated value changing per frame").
        let (mut app, root) = bench_app(&files_with(&files, "page.html", &changing));
        let mut n = 0_i64;
        group.bench_function(BenchmarkId::new("changing", size), |b| {
            b.iter(|| {
                n += 1;
                set_n(&mut app, root, n);
                black_box(app.update())
            })
        });

        // Rebuilding: every paragraph templated — a whole-document diff.
        let (mut app, root) = bench_app(&files_with(&files, "page.html", &rebuilding));
        let mut n = 0_i64;
        group.bench_function(BenchmarkId::new("rebuilding", size), |b| {
            b.iter(|| {
                n += 1;
                set_n(&mut app, root, n);
                black_box(app.update())
            })
        });
    }
    group.finish();
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
