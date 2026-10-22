#!/usr/bin/env bash
#
# Builds the golden dumper and regenerates testdata/golden/.
#
# The Java Hytale sources under .hytale-source/ are read-only reference material.
# We package their already-compiled target/classes trees into small jars under
# tools/java-dumper/lib/ because Maven's system-scope dependencies require files.
#
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
SERVER_ROOT="$ROOT/.hytale-source/hytale-shared-source/HytaleServer"
LIB_DIR="$SCRIPT_DIR/lib"
# Optional first argument: the output directory (default: testdata/golden).
# The world directory and the repository root are passed explicitly so an
# arbitrary output directory (e.g. a determinism-check temp dir) works.
OUT_DIR="${1:-$ROOT/testdata/golden}"
WORLD_DIR="$ROOT/testdata/Default"
# The asset pipeline's common fork-join parallelism. The default of 1 is the
# canonical golden-dump setting; override it (e.g. DUMPER_PARALLELISM=16) to
# verify that the dumped ids do not depend on the decode scheduling.
PARALLELISM="${DUMPER_PARALLELISM:-1}"
# The entry point (default: the golden dumper). Set it to
# com.hypixel.hytale.tools.golden.RegistrySnapshot to exercise the live-server
# snapshot writer standalone.
MAIN_CLASS="${DUMPER_MAIN:-com.hypixel.hytale.tools.golden.GoldenDumper}"
# Optional loader seed-label override for the chunk dump (see ChunkDumper).
LOADER_SEED_OPT=""
if [ -n "${DUMPER_LOADER_SEED:-}" ]; then
    LOADER_SEED_OPT="-Dhytale.dumper.loaderSeed=${DUMPER_LOADER_SEED}"
fi

if [ ! -d "$SERVER_ROOT" ]; then
    echo "error: Java source root not found: $SERVER_ROOT" >&2
    exit 1
fi

mkdir -p "$LIB_DIR" "$OUT_DIR"

build_module_jar() {
    local module="$1"
    local name="$2"
    local classes="$SERVER_ROOT/$module/target/classes"
    local jar="$LIB_DIR/hytale-$name.jar"

    # A module is (re)compiled when its class tree is missing or when it
    # contains a class that the upstream build could not resolve (the upstream
    # build produced an "Unresolved compilation problem" stub for such a
    # class). Maven's incremental check would consider the stale stub newer
    # than its source, so the class tree is removed first to force a rebuild.
    if [ ! -d "$classes" ] || [ -z "$(find "$classes" -name '*.class' -print -quit)" ]; then
        echo "Compiling $module..."
        mvn -q -f "$SERVER_ROOT/pom.xml" -pl "$module" -am compile
    elif has_unresolved_classes "$classes"; then
        echo "Removing stale unresolved classes in $module..."
        rm -rf "$classes"
        echo "Compiling $module..."
        mvn -q -f "$SERVER_ROOT/pom.xml" -pl "$module" -am compile
    fi

    echo "Packaging $module classes -> ${jar#"$ROOT/"}"
    jar --create --file "$jar" -C "$classes" .
}

# True when the class tree contains a class the upstream build could not
# resolve (the Eclipse compiler emits an error stub for those). The stub text
# is "Unresolved compilation problem: " (singular), so match that prefix.
has_unresolved_classes() {
    grep -rla --include='*.class' "Unresolved compilation problem" "$1" 2>/dev/null | head -n 1 | grep -q .
}

build_module_jar Math Math
build_module_jar ProceduralLibrary ProceduralLibrary
build_module_jar WorldGenerator WorldGenerator

# The registry dumper boots the real CoreServer asset pipeline (the vanilla
# AssetRegistryLoader), so the asset store classes and their module
# dependencies have to be on the classpath as well.
build_module_jar AssetStore AssetStore
build_module_jar Codec Codec
build_module_jar Common Common
build_module_jar Component Component
build_module_jar Protocol Protocol
build_module_jar CoreServer CoreServer
build_module_jar Logger Logger
build_module_jar Event Event
build_module_jar SneakyThrow SneakyThrow
build_module_jar Registry Registry
build_module_jar Storage Storage
build_module_jar Unsafe Unsafe
build_module_jar FunctionalInterfaces FunctionalInterfaces
build_module_jar Metrics Metrics
build_module_jar libs/ConcurrentFastUtil ConcurrentFastUtil

# The built-in plugins register the codec variants of the block tick
# procedures and of the farming stages (both are BlockType fields).
build_module_jar builtin/BlockTick BlockTick
build_module_jar builtin/RandomTick RandomTick
build_module_jar builtin/adventure/Farming Farming
build_module_jar builtin/adventure/WorldLocationCondition WorldLocationCondition
build_module_jar builtin/FallingBlocks FallingBlocks

