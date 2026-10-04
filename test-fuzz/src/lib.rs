//! The same harnesses as `fuzz/` (libFuzzer), `honggfuzz/` and
//! `fuzzcheck/`, as test-fuzz targets. test-fuzz seeds its corpus from
//! ordinary `cargo test` runs, then drives them with AFLplus:
//!
//! ```sh
//! cargo test-fuzz gen fuzz_html     # from this directory (also: css, ftl)
//! cargo test-fuzz run fuzz_html -- -V 60
//! ```
//!
//! Contract: never panics. (Needs `cargo install cargo-test-fuzz cargo-afl`
//! and a nightly toolchain for AFL instrumentation.)
#[cfg(test)]
mod tests {
    // test-fuzz writes the deserialized argument through the target; keep
    // the byte-level interface of the other drivers.
    #[test_fuzz::test_fuzz]
    fn fuzz_html(x: Vec<u8>) {
        if x.len() > 64 * 1024 {
            return;
        }
        let text = String::from_utf8_lossy(&x);
        let (source, context_json) = match text.split_once("\n---\n") {
            Some((source, context)) => (source, context),
            // No delimiter: render with an empty context.
            None => (text.as_ref(), "{}"),
        };
        let _ = p23::fuzz::render_html(source, context_json);
    }

    #[test_fuzz::test_fuzz]
    fn fuzz_css(x: Vec<u8>) {
        if x.len() > 64 * 1024 {
            return;
        }
        let css = String::from_utf8_lossy(&x);
        let _ = p23::fuzz::cascade(&css);
    }

    #[test_fuzz::test_fuzz]
    fn fuzz_ftl(x: Vec<u8>) {
        if x.len() > 64 * 1024 {
            return;
        }
        let text = String::from_utf8_lossy(&x);
        let mut parts = text.split("\n---\n");
        let (Some(ftl), Some(id), Some(args), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return;
        };
        let _ = p23::fuzz::translate(ftl, id, args);
    }
}
