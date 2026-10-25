#!/usr/bin/env bash
#
# Regenerates the Java fluid-ticker oracle
# (testdata/golden/server/fluid/fluid-tickers.json).
#
# Usage:
#   tools/java-dumper/run-fluid.sh                     # committed dir
#   tools/java-dumper/run-fluid.sh /tmp/fluid-golden    # arbitrary output dir
#   tools/java-dumper/run-fluid.sh --check              # run twice + diff committed
#
# The probe boots the real CoreServer asset pipeline and evaluates
# FluidTicker.blocksFluidFrom / decodes the P0 fluid tickers. It needs JDK 25,
# Maven and the base-game asset pack.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/fluid"
MAIN="com.hypixel.hytale.tools.golden.FluidTickerProbe"

CHECK=0
OUT_DIR=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check)
            CHECK=1
            shift
            ;;
        -h|--help)
            sed -n '3,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --*)
            echo "error: unknown option: $1" >&2
            exit 2
            ;;
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
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-fluid-check.XXXXXX")"
    trap 'rm -rf "$TMP_DIR"' EXIT
    mkdir -p "$TMP_DIR/first" "$TMP_DIR/second"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/first"
    "$SCRIPT_DIR/run.sh" "$TMP_DIR/second"
    if ! diff -r "$TMP_DIR/first" "$TMP_DIR/second"; then
        echo "error: the two fluid probes are not byte-identical" >&2
        exit 1
    fi
    if [ ! -d "$COMMITTED" ]; then
        echo "error: $COMMITTED does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -r --exclude='README.md' "$TMP_DIR/first" "$COMMITTED"; then
        echo "error: testdata/golden/server/fluid differs from a fresh probe" >&2
        exit 1
    fi
    echo "up to date: testdata/golden/server/fluid matches a fresh probe"
    exit 0
fi

if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$COMMITTED"
fi
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
