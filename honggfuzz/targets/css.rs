//! Same harnesses as `fuzz/` (libFuzzer), driven by honggfuzz instead.
//! Contract: never panics. Build+run with
//! `cargo +nightly hfuzz run <target> --run_time 120` (from the repo root).
fn main() {
    loop {
        honggfuzz::fuzz!(|data: &[u8]| {
            let css = String::from_utf8_lossy(data);
            let _ = bevy_markup::fuzz::cascade(&css);
        });
    }
}
