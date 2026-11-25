#!/bin/bash
# Encrypts files for the nightly fuzz jobs' artifacts: the given paths (the
# ones that exist) are packed and encrypted with age to the recipient in
# .github/fuzz-artifacts.age.pub, into fuzz-sealed/<name>.tar.gz.age.
# A public repository's artifacts can be downloaded by anyone signed in to
# GitHub, so nothing a fuzzer found leaves the runner unencrypted: findings
# in bevy_markup or an upstream dependency stay private until they are
# fixed (scripts/fuzz-triage.sh decrypts them locally).
# Usage: scripts/fuzz-seal.sh <name> <path>...
set -euo pipefail
cd "$(dirname "$0")/.."

name=$1
shift
existing=()
for path in "$@"; do
    [ -e "$path" ] && existing+=("$path")
done
if [ ${#existing[@]} -eq 0 ]; then
    echo "fuzz-seal: nothing to seal for $name"
    exit 0
fi
mkdir -p fuzz-sealed
tar -czf - "${existing[@]}" |
    age -R .github/fuzz-artifacts.age.pub -o "fuzz-sealed/$name.tar.gz.age"
echo "fuzz-seal: $name ($(du -sh "fuzz-sealed/$name.tar.gz.age" | cut -f1))"
