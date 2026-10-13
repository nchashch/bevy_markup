#!/bin/sh
# honggfuzz-rs (hardware-counter feedback) over bevy_markup::fuzz harnesses.
# Usage: scripts/fuzz-honggfuzz.sh <html|css|ftl> [seconds]
#
# HFUZZ_BUILD_ARGS points cargo at the honggfuzz manifest and overrides the
# repo's clang+mold linker (which cannot link the hfuzz runtime). The outer
# `timeout` bounds the run; honggfuzz's own --run_time did not stop it.
set -eu
cd "$(dirname "$0")/.."

case "${1:-}" in
    html|css|ftl) target=$1 ;;
    *) echo "usage: $0 <html|css|ftl> [seconds=60]" >&2; exit 2 ;;
esac
duration=${2:-60}

export HFUZZ_BUILD_ARGS='--manifest-path honggfuzz/Cargo.toml --config target.x86_64-unknown-linux-gnu.linker="cc"'
# Build first (unbounded: a cold build can exceed the run budget); the
# timeout then bounds only the run. `timeout` exits 124 when the budget
# elapses — that is a normal, successful end for a fuzz campaign.
cargo +nightly hfuzz build "$target"
set +e
timeout "$duration" cargo +nightly hfuzz run "$target"
status=$?
set -e
case $status in
    0 | 124) exit 0 ;;
    *) exit $status ;;
esac
