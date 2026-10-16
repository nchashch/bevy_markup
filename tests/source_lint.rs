//! Lints over the library's own source.

use std::fs;

/// No debug output in library code: `println!`, `eprintln!` and `dbg!`
/// write to every app's stdout/stderr. Debug prints shipped three times —
/// review_0004 H05 (`DBG count arm`, `G13 DBG …`), `e3646ae`'s `STATEDBG`,
/// and review_0005 I10 (`DECIDEDBG`, printed every frame without a default
/// stylesheet). Logging goes through `bevy::log` (`debug!`, `warn!`, …).
/// `src/fuzz.rs` (the fuzz harness, feature-gated) is exempt; so are
/// commented-out lines.
#[test]
fn library_code_has_no_debug_prints() {
    let mut found = Vec::new();
    for entry in fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).expect("src/") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|extension| extension != "rs")
            || path.file_name().is_some_and(|name| name == "fuzz.rs")
        {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a source file");
        for (number, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if ["println!(", "eprintln!(", "dbg!("]
                .iter()
                .any(|print| code.contains(print))
            {
                found.push(format!("{}:{}: {}", path.display(), number + 1, code));
            }
        }
    }
    assert!(
        found.is_empty(),
        "debug prints in library code:\n{}",
        found.join("\n")
    );
}

/// No file writes in library code (review_0005 I16): `build_html_ui`
/// appended a line per UI per build to `/tmp/bevy_markup_buildlog.txt` — an
/// unbounded file in every app (23 MB after one test run), a file open per
/// build, and a fixed path in a world-writable directory (another local
/// user can plant a symlink there). The crate writes no files; its tests
/// (the `#[cfg(test)] mod`s each file ends with) and `src/fuzz.rs` may.
#[test]
fn library_code_writes_no_files() {
    let mut found = Vec::new();
    for entry in fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).expect("src/") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|extension| extension != "rs")
            || path.file_name().is_some_and(|name| name == "fuzz.rs")
        {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a source file");
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            // The first test module ends the library code (a lone
            // `#[cfg(test)]` helper, like `nine_slice.rs`'s, doesn't).
            if line.starts_with("#[cfg(test)]")
                && lines
                    .get(number + 1)
                    .is_some_and(|next| next.starts_with("mod "))
            {
                break;
            }
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if [
                "OpenOptions",
                "File::create",
                "fs::write",
                "create_dir",
                "\"/tmp",
            ]
            .iter()
            .any(|write| code.contains(write))
            {
                found.push(format!("{}:{}: {}", path.display(), number + 1, code));
            }
        }
    }
    assert!(
        found.is_empty(),
        "file writes in library code:\n{}",
        found.join("\n")
    );
}
