#!/usr/bin/env python3
"""Line coverage of p23's library code (src/) per testing layer, and merged.

Each layer runs alone under source-based coverage (cargo-llvm-cov) and is
exported as lcov. The report shows, per layer, the src/ lines it covers and
the lines *only* it covers (what dropping that layer would lose), then the
merged total and per-file coverage with the uncovered line ranges.

Usage: scripts/coverage.py [--html] [--layer NAME ...]
  --html     also write a merged HTML report (re-runs all layers
             cumulatively) to target/coverage/html/index.html
  --layer    only these layers (names from LAYERS below)

Outputs in target/coverage/: <layer>.lcov per layer, merged.lcov,
uncovered.txt. Needs `cargo install cargo-llvm-cov` and
`rustup component add llvm-tools-preview`.

Not covered here: golden images (they need a GPU adapter and exercise
Bevy's rendering, not p23's logic) and doc tests. src/fuzz.rs (test-only
glue behind the `fuzzing` feature) is excluded from the denominator.
"""

import argparse
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target" / "coverage"
IGNORE = r"(^|/)(tests|vendor|fuzz|examples|honggfuzz|fuzzcheck|test-fuzz)/|src/fuzz\.rs$|/\.cargo/|/rustc/"

# Layer name → cargo test arguments (`--` separates test-binary arguments).
LAYERS = {
    "unit": ["--lib"],
    "vectors+oracles": ["--test", "html_ui"],
    "properties": ["--test", "properties", "--test", "layout_properties", "--test", "quickcheck"],
    "arbtest": ["--test", "arbtest"],
    "stateful": ["--test", "stateful"],
    "content-lint": ["--test", "content_lint"],
    "fuzz-corpora": ["--features", "fuzzing", "--test", "fuzz_corpus", "--", "--ignored"],
}


def cargo_llvm_cov(*args):
    result = subprocess.run(["cargo", "llvm-cov", *args], cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"cargo llvm-cov {' '.join(args)} failed:\n{result.stderr[-3000:]}")
    return result


def run_layer(name, args, isolated=True):
    if isolated:
        cargo_llvm_cov("clean", "--profraw-only")
    cargo_llvm_cov("--no-report", *args)


def export_lcov(path):
    cargo_llvm_cov("report", "--lcov", "--output-path", str(path), "--ignore-filename-regex", IGNORE)


def test_module_starts():
    """{"src/x.rs": first line of its `#[cfg(test)]` module}. Unit tests'
    own code lives there and isn't library code; in this repo test modules
    always sit at the end of their file."""
    starts = {}
    for path in (ROOT / "src").glob("*.rs"):
        lines = path.read_text().splitlines()
        for number, line in enumerate(lines, 1):
            nxt = lines[number].strip() if number < len(lines) else ""
            if line.strip() == "#[cfg(test)]" and nxt.startswith("mod tests"):
                starts[f"src/{path.name}"] = number
                break
    return starts


def parse_lcov(path, test_starts):
    """{(file, line): hits} for library lines of src/ files (relative to the
    repo), without `#[cfg(test)]` modules."""
    lines = {}
    current = None
    for raw in path.read_text().splitlines():
        if raw.startswith("SF:"):
            file = Path(raw[3:])
            try:
                current = str(file.relative_to(ROOT))
            except ValueError:
                current = None
            if current and not current.startswith("src/"):
                current = None
        elif raw.startswith("DA:") and current:
            line, hits = raw[3:].split(",")[:2]
            if int(line) >= test_starts.get(current, 1 << 30):
                continue
            key = (current, int(line))
            lines[key] = max(lines.get(key, 0), int(hits))
    return lines


def ranges(numbers):
    """[3, 4, 5, 9] → "3-5, 9"."""
    out, start, prev = [], None, None
    for n in sorted(numbers):
        if start is None:
            start = prev = n
        elif n == prev + 1:
            prev = n
        else:
            out.append(f"{start}-{prev}" if start != prev else str(start))
            start = prev = n
    if start is not None:
        out.append(f"{start}-{prev}" if start != prev else str(start))
    return ", ".join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--html", action="store_true")
    parser.add_argument("--layer", action="append", choices=list(LAYERS))
    options = parser.parse_args()
    layers = {name: LAYERS[name] for name in (options.layer or LAYERS)}
    OUT.mkdir(parents=True, exist_ok=True)

    covered = {}  # layer → set of covered (file, line)
    instrumented = set()
    test_starts = test_module_starts()
    for name, args in layers.items():
        print(f"running {name} …", file=sys.stderr, flush=True)
        run_layer(name, args)
        lcov = OUT / f"{name}.lcov"
        export_lcov(lcov)
        data = parse_lcov(lcov, test_starts)
        instrumented |= data.keys()
        covered[name] = {key for key, hits in data.items() if hits > 0}

    merged = set().union(*covered.values())
    total = len(instrumented)
    print(f"\nsrc/ library line coverage ({total} instrumented lines; src/fuzz.rs and "
          "#[cfg(test)] modules excluded)\n")
    print(f"{'layer':18} {'covered':>8} {'%':>6} {'only here':>10}")
    for name, lines in covered.items():
        others = set().union(*(v for k, v in covered.items() if k != name))
        print(f"{name:18} {len(lines):8} {100 * len(lines) / total:6.1f} {len(lines - others):10}")
    print(f"{'all layers':18} {len(merged):8} {100 * len(merged) / total:6.1f}")

    per_file = defaultdict(lambda: [0, 0, []])
    for file, line in instrumented:
        entry = per_file[file]
        entry[0] += 1
        if (file, line) in merged:
            entry[1] += 1
        else:
            entry[2].append(line)
    print(f"\n{'file':22} {'lines':>6} {'covered %':>10} {'uncovered':>10}")
    report = []
    for file in sorted(per_file):
        count, hit, missing = per_file[file]
        print(f"{file:22} {count:6} {100 * hit / count:10.1f} {len(missing):10}")
        if missing:
            report.append(f"{file}: {ranges(missing)}")
    (OUT / "uncovered.txt").write_text("\n".join(report) + "\n")
    with (OUT / "merged.lcov").open("w") as out:
        for file in sorted(per_file):
            out.write(f"SF:{ROOT / file}\n")
            for (f, line) in sorted(k for k in instrumented if k[0] == file):
                out.write(f"DA:{line},{1 if (f, line) in merged else 0}\n")
            out.write("end_of_record\n")
    print(f"\nuncovered line ranges: {OUT / 'uncovered.txt'}; lcov: {OUT}/*.lcov")

    if options.html:
        print("writing merged HTML report (re-running all layers) …", file=sys.stderr, flush=True)
        cargo_llvm_cov("clean", "--profraw-only")
        for name, args in layers.items():
            run_layer(name, args, isolated=False)
        cargo_llvm_cov("report", "--html", "--output-dir", str(OUT), "--ignore-filename-regex", IGNORE)
        print(f"html: {OUT / 'html' / 'index.html'}")


if __name__ == "__main__":
    main()
