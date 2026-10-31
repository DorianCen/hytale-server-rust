#!/usr/bin/env bash
# T63 performance/soak gate.
#
#   tools/perf.sh
#   HYTALE_PERF_TICKS=500 HYTALE_SOAK_CYCLES=200 tools/perf.sh
#
# Writes tab-separated metrics and the soak log under target/perf/ and prints
# them to stdout. Requires the base-game asset pack.

set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="${HYTALE_PERF_OUT:-target/perf}"
mkdir -p "$out"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
ticks="${HYTALE_PERF_TICKS:-200}"
cycles="${HYTALE_SOAK_CYCLES:-100}"

{
    echo "timestamp=$stamp"
    echo "uname=$(uname -a)"
    echo "rustc=$(rustc --version)"
    echo "cargo=$(cargo --version)"
    if command -v lscpu >/dev/null 2>&1; then
        lscpu | sed -n 's/^Model name:[[:space:]]*/cpu=/p; s/^CPU(s):[[:space:]]*/cpus=/p'
    fi
    if command -v free >/dev/null 2>&1; then
        free -b | sed -n 's/^Mem:[[:space:]]*/memory=/p'
    fi
    echo "ticks=$ticks"
    echo "soak_cycles=$cycles"
} | tee "$out/env-$stamp.txt"

echo "+ release perf probe"
HYTALE_PERF_TICKS="$ticks" cargo run --release -p hytale-server --example perf \
    | tee "$out/perf-$stamp.tsv"

echo "+ release soak"
HYTALE_SOAK_CYCLES="$cycles" cargo test --release -p hytale-server --test soak -- \
    --ignored --nocapture \
    | tee "$out/soak-$stamp.log"
