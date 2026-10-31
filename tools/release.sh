#!/usr/bin/env bash
# Build and stage a reproducible Rust CoreServer release archive.
#
#   tools/release.sh
#   HYTALE_RELEASE_OUT=/tmp/release tools/release.sh
#   HYTALE_TARGET=x86_64-unknown-linux-gnu tools/release.sh
#
# The archive is deterministic for a fixed source revision, toolchain and
# SOURCE_DATE_EPOCH. It uses GNU tar/gzip options; the primary supported host is
# Linux x86_64.

set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
if [[ -z "$version" ]]; then
    echo "could not read the workspace version from Cargo.toml" >&2
    exit 2
fi

target_dir="${CARGO_TARGET_DIR:-$root/target}"
build_args=(--release --locked)
release_dir="$target_dir/release"
target="${HYTALE_TARGET:-}"
if [[ -n "$target" ]]; then
    build_args+=(--target "$target")
    release_dir="$target_dir/$target/release"
fi
target_name="${target:-$(rustc -vV | sed -n 's/^host: //p')}"
out="${HYTALE_RELEASE_OUT:-$root/dist}"
stage_base="$target_dir/release-package/hytale-server-$version-$target_name"
stage_parent="$(dirname "$stage_base")"
stage="$stage_base"
archive="$out/hytale-server-$version-$target_name.tar.gz"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"

echo "+ building hytale-server and client-bot ($target_name)"
CARGO_INCREMENTAL=0 cargo build "${build_args[@]}" -p hytale-server -p client-bot

rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/doc" "$stage/share/deploy" "$out"
install -m 755 "$release_dir/hytale-server" "$stage/bin/hytale-server"
install -m 755 "$release_dir/client-bot" "$stage/bin/client-bot"
install -m 644 PACKAGING.md PLUGINS.md ASSET_MODS.md TESTING.md "$stage/share/doc/"
install -m 644 deploy/hytale-server.service "$stage/share/deploy/"
install -m 644 deploy/README.md "$stage/share/deploy/README.md"

{
    echo "project=hytale-server-rust"
    echo "version=$version"
    echo "target=$target_name"
    echo "git=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "source_date_epoch=$source_date_epoch"
    echo "rustc=$(rustc --version)"
    echo "cargo=$(cargo --version)"
    echo "built_utc=$(date -u -d "@$source_date_epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo unknown)"
} > "$stage/BUILD-INFO.txt"

echo "+ packaging $archive"
rm -f "$archive" "$archive.sha256"
tar --sort=name \
    --mtime="@$source_date_epoch" \
    --owner=0 --group=0 --numeric-owner \
    -C "$stage_parent" -cf - "$(basename "$stage")" \
    | gzip -n > "$archive"
(
    cd "$out"
    sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256"
)

echo "+ artifact: $archive"
echo "+ checksum: $archive.sha256"
sha256sum "$archive"
