#!/usr/bin/env bash
# T62 security gate: fuzz smoke targets + dependency/lockfile audit.
#
#   tools/security.sh                # 100k fuzz iterations, advisory audit if installed
#   HYTALE_FUZZ_ITERATIONS=1000000 tools/security.sh
#
# The fuzz targets are deterministic in-tree tests; a panic is a failure. The
# advisory database checks need `cargo-audit`/`cargo-deny`; when neither is
# installed the script still verifies the lockfile and prints the duplicate
# dependency inventory so the run is auditable.

set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

iterations="${HYTALE_FUZZ_ITERATIONS:-100000}"

echo "+ fuzz budget: ${iterations} iterations"
HYTALE_FUZZ_ITERATIONS="$iterations" cargo test -p hytale-protocol --test fuzz_decoder
HYTALE_FUZZ_ITERATIONS="$iterations" cargo test -p hytale-assets --test fuzz_asset_json

echo "+ dependency lockfile"
cargo metadata --locked --format-version 1 >/dev/null

echo "+ duplicate dependency inventory"
cargo tree --duplicates

if command -v cargo-audit >/dev/null 2>&1; then
    echo "+ cargo audit"
    cargo audit
else
    echo "cargo-audit is not installed; advisory-database scan skipped" >&2
    echo "install it with: cargo install cargo-audit --locked" >&2
fi

if command -v cargo-deny >/dev/null 2>&1; then
    echo "+ cargo deny"
    cargo deny check
else
    echo "cargo-deny is not installed; license/source policy scan skipped" >&2
fi
