#!/usr/bin/env bash
# Test tiers for the CoreServer rewrite.
#
#   ./tools/test.sh unit          pure in-crate unit tests (no asset pack)
#   ./tools/test.sh fast          unit + integration with asset tests skipped
#   ./tools/test.sh integration   all non-E2E integration tests (asset pack on)
#   ./tools/test.sh assets        fast asset/golden packages
#   ./tools/test.sh slow          worldgen/world-boot/server generation suites
#   ./tools/test.sh e2e           ignored loopback/auth/trace tests, serial
#   ./tools/test.sh all           the full workspace gate
#
# Extra arguments are passed to the runner, e.g.
#   ./tools/test.sh unit -- --nocapture
#
# HYTALE_TEST_ASSETS=0 skips asset-backed tests. The `fast` tier forces it;
# the other tiers default it to 1 but keep an explicit value if one is set.
#
# HYTALE_TEST_RUNNER=auto|nextest|cargo selects the runner. `auto` uses
# cargo-nextest when it is installed, otherwise plain cargo. `all` always uses
# cargo test because it also runs doc tests.

set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# Reuse the decoded asset registry, interaction catalog and encoded config
# burst across world boots in one test process. Set HYTALE_SHARED_ASSETS=0 to
# exercise the per-world path.
export HYTALE_SHARED_ASSETS="${HYTALE_SHARED_ASSETS:-1}"

usage() {
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

tier="${1:-fast}"
if [[ $# -gt 0 ]]; then
    shift
fi

enable_assets() {
    if [[ -z "${HYTALE_TEST_ASSETS+x}" ]]; then
        export HYTALE_TEST_ASSETS=1
    fi
}

runner="${HYTALE_TEST_RUNNER:-auto}"
case "$runner" in
    auto)
        if command -v cargo-nextest >/dev/null 2>&1; then
            use_nextest=true
        else
            use_nextest=false
        fi
        ;;
    nextest)
        if ! command -v cargo-nextest >/dev/null 2>&1; then
            echo "HYTALE_TEST_RUNNER=nextest but cargo-nextest is not installed" >&2
            echo "install it with: cargo install cargo-nextest --locked" >&2
            exit 2
        fi
        use_nextest=true
        ;;
    cargo)
        use_nextest=false
        ;;
    *)
        echo "unknown HYTALE_TEST_RUNNER=$runner (expected auto, nextest or cargo)" >&2
        exit 2
        ;;
esac

run() {
    printf '+'
    printf ' %q' "$@"
    printf '\n'
    "$@"
}

if $use_nextest; then
    base=(cargo nextest run)
else
    base=(cargo test)
fi

case "$tier" in
    unit)
        run "${base[@]}" --workspace --lib "$@"
        ;;
    fast)
        export HYTALE_TEST_ASSETS=0
        if $use_nextest; then
            run "${base[@]}" --workspace --profile fast "$@"
        else
            run "${base[@]}" --workspace "$@"
        fi
        ;;
    integration)
        enable_assets
        if $use_nextest; then
            run "${base[@]}" --workspace --tests --profile default "$@"
        else
            run "${base[@]}" --workspace --tests "$@"
        fi
        ;;
    assets)
        enable_assets
        packages=(
            -p hytale-assets
            -p hytale-entities
            -p hytale-interactions
            -p hytale-world
        )
        if $use_nextest; then
            run "${base[@]}" "${packages[@]}" --tests --profile assets "$@"
        else
            run "${base[@]}" "${packages[@]}" --tests "$@"
        fi
        ;;
    slow)
        enable_assets
        packages=(
            -p hytale-server
            -p worldgen-hytale
            -p worldgen-cli
            -p worldgen-server
        )
        if $use_nextest; then
            run "${base[@]}" "${packages[@]}" --tests --profile slow "$@"
        else
            run "${base[@]}" "${packages[@]}" --tests "$@"
        fi
        ;;
    e2e)
        enable_assets
        if $use_nextest; then
            run "${base[@]}" --workspace --profile e2e --run-ignored ignored-only "$@"
        else
            run "${base[@]}" --workspace -- --ignored --test-threads=1 "$@"
        fi
        ;;
    all)
        enable_assets
        run cargo test --workspace "$@"
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
