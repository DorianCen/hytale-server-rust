#!/usr/bin/env bash
#
# Regenerates the Java world-storage fixture
# (testdata/golden/server/storage/java-world/) from the real Default-world
# generator and the server's BSON chunk codec.
#
# Usage:
#   tools/java-dumper/run-storage.sh                     # writes the committed directory
#   tools/java-dumper/run-storage.sh /tmp/storage-golden  # writes an arbitrary directory
#   tools/java-dumper/run-storage.sh --check              # run twice into temp dirs and diff
#
# This is a thin wrapper around run.sh: it only selects the StorageDumper entry
# point, so the build (upstream jars, classpath) is shared with the worldgen and
# asset dumps. The world directory is ignored by the dumper.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/storage/java-world"
MAIN="com.hypixel.hytale.tools.golden.StorageDumper"

# The dump is only deterministic with the canonical single-worker decode.
export DUMPER_PARALLELISM=1

if [ "${1:-}" = "--check" ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-storage-check.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    FIRST="$TMP_DIR/first"
    SECOND="$TMP_DIR/second"
    mkdir -p "$FIRST" "$SECOND"

    echo "== world storage dump 1/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$FIRST"
    echo "== world storage dump 2/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$SECOND"

    echo "== comparing the two runs =="
    if ! diff -r --exclude='README.md' "$FIRST" "$SECOND"; then
        echo "error: the two world storage dumps are not byte-identical (nondeterministic output)" >&2
        exit 1
    fi
    echo "deterministic: both world storage dumps produced byte-identical files"

    echo "== comparing the fresh dump with $COMMITTED =="
    if [ ! -d "$COMMITTED" ]; then
        echo "error: $COMMITTED does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -r --exclude='README.md' "$FIRST" "$COMMITTED"; then
        echo "error: testdata/golden/server/storage/java-world differs from a fresh dump" >&2
        echo "       Review the change, then regenerate with tools/java-dumper/run-storage.sh." >&2
        exit 1
    fi
    echo "up to date: testdata/golden/server/storage/java-world matches a fresh dump"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
mkdir -p "$OUT_DIR"
export DUMPER_MAIN="$MAIN"
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
