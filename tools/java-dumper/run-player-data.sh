#!/usr/bin/env bash
#
# Regenerates the player save fixture
# (testdata/golden/server/player-data/) with the real Java EntityStore codec.
#
# Usage:
#   tools/java-dumper/run-player-data.sh                   # writes the committed directory
#   tools/java-dumper/run-player-data.sh /tmp/player-data   # writes an arbitrary directory
#   tools/java-dumper/run-player-data.sh --check            # run twice into temp dirs and diff
#   tools/java-dumper/run-player-data.sh --probe <file>     # read a Rust-written player in Java
#
# This is a thin wrapper around run.sh: it selects WorldPlayerDumper, so the
# build (upstream jars, classpath) is shared with the other dumps.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/player-data"
MAIN="com.hypixel.hytale.tools.golden.WorldPlayerDumper"
FILE="12345678-1234-5678-9abc-def012345678.json"

# The holder map order comes from the component archetype; keep the canonical
# single-worker decode used by the other fixtures.
export DUMPER_PARALLELISM=1

if [ "${1:-}" = "--check" ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-player-data-check.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    FIRST="$TMP_DIR/first"
    SECOND="$TMP_DIR/second"
    mkdir -p "$FIRST" "$SECOND"

    echo "== player data dump 1/2 =="
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $FIRST/$FILE" \
        "$SCRIPT_DIR/run.sh" "$FIRST"
    echo "== player data dump 2/2 =="
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $SECOND/$FILE" \
        "$SCRIPT_DIR/run.sh" "$SECOND"

    echo "== comparing the two runs =="
    if ! diff -r --exclude='README.md' "$FIRST" "$SECOND"; then
        echo "error: the two player data dumps are not byte-identical (nondeterministic output)" >&2
        exit 1
    fi
    echo "deterministic: both player data dumps produced byte-identical files"

    echo "== comparing the fresh dump with $COMMITTED =="
    if [ ! -f "$COMMITTED/$FILE" ]; then
        echo "error: $COMMITTED/$FILE does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -u "$COMMITTED/$FILE" "$FIRST/$FILE"; then
        echo "error: the committed player data differs from a fresh dump" >&2
        echo "       Review the change, then regenerate with tools/java-dumper/run-player-data.sh." >&2
        exit 1
    fi
    echo "up to date: $COMMITTED matches a fresh dump"
    exit 0
fi

if [ "${1:-}" = "--probe" ]; then
    if [ "$#" -ne 2 ]; then
        echo "usage: $0 --probe <player.json>" >&2
        exit 2
    fi
    PLAYER="$(cd -- "$(dirname -- "$2")" && pwd)/$(basename -- "$2")"
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-player-data-probe.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT
    DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="probe $PLAYER" \
        "$SCRIPT_DIR/run.sh" "$TMP_DIR"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
mkdir -p "$OUT_DIR"
DUMPER_MAIN="$MAIN" DUMPER_EXTRA_ARGS="write $OUT_DIR/$FILE" \
    "$SCRIPT_DIR/run.sh" "$OUT_DIR"
# The Java writer keeps a .bak when it replaces an existing file; fixtures only
# commit the canonical document.
rm -f "$OUT_DIR/$FILE.bak" "$OUT_DIR/$FILE.tmp"
