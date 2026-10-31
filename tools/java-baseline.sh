#!/usr/bin/env bash
# Reproduce the Java CoreServer baseline used by BENCHMARKS.md (T63).
#
# Requires the shared-source checkout and a built HytaleServer.jar:
#
#   cd .hytale-source/hytale-shared-source/HytaleServer
#   ./mvnw package -DskipTests
#
#   tools/java-baseline.sh
#   HYTALE_JAVA_ASSETS=.hytale-source/hytale-shared-source/HytaleAssets tools/java-baseline.sh
#
# The asset-validation comparison is directory-based. Full boot uses the same
# Assets/Assets.zip pack as the Rust probe.

set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
repo_root="$root"

source_root="${HYTALE_JAVA_SOURCE:-.hytale-source/hytale-shared-source}"
jar="${HYTALE_JAVA_JAR:-$source_root/HytaleServer/dist/HytaleServer/HytaleServer.jar}"
assets_dir="${HYTALE_JAVA_ASSETS:-$source_root/HytaleAssets}"
assets_zip="${HYTALE_JAVA_ASSETS_ZIP:-Assets/Assets.zip}"
out="$repo_root/${HYTALE_PERF_OUT:-target/perf}"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
run_dir="$out/java-run-$stamp"
universe="$run_dir/universe"

if [[ ! -f "$jar" ]]; then
    echo "Java server jar not found: $jar" >&2
    echo "build it with: (cd $source_root/HytaleServer && ./mvnw package -DskipTests)" >&2
    exit 2
fi
if [[ ! -d "$assets_dir" ]]; then
    echo "Java asset directory not found: $assets_dir" >&2
    exit 2
fi
if [[ ! -f "$assets_zip" ]]; then
    echo "Rust comparison asset pack not found: $assets_zip" >&2
    exit 2
fi
jar="$(readlink -f "$jar")"
assets_dir="$(readlink -f "$assets_dir")"
assets_zip="$(readlink -f "$assets_zip")"

mkdir -p "$out" "$universe"
cd "$run_dir"

echo "+ Java asset validation (directory assets)"
python3 - "$jar" "$assets_dir" <<'PY' | tee "$out/java-validate-$stamp.log"
import resource, subprocess, sys, time
jar, assets = sys.argv[1], sys.argv[2]
cmd = ["java", "-jar", jar, "--assets", assets, "--disable-sentry",
       "--disable-file-watcher", "--validate-assets", "--shutdown-after-validate"]
start = time.perf_counter()
result = subprocess.run(cmd)
elapsed = time.perf_counter() - start
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
print(f"JAVA_VALIDATE_EXIT={result.returncode}")
print(f"JAVA_VALIDATE_WALL_SECONDS={elapsed:.3f}")
print(f"JAVA_VALIDATE_MAXRSS_KIB={usage.ru_maxrss}")
PY

echo "+ Java boot to ready (zip assets, offline, boot-command stop)"
python3 - "$jar" "$assets_zip" "$universe" <<'PY' | tee "$out/java-boot-$stamp.log"
import resource, subprocess, sys, time
jar, assets, universe = sys.argv[1], sys.argv[2], sys.argv[3]
cmd = ["java", "-jar", jar, "--assets", assets, "--auth-mode", "offline",
       "--universe", universe, "--disable-sentry", "--disable-file-watcher",
       "--boot-command", "stop"]
start = time.perf_counter()
try:
    result = subprocess.run(cmd, timeout=180)
    code = result.returncode
except subprocess.TimeoutExpired:
    code = 124
elapsed = time.perf_counter() - start
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
print(f"JAVA_BOOT_EXIT={code}")
print(f"JAVA_BOOT_WALL_SECONDS={elapsed:.3f}")
print(f"JAVA_BOOT_MAXRSS_KIB={usage.ru_maxrss}")
PY

echo "logs: $out/java-*-$stamp.log"
