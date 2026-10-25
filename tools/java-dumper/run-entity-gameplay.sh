#!/usr/bin/env bash
#
# Regenerates the Java entity-gameplay fixture
# (testdata/golden/server/entity/entity-gameplay.json).
#
# Usage:
#   tools/java-dumper/run-entity-gameplay.sh                 # committed dir
#   tools/java-dumper/run-entity-gameplay.sh /tmp/entity     # arbitrary output dir
#   tools/java-dumper/run-entity-gameplay.sh --check         # run twice + diff committed
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/entity"
MAIN="com.hypixel.hytale.tools.golden.EntityGameplayDumper"

CHECK=0
OUT_DIR=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) CHECK=1; shift ;;
        -h|--help)
            sed -n '3,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-entity-check.XXXXXX")"
    trap 'rm -rf "$TMP_DIR"' EXIT
    mkdir -p "$TMP_DIR/first" "$TMP_DIR/second"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/first"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/second"
    diff -r "$TMP_DIR/first" "$TMP_DIR/second"
    diff -r "$TMP_DIR/first" "$COMMITTED" --exclude='README.md'
    echo "up to date: the Java entity-gameplay fixture is unchanged"
    exit 0
fi

if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$COMMITTED"
fi
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
