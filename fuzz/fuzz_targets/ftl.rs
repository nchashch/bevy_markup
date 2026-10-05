//! Fuzzes Fluent: FTL parse → bundle → `translate` with arbitrary message
//! ids and `data-l10n-args` JSON (number/string/bool coercion, HTML escaping
//! of string args, bidi-isolate stripping).
//!
//! Contract: never panics. Missing messages and bad args are fine.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
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
    let _ = bevy_markup::fuzz::translate(ftl, id, args);
});
