#!/bin/sh
# test-fuzz (AFLplus via cargo-afl). Corpus seeds come from ordinary
# `cargo test` runs of the test-fuzz crate.
# Usage: scripts/fuzz-test-fuzz.sh <html|css|ftl> [seconds]
#
# The AFL variables skip the core-pattern and CPU-governor checks, which
# require sudo on this machine.
set -eu
cd "$(dirname "$0")/.."

case "${1:-}" in
    html|css|ftl) target=$1 ;;
    *) echo "usage: $0 <html|css|ftl> [seconds=60]" >&2; exit 2 ;;
esac
duration=${2:-60}

cd test-fuzz
export AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES=1
export AFL_SKIP_CPUFREQ=1
# Build first (unbounded: a cold build can exceed the run budget); the
# timeout then bounds only the run. `timeout` exits 124 when the budget
# elapses — that is a normal, successful end for a fuzz campaign.
cargo +nightly test-fuzz --no-run "tests::fuzz_$target"
set +e
timeout "$duration" cargo +nightly test-fuzz "tests::fuzz_$target"
status=$?
set -e
cd ..
case $status in
    0 | 124) exit 0 ;;
    *) exit $status ;;
esac
