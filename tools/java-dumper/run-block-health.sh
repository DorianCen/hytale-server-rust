#!/usr/bin/env bash
#
# Regenerates the Java `BlockHealthSection` v0 BSON fixture
# (testdata/golden/server/storage/block-health-section.bson).
#
# Usage:
#   tools/java-dumper/run-block-health.sh                 # committed dir
#   tools/java-dumper/run-block-health.sh /tmp/health     # arbitrary output dir
#   tools/java-dumper/run-block-health.sh --check         # run twice + diff committed
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/storage"
MAIN="com.hypixel.hytale.tools.golden.BlockHealthDumper"

CHECK=0
OUT_DIR=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) CHECK=1; shift ;;
        -h|--help)
            sed -n '3,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --*) echo "error: unknown option: $1" >&2; exit 2 ;;
        *)
            if [ -n "$OUT_DIR" ]; then
                echo "error: more than one output directory: $1" >&2
                exit 2
            fi
            OUT_DIR="$1"
            shift
            ;;
    esac
done

export DUMPER_MAIN="$MAIN"
if [ "$CHECK" -eq 1 ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-health-check.XXXXXX")"
    trap 'rm -rf "$TMP_DIR"' EXIT
    mkdir -p "$TMP_DIR/first" "$TMP_DIR/second"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/first"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/second"
    diff -r "$TMP_DIR/first" "$TMP_DIR/second"
    diff -r "$TMP_DIR/first" "$COMMITTED" --exclude='*.region.bin' --exclude='manifest.json' --exclude='README.md' \
        --exclude='player-data' --exclude='world-config'
    echo "up to date: the Java BlockHealthSection fixture is unchanged"
    exit 0
fi

if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$COMMITTED"
fi
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
