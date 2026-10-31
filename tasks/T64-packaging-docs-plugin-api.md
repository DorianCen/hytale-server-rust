# T64 — Packaging, docs and plugin/mod API

**Status:** done (session 69)
**Package:** docs, `hytale-server`, `hytale-builtin`, `tools`, `deploy`, CI
**Depends on:** T60, T63
**Agent fit:** medium

## Goal

Turn the phase-2 server into a deployable release with a documented plugin and
asset-mod contract, without claiming Java plugin compatibility or inventing a
dynamic ABI that the Rust server does not implement.

## Delivered

### Reproducible release artifact

`tools/release.sh`:

- builds `hytale-server` and `client-bot` with `--release --locked`;
- stages binaries, docs and the systemd unit;
- writes `BUILD-INFO.txt` (version, target, git revision, toolchain,
  `SOURCE_DATE_EPOCH`);
- creates a sorted, fixed-mtime, `gzip -n` archive and a SHA-256 checksum.

Verified twice on this host: both runs produced
`a7c7790728eb89e67987268ce8dc3ec59ae27ce1949f87a6e27df8741f41c6d2` for the
same checkout. The archive contains:

```text
bin/hytale-server
bin/client-bot
share/doc/{PACKAGING,PLUGINS,ASSET_MODS,TESTING}.md
share/deploy/hytale-server.service
BUILD-INFO.txt
```

CI now has a `release` job that builds twice, compares hashes, verifies the
checksum/content and uploads the archive.

### Deployment docs and service

- `PACKAGING.md` — supported hosts, build commands, artifact layout,
  reproducibility, universe/asset layout, systemd, network, auth, upgrades,
  rollback and verification.
- `deploy/hytale-server.service` — unprivileged, write-confined systemd unit
  with restart and file-descriptor limits.
- `deploy/README.md` — install, validation, firewall and upgrade steps.
- `TESTING.md` — `tools/release.sh` and the reproducibility gate.

### Rust plugin API

- `PLUGINS.md` documents the compiled-in `BuiltinModule`/`BuiltinHost` API,
  manifest fields, lifecycle ordering, embedding, limitations and a Java
  migration table.
- `crates/hytale-builtin/examples/hello_plugin.rs` is a runnable custom plugin.
- `crates/hytale-builtin/tests/custom_plugin.rs` regression-tests manifest
  validation, deterministic start order, hooks, bad manifests and reverse
  shutdown.

The documented API is deliberately compile-time only. There is no Java
classloader, Java jar support, dynamic-library ABI or plugin command/world
context yet. Plugins that need those hooks stay follow-ups rather than being
claimed as working.

### Asset-mod support and docs

- `ASSET_MODS.md` documents the supported single-pack model, directory and zip
  shapes, merge workflow, `Parent`/reference rules, validation and limits.
- The server now accepts an extracted directory as `--assets-pack` for the world
  path (previously only `--validate-assets` accepted it). This makes the
  documented merge-a-directory workflow runnable.
- `an_extracted_asset_pack_can_boot_a_world` is the asset-backed regression.
- Runtime overlay stacking, hot reload and manifest-driven mod discovery are
  explicitly documented as not implemented. Asset mods are data/mods, not Java
  code plugins.

## Acceptance

```sh
tools/release.sh
(cd dist && sha256sum -c hytale-server-*.tar.gz.sha256)
cargo test -p hytale-builtin --test custom_plugin
cargo run -p hytale-builtin --example hello_plugin
HYTALE_TEST_ASSETS=1 cargo test -p hytale-server --test world_thread an_extracted_asset_pack_can_boot_a_world
HYTALE_TEST_ASSETS=1 cargo test -p hytale-server --test world_thread config_assets_are_cached_per_world
./tools/test.sh fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Deviations

- No Java plugin compatibility is claimed or implemented. Java jars are not
  discovered or classloaded; `PLUGINS.md` states this explicitly.
- The plugin API is compile-time and lifecycle/runtime-switch oriented. A
  world/command/event plugin context and a dynamic/WASM ABI remain follow-ups.
- Asset mods are applied by merging into one directory/zip pack. Runtime
  overlay stacking is not implemented; `ASSET_MODS.md` gives the supported
  workflow and the exact limitations.
- The release archive is reproducible on the same host/toolchain. Bit-identical
  binaries across different hosts still depend on compiler/linker environment
  and are not promised by the script.

## Follow-ups

- A stable plugin context exposing world/command/event hooks.
- Runtime asset overlay discovery and mod manifests.
- Cross-platform installers for Windows/macOS; the current unit/docs cover
  Linux deployment and build-from-source paths for the others.
