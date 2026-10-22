#!/usr/bin/env bash
#
# Regenerates the per-world config fixture
# (testdata/golden/server/world-config/) with the real Java WorldConfig codec.
#
# Usage:
#   tools/java-dumper/run-world-config.sh                  # writes the committed directory
#   tools/java-dumper/run-world-config.sh /tmp/world-config # writes an arbitrary directory
#   tools/java-dumper/run-world-config.sh --check           # run twice into temp dirs and diff
#   tools/java-dumper/run-world-config.sh --probe <config>  # read a Rust-written config in Java
#
# This is a thin wrapper around run.sh: it selects WorldConfigDumper, so the
# build (upstream jars, classpath) is shared with the other dumps.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/world-config"
MAIN="com.hypixel.hytale.tools.golden.WorldConfigDumper"

# The world config is written by the real asset boot; keep the canonical
# single-worker decode so the values do not depend on scheduling.
export DUMPER_PARALLELISM=1

if [ "${1:-}" = "--check" ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-world-config-check.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    FIRST="$TMP_DIR/first"
    SECOND="$TMP_DIR/second"
    mkdir -p "$FIRST" "$SECOND"

    echo "== world config dump 1/2 =="
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $FIRST/config.json" \
        "$SCRIPT_DIR/run.sh" "$FIRST"
    echo "== world config dump 2/2 =="
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $SECOND/config.json" \
        "$SCRIPT_DIR/run.sh" "$SECOND"

    echo "== comparing the two runs =="
    if ! diff -r --exclude='README.md' "$FIRST" "$SECOND"; then
        echo "error: the two world config dumps are not byte-identical (nondeterministic output)" >&2
        exit 1
    fi
    echo "deterministic: both world config dumps produced byte-identical files"

    echo "== comparing the fresh dump with $COMMITTED =="
    if [ ! -f "$COMMITTED/config.json" ]; then
        echo "error: $COMMITTED/config.json does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -u "$COMMITTED/config.json" "$FIRST/config.json"; then
        echo "error: the committed world config differs from a fresh dump" >&2
        echo "       Review the change, then regenerate with tools/java-dumper/run-world-config.sh." >&2
        exit 1
    fi
    echo "up to date: $COMMITTED matches a fresh dump"
    exit 0
fi

if [ "${1:-}" = "--probe" ]; then
    if [ "$#" -ne 2 ]; then
        echo "usage: $0 --probe <config.json>" >&2
        exit 2
    fi
    CONFIG="$(cd -- "$(dirname -- "$2")" && pwd)/$(basename -- "$2")"
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-world-config-probe.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="probe $CONFIG" \
        "$SCRIPT_DIR/run.sh" "$TMP_DIR"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
mkdir -p "$OUT_DIR"
DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $OUT_DIR/config.json" \
    "$SCRIPT_DIR/run.sh" "$OUT_DIR"
# The Java writer keeps a .bak when it replaces an existing file; fixtures only
# commit the canonical document.
rm -f "$OUT_DIR/config.json.bak" "$OUT_DIR/config.json.tmp"
