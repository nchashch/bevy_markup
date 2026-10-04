#!/bin/sh
# Mutation testing (cargo-mutants) over p23's library code: every mutant
# rebuilds p23 and relinks the selected test binaries. Fast settings (each
# measured; see Cargo.toml `[profile.mutants]`): the `mutants` profile (no
# debug info, optimized dependencies), Bevy linked dynamically, and only the
# tests that run get built (`--cargo-arg`; `--cargo-test-arg` would reach
# only the test run, and the build step would link every test binary and
# example).
# Usage: scripts/mutants.sh [--full] [extra cargo-mutants args, e.g. --file src/cascade.rs]
#   default: unit tests + tests/html_ui.rs (vectors and oracles) + tests/stateful.rs
#            (the same set as the weekly CI run)
#   --full:  also proptest, quickcheck and layout properties
# Results: target/mutants.out/{caught,missed,timeout,unviable}.txt; reruns
# with --iterate skip mutants already caught.
#
# cargo-mutants copies the source tree per job and ignores nested
# .gitignore files, so the fuzz drivers' build caches (GBs) are moved aside
# for the run and restored afterwards, even on interruption. Copies go to
# $P23_MUTANTS_TMP (default ~/.cache/p23-mutants); each job builds its own
# target dir once. Needs `cargo install cargo-mutants`.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

tests="--cargo-arg=--lib --cargo-arg=--test=html_ui --cargo-arg=--test=stateful"
if [ "${1:-}" = "--full" ]; then
    shift
    tests="$tests --cargo-arg=--test=properties --cargo-arg=--test=quickcheck --cargo-arg=--test=layout_properties"
fi

tmp=${P23_MUTANTS_TMP:-$HOME/.cache/p23-mutants}
parked="$tmp/parked"
mkdir -p "$parked"
caches="fuzz/target fuzz/corpus fuzz/artifacts honggfuzz/target test-fuzz/target fuzzcheck/target fuzzcheck/fuzz hfuzz_target hfuzz_workspace"

restore() {
    for d in $caches; do
        if [ -e "$parked/$d" ]; then
            mkdir -p "$(dirname "$root/$d")"
            mv "$parked/$d" "$root/$d"
        fi
    done
    find "$tmp" -maxdepth 1 -name 'cargo-mutants-*' -exec rm -rf {} +
}
trap restore EXIT INT TERM

for d in $caches; do
    if [ -e "$d" ]; then
        mkdir -p "$parked/$(dirname "$d")"
        mv "$d" "$parked/$d"
    fi
done

# shellcheck disable=SC2086
TMPDIR="$tmp" cargo mutants --iterate -j "${P23_MUTANTS_JOBS:-4}" \
    --profile mutants --cargo-arg=--features=bevy/dynamic_linking \
    -e src/fuzz.rs -e src/lib.rs $tests --minimum-test-timeout 60 -o target "$@"
