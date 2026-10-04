//! Same harnesses as `fuzz/` (libFuzzer), driven by honggfuzz instead.
//! Contract: never panics. Build+run with
//! `cargo +nightly hfuzz run <target> --run_time 120` (from the repo root).
fn main() {
    loop {
        honggfuzz::fuzz!(|data: &[u8]| {
            let text = String::from_utf8_lossy(data);
            let (source, context_json) = match text.split_once("\n---\n") {
                Some((source, context)) => (source, context),
                // No delimiter: render with an empty context.
                None => (text.as_ref(), "{}"),
            };
            let _ = p23::fuzz::render_html(source, context_json);
        });
    }
}
