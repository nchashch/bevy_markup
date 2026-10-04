//! Same harnesses as `fuzz/` (libFuzzer) and `honggfuzz/`, driven by
//! fuzzcheck instead. Contract: never panics. Run with `cargo +nightly
//! fuzzcheck --test <name> fuzz_<name> --stop-after-duration 60` from this
//! directory.
use p23 as _p23;

#[test]
fn fuzz_ftl() {
    let _ = fuzzcheck::fuzz_test(|data: &Vec<u8>| {
        if data.len() > 64 * 1024 {
            return;
        }
        let text = String::from_utf8_lossy(data);
        let mut parts = text.split("\n---\n");
        let (Some(ftl), Some(id), Some(args), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return;
        };
        let _ = p23::fuzz::translate(ftl, id, args);
    })
    .default_options()
    .launch();
}
