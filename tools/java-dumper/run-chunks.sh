#!/usr/bin/env bash
#
# Regenerates the Java chunk-packet golden data
# (testdata/golden/server/chunks-pkts/) from the real Default-world generator.
#
# Usage:
#   tools/java-dumper/run-chunks.sh                     # writes the committed directory
#   tools/java-dumper/run-chunks.sh /tmp/chunks-golden   # writes an arbitrary directory
#   tools/java-dumper/run-chunks.sh --check              # run twice into temp dirs and diff
#
# Environment:
#   ASSET_PACK=Assets/Assets.zip   the pack (relative to the repo root or absolute)
#   DUMPER_PARALLELISM=1           the canonical single-threaded decode (forced below)
#
# This is a thin wrapper around run.sh: it only selects the
# ChunkPacketDumper entry point, so the build (upstream jars, classpath) is
# shared with the worldgen/asset dumps. The output directory is the first
# positional argument; the world directory is ignored by the dumper.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/chunks-pkts"
MAIN="com.hypixel.hytale.tools.golden.ChunkPacketDumper"

# The dump is only deterministic with the canonical single-worker decode.
export DUMPER_PARALLELISM=1

if [ "${1:-}" = "--check" ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-chunks-check.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    FIRST="$TMP_DIR/first"
    SECOND="$TMP_DIR/second"
    mkdir -p "$FIRST" "$SECOND"

    echo "== chunk packet dump 1/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$FIRST"
    echo "== chunk packet dump 2/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$SECOND"

    echo "== comparing the two runs =="
    if ! diff -r "$FIRST" "$SECOND"; then
        echo "error: the two chunk packet dumps are not byte-identical (nondeterministic output)" >&2
        exit 1
    fi
    echo "deterministic: both chunk packet dumps produced byte-identical files"

    echo "== comparing the fresh dump with testdata/golden/server/chunks-pkts =="
    if [ ! -d "$COMMITTED" ]; then
        echo "error: $COMMITTED does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -r --exclude='README.md' "$FIRST" "$COMMITTED"; then
        echo "error: testdata/golden/server/chunks-pkts differs from a fresh dump" >&2
        echo "       Review the change, then regenerate with tools/java-dumper/run-chunks.sh." >&2
        exit 1
    fi
    echo "up to date: testdata/golden/server/chunks-pkts matches a fresh dump"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
export DUMPER_MAIN="$MAIN"
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
