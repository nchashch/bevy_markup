#!/bin/sh
# honggfuzz-rs (hardware-counter feedback) over p23::fuzz harnesses.
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
exec timeout "$((duration + 30))" cargo +nightly hfuzz run "$target"
