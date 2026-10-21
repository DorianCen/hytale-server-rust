# T09 — Asset pack VFS and Parent resolver

**Status:** done (session 1)
**Agent fit:** medium (self-contained, small fixtures)
**Depends on:** T01
**Owns:** `crates/hytale-assets/src/{lib,pack,parent,order}.rs`,
`crates/hytale-assets/tests/*.rs`,
`crates/hytale-assets/Cargo.toml`

## Goal

Implement the first real asset-layer piece: read a zip or directory as a pack,
resolve `Parent` inheritance on JSON values, and topologically order stores.
Do not port asset classes yet.

## Read

- `AssetStore/src/main/java/com/hypixel/hytale/assetstore/AssetPack.java`
- `AssetStore/src/main/java/com/hypixel/hytale/assetstore/AssetStore.java`
  (load-order fields and `loadsBefore`/`loadsAfter` only)
- `AssetStore/.../codec/AssetCodec.java`,
  `ContainedAssetCodec.java` (inheritance intent only)
- One real asset with a parent, e.g.
  `Assets/extracted/Server/Item/Items/Rock/Aqua/Rock_Aqua.json` if the pack is
  available; otherwise use a small fixture.
- [`PORTING.md`](../PORTING.md) §5

## Work

1. `AssetPack`: open a directory or `.zip`; list files by relative path;
   case-insensitive lookup for JSON keys where the asset maps are
   case-insensitive; reject path traversal.
2. `AssetKey`: the asset id is the file stem by default; support the
   `ContainedAssetCodec` case where a child asset inherits the container's key.
3. `ParentResolver`:
   - load `Parent` chain from a `serde_json::Value` provider;
   - merge object fields recursively, replacing scalars and arrays by default;
   - expose a per-store merge hook (`fn merge(parent, child) -> Value`) so
     stores that append/merge differently can override it;
   - ignore `$Title`, `$Comment`, `$Author`, `$TODO`, and `Parent` in the final
     value;
   - detect cycles and missing parents with the chain in the error.
4. `StoreOrder`: topological sort of store names from `loadsBefore`/`loadsAfter`
   edges; stable tie-break; cycle error lists the cycle.
5. Tests with a tiny fixture tree:
   - child scalar overrides parent;
   - nested object merges;
   - array replacement and a store hook that appends;
   - missing parent/cycle errors;
   - zip and directory produce the same keys/values;
   - topo order respects edges and is stable.

## Out of scope

- No typed assets, no index maps, no packet encoders.
- No full asset pack in tests.
- No storage.

## Acceptance

```sh
cargo test -p hytale-assets
cargo clippy -p hytale-assets --all-targets -- -D warnings
cargo fmt -p hytale-assets -- --check
```

Fixtures live under `crates/hytale-assets/tests/fixtures/` and are small.

## Handoff

Report the merge API, how a store overrides it, and any Java behavior that
needed a manual rule.
