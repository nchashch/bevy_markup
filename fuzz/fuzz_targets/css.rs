//! Fuzzes CSS: lightningcss parse → cascade matching → root box, including
//! `border-image` slice arithmetic against a fake 32×24 image.
//!
//! Contract: never panics. Parse errors are fine.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let css = String::from_utf8_lossy(data);
    let _ = p23::fuzz::cascade(&css);
});
