//! Fuzzes the Render stage: Tera compile + render (autoescaping, JSON
//! context) → `tl` parse → outline → entity decoding.
//!
//! Contract: never panics. Parse/render errors are fine.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Cap input: past this, exec time dominates without new coverage.
    if data.len() > 64 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let (source, context_json) = match text.split_once("\n---\n") {
        Some((source, context)) => (source, context),
        // No delimiter: render with an empty context.
        None => (text.as_ref(), "{}"),
    };
    let _ = bevy_markup::fuzz::render_html(source, context_json);
});
