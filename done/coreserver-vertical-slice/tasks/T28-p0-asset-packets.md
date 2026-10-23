# T28 — P0 asset packet encoders (PhysicalMaterial + BlockHitboxes)

**Status:** done (session 18)
**Package:** `hytale-assets`
**Depends on:** T15 (asset registry), T11 (asset goldens)
**Agent fit:** medium

## Goal

Start the asset packet encoders needed by the join/setup flow. This slice lands
the encoder framework and the two P0 stores whose Java ids are deterministic and
whose payloads need no other store indexes: `PhysicalMaterial` and
`BlockBoundingBoxes`. Both are byte-compared against the real Java payloads.

## Deliverables

- `hytale-assets::packets`:
  - `AssetPacket { store, id, payload }` and `AssetPacketError`.
  - `AssetRegistry::physical_material_packet()` → `UpdatePhysicalMaterials`
    (id 87): `Init`, sorted id→`PhysicalMaterial`, `maxId = max + 1`.
  - `AssetRegistry::block_hitbox_packet()` → `UpdateBlockHitboxes` (id 41):
    `Init`, sorted id→`[Hitbox]`, double→float components, `maxId = max + 1`.
  - Missing `Min`/`Max`/axis fields decode as Java's zero vector.
  - Maps use `BTreeMap`, matching `AssetDumper.canonicalizeMaps` ascending key
    order.
- `crates/hytale-assets/tests/asset_packets.rs`:
  - loads the real pack, encodes both packets, compares bytes to
    `testdata/golden/server/assets/PhysicalMaterial.bin` and
    `BlockBoundingBoxes.bin`;
  - decodes each payload with the generated codec and checks ids/max ids/counts
    and representative values.

## Acceptance

```sh
cargo test -p hytale-assets --test asset_packets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Both payloads are byte-identical to Java (12 materials, 259 hitbox assets).
- The generated decoders accept the payloads.

## Deviations

- Only two P0 stores are encoded. `Environment` (id 61) needs the TagPattern
  store's expanded tag indexes; `Fluid` (id 83) needs FluidFX, sound-set,
  particle/texture indexes and tags.
- `BlockType` (40) and `Item` (54) are not encoded: contained-asset ids are
  Rust-owned (documented in `registry`), so their payloads cannot be compared to
  the Java goldens until the Java contained-asset order is modelled.
- No `UpdateType::AddOrUpdate`/`Remove` variants yet; only the init envelopes.
- The packets are not wired into the session/setup flow yet.
- The encoder reads the raw JSON `Value` for these stores rather than introducing
  new typed asset structs.

## Follow-ups

- Add the remaining file-backed stores (`Environment`, `Fluid`) with the child
  store indexes/tags.
- Encode `BlockType`/`Item` from the Rust registry and validate by decode/size
  until the Java order is modelled.
- Build the `WorldSettings`/asset-init sequence and wire it into the session
  state machine.
- Add `AddOrUpdate`/`Remove` generation for asset reloads.
