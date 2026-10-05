//! Same harnesses as `fuzz/` (libFuzzer) and `honggfuzz/`, driven by
//! fuzzcheck instead. Contract: never panics. Run with `cargo +nightly
//! fuzzcheck --test <name> fuzz_<name> --stop-after-duration 60` from this
//! directory.
use bevy_markup as _bevy_markup;

#[test]
fn fuzz_css() {
    let _ = fuzzcheck::fuzz_test(|data: &Vec<u8>| {
        if data.len() > 64 * 1024 {
            return;
        }
        let css = String::from_utf8_lossy(data);
        let _ = bevy_markup::fuzz::cascade(&css);
    })
    .default_options()
    .launch();
}
