# Connection-debug session handoff — 2026-10-31

## Objective

Debug the release-client connection to the Rust server. **Only the user runs the
Hytale client.** The agent must never launch it.

## Current status

- The authenticated release client now passes setup and reaches `Playing`.
- It then crashed while preparing the world.
- The first crash was caused by the code-defined `Debug_Model` block arriving
  with an empty model path.
- That preload is fixed, but the client has **not yet been rerun** to confirm the
  crash is gone.

Latest code commit: `c3ae322` (`fix: preload debug block models and send translations`).

## Client evidence

Log:

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_21-20-39_client.log
```

Key lines:

```text
Stage SettingUp -> Playing took 102ms
Missing model asset:  for block Debug_Model
Loading world: PrepareRootInterations: 0ms
PrepareAmbienceFX: 0ms
PrepareInterations: 0ms
PrepareBlockOverlayAtlas: 1ms
PrepareWorldMap: 2ms
```

The log ends immediately after `PrepareWorldMap`, with no managed exception.
The empty `Debug_Model` model path is the only asset warning in the log.

## Fixes landed

- `Debug_Model` preload now has:
  - `Blocks/_Debug/Model.blockymodel`
  - `Blocks/_Debug/Texture.png`
- `Debug_Cube` preload now has all six `BlockTextures/_Debug/*.png` faces.
- `UpdateTranslations` now sends the real `Server/Languages/<language>` catalog
  instead of an empty map, including Java’s `fallback.lang` behavior.
- `serve --help` now lists `--auth-mode authenticated`.
- New regressions:
  - `debug_block_preloads_carry_models_and_textures`
  - `translations_carry_server_catalog_and_language_fallback`

## Verification evidence

```text
./tools/test.sh fast
Summary [ 19.689s] 1551 tests run: 1551 passed, 24 skipped
```

- `HYTALE_TEST_ASSETS=1 cargo test -p hytale-assets` → all pass
- `cargo clippy --workspace --all-targets -- -D warnings` → clean
- `cargo fmt --all -- --check` → clean
- `--validate-assets` → 50 stores, `failures: 0`

## Version comparison (done earlier)

Old Java source used by the rewrite: `.hytale-source/hytale-shared-source`
(`387dc7e0a`, 2026-10-14).
Current Java source: `/home/chome/Projects/hytale-shared-source`
(`bcc393865`, 2026-10-30).

- Wire protocol is **identical**: CRC `3908406161`, build `258`, zero changed
  files under `Protocol/`.
- Rust `Assets/extracted/Server` matches the **old** checkout byte-for-byte on
  all 36,498 common JSON files.
- 0.7.5 changes 30 cosmetic/Halloween documents and adds 4 items; no asset-store
  or `*PacketGenerator` code changed.
- Therefore the version gap is not a protocol-level incompatibility. See
  `JAVA_VERSION_DIFF.md` and `tools/compare-java-versions.py`.

## What to do next

1. User reruns the client against commit `c3ae322` or newer.
2. Capture the new client log.
3. If it passes `PrepareWorldMap`, report the next failing stage or the first
   warning/error.
4. If it crashes again after the same point, inspect whether the next
   client-side `Prepare*` stage dereferences another preload or missing asset.
5. Do not speculate about protocol mismatch; protocol is proven identical.

## Commands

Create the world:

```bash
cargo run --release -p hytale-server -- universe --root target/universe create RealTest
cargo run --release -p hytale-server -- universe --root target/universe default RealTest
```

Launch:

```bash
cargo run --release -p hytale-server -- serve \
  --universe target/universe \
  --world RealTest \
  --auth-mode authenticated \
  --trace target/trace/RealTest.jsonl
```

Server errors are printed as `session error: ...`. The trace records packet
ids, channels, order and payload hashes.

## Important files

- `crates/hytale-assets/src/registry.rs` — block preloads (Debug_Model/Debug_Cube)
- `crates/hytale-assets/src/translations.rs` — `.lang` parser/catalog
- `crates/hytale-server/src/session.rs` — auth/setup/asset burst
- `crates/hytale-server/src/main.rs` — CLI help text
- `crates/hytale-assets/tests/block_item_packets.rs` — block/config regressions
- `crates/hytale-assets/tests/translations.rs` — translation regression
- `tasks/T66-live-client-assets.md` — current T66 status
- `PORTING_HISTORY.md` — session 72 entry
- `JAVA_VERSION_DIFF.md` — 0.7.2 vs 0.7.5 comparison

## Remaining T66 gaps

- Confirm the `Debug_Model` fix with the release client.
- Interaction bodies are typed for 35 common node types; the remaining long-tail
  types use the base form.
- T66 is **not complete** until the release client reaches and remains in
  `Playing` through world load/spawn without crashing.
