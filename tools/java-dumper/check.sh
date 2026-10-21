#!/usr/bin/env bash
#
# Golden-data determinism gate.
#
# Runs the Java golden dumper twice into isolated temporary directories and
# fails when the two runs differ, then compares the fresh dump against the
# committed golden files (testdata/golden/) so a stale or hand-edited file is
# caught as well.
#
# This is the gate described in NEXT_STEPS.md §2; it needs the same
# prerequisites as run.sh (JDK 25, Maven, the Java source tree and, for the
# registry/chunk dumps, Assets/Assets.zip + Assets/extracted).
#
# Usage:
#   tools/java-dumper/check.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hytale-golden-check.XXXXXX")"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

FIRST="$TMP_DIR/first"
SECOND="$TMP_DIR/second"
mkdir -p "$FIRST" "$SECOND"

echo "== run 1/2 =="
"$SCRIPT_DIR/run.sh" "$FIRST"
echo "== run 2/2 =="
"$SCRIPT_DIR/run.sh" "$SECOND"

echo "== comparing the two runs =="
if ! diff -r "$FIRST" "$SECOND"; then
    echo "error: the two golden-dumper runs are not byte-identical (nondeterministic output)" >&2
    exit 1
fi
echo "deterministic: both runs produced byte-identical files"

echo "== comparing the fresh dump with testdata/golden =="
status=0
for file in "$FIRST"/*.json; do
    name="$(basename "$file")"
    if [ ! -f "$COMMITTED/$name" ]; then
        echo "error: $name is missing from testdata/golden" >&2
        status=1
    elif ! cmp -s "$file" "$COMMITTED/$name"; then
        echo "error: testdata/golden/$name differs from a fresh dump" >&2
        status=1
    fi
done
for file in "$COMMITTED"/*.json; do
    name="$(basename "$file")"
    if [ ! -f "$FIRST/$name" ]; then
        echo "error: testdata/golden/$name was not produced by the dumper" >&2
        status=1
    fi
done
if [ "$status" -ne 0 ]; then
    echo "       Review the change, then regenerate with tools/java-dumper/run.sh." >&2
    exit 1
fi
echo "up to date: testdata/golden matches a fresh dump"
