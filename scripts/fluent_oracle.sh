#!/bin/sh
# Reference Fluent translation for test vectors: @fluent/dom (in jsdom)
# translates tests/vectors/*/page.html with messages.ftl and writes
# fluent.html next to them. Review the diff and commit it.
# Usage: scripts/fluent_oracle.sh [tests/vectors/<name> ...]
#
# Needs Node and npm; the pinned packages (package-lock.json) are installed
# into scripts/fluent-oracle/node_modules on first use.
set -eu
dir="$(cd "$(dirname "$0")" && pwd)/fluent-oracle"
[ -d "$dir/node_modules" ] || npm ci --prefix "$dir" --silent --no-audit --no-fund
exec node "$dir/oracle.mjs" "$@"
