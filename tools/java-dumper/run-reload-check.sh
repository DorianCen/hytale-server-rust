#!/usr/bin/env bash
#
# Reverse storage gate: read a Rust-written world save with the real Java
# chunk/storage codecs and check one block.
#
# Usage:
#   tools/java-dumper/run-reload-check.sh <save-dir> <chunkX> <chunkZ> \
#       <blockX> <blockY> <blockZ> <expected-block-name>
#
# Example:
#   cargo run -p hytale-server -- headless --save /tmp/rust-world --chunk 4,-5
#   cargo run -p hytale-server -- load-chunk --save /tmp/rust-world --chunk 4,-5 \
#       --set-block 129,250,-159,4
#   tools/java-dumper/run-reload-check.sh /tmp/rust-world 4 -5 129 250 -159 Debug_Test_Block
set -euo pipefail

if [ "$#" -ne 7 ]; then
    echo "usage: $0 <save-dir> <chunkX> <chunkZ> <blockX> <blockY> <blockZ> <expected-block-name>" >&2
    exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
SAVE_DIR="$(cd -- "$1" && pwd)"
shift

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-reload-check.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

export DUMPER_MAIN="com.hypixel.hytale.tools.golden.WorldStorageProbe"
export DUMPER_EXTRA_ARGS="$SAVE_DIR $*"

"$SCRIPT_DIR/run.sh" "$TMP_DIR/out" "$ROOT/testdata/Default" "$ROOT"
