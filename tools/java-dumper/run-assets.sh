#!/usr/bin/env bash
#
# Regenerates the P0 asset-store golden data (testdata/golden/server/assets/)
# with the real CoreServer asset pipeline.
#
# Usage:
#   tools/java-dumper/run-assets.sh                     # writes testdata/golden/server/assets
#   tools/java-dumper/run-assets.sh /tmp/assets-golden   # writes an arbitrary directory
#   tools/java-dumper/run-assets.sh --check              # run twice into temp dirs and diff
#
# Environment:
#   ASSET_PACK=Assets/Assets.zip   the pack (relative to the repo root or absolute)
#   DUMPER_PARALLELISM=1           the canonical single-threaded decode
#
# This is a thin wrapper around run.sh: it only selects the AssetDumper entry
# point, so the build (upstream jars, classpath) is shared with the worldgen
# dump. The output directory is the third positional argument run.sh forwards
# (repo root) plus the first one (output directory); the world directory is
# ignored by AssetDumper.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/assets"
MAIN="com.hypixel.hytale.tools.golden.AssetDumper"

if [ "${1:-}" = "--check" ]; then
    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-assets-check.XXXXXX")"
    cleanup() {
        rm -rf "$TMP_DIR"
    }
    trap cleanup EXIT

    FIRST="$TMP_DIR/first"
    SECOND="$TMP_DIR/second"
    mkdir -p "$FIRST" "$SECOND"

    echo "== asset dump 1/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$FIRST"
    echo "== asset dump 2/2 =="
    DUMPER_MAIN="$MAIN" "$SCRIPT_DIR/run.sh" "$SECOND"

    echo "== comparing the two runs =="
    if ! diff -r "$FIRST" "$SECOND"; then
        echo "error: the two asset dumps are not byte-identical (nondeterministic output)" >&2
        exit 1
    fi
    echo "deterministic: both asset dumps produced byte-identical files"

    echo "== comparing the fresh dump with testdata/golden/server/assets =="
    if [ ! -d "$COMMITTED" ]; then
        echo "error: $COMMITTED does not exist; run without --check first" >&2
        exit 1
    fi
    if ! diff -r --exclude='README.md' "$FIRST" "$COMMITTED"; then
        echo "error: testdata/golden/server/assets differs from a fresh dump" >&2
        echo "       Review the change, then regenerate with tools/java-dumper/run-assets.sh." >&2
        exit 1
    fi
    echo "up to date: testdata/golden/server/assets matches a fresh dump"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
export DUMPER_MAIN="$MAIN"
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
