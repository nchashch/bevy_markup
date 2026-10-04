//! Replays the local cargo-fuzz corpora (`fuzz/corpus/<target>/`, gitignored)
//! through the same `p23::fuzz` harnesses the fuzz targets call, so coverage
//! tools can measure what fuzzing reaches alongside the tests
//! (`scripts/coverage.sh`). Inputs are split exactly as in
//! `fuzz/fuzz_targets/*.rs`.
//!
//! `#[ignore]`d (the corpora hold thousands of inputs) and gated on the
//! `fuzzing` feature: `cargo test --features fuzzing --test fuzz_corpus --
//! --ignored`. Without corpora each test prints a skip note and passes.
#![cfg(feature = "fuzzing")]

use std::path::PathBuf;

/// Every input of `fuzz/corpus/<target>/`, lossily decoded; `None` without
/// a corpus. Over-long inputs are skipped, as the fuzz targets do.
fn corpus(target: &str) -> Option<Vec<String>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus").join(target);
    let entries = std::fs::read_dir(&dir).ok()?;
    let inputs: Vec<String> = entries
        .filter_map(|entry| std::fs::read(entry.ok()?.path()).ok())
        .filter(|bytes| bytes.len() <= 64 * 1024)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect();
    eprintln!("fuzz_corpus: {} inputs from {}", inputs.len(), dir.display());
    Some(inputs)
}

fn skipped(target: &str) {
    eprintln!("fuzz_corpus: skipped {target}, no fuzz/corpus/{target}/ (gitignored)");
}

/// Runs `replay` on a thread with a large stack. The fuzzers run optimized
/// builds on the 8 MB main thread; this debug build on the test harness's
/// 2 MB threads would overflow on the corpus's deeply nested FTL placeables,
/// which is bug_0015 (fluent-syntax's parser recursion has no depth limit),
/// tracked there — this replay measures coverage, not stack depth.
fn with_large_stack(replay: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(replay)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
#[ignore = "replays thousands of corpus inputs; run for coverage"]
fn replay_html_corpus() {
    let Some(inputs) = corpus("html") else { return skipped("html") };
    with_large_stack(move || {
        for text in inputs {
            let (source, context_json) = text.split_once("\n---\n").unwrap_or((&text, "{}"));
            let _ = p23::fuzz::render_html(source, context_json);
        }
    });
}

#[test]
#[ignore = "replays thousands of corpus inputs; run for coverage"]
fn replay_css_corpus() {
    let Some(inputs) = corpus("css") else { return skipped("css") };
    with_large_stack(move || {
        for css in inputs {
            let _ = p23::fuzz::cascade(&css);
        }
    });
}

#[test]
#[ignore = "replays thousands of corpus inputs; run for coverage"]
fn replay_ftl_corpus() {
    let Some(inputs) = corpus("ftl") else { return skipped("ftl") };
    with_large_stack(move || {
        for text in inputs {
            let mut parts = text.split("\n---\n");
            if let (Some(ftl), Some(id), Some(args), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            {
                let _ = p23::fuzz::translate(ftl, id, args);
            }
        }
    });
}
