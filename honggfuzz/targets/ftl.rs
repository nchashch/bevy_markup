//! Same harnesses as `fuzz/` (libFuzzer), driven by honggfuzz instead.
//! Contract: never panics. Build+run with
//! `cargo +nightly hfuzz run <target> --run_time 120` (from the repo root).
fn main() {
    loop {
        honggfuzz::fuzz!(|data: &[u8]| {
            let text = String::from_utf8_lossy(data);
            let mut parts = text.split("\n---\n");
            let (Some(ftl), Some(id), Some(args), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return;
            };
            let _ = p23::fuzz::translate(ftl, id, args);
        });
    }
}