# Prefer the exact versions declared in pom.xml, but resolve from the local repository
# so the run does not depend on the exec plugin being installed.
find_jar() {
    local group_path="$1"
    local artifact="$2"
    local jar
    jar="$(find "$HOME/.m2/repository/$group_path" -name "$artifact-*.jar" ! -name '*-sources.jar' ! -name '*-javadoc.jar' 2>/dev/null \
        | sort -V | tail -n 1)"
    if [ -z "$jar" ]; then
        echo "error: could not find $artifact jar in $HOME/.m2/repository/$group_path" >&2
        exit 1
    fi
    printf '%s\n' "$jar"
}

GSON_JAR="$(find_jar 'com/google/code/gson/gson' 'gson')"
JOML_JAR="$(find_jar 'org/joml/joml' 'joml')"
FASTUTIL_JAR="$(find_jar 'it/unimi/dsi/fastutil' 'fastutil')"
SENTRY_JAR="$(find_jar 'io/sentry/sentry' 'sentry')"

# Runtime dependencies of the CoreServer asset pipeline.
BSON_JAR="$(find_jar 'org/mongodb/bson' 'bson')"
JOPT_JAR="$(find_jar 'net/sf/jopt-simple' 'jopt-simple')"
JLINE_JAR="$(find_jar 'org/jline/jline' 'jline')"
JSR305_JAR="$(find_jar 'com/google/code/findbugs/jsr305' 'jsr305')"
ERRORPRONE_JAR="$(find_jar 'com/google/errorprone' 'error_prone_annotations')"
FLOGGER_JAR="$(find_jar 'com/google/flogger' 'flogger')"
FLOGGER_BACKEND_JAR="$(find_jar 'com/google/flogger' 'flogger-system-backend')"
TINK_JAR="$(find_jar 'com/google/crypto/tink' 'tink')"
NIMBUS_JAR="$(find_jar 'com/nimbusds' 'nimbus-jose-jwt')"
BCPKIX_JAR="$(find_jar 'org/bouncycastle' 'bcpkix-jdk18on')"
BCPROV_JAR="$(find_jar 'org/bouncycastle' 'bcprov-jdk18on')"
ZSTD_JAR="$(find_jar 'com/github/luben/zstd-jni' 'zstd-jni')"
FASTDOUBLE_JAR="$(find_jar 'ch/randelshofer/fastdoubleparser' 'fastdoubleparser')"
# CachedPacket (used by the chunk-packet dump) wraps its payload in a Netty ByteBuf.
NETTY_BUFFER_JAR="$(find_jar 'io/netty/netty-buffer' 'netty-buffer')"
NETTY_COMMON_JAR="$(find_jar 'io/netty/netty-common' 'netty-common')"

echo "Building dumper..."
if ! mvn -q -o -f "$SCRIPT_DIR/pom.xml" compile; then
    echo "Offline build failed; retrying with network access..." >&2
    mvn -q -f "$SCRIPT_DIR/pom.xml" compile
fi

CP="$SCRIPT_DIR/target/classes"
for module in Math ProceduralLibrary WorldGenerator \
             AssetStore Codec Common Component Protocol CoreServer \
             Logger Event SneakyThrow Registry Storage Unsafe \
             FunctionalInterfaces Metrics ConcurrentFastUtil \
             BlockTick RandomTick Farming FallingBlocks WorldLocationCondition; do
    CP="$CP:$LIB_DIR/hytale-$module.jar"
done
CP="$CP:$GSON_JAR:$JOML_JAR:$FASTUTIL_JAR:$SENTRY_JAR"
CP="$CP:$BSON_JAR:$JOPT_JAR:$JLINE_JAR:$JSR305_JAR:$ERRORPRONE_JAR"
CP="$CP:$FLOGGER_JAR:$FLOGGER_BACKEND_JAR:$TINK_JAR:$NIMBUS_JAR:$BCPKIX_JAR:$BCPROV_JAR:$ZSTD_JAR"
CP="$CP:$FASTDOUBLE_JAR:$NETTY_BUFFER_JAR:$NETTY_COMMON_JAR"

echo "Running ${MAIN_CLASS##*.} -> ${OUT_DIR#"$ROOT/"}"
# The asset pipeline decodes the assets in parallel and collects the decoded
# assets in a ConcurrentHashMap, whose iteration order depends on the order in
# which the parallel decodes finish. A single worker thread of the common fork
# join pool makes that order deterministic (the tasks then run in submission
# order), which is what makes the dumped registry ids reproducible.
java -Djava.util.concurrent.ForkJoinPool.common.parallelism="$PARALLELISM" $LOADER_SEED_OPT \
     -cp "$CP" "$MAIN_CLASS" "$OUT_DIR" "$WORLD_DIR" "$ROOT" ${DUMPER_EXTRA_ARGS:-}

echo "Done."
# Tools such as WorldStorageProbe only print a verdict and write no JSON.
if compgen -G "$OUT_DIR/*.json" > /dev/null; then
    echo "Golden files:"
    ls -lh "$OUT_DIR"/*.json
fi
