//! Pipeline benchmarks with Divan (ADR 0019): the same workloads,
//! documents and headless app composition as the criterion harness
//! (`benches/pipeline.rs`), via `benches/shared.rs` — so the two
//! harnesses' numbers corroborate each other. What Divan adds is
//! **allocation counting** (counts and bytes per benchmark, via its
//! global allocator): the update model's whole thesis (0009, 0017) is
//! about not redoing allocating work, and allocation counts are
//! deterministic where wall time is noisy.
//!
//! Run: `cargo bench --bench pipeline-divan` (or every target with
//! `cargo bench`). `max_time` bounds each benchmark the way the
//! criterion run's `--measurement-time` does.

use bevy_markup::template::HtmlDocument;
use divan::{black_box, black_box_drop, AllocProfiler, Bencher};

mod shared;

use shared::{bench_app, bench_app_loading, frame_files, set_n, specialized_files, SIZES,
    template_path};

/// Divan's allocator profiler: with this as the global allocator, every
/// benchmark reports allocation counts and bytes alongside time.
#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

/// Tera rendering the synthetic document (the crate's `.html` render is a
/// pass-through over this; `HtmlTemplate` has no public constructor).
#[divan::bench(args = SIZES, max_time = 2)]
fn render(bencher: Bencher, size: usize) {
    let source = shared::workload_page(size, "rebuilding");
    bencher.bench_local(|| {
        black_box(tera::Tera::one_off(&source, &shared::context(0), true).unwrap())
    });
}

/// The `tl` parse stage. `parse` takes the string; the clone is setup
/// outside the measured closure, and the result is dropped black-boxed so
/// the parse's allocations are counted but not optimized away.
#[divan::bench(args = SIZES, max_time = 2)]
fn parse(bencher: Bencher, size: usize) {
    let html = shared::rendered(&shared::workload_page(size, "rebuilding"));
    bencher.bench_local(|| {
        let document = HtmlDocument::parse(html.clone());
        black_box_drop(document.unwrap());
    });
}

/// Idle: nothing changes — the render is identical, everything skipped.
#[divan::bench(args = SIZES, max_time = 2)]
fn idle(bencher: Bencher, size: usize) {
    let files = frame_files(size, "idle");
    let (mut app, _root) = bench_app(&shared::borrowed(&files));
    bencher.bench_local(|| black_box(app.update()));
}

/// Changing: one value and one templated color differ — full
/// render/parse/cascade, a tiny diff, a transition entry advancing every
/// frame.
#[divan::bench(args = SIZES, max_time = 2)]
fn changing(bencher: Bencher, size: usize) {
    let files = frame_files(size, "changing");
    let (mut app, root) = bench_app(&shared::borrowed(&files));
    let mut n = 0_i64;
    bencher.bench_local(|| {
        n += 1;
        set_n(&mut app, root, n);
        black_box(app.update());
    });
}

/// Rebuilding: every paragraph templated — a whole-document diff.
#[divan::bench(args = SIZES, max_time = 2)]
fn rebuilding(bencher: Bencher, size: usize) {
    let files = frame_files(size, "rebuilding");
    let (mut app, root) = bench_app(&shared::borrowed(&files));
    let mut n = 0_i64;
    bencher.bench_local(|| {
        n += 1;
        set_n(&mut app, root, n);
        black_box(app.update());
    });
}

/// Localized: every paragraph a `data-l10n-id` message whose arg changes
/// per frame (the Fluent stage, 600 paragraphs).
#[divan::bench(max_time = 2)]
fn localized(bencher: Bencher) {
    let files = specialized_files("localized");
    let (mut app, root) = bench_app(&shared::borrowed(&files));
    let mut n = 0_i64;
    bencher.bench_local(|| {
        n += 1;
        set_n(&mut app, root, n);
        black_box(app.update());
    });
}

/// Markdown: a `.md` template whose Fluent values are Markdown too (the
/// pulldown-cmark stage, per frame).
#[divan::bench(max_time = 2)]
fn markdown(bencher: Bencher) {
    let files = specialized_files("markdown");
    let (mut app, root) = bench_app_loading(&shared::borrowed(&files), template_path("markdown"));
    let mut n = 0_i64;
    bencher.bench_local(|| {
        n += 1;
        set_n(&mut app, root, n);
        black_box(app.update());
    });
}

