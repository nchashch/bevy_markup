#!/bin/sh
# cargo-fuzz (libFuzzer + AddressSanitizer) over p23::fuzz harnesses.
# Usage: scripts/fuzz-libfuzzer.sh <html|css|ftl> [seconds]
set -eu
cd "$(dirname "$0")/.."

case "${1:-}" in
    html|css|ftl) target=$1 ;;
    *) echo "usage: $0 <html|css|ftl> [seconds=60]" >&2; exit 2 ;;
esac
duration=${2:-60}

# cargo-fuzz defaults --target to the triple *it* was built for; prebuilt
# binaries (e.g. CI's install-action) are musl, which ASan can't use.
host=$(rustc +nightly -vV | sed -n 's/^host: //p')

exec cargo +nightly fuzz run --target "$host" "$target" -- \
    -max_total_time="$duration" -max_len=65536
