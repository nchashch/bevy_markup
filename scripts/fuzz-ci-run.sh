#!/bin/bash
# One nightly fuzz campaign (nightly.yml): runs scripts/fuzz-<driver>.sh
# with its whole output in fuzz-private/<driver>-<target>.log, which the
# job seals (scripts/fuzz-seal.sh) instead of printing: a public
# repository's job logs are public, and fuzzer output holds crashing inputs
# (libFuzzer prints small ones in Base64), panic messages and stack traces.
# The job log and the job summary get counts only: coverage at the start
# and end, corpus size, crashes found.
# Usage: scripts/fuzz-ci-run.sh <libfuzzer|honggfuzz|fuzzcheck|test-fuzz> <target> <seconds> [driver args...]
# Env: CORPUS (the working corpus directory, for its size).
set -uo pipefail
cd "$(dirname "$0")/.." || exit

driver=$1 target=$2 seconds=$3
shift 3
mkdir -p fuzz-private
log="fuzz-private/$driver-$target.log"
count() { find "$1" -type f 2>/dev/null | wc -l; }
# Where each driver saves a crashing input (the same places its
# scripts/fuzz-<driver>.sh counts; the rest of those directories is corpus).
crash_count() {
    case $driver in
        libfuzzer) find "fuzz/artifacts/$target" -maxdepth 1 -type f ;;
        honggfuzz) find "hfuzz_workspace/$target" -maxdepth 1 -name '*.fuzz' ;;
        fuzzcheck) find "fuzzcheck/fuzz/fuzz_$target/artifacts" -type f ;;
        test-fuzz) find test-fuzz/target/afl/output -path "*fuzz_$target/*/crashes/*" \
            -type f ! -name README.txt ;;
    esac 2>/dev/null | wc -l
}
crashes_before=$(crash_count)

"scripts/fuzz-$driver.sh" "$target" "$seconds" "$@" >"$log" 2>&1
status=$?
crashes=$(($(crash_count) - crashes_before))
# libFuzzer's fork mode keeps going after a crash (-ignore_crashes=1):
# a new file in its artifact directory is what reports one.
if [ "$crashes" -gt 0 ] && [ "$status" -eq 0 ]; then
    status=1
fi

# Coverage, by each driver's own measure.
case $driver in
    libfuzzer)
        start=$(grep -oE 'cov: [0-9]+' "$log" | head -1 | grep -oE '[0-9]+')
        end=$(grep -oE 'cov: [0-9]+' "$log" | tail -1 | grep -oE '[0-9]+')
        unit=edges ;;
    honggfuzz)
        start=$(grep -oE 'Cur:[0-9/]+' "$log" | head -1 | cut -d/ -f4)
        end=$(grep -oE 'Cur:[0-9/]+' "$log" | tail -1 | cut -d/ -f4)
        unit=edges ;;
    fuzzcheck)
        start=$(grep -oE 'cov: [0-9]+' "$log" | head -1 | grep -oE '[0-9]+')
        end=$(grep -oE 'cov: [0-9]+' "$log" | tail -1 | grep -oE '[0-9]+')
        unit=counters ;;
    test-fuzz)
        stats=$(find test-fuzz/target -name fuzzer_stats 2>/dev/null | head -1)
        start=""
        end=$(sed -n 's/^edges_found *: *//p' "${stats:-/dev/null}" 2>/dev/null)
        unit=edges ;;
esac
corpus=$(count "${CORPUS:-/nonexistent}")
if [ "$status" -eq 0 ]; then result=ok; else result="**$crashes crash(es)** (sealed)"; fi
[ "$crashes" -eq 0 ] && [ "$status" -ne 0 ] && result="**failed** (status $status, log sealed)"

echo "fuzz: $driver $target, $((seconds / 60)) min: $unit ${start:-?} → ${end:-?}, corpus ${corpus}, $result"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
        echo "| Driver | Target | Minutes | Coverage | Corpus | Result |"
        echo "|---|---|---|---|---|---|"
        echo "| $driver | $target | $((seconds / 60)) | ${start:-?} → ${end:-?} $unit | $corpus | $result |"
    } >>"$GITHUB_STEP_SUMMARY"
fi
exit "$status"
