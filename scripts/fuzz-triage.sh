#!/bin/bash
# Triage of the nightly fuzz findings, locally: downloads a Nightly run's
# sealed bundles (scripts/fuzz-seal.sh), decrypts them with the maintainer's
# age key, replays every crashing input against the current code, groups
# them by crash signature and archives one sample per reproducing
# signature. Everything stays in the gitignored docs/agents/bugs/private/.
#
# Usage:
#   scripts/fuzz-triage.sh [run-id]          # default: the newest completed Nightly
#   scripts/fuzz-triage.sh --local <dir>     # sealed bundles already on disk
# Env:
#   BEVY_MARKUP_FUZZ_KEY   the age identity (default ~/.config/bevy_markup/fuzz-artifacts.key)
#   BEVY_MARKUP_NIGHTLY    toolchain for the replays (default: nightly)
#
# Output:
#   docs/agents/bugs/private/fuzz-inbox/<run>/        decrypted bundles + triage.md
#   docs/agents/bugs/private/fuzz-findings/<signature>/  one sample + replay log per
#                                                    reproducing signature (kept across runs)
# Inputs of libFuzzer and honggfuzz (raw bytes, the same harnesses) are
# replayed with `cargo fuzz run`; fuzzcheck's and test-fuzz's own formats
# are listed with the command that replays them.
set -euo pipefail
cd "$(dirname "$0")/.."

key=${BEVY_MARKUP_FUZZ_KEY:-$HOME/.config/bevy_markup/fuzz-artifacts.key}
toolchain=${BEVY_MARKUP_NIGHTLY:-nightly}
private=docs/agents/bugs/private
[ -f "$key" ] || { echo "no age key at $key (BEVY_MARKUP_FUZZ_KEY)" >&2; exit 2; }
command -v age >/dev/null || { echo "age is not installed" >&2; exit 2; }

if [ "${1:-}" = "--local" ]; then
    sealed=$(realpath "$2")
    run=local-$(date +%Y%m%d-%H%M%S)
else
    run=${1:-$(gh run list --workflow Nightly --status completed --limit 1 \
        --json databaseId --jq '.[0].databaseId')}
    sealed=""
fi
inbox=$private/fuzz-inbox/$run
findings=$private/fuzz-findings
mkdir -p "$inbox" "$findings"

if [ -z "$sealed" ]; then
    echo "downloading the sealed bundles of Nightly run $run"
    gh run download "$run" -p 'fuzz-sealed-*' -D "$inbox/download"
    sealed=$inbox/download
fi

# Decrypt: logs and crashes always, corpora only with TRIAGE_CORPORA=1.
while IFS= read -r bundle; do
    name=$(basename "$bundle" .tar.gz.age)
    case $name in corpus-*) [ "${TRIAGE_CORPORA:-0}" = 1 ] || continue ;; esac
    mkdir -p "$inbox/$name"
    age -d -i "$key" "$bundle" | tar -xzf - -C "$inbox/$name"
done < <(find "$sealed" -name '*.tar.gz.age' | sort)

# The crashing inputs, by driver and target (from the bundle's name).
report=$inbox/triage.md
replays=$inbox/replays
mkdir -p "$replays"
declare -A count samples drivers status
manual=()
host=$(rustc "+$toolchain" -vV | sed -n 's/^host: //p')

signature_of() {
    # The first line naming the failure: a panic's location and message,
    # a sanitizer's report, a libFuzzer verdict or a signal.
    grep -m1 -oE "panicked at [^ ]+|ERROR: AddressSanitizer: [a-z-]+|ERROR: libFuzzer: [a-z-]+|SUMMARY: [A-Za-z]+: [a-z-]+|deadly signal|fatal runtime error: [a-z ]+" "$1" ||
        echo "unclassified"
}

for dir in "$inbox"/*/; do
    bundle=$(basename "$dir")
    case $bundle in download | replays | corpus-*) continue ;; esac
    driver=${bundle%-*}
    target=${bundle##*-}
    case $driver in
        # Only where each driver saves crashing inputs: the bundles also
        # hold corpus files (honggfuzz's input/, AFL's queue/).
        libfuzzer) files=$(find "$dir/fuzz/artifacts" -mindepth 2 -maxdepth 2 -type f 2>/dev/null) ;;
        honggfuzz) files=$(find "$dir/hfuzz_workspace" -mindepth 2 -maxdepth 2 -name '*.fuzz' 2>/dev/null) ;;
        fuzzcheck | test-fuzz)
            while IFS= read -r file; do
                [ -n "$file" ] && manual+=("$driver $target: $file")
            done < <(find "$dir" \( -path '*fuzzcheck/fuzz/*/artifacts/*' -o -path '*/crashes/*' \) \
                -type f ! -name README.txt 2>/dev/null)
            continue ;;
        *) continue ;;
    esac
    for file in $files; do
        out=$replays/$(basename "$file").log
        if timeout 120 cargo "+$toolchain" fuzz run --target "$host" "$target" "$file" \
            -- -runs=1 >"$out" 2>&1; then
            sig="does not reproduce"
        else
            sig="$target: $(signature_of "$out")"
        fi
        count[$sig]=$((${count[$sig]:-0} + 1))
        drivers[$sig]="${drivers[$sig]:-} $driver"
        # Keep the smallest input as the sample.
        prev=${samples[$sig]:-}
        if [ -z "$prev" ] || [ "$(stat -c %s "$file")" -lt "$(stat -c %s "$prev")" ]; then
            samples[$sig]=$file
        fi
    done
done

{
    echo "# Fuzz triage: Nightly run $run ($(date -u +%Y-%m-%dT%H:%MZ))"
    echo
    echo "| Signature | Inputs | Drivers | Sample | Archived |"
    echo "|---|---|---|---|---|"
    for sig in "${!count[@]}"; do
        sample=${samples[$sig]}
        archived=""
        if [ "$sig" != "does not reproduce" ]; then
            slug=$(printf '%s' "$sig" | sha256sum | cut -c1-12)
            dest=$findings/$slug
            if [ ! -d "$dest" ]; then
                mkdir -p "$dest"
                cp "$sample" "$dest/input"
                cp "$replays/$(basename "$sample").log" "$dest/replay.log"
                printf '%s\nfirst seen: Nightly run %s\n' "$sig" "$run" >"$dest/signature.txt"
                archived="new: $dest"
            else
                archived="known: $dest"
            fi
        fi
        drv=$(printf '%s\n' ${drivers[$sig]} | sort -u | tr '\n' ' ')
        echo "| \`$sig\` | ${count[$sig]} | $drv | \`${sample#"$inbox"/}\` | $archived |"
    done
    if [ ${#manual[@]} -gt 0 ]; then
        echo
        echo "Inputs in a driver's own format (replay with that driver):"
        echo
        printf -- '- %s\n' "${manual[@]}"
        echo
        echo "fuzzcheck: \`cd fuzzcheck && cargo +nightly fuzzcheck --test <target> fuzz_<target> --command minify --input-file <file>\`;"
        echo "test-fuzz: \`cd test-fuzz && cargo +nightly test-fuzz tests::fuzz_<target> --replay\` with the file in its corpus."
    fi
} >"$report"

echo
cat "$report"
echo
echo "logs: $inbox/*/fuzz-private/*.log; replays: $replays/"
