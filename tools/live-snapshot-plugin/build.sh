#!/usr/bin/env bash
#
# Builds the RegistrySnapshot plugin jar against a Hytale server jar.
#
# Usage:
#   tools/live-snapshot-plugin/build.sh [path/to/HytaleServer.jar]
#
# The server jar can also be supplied through HYTALE_SERVER_JAR. The default
# is the launcher install path used by the development machine.
#
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_SERVER_JAR="$HOME/.var/app/com.hypixel.HytaleLauncher/data/Hytale/install/release/package/game/latest/Server/HytaleServer.jar"
SERVER_JAR="${1:-${HYTALE_SERVER_JAR:-$DEFAULT_SERVER_JAR}}"

if [ ! -f "$SERVER_JAR" ]; then
    echo "error: server jar not found: $SERVER_JAR" >&2
    echo "usage: $0 [path/to/HytaleServer.jar]" >&2
    exit 1
fi

OUT_DIR="$SCRIPT_DIR/target/classes"
JAR="$SCRIPT_DIR/target/RegistrySnapshotPlugin.jar"

rm -rf "$SCRIPT_DIR/target"
mkdir -p "$OUT_DIR"

echo "Compiling the snapshot plugin against $SERVER_JAR"
find "$SCRIPT_DIR/src/main/java" -name '*.java' -print0 \
    | xargs -0 javac -cp "$SERVER_JAR" -d "$OUT_DIR"
cp "$SCRIPT_DIR/src/main/resources/manifest.json" "$OUT_DIR/"

echo "Packaging $JAR"
jar --create --file "$JAR" -C "$OUT_DIR" .

echo "Built: $JAR"
