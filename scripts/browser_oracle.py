#!/usr/bin/env python3
"""Record a real browser's computed styles for p23 test vectors.

For each vector directory (default: every `tests/vectors/*/` holding
`page.html` + `style.css`) this builds a page with

    <style>* { all: unset }</style>   (no UA stylesheet — p23 has none)
    <style>{style.css}</style>
    <body>{page.html}<script>{collector}</script></body>

runs it in headless Chromium (`--dump-dom`), and writes `browser.json` next to
the inputs: computed styles per element (document order, `html` first) and
the parent element's text style per non-whitespace text node. The Rust test
`browser_oracle` in `tests/html_ui.rs` compares p23's output against it; no
browser is needed to run the tests.

Oracle vectors must be plain HTML (no Tera syntax, no `data-l10n-id`): the
browser sees the file as-is.

Usage:
    scripts/browser_oracle.py [--browser PATH] [VECTOR_DIR ...]

Only the Python standard library is required, plus a Chromium/Chrome binary
(found on PATH, or given with --browser / $P23_BROWSER).
"""

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VECTORS = ROOT / "tests" / "vectors"
BROWSERS = ["chromium", "chromium-browser", "google-chrome", "google-chrome-stable", "chrome"]

COLLECTOR = r"""
(() => {
  const px = (value) => parseFloat(value);
  const text = (s) => ({
    color: s.color,
    fontSize: px(s.fontSize),
    fontWeight: Number(s.fontWeight),
    fontStyle: s.fontStyle,
    fontFamily: s.fontFamily,
  });
  const box = (s) => ({
    padding: [s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft].map(px),
    border: [s.borderTopWidth, s.borderRightWidth, s.borderBottomWidth, s.borderLeftWidth].map(px),
    borderImageSource: s.borderImageSource,
    borderImageSlice: s.borderImageSlice,
    borderImageRepeat: s.borderImageRepeat,
    rowGap: s.rowGap,
    backgroundColor: s.backgroundColor,
  });
  const record = (element) => {
    const s = getComputedStyle(element);
    return {
      tag: element.localName,
      id: element.id || null,
      classes: [...element.classList],
      ...text(s),
      ...box(s),
    };
  };
  const self = document.currentScript;
  const elements = [record(document.documentElement)];
  const runs = [];
  const visit = (parent) => {
    for (const node of parent.childNodes) {
      if (node === self) continue;
      if (node.nodeType === Node.ELEMENT_NODE) {
        elements.push(record(node));
        visit(node);
      } else if (node.nodeType === Node.TEXT_NODE && node.data.trim()) {
        runs.push({
          text: node.data.replace(/\s+/g, " "),
          ...text(getComputedStyle(node.parentElement)),
        });
      }
    }
  };
  visit(document.body);
  const out = document.createElement("pre");
  out.id = "p23-oracle";
  out.textContent = JSON.stringify({ elements, runs });
  document.body.append(out);
})();
"""


def find_browser(explicit):
    for candidate in [explicit, os.environ.get("P23_BROWSER"), *BROWSERS]:
        if candidate and (path := shutil.which(candidate)):
            return path
    sys.exit("no Chromium/Chrome found; pass --browser PATH or set P23_BROWSER")


def browser_version(browser):
    result = subprocess.run([browser, "--version"], capture_output=True, text=True, check=True)
    return result.stdout.strip()


def record(browser, vector):
    page = (vector / "page.html").read_text(encoding="utf-8")
    css = (vector / "style.css").read_text(encoding="utf-8")
    if re.search(r"\{\{|\{%|data-l10n-id", page):
        sys.exit(f"{vector}: oracle vectors must be plain HTML (no Tera, no data-l10n-id)")

    document = (
        "<!doctype html><html><head><meta charset=\"utf-8\">"
        "<style>* { all: unset; }</style>"
        f"<style>{css}</style></head><body>{page}"
        f"<script>{COLLECTOR}</script></body></html>"
    )
    with tempfile.TemporaryDirectory(prefix="p23-oracle-") as tmp:
        harness = Path(tmp) / "index.html"
        harness.write_text(document, encoding="utf-8")
        result = subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                "--no-sandbox",
                f"--user-data-dir={tmp}/profile",
                "--dump-dom",
                harness.as_uri(),
            ],
            capture_output=True,
            text=True,
            timeout=60,
            check=True,
        )
    match = re.search(r'<pre id="p23-oracle">(.*?)</pre>', result.stdout, re.S)
    if not match:
        sys.exit(f"{vector}: collector output missing; browser stderr:\n{result.stderr}")
    return json.loads(html.unescape(match.group(1)))


def to_json(generator, data):
    """One record per line, so browser changes show up as readable diffs."""
    lines = ["{", f' "generator": {json.dumps(generator)},']
    for key in ("elements", "runs"):
        records = [f"  {json.dumps(item, ensure_ascii=False)}" for item in data[key]]
        comma = "," if key == "elements" else ""
        lines += [f' "{key}": [', ",\n".join(records), f" ]{comma}"]
    lines.append("}")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--browser", help="Chromium/Chrome binary")
    parser.add_argument("vectors", nargs="*", type=Path, help="vector directories")
    args = parser.parse_args()

    browser = find_browser(args.browser)
    version = browser_version(browser)
    vectors = args.vectors or sorted(
        path.parent for path in VECTORS.glob("*/style.css") if (path.parent / "page.html").exists()
    )
    for vector in vectors:
        data = record(browser, vector)
        generator = {"browser": version, "script": "scripts/browser_oracle.py"}
        (vector / "browser.json").write_text(to_json(generator, data), encoding="utf-8")
        print(f"{vector.resolve().relative_to(ROOT)}: {len(data['elements'])} elements, {len(data['runs'])} text runs")


if __name__ == "__main__":
    main()
