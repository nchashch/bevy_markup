//! Same harnesses as `fuzz/` (libFuzzer) and `honggfuzz/`, driven by
//! fuzzcheck instead. Contract: never panics. Run with `cargo +nightly
//! fuzzcheck --test <name> fuzz_<name> --stop-after-duration 60` from this
//! directory.
use p23 as _p23;

#[test]
fn fuzz_html() {
    let _ = fuzzcheck::fuzz_test(|data: &Vec<u8>| {
        if data.len() > 64 * 1024 {
            return;
        }
        let text = String::from_utf8_lossy(data);
        let (source, context_json) = match text.split_once("\n---\n") {
            Some((source, context)) => (source, context),
            // No delimiter: render with an empty context.
            None => (text.as_ref(), "{}"),
        };
        let _ = p23::fuzz::render_html(source, context_json);
    })
    .default_options()
    .launch();
}
