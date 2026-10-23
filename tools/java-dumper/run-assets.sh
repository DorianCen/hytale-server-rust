#!/usr/bin/env bash
#
# Regenerates the asset-store golden data (testdata/golden/server/assets/)
# with the real CoreServer asset pipeline.
#
# Usage:
#   tools/java-dumper/run-assets.sh                     # phase-2 set -> committed dir
#   tools/java-dumper/run-assets.sh /tmp/assets-golden   # arbitrary output dir
#   tools/java-dumper/run-assets.sh --check              # run twice + diff committed
#
#   --priority P0[,P1,...]   select stores by priority
#   --stores A,B,C           select explicit stores by class name
#   --closure                include the transitive loadsAfter closure
#
# The default is the phase-2 set: P0 + P1 + the P2/P3 roots that feed their
# indexes, plus that root set's dependency closure. `--check` only supports the
# default set because it compares the complete committed directory.
#
# Environment:
#   ASSET_PACK=Assets/Assets.zip   the pack (relative to the repo root or absolute)
#   DUMPER_PARALLELISM=1           the canonical single-threaded decode
#
# This is a thin wrapper around run.sh: it only selects the AssetDumper entry
# point, so the build (upstream jars, classpath) is shared with the worldgen
# dump. The output directory is the first positional argument; the world
# directory is ignored by AssetDumper.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
COMMITTED="$ROOT/testdata/golden/server/assets"
MAIN="com.hypixel.hytale.tools.golden.AssetDumper"

usage() {
    sed -n '3,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

CHECK=0
OUT_DIR=""
EXTRA_ARGS=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --check)
            CHECK=1
            shift
            ;;
        --priority)
            [ "$#" -ge 2 ] || { echo "error: --priority needs a value" >&2; exit 2; }
            EXTRA_ARGS+=("--priority" "$2")
            shift 2
            ;;
        --priority=*)
            EXTRA_ARGS+=("--priority" "${1#*=}")
            shift
            ;;
        --stores)
            [ "$#" -ge 2 ] || { echo "error: --stores needs a value" >&2; exit 2; }
            EXTRA_ARGS+=("--stores" "$2")
            shift 2
            ;;
        --stores=*)
            EXTRA_ARGS+=("--stores" "${1#*=}")
            shift
            ;;
        --closure)
            EXTRA_ARGS+=("--closure")
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            echo "error: unknown option: $1" >&2
            exit 2
            ;;
        *)
            if [ -n "$OUT_DIR" ]; then
                echo "error: more than one output directory: $1" >&2
                exit 2
            fi
            OUT_DIR="$1"
            shift
            ;;
    esac
done

if [ "$CHECK" -eq 1 ] && [ "${#EXTRA_ARGS[@]}" -gt 0 ]; then
    echo "error: --check requires the default phase-2 store selection" >&2
    exit 2
fi

# run.sh expands DUMPER_EXTRA_ARGS unquoted; store/priority values contain no spaces.
export DUMPER_EXTRA_ARGS="${EXTRA_ARGS[*]:-}"

if [ "$CHECK" -eq 1 ]; then
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

if [ -z "$OUT_DIR" ]; then
    OUT_DIR="$COMMITTED"
fi
export DUMPER_MAIN="$MAIN"
exec "$SCRIPT_DIR/run.sh" "$OUT_DIR"
