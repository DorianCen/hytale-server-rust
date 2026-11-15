#!/usr/bin/env bash
#
# Regenerates the canonical block-component holder golden
# (testdata/golden/server/block-components/hunt-site.json) with the real
# ChunkStore component registry.
#
# The regular dumper classpath is not used here: the release HytaleServer.jar
# already contains the real component codecs, and the fixture only needs one
# component. Set HYTALE_SERVER_JAR to point at another release jar.
#
# Usage:
#   tools/java-dumper/run-block-components.sh
#   tools/java-dumper/run-block-components.sh --check
#
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/block-components"
SOURCE="$SCRIPT_DIR/src/main/java/com/hypixel/hytale/tools/golden/BlockComponentDumper.java"
JAR="${HYTALE_SERVER_JAR:-}"

if [ -z "$JAR" ]; then
    for candidate in \
        "$HOME/.var/app/com.hypixel.HytaleLauncher/data/Hytale/install/release/package/game/latest/Server/HytaleServer.jar" \
        "$HOME/.var/app/com.hypixel.HytaleLauncher/data/Hytale/install/release/package/game/latest/Server/HytaleServer.aot.jar"; do
        if [ -f "$candidate" ]; then
            JAR="$candidate"
            break
        fi
    done
fi

if [ ! -f "$JAR" ]; then
    echo "error: release HytaleServer.jar not found; set HYTALE_SERVER_JAR" >&2
    exit 2
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-block-components.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

javac -cp "$JAR" -d "$TMP_DIR/classes" "$SOURCE"

if [ "${1:-}" = "--check" ]; then
    run_dump() {
        local out="$1"
        mkdir -p "$out"
        java -cp "$TMP_DIR/classes:$JAR" com.hypixel.hytale.tools.golden.BlockComponentDumper "$ROOT" "$out"
    }
    run_dump "$TMP_DIR/first"
    run_dump "$TMP_DIR/second"
    diff -r "$TMP_DIR/first" "$TMP_DIR/second"
    if ! diff -u "$COMMITTED/hunt-site.json" "$TMP_DIR/first/hunt-site.json"; then
        echo "error: the committed block-component golden differs from a fresh dump" >&2
        exit 1
    fi
    echo "deterministic: the block-component golden matches a fresh dump"
    exit 0
fi

OUT_DIR="${1:-$COMMITTED}"
mkdir -p "$OUT_DIR"
java -cp "$TMP_DIR/classes:$JAR" com.hypixel.hytale.tools.golden.BlockComponentDumper "$ROOT" "$OUT_DIR"
