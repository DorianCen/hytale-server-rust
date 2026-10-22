#!/usr/bin/env bash
#
# Whole-save compatibility gate: read a Rust-written world with the real Java
# codecs — chunk region, world config and player holder.
#
# Usage:
#   tools/java-dumper/run-save-check.sh <save-dir> <chunkX> <chunkZ> \
#       <blockX> <blockY> <blockZ> <expected-block-name>
#
# Example:
#   rm -rf target/save-smoke
#   cargo run -q -p hytale-server -- headless --save target/save-smoke --chunk 4,-5
#   cargo run -q -p hytale-server -- world-config \
#       --input testdata/golden/server/world-config/config.json --save target/save-smoke
#   cargo run -q -p hytale-server -- player-data \
#       --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
#       --save target/save-smoke
#   cargo run -q -p hytale-server -- world-boot --save target/save-smoke \
#       --set-block 129,250,-159,4
#   tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159 Debug_Test_Block
set -euo pipefail

if [ "$#" -ne 7 ]; then
    echo "usage: $0 <save-dir> <chunkX> <chunkZ> <blockX> <blockY> <blockZ> <expected-block-name>" >&2
    exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SAVE_DIR="$(cd -- "$1" && pwd)"
CHUNK_X="$2"
CHUNK_Z="$3"
BLOCK_X="$4"
BLOCK_Y="$5"
BLOCK_Z="$6"
EXPECTED="$7"

CONFIG="$SAVE_DIR/config.json"
if [ ! -f "$CONFIG" ]; then
    echo "error: world config is missing: $CONFIG" >&2
    exit 1
fi

PLAYER="$(find "$SAVE_DIR/players" -maxdepth 1 -type f -name '*.json' 2>/dev/null | sort | head -n 1)"
if [ -z "$PLAYER" ]; then
    echo "error: no player file under $SAVE_DIR/players" >&2
    exit 1
fi

echo "== world config =="
"$SCRIPT_DIR/run-world-config.sh" --probe "$CONFIG"

echo "== player =="
"$SCRIPT_DIR/run-player-data.sh" --probe "$PLAYER"

echo "== chunk region =="
"$SCRIPT_DIR/run-reload-check.sh" "$SAVE_DIR" "$CHUNK_X" "$CHUNK_Z" \
    "$BLOCK_X" "$BLOCK_Y" "$BLOCK_Z" "$EXPECTED"

echo "whole-save check: ok"
