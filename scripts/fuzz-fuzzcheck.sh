#!/bin/sh
# fuzzcheck (its own coverage sensor over `-C instrument-coverage`).
# Usage: scripts/fuzz-fuzzcheck.sh <html|css|ftl> [seconds]
#
# Uses the vendored, patched fuzzcheck (see fuzzcheck/vendor/) and the
# linker override in fuzzcheck/.cargo/config.toml.
set -eu
cd "$(dirname "$0")/.."

case "${1:-}" in
    html|css|ftl) target=$1 ;;
    *) echo "usage: $0 <html|css|ftl> [seconds=60]" >&2; exit 2 ;;
esac
duration=${2:-60}

cd fuzzcheck
exec cargo +nightly fuzzcheck --test "$target" "fuzz_$target" \
    --stop-after-duration "$duration"
