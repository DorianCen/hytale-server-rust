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
OUT_DIR="$ROOT/testdata/golden"

if [ ! -d "$SERVER_ROOT" ]; then
    echo "error: Java source root not found: $SERVER_ROOT" >&2
    exit 1
fi

mkdir -p "$LIB_DIR" "$OUT_DIR"

build_module_jar() {
    local module="$1"
    local classes="$SERVER_ROOT/$module/target/classes"
    local jar="$LIB_DIR/hytale-$module.jar"

    if [ ! -d "$classes" ] || [ -z "$(find "$classes" -name '*.class' -print -quit)" ]; then
        echo "Compiling $module..."
        mvn -q -f "$SERVER_ROOT/pom.xml" -pl "$module" -am compile
    fi

    echo "Packaging $module classes -> ${jar#"$ROOT/"}"
    jar --create --file "$jar" -C "$classes" .
}

build_module_jar Math
build_module_jar ProceduralLibrary
build_module_jar WorldGenerator

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

echo "Building dumper..."
if ! mvn -q -o -f "$SCRIPT_DIR/pom.xml" compile; then
    echo "Offline build failed; retrying with network access..." >&2
    mvn -q -f "$SCRIPT_DIR/pom.xml" compile
fi

CP="$SCRIPT_DIR/target/classes"
CP="$CP:$LIB_DIR/hytale-Math.jar"
CP="$CP:$LIB_DIR/hytale-ProceduralLibrary.jar"
CP="$CP:$LIB_DIR/hytale-WorldGenerator.jar"
CP="$CP:$GSON_JAR:$JOML_JAR:$FASTUTIL_JAR:$SENTRY_JAR"

echo "Running GoldenDumper -> ${OUT_DIR#"$ROOT/"}"
java -cp "$CP" com.hypixel.hytale.tools.golden.GoldenDumper "$OUT_DIR"

echo "Done. Golden files:"
ls -lh "$OUT_DIR"/*.json
