# Porting history — CoreServer rewrite

Append-only log of landed sessions. Do not read this for planning; use
[`NEXT_STEPS.md`](NEXT_STEPS.md) for current work. Read this when debugging a
regression or verifying why a design choice was made.

The worldgen port history is archived in
[`done/PORTING_HISTORY.md`](../world-generation/PORTING_HISTORY.md).

Format:

```text
## Session N — <short title>
- Goal:
- Landed:
- Verified:
- Deviations:
- Follow-ups:
```

---

## Session 0 — planning and documentation (2026-10-21)

- **Goal:** define the CoreServer rewrite scope and prepare session 1.
- **Landed:**
  - `plan.md` — full rewrite plan, milestones M0–M7.
  - `ARCHITECTURE.md` — crate layout, runtime topology, invariants, mapping table.
  - `PORTING.md` — server porting conventions and subagent rules.
  - `REMAINING.md` — deferred scope.
  - `NEXT_STEPS.md` — session 1 must-do/stretch tasks.
  - `GOLDEN_DATA.md` — oracle/fixture strategy.
  - `BENCHMARKS.md` — planned measurement matrix.
  - `tasks/` — task specs T01–T14.
  - Moved the completed worldgen docs to `done/`; fixed links that pointed back
    into `testdata/` and `tools/`.
- **Verified:** no code changed; `git status` shows only documentation moves and
  additions.
- **Deviations:** none.
- **Follow-ups:** session 1 starts with T01, T02, T03, T05, T07, T08.

---

## Session 1 — workspace, protocol, wire, storage, assets (2026-10-21)

- **Goal:** workspace scaffolding, protocol inventory/IR, wire primitives,
  `IndexedStorageFile`, asset-store inventory.
- **Landed:**
  - T01: ten `hytale-*` crates wired into the workspace; `hytale-server` prints
    its version. No unused per-crate dependencies beyond the planned internal
    graph.
  - T02/T03: `tools/protocol-dumper` with `inventory` and `connection` modes.
    Checked-in fixtures in `testdata/golden/server/protocol/`.
  - T05: `hytale-protocol::io` (varint, little-endian primitives, big-endian
    UUIDs, bounded strings/arrays, nullable bitsets) and `::frame` (8-byte
    header, zstd with declared-size checks).
  - T07: `hytale-storage::indexed_storage_file` — v1 header, blob index table,
    first-fit segment allocation, zstd blobs, flush/remove/reopen.
  - T08: `tools/asset-golden-dumper/asset-stores.{json,md}` plus the
    deterministic `extract_stores.py` generator (54 stores, 39 packet
    generators, P0=8/P1=8/P2=21/P3=17).
  - Stretch T09: zip/directory asset VFS, `Parent` resolver with cycle/missing
    errors and per-store merge hooks, stable store topological order.
  - Stretch T10a: `Palette` (`Empty`/`HalfByte`/`Byte`/`Short`) with Java
    promotion thresholds, nibble packing, packet layout (LE), and storage layout
    (BE + key codec); demotion compacts internal ids.
- **Verified:**
  - `cargo test --workspace` green, including the existing worldgen goldens.
  - `cargo clippy --workspace --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
  - Protocol inventory parses 389 packets / 443 generated structs / 175 top-level enums;
    the 445/177 `ProtocolSettings` delta is documented in `meta.discrepancies`.
  - `Connect` IR matches the Java constants and field order; connection IR has
    8 packets and 5 enums.
- **Deviations:**
  - T02 classifies generated interfaces and constant/registry helpers as
    `other` instead of forcing them into `struct`; this is why the struct count
    is 443 rather than the declared 445.
  - The storage task sketch said little-endian, but the Java oracle is
    big-endian (`ByteBuffer` default). A real Java-produced fixture,
    `testdata/golden/server/storage/java-sample.region.bin`, is committed and
    the port/tests use big-endian; this was caught by the fixture, not by a Rust
    round-trip.
- **Follow-ups:**
  - T04 connection codegen is next; `connection.json` is its input. Nested
    `HostAddress`/`InstantData` are simple, while `FormattedMessage`/`PlayerSkin`
    need either recursive struct parsing or a curated common-struct set.
  - T06 QUIC transport spike is still open.
  - Session 2: T04, T09 asset stores from the inventory, T10b `BlockSection`,
    T10c `ChunkColumn::from_generated_chunk`, then T11/T12 goldens.

---

## Session 3 — full protocol codegen and chunk-packet goldens (2026-10-22)

- **Goal:** finish the wire layer (all packets, registry, settings) and capture
  the Java chunk network payloads that T10b/T10c must match.
- **Landed:**
  - T14: `protocol-dumper all` emits `testdata/golden/server/protocol/protocol.json`
    (389 packets, 457 struct entries: 443 concrete structs + 13 union bases + one packet reused as a nested struct, 175 top-level enums,
    `ProtocolSettings`). It handles inheritance (flattened onto concrete
    subtypes), bit-packed booleans, fixed arrays (`SetColumn.heightmap`), maps
    with string/integer/float/enum/struct keys, maps/arrays of structs, nested
    maps, JOML vector interfaces and packet classes used as nested wire types.
  - T14: `protocol-gen all` emits one module per packet family, `common.rs`
    (all structs/unions/enums), `wire.rs` (traits + vector helpers) and
    `registry.rs` (`PacketInfo`, `packet_info`, `validate_packet`,
    `ProtocolSettings`, generated round-trip helpers). Cycle detection boxes
    only the fields/variants that need indirection.
  - T14: `crates/hytale-protocol/tests/registry.rs` checks 389 entries, unique
    ascending ids, every family, header rejection and `ProtocolSettings`;
    `tests/full.rs` covers bit-packed booleans, maps of arrays of structs,
    numeric/enum/float map keys, arrays of maps, fixed arrays and JOML vectors.
    Every packet's default value round-trips byte-for-byte.
  - T12: `ChunkPacketDumper` + `run-chunks.sh` produce
    `testdata/golden/server/chunks-pkts/` for seed 0, chunks `(0,0)` and
    `(4,-5)`: 46 uncompressed payloads (569,520 bytes) plus a manifest and
    README. `run-chunks.sh --check` passes two isolated runs and the committed
    fixture.
- **Verified:**
  - `cargo test --workspace` green, including all existing worldgen goldens.
  - `cargo clippy --workspace --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
  - `cargo run -p protocol-gen -- --check` is clean (full regeneration).
  - `protocol.json` validates: 389 packets, no duplicate ids, every field has a
    fixed offset or variable order.
  - `tools/java-dumper/run-chunks.sh --check` is byte-identical across runs and
    matches the committed chunk packets.
- **Deviations:**
  - The IR has 457 struct entries: 443 concrete structs, 13 abstract union bases
    and the `CameraShakeEffect` packet (also a nested field type);
    `ProtocolSettings.STRUCT_COUNT` is 445.
    The registry does not depend on the declared count, and the delta is
    documented in the protocol fixture README.
  - 175 top-level enums are parsed; the declared 177 includes the nested
    `PacketRegistry.PacketDirection` and schema bookkeeping.
  - The Java generator drops a trailing `Value` for `defaultValue` nullable
    getters (`hasDefault`); the dumper handles both spellings.
  - `SetChunkTintmap` and `SetFluids` are captured from the real Java
    serializers but CoreServer's initial load-query systems do not send them;
    the manifest marks them `sentAtInit: false` and lists them in
    `packetsNotSentAtInit`.
  - `run.sh` now puts netty-buffer/netty-common on the runtime classpath because
    `CachedPacket` uses `io.netty.buffer.Unpooled`.
  - Float-keyed maps are represented as `Vec<(f32, V)>` because `f32` is not
    `Ord`; other supported key types use `BTreeMap`.
- **Follow-ups:**
  - Consume `protocol.json` in the asset packet encoders and compare against
    `testdata/golden/server/assets/*.bin`.
  - Add a Rust chunk-packet test that regenerates the two T12 chunks and
    byte-compares the committed payloads (filtering on `sentAtInit` for strict
    join parity).
  - Asset registry / typed stores and BSON world storage are the next slice.

---

## Session 2 — codegen, transport, runtime chunks, client bot (2026-10-22)

- **Goal:** turn the session-1 foundations into the first end-to-end wire and
  runtime-chunk slice.
- **Landed:**
  - T04: `tools/protocol-gen` (connection mode, `--check`) and generated
    `hytale-protocol/src/generated/{mod,wire,common,connection}.rs`. The
    generator consumes `connection.json`, resolves nested structs/unions
    (`HostAddress`, `InstantData`, `FormattedMessage`, `PlayerSkin`,
    `ParamValue` and its subtypes), and emits `Wire`/`Packet` impls.
  - Protocol-dumper connection IR now includes the reachable struct/union
    closure, maps, arrays of structs, and absolute nullable bit indexes.
  - T06: `hytale-net` QUIC mTLS spike with ALPN `hytale/3`, injected
    certificate-policy seam, `PeerIdentity` fingerprints, example and ignored
    loopback test.
  - T10b: `BlockSection`, `FluidSection`, `EnvironmentSection`, and
    `LightData` (octree storage/packet layout) with storage and packet
    round-trips.
  - T10c: `ChunkColumn`, lazy 10-section allocation, tint/environment
    columns, flags/heightmap placeholders, and lossless
    `ChunkColumn::from_generated_chunk`.
  - T13: `tools/client-bot` with `--dry-run`, framed `Connect`, packet logging,
    and an ignored mock-server round-trip.
  - T11: `AssetDumper` + `run-assets.sh` produce the P0 store init packets and
    `index.json` under `testdata/golden/server/assets/` (6 payloads, 8.5 MB;
    9.2 MB total fixture). Protocol dictionaries are canonicalized before
    serialization so repeated JVM runs are byte-identical.
  - `GoldenDumper` now writes `registries.json` before the world-mask dumps.
    The asset store iteration order depends on class-initialization order, so
    the old order changed block ids after `Phase2Dumper` booted the world. The
    reorder keeps the committed registries and all chunk goldens reproducible.
- **Verified:**
  - `cargo test --workspace` green, including all existing worldgen goldens.
  - `cargo clippy --workspace --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
  - `cargo run -p protocol-gen -- --check` is clean.
  - `cargo test -p hytale-net -- --ignored`, `cargo test -p client-bot --
    --ignored`, and `cargo run -p hytale-net --example quic_spike` pass.
  - `tools/java-dumper/run-assets.sh --check` passes: two isolated runs are
    byte-identical and match the committed `testdata/golden/server/assets/`.
  - `tools/java-dumper/check.sh` passes: both full runs are byte-identical and
    match every committed `testdata/golden/*.json`, including `registries.json`,
    `loader.json`, and `chunks.json`.
- **Deviations:**
  - Generated code is formatted by invoking the toolchain `rustfmt` from
    `protocol-gen`; `--check` compares formatted output, so regeneration is
    deterministic.
  - Section storage has no asset-name registry yet. Block external ids use
    `u16` storage keys (the T10a sketch said bytes; real block ids exceed 255),
    fillers use `u16`, rotations use `u8`. Java stores block names as UTF
    strings, so T12 must confirm or replace this choice against a Java fixture.
  - Environment storage writes numeric runtime ids as varints instead of Java
    environment-name strings. Structural modes (uniform/columns/dedup) and
    packet runs match Java; name mapping is part of the asset-registry slice.
  - `ChunkColumn::from_generated_chunk` copies effective environments per
    block rather than reproducing Java's lazy per-component allocation; the
    observable values are preserved.
  - Asset-packet dictionaries are canonicalized by replacing every
    `java.util.Map` in the generated protocol object graph with a
    `LinkedHashMap` in deterministic key order. Values are unchanged; without
    this, enum identity-hash iteration order made `BlockType.bin`/`Item.bin`
    differ between JVM runs.
  - `PrefabListAsset` cannot boot because the dumper does not boot
    `AssetModule`; all 78 failures are recorded in `index.json`, and the one
    dependent block type is absent exactly as in `registries.json`. 37 `Item`
    assets with unresolved interaction references are also recorded.
  - `run.sh` now detects the singular Eclipse "Unresolved compilation problem"
    stub and removes the stale class tree before recompiling. This is build
    output only; the reference sources are untouched.
- **Follow-ups:**
  - T12 chunk-packet goldens.
  - T14 full protocol codegen (389 packets, registry, `ProtocolSettings`).
  - T12 should pin the block-key width, light octree bytes, and environment
    name mapping against Java output.

---

## Session 4 — Java-compatible chunk packet encoders (2026-10-22)

- **Goal:** consume the generated protocol types in `hytale-world`, encode the
  initial chunk packet set, and match T12's Java payloads byte-for-byte.
- **Landed:**
  - `hytale-world::chunk::packet` builds `SetColumn`, `SetChunkTintmap`,
    `SetChunk`, `SetChunkEnvironments` and `SetFluids` from a runtime
    `ChunkColumn` with the same field semantics as the CoreServer builders
    (all-air sections send `data = null`, empty local light is still sent, and
    global light stays disabled like `BlockChunk.SEND_GLOBAL_LIGHTING_DATA`).
  - `ChunkColumn` now owns a Java `IntBytePalette` port for the tint map; the
    serialized key order is the generator's first-use order.
  - `ChunkColumn::from_generated_chunk_with_registries` computes the legacy
    heightmap from block opacity (`GetOpacity.FROM_SECTION`: id 0 and missing
    ids are transparent). `Heightmap::Legacy` now stores `i32` so `NO_HEIGHT`
    survives.
  - `worldgen-model::GeneratedChunk` records the first-use order of non-zero
    filler and rotation values per section. Java builds those palettes with
    live `set` calls, so this order is observable in the packet bytes; the
    flat Rust generation buffer had lost it.
  - `Palette` reproduces Java's fastutil iteration order: the generated block
    palette replays the `Int2ShortOpenHashMap` run-count map, and all palette
    serializers walk `Byte2IntOpenHashMap`/`Short2IntOpenHashMap` slots in
    fastutil's descending order (null key first). This also fixes the
    storage-layout entry order, which uses the same maps.
  - `crates/hytale-world/tests/chunk_packets_golden.rs` regenerates seed 0
    chunks `(0,0)` and `(4,-5)`, converts them through the runtime model, and
    byte-compares all 46 committed payloads.
  - Tint/palette unit tests cover the packed layout and first-use ordering.
- **Verified:**
  - `cargo test -p hytale-world` green, including the 46-packet golden test
    with the base-game pack present.
  - `cargo test -p worldgen-model -p worldgen-hytale` green (all existing
    worldgen goldens unchanged).
  - `cargo clippy -p hytale-world -p worldgen-model --all-targets -- -D
    warnings` green.
  - `cargo fmt --all -- --check` green.
- **Deviations:**
  - The Java block palette order comes from fastutil's hash-table slot order,
    not a sorted id order. The port now models the exact table growth
    (`arraySize`/`maxFill`, rehash after `size > maxFill`, null key first,
    descending slot iteration) instead of claiming a Rust-owned order.
  - Filler/rotation palettes use a recorded first-use list rather than carrying
    a full Java palette through world generation. This matches the committed
    Java chunks; a value that appears, disappears and reappears across a
    promotion/demotion would need the live palette to be carried instead.
  - `SetChunk` light uses the CoreServer default (`SEND_LOCAL_LIGHTING_DATA =
    true`, `SEND_GLOBAL_LIGHTING_DATA = false`). A server option for global
    light is not exposed yet.
- **Follow-ups:**
  - Asset registry and typed P0/P1 stores; the heightmap still reads the
    opacity table from the golden registries instead of a loaded pack.
  - BSON world storage and the Java-written region fixture.
  - `SetColumn`/section packets for edited chunks should be re-checked against
    a Java fixture after the asset registry owns the block ids.

---

## Session 5 — P0 asset registry and typed stores (2026-10-22)

- **Goal:** load the P0 asset stores from the unmodified pack, assign numeric
  ids once, expose the typed fields the world generator/heightmap need, and
  bridge them into `worldgen-model`.
- **Landed:**
  - `hytale-assets::registry` with `AssetRegistry::load_p0`, `StoreSpec`/
    `MapKind`, `AssetCollection`/`AssetRecord`, preloaded assets and the P0
    store specs.
  - `AssetPack::list_in_storage_order` (zip central-directory order) and
    `read_many` (one zip open per store instead of one per file).
  - `MergeHook::Shallow`/`merge_shallow`: Java `AssetBuilderCodec` field
    replacement (child field replaces the inherited field; absent fields are
    inherited). The existing recursive `Replace` remains available for stores
    that document it.
  - Id assignment reproduces the Java file-backed load order: preloads, reverse
    zip walk, non-parented first batch, then parented waves in Java
    `ConcurrentHashMap` slot/insertion order with the table capacity fixed by
    the original waiting count. `BlockBoundingBoxes`, `Fluid`, `Environment`
    and `PhysicalMaterial` ids match `assets/index.json` exactly.
  - `BlockType` extraction from item documents, including item `State` keys and
    nested `State.Definitions` keys (`*`/`**` prefixes), string definitions as
    references, and item-parent inheritance for contained block types.
  - Typed `BlockTypeAsset` (draw type, opacity, material, support map) with all
    64 `RotationTuple` support probes, including `MergedBlockFaces` aliases,
    `RequiredBlockFaceSupport.Rotate`, Java's plant default support and the
    `@Tech` exclusion. Typed `FluidAsset` (`MaxFluidLevel`).
  - `hytale-world::registry::registries_from_assets`: asset snapshot →
    `worldgen-model::registry::Registries`, preserving names, ids, draw type,
    opacity and rotated support.
  - Fixture tests for loading/ids/inheritance/item-state extraction and
    real-pack gates for exact simple-store ids and every `registries.json`
    block's semantics.
- **Verified:**
  - `cargo test -p hytale-assets` green, including the real-pack registry tests.
  - `cargo test -p hytale-world` green, including the 46-packet chunk golden.
  - `cargo clippy -p hytale-assets -p hytale-world --all-targets -- -D warnings`
    green.
  - `cargo fmt --all -- --check` green.
  - Real pack: all 6135 golden block names resolve; draw type, opacity and all
    64 support probes match `registries.json` for every one; no golden key is
    missing. The only extras are the known Java load-failure keys
    (`ExamplePrefabPlacementBlock_Tree`, two `PortalKey_*` blocks).
- **Deviations:**
  - `BlockType`/`Item` numeric ids are Rust-owned. Java groups contained assets
    through `HashMap`s in `AssetStore.loadContainedAssets`; those ids are not a
    stable content order (the golden README documents schedule-dependent
    sibling swaps). The Rust snapshot matches all names/fields and keeps one
    consistent id table for generation, packets and storage. The first five
    block ids (preloads + `Debug_Test_Block`) still match Java.
  - Asset packet bytes for `BlockType`/`Item` therefore cannot be compared to
    the Java goldens until the Java contained-asset order is modelled or the
    comparison accepts the Rust id order.
- **Follow-ups:**
  - Wire `registries_from_assets` into the headless server boot and the
    `ChunkColumn::from_generated_chunk_with_registries` path.
  - P1 stores and the 39 asset packet encoders.
  - BSON world storage and the Java-written region fixture.

---

## Session 6 — BSON chunk storage and the Java world fixture (2026-10-22)

- **Goal:** persist runtime chunk columns in the vanilla Java world layout and
  prove the codec against a Java-written `IndexedStorage` save.
- **Landed:**
  - `hytale-world::chunk::storage`:
    - `ChunkCodec` encodes/decodes the Java `ChunkStore` holder document
      (`Components` → `WorldChunk`, `BlockChunk`, `ChunkColumn` sections,
      `HeightmapColumn`).
    - Section storage uses the Java layouts: block/fluid/environment palette
      keys are asset names, fillers use big-endian `u16` keys, rotations use
      `u8`; block ticking, both light blobs, change counters and the sky-lit
      stamp are preserved.
    - `BlockChunk.Data` preserves `needsPhysics` and the tint palette.
    - Legacy heightmaps encode/decode the packed 10-bit `ShortBytePalette`
      (all 1024 columns, including the last packed value).
    - Region helpers: `region_coords`, `chunk_blob_index`,
      `region_file_name`, `region_path`, and `RegionFile` (create/open/read/
      write/remove/flush over `IndexedStorageFile`, 1024 blobs by default).
  - `ChunkColumn` gained `needs_physics` and tint-palette accessors.
  - `palette::NameLookup` / `NamedKey` and section `serialize_with_*` /
    `deserialize_with_*` methods. The numeric defaults remain for tests that do
    not load assets; Java-compatible storage uses the named path.
  - `EnvironmentSection` storage gained `serialize_with_lookup` /
    `deserialize_with_lookup`; the existing dedup decoder is reused.
  - `StorageDumper` + `run-storage.sh` produce
    `testdata/golden/server/storage/java-world/`: two real Java region files for
    seed-0 chunks `(0,0)` and `(4,-5)` plus a manifest. The dumper calls
    `ChunkStore.REGISTRY.serialize(holder)` and `BsonUtil.writeToBytes`.
  - `crates/hytale-world/tests/chunk_storage.rs` decodes both chunks and checks
    every block, filler, rotation, fluid level, environment, the tint and the
    legacy heightmap against the Rust-generated chunk; a second test round-trips
    a runtime column through BSON and an on-disk region file.
- **Verified:**
  - `cargo test -p hytale-world` green, including the 46 chunk-packet goldens,
    the new storage fixture gate and the asset-free unit tests.
  - `cargo clippy -p hytale-world --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
  - `tools/java-dumper/run-storage.sh --check` green: two isolated runs are
    byte-identical and match the committed fixture.
- **Deviations:**
  - `BlockChunk.Data` is decoded for `needsPhysics` and tint, but the legacy
    `EnvironmentChunk` component is not part of the save (Java never writes it);
    `ChunkColumn::environment_column` is not persisted and decodes as `Unknown`.
    Per-block environments come from the section components.
  - The Java fixture still uses the golden registry ids for generated blocks
    (the generator loads `registries.json` internally). The storage codec
    resolves names back through whichever `NameLookup` the caller supplies;
    the fixture gate uses the golden lookup, while the runtime path uses the
    asset registry. Both paths agree on names.
  - The storage codec implements only the legacy heightmap backend; a
    `Heightmap::Tree` column returns a documented `Unsupported` error instead
    of writing a non-Java document. The seed-0 fixture uses Legacy.
  - A generated palette's fastutil table order is not stable across a
    save/load cycle (Java rebuilds the map on deserialize), so the Rust
    round-trip test asserts steady-state stability after one load, not
    byte-identical bytes between the first and second save.
  - `ShortKey` now writes big-endian keys; it was little-endian-only before and
    no storage path used it until this session.
- **Follow-ups:**
  - Player BSON matching `DiskPlayerStorageProvider` (`<uuid>.json`, an
    `EntityStore` holder document) and a Java-written player fixture.
  - `World.json`/`WorldConfig` load, and the mutate → Rust save → Java reload
    compatibility check.
  - Use one registry for generation, packets and storage; the generator still
    resolves block ids from `registries.json`.
  - P1 stores and the asset packet encoders.

---

## Session 7 — headless boot with one chunk and a region save (2026-10-22)

- **Goal:** compose the asset registry, worldgen worker, runtime chunk model and
  storage codec into the first real server boot, and use one registry for all
  of them.
- **Landed:**
  - `ChunkGeneratorJsonLoader::with_registries(Rc<Registries>)` and a
    `registries` field. An in-memory snapshot takes precedence over the
    `registries.json` path.
  - `WorldGenOptions.registries: Option<Arc<Registries>>`; each
    `worldgen-server` worker clones the snapshot into its `Rc`-based generator.
    The worldgen CLI keeps using the file-backed option.
  - `hytale-server` now has a library target (`headless`):
    - load the asset pack and P0 registry once;
    - bridge it to `worldgen_model::Registries`;
    - generate one chunk through `WorldGenChunkProvider`;
    - convert it with `ChunkColumn::from_generated_chunk_with_registries`;
    - encode it with `ChunkCodec` and write `chunks/<regionX>.<regionZ>.region.bin`;
    - reopen, decode and compare every block, fluid, environment, tint and the
      heightmap.
  - CLI: `hytale-server headless` with `--assets-pack`, `--assets-root`,
    `--world`, `--save`, `--seed`, `--chunk x,z` and `--workers`. No arguments
    still prints the version.
  - `ChunkCodec::decode_at` (the BSON document has no chunk coordinates);
    `decode` remains the `(0,0)` convenience form.
  - `crates/hytale-server/tests/headless.rs` runs the boot for chunk `(4,-5)`
    and independently decodes the saved region with a fresh registry.
- **Verified:**
  - `cargo run -p hytale-server -- headless --save target/headless-smoke`
    generated seed-0 chunk `(0,0)`: 6138 registry blocks, 98,378 non-air
    blocks, 317,335-byte BSON document, reload verified.
  - `cargo test -p hytale-server --test headless` green (base-game pack
    present).
  - `cargo test -p hytale-world -p hytale-server -p worldgen-server` green.
  - `cargo clippy --workspace --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
- **Deviations:**
  - The headless boot still generates only one seed-0 chunk; the world runtime,
    player spawn and packet stream are later slices.
  - `WorldGenOptions` clones the registry snapshot per worker. The generator
    graph is `Rc`-based, so sharing the `Arc` directly would require an
    `Arc`-based refactor; the snapshot is small relative to the generation
    caches.
  - `WorldGenConfig.name` is fixed to `"Default"` and version to `"0.0.0"` in
    the boot; the loader only logs them. Reading the real `World.json` fields
    is part of the world-config task.
- **Follow-ups:**
  - Load an existing region back into a world runtime (region →
    `ChunkColumn`) and add the mutate → Java-reload gate.
  - Player BSON and `World.json`/`WorldConfig`.
  - Client-bot auth/setup/game with the packet-trace fixture.
  - P1 stores and asset packet encoders.

---

## Session 8 — runtime world storage and the Java reload gate (2026-10-22)

- **Goal:** load a saved chunk into a runtime world, mutate a block, save it back,
  and verify the Rust-written region with the real Java codecs.
- **Landed:**
  - `hytale-world::world`:
    - `WorldStorage` owns the region-file cache for a save directory and maps
      chunk coordinates to `IndexedStorageFile` blobs. It provides
      `load_chunk`/`save_chunk`/`remove_chunk`, `stored_chunk_coordinates` and
      `flush`.
    - `World` keeps the loaded `ChunkColumn`s, tracks dirty chunks and provides
      `load_chunk`, `insert_chunk`, `get_block`/`set_block`,
      `save_chunk`/`save_all` and `flush`.
  - `parse_region_file_name` (Java `fromFileName`) in
    `hytale-world::chunk::storage`, used by the region scan.
  - `hytale-server::world_tool`: loads the P0 registry, loads a stored chunk,
    optionally edits one block, saves, reloads with a fresh `World` and verifies
    the edited block.
  - CLI: `hytale-server load-chunk --assets-pack --save --chunk x,z
    --set-block x,y,z,id`.
  - `crates/hytale-world/tests/world_storage.rs`: storage round-trip plus
    chunk listing/removal, and world load → mutate → save → reload.
    `crates/hytale-server/tests/headless.rs` now also exercises
    load → mutate → save → reload.
  - Java reverse gate:
    - `ChunkStoreBootstrap` extracts the minimal `AssetModule`/`LegacyModule`
      boot and chunk component registration from `StorageDumper`; the dumper
      now reuses it.
    - `WorldStorageProbe` opens the Rust region with the real
      `IndexedStorageFile`, `BsonUtil.readFromBuffer`,
      `ChunkStore.REGISTRY.deserialize`, `ChunkColumn`/`BlockSection` and
      resolves the block through the real `BlockType` asset map.
    - `run-reload-check.sh <save-dir> <chunkX> <chunkZ> <blockX> <blockY>
      <blockZ> <expected-name>`.
    - `run.sh` accepts `DUMPER_EXTRA_ARGS`; the final JSON listing is now
      conditional so probe-only entry points exit successfully.
- **Verified:**
  - `cargo run -p hytale-server -- headless --save target/headless-smoke
    --chunk 4,-5` generated and saved the chunk.
  - `cargo run -p hytale-server -- load-chunk --save target/headless-smoke
    --chunk 4,-5 --set-block 129,250,-159,4` edited and reload-verified it.
  - `tools/java-dumper/run-reload-check.sh target/headless-smoke 4 -5 129 250
    -159 Debug_Test_Block` printed
    `chunk (4, -5) block (129, 250, -159) = 4 (Debug_Test_Block)` and
    `java reload check: ok`.
  - `tools/java-dumper/run-storage.sh --check` remains deterministic and matches
    the committed fixture after the bootstrap refactor.
  - `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
    warnings` and `cargo fmt --all -- --check` are green.
- **Deviations:**
  - `WorldStorage::remove_chunk` leaves an empty region file in place; region
    compaction/unlink is deferred.
  - `stored_chunk_coordinates` opens a separate read-only region handle while
    the write cache may hold the same file open. This is fine on Linux; Windows
    exclusive-handle behavior is untested.
  - No saved-world generation caching yet: the runtime loads stored chunks but
    does not generate missing neighbours.
  - `World.json`/`WorldConfig` and player BSON are still absent.
- **Follow-ups:**
  - `World.json`/`WorldConfig` load and `DiskPlayerStorageProvider` player BSON.
  - Generate-missing-chunks path on top of `World`.
  - Client-bot auth/setup/game with the packet-trace fixture.
  - P1 stores and asset packet encoders.

---

## Session 9 — world config and the Java config gate (2026-10-22)

- **Goal:** load and write the per-world `config.json` (`WorldConfig` v4) with
  the real Java codec as the oracle, completing the saved-world metadata half
  next to the chunk regions.
- **Landed:**
  - `hytale-world::world_config`:
    - `WorldConfig` models the v4 fields in codec order: `Version`, `UUID`,
      `DisplayName`, `Seed`, providers, `ChunkConfig`, world flags, `GameTime`,
      `ClientEffects`, `RequiredPlugins`, `GameMode`, `GameplayConfig`,
      `DisabledFluidTickers`, `ResourceStorage` and `Plugin`.
    - `WorldUuid` serializes as strict extended JSON
      (`{"$binary":"<base64>","$type":"04"}`) and also accepts Java's hex-string
      form.
    - Provider sections stay as ordered JSON objects (`ConfigObject`), so
      provider options are preserved without porting every provider codec.
    - Unknown top-level fields are kept in `extra`; load → save → reload is
      lossless for newer Java documents.
    - `load`/`to_json_pretty`/`save` match Java's pretty JSON, `.bak` backup and
      atomic temp-file rename; `WorldConfig::path_in(save_dir)`.
  - `hytale-server::world_config_tool` and `hytale-server world-config
    [--input] [--save] [--seed] [--name] [--uuid]`. New headless worlds derive a
    stable UUID from seed + name; existing configs keep theirs.
  - Java oracle:
    - `WorldConfigDumper` `write` builds a config and saves it through the real
      `WorldConfig.save` (booting the asset stores and registering the provider
      codecs that `Universe` normally installs).
    - `WorldConfigDumper` `probe` decodes a Rust-written file with
      `WorldConfig.CODEC` and checks UUID, seed, name, game time, game mode,
      flags and fluid tickers.
    - `tools/java-dumper/run-world-config.sh` (`--check`, `--probe`).
  - Fixture `testdata/golden/server/world-config/config.json` plus README.
  - Tests: `crates/hytale-world/tests/world_config.rs` (fixture fields, lossless
    round-trip, unknown fields, atomic save/backup) and
    `crates/hytale-server/tests/world_config.rs` (CLI copy and stable default
    UUIDs).
- **Verified:**
  - `tools/java-dumper/run-world-config.sh --check` is deterministic and matches
    the committed fixture.
  - `cargo run -q -p hytale-server -- world-config --input
    testdata/golden/server/world-config/config.json --save
    target/world-config-smoke` wrote a Rust config, and
    `tools/java-dumper/run-world-config.sh --probe
    target/world-config-smoke/config.json` printed
    `java world config check: ok`.
  - `cargo test -p hytale-world --test world_config` and `cargo test -p
    hytale-server --test world_config` are green.
  - `cargo clippy -p hytale-world -p hytale-server --all-targets -- -D warnings`
    and `cargo fmt --all -- --check` are green.
- **Deviations:**
  - The Java probe checks `GameplayConfig` from the parsed document and removes
    it before the codec decode. The harness cannot load the gameplay-config
    assets (they depend on the camera-effect store), so that key's validator
    cannot run there; a production Java boot validates it.
  - `DisabledFluidTickers` is a `BTreeSet`, so Rust writes a sorted array; Java
    uses a `HashSet`.
  - Provider payloads remain opaque JSON; only the world config's own fields are
    typed.
  - The headless chunk boot still uses a fixed generator name/version and does
    not read `config.json`; the CLI is the only writer today.
  - New headless worlds derive a deterministic UUID instead of Java's random one.
- **Follow-ups:**
  - `DiskPlayerStorageProvider` player BSON and a Java-written player fixture.
  - Wire `WorldConfig` into the headless boot / `World` lifecycle (seed, spawn,
    world-gen settings, save flags).
  - Generate-missing-chunks path on top of `World`.
  - Client-bot auth/setup/game with the packet-trace fixture.
  - P1 stores and asset packet encoders.

---

## Session 10 — player saves and the Java player gate (2026-10-22)

- **Goal:** read and write `DiskPlayerStorageProvider` player files
  (`<world>/players/<uuid>.json`) with the real `EntityStore` holder codec as the
  oracle, completing the saved-world triad after chunks (session 8) and config
  (session 9).
- **Landed:**
  - `hytale-world::player_data`:
    - `PlayerData` is the `{"Components": {...}}` holder document with ordered
      component preservation.
    - Typed core components: `UUID` (Java extended JSON), `Transform`
      (`Position` XYZ doubles + `Rotation` Pitch/Yaw/Roll floats in radians) and
      `WorldGenId`.
    - Every other component (inventory, player, stats, settings, skin, …) is
      kept as an opaque JSON value, so a full Java player save survives a Rust
      load/save cycle.
    - `PlayerStorage` implements `<save>/players/<uuid>.json` load/save/remove/
      list with Java-style pretty JSON, `.bak` copy and atomic rename.
  - `hytale-server::player_data_tool` and CLI `hytale-server player-data
    [--input] [--save] [--uuid] [--x --y --z] [--pitch --yaw --roll]
    [--world-gen-id]`.
  - Java oracle:
    - `WorldPlayerDumper` registers the `EntityModule.setup()` component ids for
      the core set, builds a holder and writes
      `EntityStore.REGISTRY.serialize(holder)` through `BsonUtil.writeDocument`.
    - `probe` deserializes a Rust-written file with the real registry, checks
      UUID/Transform/WorldGenId, and re-serializes the holder to check the
      `Components` map.
    - `tools/java-dumper/run-player-data.sh` (`--check`, `--probe`).
  - Fixture `testdata/golden/server/player-data/` plus README, and tests in
    `crates/hytale-world/tests/player_data.rs` and
    `crates/hytale-server/tests/player_data.rs`.
- **Verified:**
  - `tools/java-dumper/run-player-data.sh --check` is deterministic and matches
    the committed fixture.
  - Rust write → Java read: the CLI copied the fixture into
    `target/player-smoke/players/…`, and `run-player-data.sh --probe` printed
    `java player data check: ok`.
  - `cargo test -p hytale-world --test player_data` and `cargo test -p
    hytale-server --test player_data` are green.
  - `cargo clippy -p hytale-world -p hytale-server --all-targets -- -D warnings`
    and `cargo fmt --all -- --check` are green.
- **Deviations:**
  - Only `UUID`, `Transform` and `WorldGenId` are typed; the remaining player
    components are opaque until their gameplay systems are ported.
  - The fixture is a minimal holder, not a full join state. Java accepts the
    missing components; a real spawn adds defaults.
  - `PlayerStorage::players` sorts UUID strings instead of exposing directory
    order.
  - The world boot does not load/save players yet; the CLI is the only caller.
- **Follow-ups:**
  - Wire `WorldConfig` and player load into a world boot and the join path.
  - Port `Player`/inventory/settings/skin component codecs as gameplay lands.
  - Generate-missing-chunks path on top of `World`.
  - Client-bot auth/setup/game with the packet-trace fixture.

---

## Session 11 — complete saved-world boot and the whole-save gate (2026-10-22)

- **Goal:** compose the three saved-world parts (config, players, chunks) into
  one boot path and verify a Rust-written save in all three places with Java.
- **Landed:**
  - `hytale-world::world::WorldSave`: opens a save root, loads `config.json` or
    creates it, owns the runtime `World` and `PlayerStorage`,
    `load_or_create_player` (using the config spawn point), `save_player`,
    `save_config` and `save_all` (dirty chunks + config + flush).
  - `WorldUuid::derive(seed, label)` moved into `world_config`; the config CLI and
    the new boot share it.
  - `hytale-server::world_boot` and `hytale-server world-boot`:
    - load/create config and player;
    - load every stored chunk;
    - optional block edit;
    - save dirty chunks + config and re-save the player holder.
  - Tests:
    - `crates/hytale-world/tests/world_save.rs` (config+player+chunk create and
      reopen, spawn-point and existing-config cases);
    - `crates/hytale-server/tests/world_boot.rs` (config/player create then
      reopen);
    - the existing `tests/headless.rs` now boots the complete save after
      generation and applies a second block edit.
  - `tools/java-dumper/run-save-check.sh`: one command that runs the config,
    player and chunk Java probes against the same save directory.
- **Verified:**
  - `tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159
    Debug_Test_Block` printed `java world config check: ok`, `java player data
    check: ok`, `java reload check: ok` and `whole-save check: ok`.
  - `cargo test --workspace` (1098 passed), clippy and fmt are green.
- **Deviations:**
  - `world-boot` loads stored chunks but does not generate missing ones yet;
    `headless` remains the generation path.
  - It loads every stored chunk, not a view-radius demand set.
  - A new config has no spawn provider, so a new player starts at the origin;
    with a `Global`/`SpawnPoint` only the position is used (rotation starts at
    zero).
  - A new player gets `WorldGenId = 0`; gameplay components are not hydrated.
  - `headless` still writes the chunk directly; the fixture setup uses the
    dedicated config/player CLIs before `world-boot`.
- **Follow-ups:**
  - Generate missing chunks on boot and add `WorldGenRuntime`.
  - Demand-load chunks around the player view radius.
  - Hydrate the join player components and wire the live world thread.
  - Client-bot auth/setup/game with the packet-trace fixture.

---

## Session 12 — missing-chunk generation and demand loading (2026-10-23)

- **Goal:** make the saved-world boot load what the player needs: stored chunks
  from disk, missing chunks from `worldgen-server`, then save the result.
- **Landed:**
  - `hytale-world::world`:
    - `WorldSave::ensure_chunks(codec, coordinates, generate)` counts
      in-memory chunks, loads stored ones, generates the rest and inserts them
      dirty.
    - `DemandReport` and `chunk_coordinates_around(center, radius)`.
  - `hytale-server::generation::WorldGenerator`: one shared assets → P0 registry
    → worldgen registries → worker-pool → runtime column boot. `headless` now
    uses it instead of building the provider itself.
  - `hytale-server world-boot` gained `--assets-root`, `--world`, `--workers`
    and `--view-radius` (default 0 = the player's chunk). It unions stored chunk
    coordinates with the radius around the player chunk, loads/generates, then
    edits and saves. The report now includes requested/loaded/generated counts.
  - Tests:
    - `hytale-world/tests/world_save.rs` demand pass (stored vs generated, then
      all from disk);
    - `hytale-server/tests/world_boot.rs` generates the player chunk then
      reloads it;
    - `hytale-server/tests/headless.rs` boots with demand loading and saves the
      stored plus generated chunks.
- **Verified:**
  - `world-boot --view-radius 1` on the golden-config save reported
    `stored=1 requested=10 loaded=1 generated=9 saved=10`.
  - `run-save-check.sh` still passes on chunk `(4, -5)`.
  - A generated region reads in Java:
    `run-reload-check.sh target/save-smoke 0 0 0 64 0 Empty` →
    `java reload check: ok`.
  - `cargo test --workspace` (1099 passed), clippy and fmt green; worldgen
    goldens (`780 passed`) unchanged.
- **Deviations:**
  - `view_radius` is a square chunk radius; no view-distance shape, priority
    queue or cancellation yet.
  - The boot still loads every stored chunk, even outside the radius.
  - Demand loading runs synchronously on the calling thread.
  - Generated chunks are dirty and saved immediately by the boot.
  - The config `Seed` is a Java `long`; the boot casts it to `i32` for
    `worldgen-server`, matching the Java worldgen API.
- **Bug fixed:**
  - `PrefabPatternGenerator::get_displacement` used checked addition
    (`seed + seed_offset`) and panicked for the config seed applied to chunks
    around `(0, -1)`. Java `int` addition wraps, so it now uses
    `seed.wrapping_add(seed_offset)`. The demand gate found this; all worldgen
    goldens still pass.
- **Follow-ups:**
  - Move demand loading onto the world thread with priorities and cancellation.
  - Add the view-distance tracker used by the join path.
  - Stream the loaded/generated chunks to the client.
  - Client-bot auth/setup/game with the packet-trace fixture.

---

## Session 13 — world thread, view tracking and demand scheduling (2026-10-23)

- **Goal:** move chunk demand onto a dedicated world thread, order it
  nearest-first, cancel queued work when the view changes, unload chunks that
  leave the view, and expose the events a chunk stream will consume.
- **Landed:**
  - `hytale-world::view::ChunkView`/`ChunkPlan`: square view, desired set,
    nearest-first loads (with deterministic X/Z ties), X/Z-ordered unloads, and
    unit tests for priority, view replacement and negative centers.
  - `World::loaded_chunk_coordinates()` and `World::unload_chunk(codec, x, z)`
    (saves dirty chunks before dropping them).
  - `hytale-server::world_thread::WorldThread`:
    - one OS thread (`hytale-world`) owns `WorldSave`, `WorldGenerator` and the
      pending queue;
    - commands `SetView`, `Tick`, `LoadChunk`, `SetBlock`, `Save`, `Shutdown`;
    - `SetView` rebuilds the plan, unloads chunks that left, replaces the queue
      (cancelling pending loads) and orders the new work;
    - `Tick` processes a bounded number of chunks and emits
      `WorldEvent::ChunkLoaded { generated }`; `SetView` emits
      `WorldEvent::ChunkUnloaded`;
    - `WorldStartReport`, `ViewReport`, `TickReport`, `LoadOutcome`;
    - `Drop` shuts the thread down, so early returns do not leak it.
  - `world-boot` now drives the thread (spawn → tick until empty → optional
    explicit load/edit → shutdown) and has `--chunks-per-tick`.
  - `crates/hytale-server/tests/world_thread.rs`: radius-0 view ticks, a one-chunk
    view move unloads the old chunk and generates the new one, with events.
- **Verified:**
  - `world-boot --view-radius 1 --chunks-per-tick 4` reported
    `stored=1 requested=9 loaded=1 generated=9 saved=10`.
  - `run-save-check.sh` still prints `whole-save check: ok`; a generated region
    still reads in Java (`chunk (0,0) … Empty` → `java reload check: ok`).
  - `cargo test -p hytale-server --test world_thread` and the existing
    `world_boot`/`headless` gates are green.
  - `cargo test --workspace` (1105 passed), clippy and fmt green.
- **Deviations:**
  - Generation is synchronous in the world thread; a `SetView` arriving during a
    generation is handled after it completes. Cancellation covers queued chunks,
    not an in-flight generator call.
  - `Tick` processes a fixed budget per command; the live loop still has to drive
    it.
  - The view is square; per-player views and priority aging are not implemented.
  - Unloading saves dirty chunks synchronously on the world thread.
  - `WorldEvent` has no consumer yet; it is the seam for the chunk stream.
- **Follow-ups:**
  - Drive `Tick` from the real world tick loop and add async generation
    completion.
  - Stream `ChunkLoaded` events to a player as chunk packets.
  - Per-player views and a non-square view-distance shape.
  - Client-bot auth/setup/game with the packet-trace fixture.

---

## Session 14 — chunk packet stream from world events (2026-10-23)

- **Goal:** turn a `ChunkLoaded` event into the exact packet batch a client
  receives on join, framed for the QUIC chunk stream.
- **Landed:**
  - `hytale-world::chunk::stream`:
    - `StreamPacket` carries id/channel/compressed/max-size/payload and
      `frame(level)` compresses with zstd then calls `encode_frame`.
    - `ChunkPacketBatch::initial` = `SetColumn` + per-section `SetChunk` and
      `SetChunkEnvironments` (the Java join set).
    - `ChunkPacketBatch::full` adds `SetChunkTintmap` and per-section
      `SetFluids` in `ChunkPacketDumper` order.
    - `frames`, `payload_bytes`, `chunk_packet_ids`.
    - Unit tests cover initial/full membership and frame → decode-frame →
      decompress → generated `Wire` decode.
  - `WorldThread` command/method `chunk_packets(x, z, full)`, so a connection
    task can answer a `ChunkLoaded` event without touching world state.
  - The chunk-packet golden test now checks the full batch's packet ids and
    payloads byte-for-byte in manifest order, and the initial batch against the
    `sentAtInit` subset.
  - `world_thread` test consumes `ChunkLoaded`, builds the initial batch, checks
    the `Chunks` channel and frames it.
- **Verified:**
  - `cargo test -p hytale-world --lib chunk::stream` (2 tests),
    `cargo test -p hytale-world --test chunk_packets_golden` (all 46 Java
    payloads), and `cargo test -p hytale-server --test world_thread` are green.
  - `cargo test --workspace` (1107 passed), clippy and fmt green.
- **Deviations:**
  - Nothing is written to a connection yet; the connection task/stream is the
    next slice.
  - Initial batches omit tint/fluids to match Java's `sentAtInit = false`; the
    full batch carries them for updates.
  - zstd level defaults to Java `PacketIO`'s default (3), configurable per call.
  - Block-edit/update packets are not built yet.
  - No cross-chunk batching, priority or coalescing.
- **Follow-ups:**
  - Write the frames on the client's `Chunks` stream from the connection task.
  - Teach `client-bot` to receive/decode chunks.
  - Add block/section update packets.
  - Client-bot auth/setup/game join path.

---

## Session 15 — Chunks QUIC stream and client-bot receiver (2026-10-23)

- **Goal:** carry the chunk packet frames across a real QUIC `Chunks` stream and
  teach the client bot to receive and decode them.
- **Landed:**
  - `hytale-net::chunk_stream`:
    - `ChunkStreamWriter::open` opens a server→client unidirectional stream and
      writes frames (`write_frame`/`write_frames`/`finish`).
    - `ChunkStreamReader::accept` reads frames incrementally (`read_exact`
      header + payload), validates id/max size against the registry, and
      zstd-decompresses packets whose registry entry is compressed.
    - `ChunkFrame`, `ChunkStreamError`, and truncated/unknown-packet errors.
  - `client-bot`:
    - `ChunkPacketSummary` and `summarize_chunk_packet` decode `SetColumn`,
      `SetChunkTintmap`, `SetChunk`, `SetChunkEnvironments` and `SetFluids` with
      the generated `Wire` codecs.
    - `receive_chunk_stream(&ClientSession)` reads to FIN and returns summaries.
  - Three `#[ignore]`d loopback tests:
    - `hytale-net/tests/chunk_stream.rs` — two compressed frames over mTLS QUIC,
      decoded back with `SetColumn`/`SetChunk`;
    - `tools/client-bot/tests/chunk_stream.rs` — the bot receives summaries;
    - `crates/hytale-server/tests/chunk_transport.rs` — a real
      `ChunkPacketBatch::full` crosses the stream with identical ids/payloads.
- **Verified:**
  - All three loopback tests pass with `--ignored`.
  - `cargo test --workspace` (1107 passed, 5 ignored), clippy and fmt green.
- **Deviations:**
  - The stream is a server→client unidirectional stream; the exact Java
    channel/stream mapping still needs a packet trace.
  - One chunk stream per connection, no stream ids or multiplexing.
  - No server session/connection task yet; tests wire the endpoints directly.
  - `client-bot`'s live CLI does not wait for chunks yet.
  - Flow control/batching across chunks use quinn defaults.
- **Follow-ups:**
  - Open the chunk stream from the server state machine and feed it from
    `WorldThread::chunk_packets`.
  - Add chunk reception to the client-bot join flow.
  - Confirm the Java stream/channel mapping with a packet trace.
  - Add block/section update packets on the same stream.

---

## Session 16 — live chunk session and client-bot stream mode (2026-10-23)

- **Goal:** connect the real world thread to a real QUIC `Chunks` stream and
  drive it from `client-bot` end to end.
- **Landed:**
  - `hytale-server::chunk_streamer`:
    - `send_view` sets the view, ticks the world thread until the queue is
      empty, then sends every loaded chunk in the view nearest-first as an
      initial `ChunkPacketBatch`.
    - `send_chunk`, `ChunkStreamReport` and `ChunkSessionError`.
  - `WorldThread::loaded_chunks()` and a `Sync` event receiver (mutex), so a
    connection task can hold `&WorldThread` across `.await`.
  - `hytale-server::chunk_serve`:
    - `serve_connection` opens the writer, sends one view and finishes the
      stream.
    - `run` builds a tokio runtime, spawns the world thread, binds a QUIC
      endpoint, accepts one client and streams the player view.
  - CLI `hytale-server chunk-serve` with `--bind`, view radius, tick budget and
    compression level (transport demo, no auth).
  - `client-bot --chunks` receives the stream after `Connect` and prints
    `ChunkPacketSummary` lines.
  - `crates/hytale-server/tests/chunk_serve.rs` (`#[ignore]`, pack required)
    asserts the live stream has 1 `SetColumn`, 10 `SetChunk` and 10
    `SetChunkEnvironments` for a radius-0 view.
- **Verified:**
  - Manual end-to-end:
    `chunk-serve --save target/chunk-serve-smoke --bind 127.0.0.1:5521` and
    `client-bot --host 127.0.0.1:5521 --chunks` printed 21 summaries; the server
    reported `streamed 1 chunks (21 packets, 21 frames, 283481 bytes)`.
  - The ignored `chunk_serve` test passes.
  - `cargo test --workspace` (1107 passed, 6 ignored), clippy and fmt green.
- **Deviations:**
  - No Initial/Authentication/Setup/Game exchange: the demo accepts the first
    QUIC connection and opens the stream immediately.
  - One client/connection/view per process; no incremental view updates after
    the snapshot (the event path exists for that).
  - The snapshot is read from the loaded set after the queue drains; demand
    events are drained to avoid double-sending preloaded chunks.
  - `chunk_serve::run` builds its own tokio runtime and cannot run inside one.
  - Certificates are still accepted unconditionally (spike policy).
  - No unload/`ClearChunks` packets; square view only.
- **Follow-ups:**
  - Add the connection state machine and open the stream after `Setup`/`Game`.
  - Stream incremental `ChunkLoaded` events and unloads after movement.
  - Drive the bot through auth/setup with the trace fixture.
  - Confirm the Java stream/channel mapping with a packet trace.

---

## Session 17 — incremental view updates and unloads (2026-10-23)

- **Goal:** stream view changes after the initial snapshot: new chunk batches
  and `UnloadChunk` packets. This is the streaming half the real session will
  drive after reaching `Game`.
- **Landed:**
  - `chunk::stream::unload_chunk(x, z)` builds the 135 `UnloadChunk` packet
    (Chunks channel) and frames it; unit test decodes it back.
  - `ChunkStreamReport::unloaded`.
  - `chunk_streamer::send_updates` drains `WorldEvent`s: `ChunkLoaded` sends the
    batch, `ChunkUnloaded` sends `UnloadChunk`.
  - `send_view` forwards unloads caused by the initial `SetView` before the
    snapshot.
  - `hytale-server chunk-serve --move-demo` moves the view one chunk after the
    snapshot and streams the update.
  - `client-bot` decodes `UnloadChunk` into `ChunkPacketSummary::Unload`.
  - The `chunk_serve` loopback test now runs the move demo and asserts 2
    `SetColumn`, 20 `SetChunk`, 20 `SetChunkEnvironments`, 1 `UnloadChunk`
    (43 frames); the client-bot test asserts the unload summary.
- **Verified:**
  - Manual demo: `chunk-serve --move-demo --view-radius 0` + `client-bot
    --chunks` printed 43 packets and `Unload { x: 0, z: 0 }`; the server reported
    `streamed 2 chunks (43 packets, 43 frames, 552789 bytes)`.
  - `cargo test --workspace` (1108 passed, 6 ignored), clippy and fmt green.
- **Deviations:**
  - Only the demo's single explicit move drives incremental updates; player
    movement packets are not wired.
  - Per-chunk `UnloadChunk` only; no `UnloadSection`/`ClearChunks`.
  - The demo still opens the stream on accept (no auth/setup/game flow).
  - The snapshot path discards demand `ChunkLoaded` events and reads the loaded
    set; incremental updates use the event path.
  - No backpressure between event draining and stream writes.
- **Follow-ups:**
  - Session state machine after the asset packet encoders + packet trace land.
  - Feed view changes from player movement and stream `send_updates`.
  - Add section unloads / `ClearChunks` where appropriate.
  - Drive `client-bot` through the real handshake.

---

## Session 18 — P0 asset packet encoders, part 1 (2026-10-23)

- **Goal:** start the asset packet encoders required by the setup/join flow,
  with byte parity against the Java `AssetDumper` payloads.
- **Landed:**
  - `hytale-assets::packets`: `AssetPacket { store, id, payload }`,
    `AssetPacketError`.
  - `AssetRegistry::physical_material_packet()` → `UpdatePhysicalMaterials`
    (id 87, 12 materials) and `block_hitbox_packet()` → `UpdateBlockHitboxes`
    (id 41, 259 assets), both `UpdateType::Init` with `maxId = max + 1`.
  - Maps are `BTreeMap` (ascending id), matching Java
    `AssetDumper.canonicalizeMaps`.
  - Box components are double→float; missing `Min`/`Max`/axis fields decode as
    Java's zero vector (found with `GoblinLab_Fence`).
  - `crates/hytale-assets/tests/asset_packets.rs` compares both payloads
    byte-for-byte with `PhysicalMaterial.bin` / `BlockBoundingBoxes.bin` and
    decodes them with the generated codec.
- **Verified:**
  - `cargo test -p hytale-assets --test asset_packets` (2 tests) passes against
    the real pack.
  - `cargo test --workspace` (1110 passed, 6 ignored), clippy and fmt green.
- **Deviations:**
  - Only the two stores that need no child indexes are encoded.
  - `Environment` needs TagPattern-expanded indexes; `Fluid` needs FluidFX,
    sound-set, particle/texture and tag indexes.
  - `BlockType`/`Item` are not encoded; their contained-asset ids are Rust-owned.
  - Only the init envelopes exist; no `AddOrUpdate`/`Remove`.
  - Not wired into the session/setup flow yet.
- **Follow-ups:**
  - Add `Environment`/`Fluid` with their child stores and tags.
  - Encode `BlockType`/`Item` from the Rust snapshot (decode/size validation until
    the Java order is modelled).
  - Build the `WorldSettings`/asset-init sequence and the connection state
    machine on top.

---

## Session 19 — dev/insecure join handshake and first bot join (2026-10-23)

- **Goal:** run the Java insecure-mode control flow end to end and stream chunks:
  `Connect` → `RequestInsecurePlayerOptions` → `InsecurePlayerOptions` →
  `ConnectAccept` → `WorldSettings`/`ServerInfo` → `RequestAssets` →
  `WorldLoadProgress`/`WorldLoadFinished` → `SetupFinalize` → chunks.
- **Landed:**
  - `hytale-net::frame_stream`: generic `FrameReader`/`FrameWriter` for any QUIC
    stream, `write_packet` with registry-driven zstd compression, incremental
    reads with id/max-size validation, `Frame::decode::<T>()`.
  - `hytale-server::session`:
    - `SessionOptions`/`SessionReport`/`SessionError`;
    - `serve_connection` accepts the client's bidirectional setup stream, runs
      the insecure handshake (CRC check, Ping/Pong, ViewRadius, duplicate-assets
      guard, SetupFinalize), then streams the player view;
    - reports UUID/username/view radius/stream counts.
  - `chunk-serve --handshake` plus `--server-name`/`--motd`/`--max-players`/
    `--handshake-timeout`.
  - `client-bot::dev_join` + CLI `--dev-join --username --view-radius`, which
    performs the same flow and then receives the chunk stream.
  - `tools/client-bot/tests/dev_join.rs` (ignored, pack + loopback UDP).
- **Verified:**
  - Manual dev join: handshake packet ids `[364, 14, 20, 223, 21, 22]`, 21 chunk
    packets; server `handshake ok: RustBot view_radius=0 chunks=1`.
  - Ignored `dev_join` loopback test passes.
  - `cargo test --workspace` (1110 passed, 7 ignored), clippy and fmt green.
- **Deviations:**
  - Insecure/dev mode only; no OAuth/JWT and no certificate binding check.
  - No common/config asset streaming (`WorldSettings.requiredAssets` empty,
    `RequestAssets` answered with progress/finished only).
  - No translations, player add/spawn/game packets or game loop; chunks start
    right after SetupFinalize.
  - ServerDisconnect is only sent on CRC mismatch; other violations close.
  - The world-thread player UUID is independent of the InsecurePlayerOptions
    UUID; the dev flow only reports the latter.
  - One client/connection per demo process.
- **Follow-ups:**
  - Add authentication and the JWT `cnf.x5t#S256` binding.
  - Encode the remaining asset packets and stream common/config assets.
  - Send player add/spawn/world settings after SetupFinalize.
  - Add the game packet loop and movement-driven view updates.

---

## Session 20 — local EdDSA auth, common assets and the Game transition (2026-10-23)

- **Goal:** add real authentication to the join flow, stream the common/config
  assets, and drive the control flow into `Game` before the chunk stream.
- **Landed:**
  - `hytale-auth`: Ed25519 key generation/PKCS#8 load, EdDSA JWT
    encode/decode, `iss`/`aud`/`sub`/`username`/`scope`/`iat`/`nbf`/`exp`/
    `cnf.x5t#S256` validation, local token-use separation, and an `AuthService`
    that issues identity/access/grant/server tokens.
  - `hytale-net`: `TlsIdentity::fingerprint_base64url` and
    `PeerIdentity::fingerprint_base64url` for the RFC 8705 binding; raw-packet
    writes on `FrameWriter` and `ChunkStreamWriter`.
  - `hytale-assets::common`: `CommonAssetsIndex.hashes` parser, name/hash
    lookup, deduplicated advertised list, lazy verified blob loading
    (`CommonAssetRegistry`).
  - `hytale-assets::packets::config_packets`: the encodable P0 config packets
    (`UpdateBlockHitboxes` 41 and `UpdatePhysicalMaterials` 87).
  - `hytale-server::session`:
    - `AuthMode::{Insecure, Local}`; the local branch runs
      `Connect(identityToken)` → `AuthGrant` → `AuthToken` →
      `ServerAuthToken` and enforces `cnf.x5t#S256` against the client
      certificate;
    - `WorldSettings.requiredAssets` from the common index;
    - `RequestAssets` subset handling with per-asset `AssetInitialize` +
      `WorldLoadProgress` + `AssetPart` + `AssetFinalize`, then
      `RequestCommonAssetsRebuild`;
    - config asset packets + `UpdateTranslations(Init)` + world progress;
    - `ClearChunks` on the chunks stream, `ServerTags`, `JoinWorld`, a
      `ClientReady(readyForChunks)` gate, and the post-join `ViewRadius`,
      `SetEntitySeed`, `SetClientId`, `SetTimeDilation`, `UpdateFeatures`,
      `SetUpdateRate` burst before the chunk snapshot;
    - `AddToServerPlayerList` and the joining player's self `EntityUpdates`
      (default model/skin/equipment plus the saved transform);
    - `AssetStreamReport` and `GameReport` in `SessionReport`.
  - `WorldThread::config_asset_packets`, `WorldThread::common_assets` and
    `WorldThread::player_transform` expose the already-loaded registry, pack
    index and saved transform without loading either twice.
  - `client-bot`: `auth_join` (identity/access token minting, grant exchange,
    asset reassembly and SHA-256 verification, Game transition) and
    `--auth-join`/`--auth-key`/`--auth-issuer`/`--auth-audience`/
    `--asset-limit`; `--dev-join` now also sends `ClientReady`.
  - New ignored loopback test `tools/client-bot/tests/auth_join.rs`.
- **Verified:**
  - Manual local-auth join: handshake ids
    `[11, 13, 20, 223, 24, 21, 25, 26, 24, 21, 25, 26, 28, 41, 87, 64, 21, 22]`,
    2 verified common assets (3441 bytes), config packets `[41, 87]`,
    `clear_chunks/join_world/ready_chunks = true`, 21 chunk packets.
  - `cargo test --workspace` (1120 passed, 0 failed, 9 ignored), clippy and fmt
    green.
- **Deviations:**
  - Authentication is a local EdDSA profile, not the live Hytale OAuth/session
    service (no device/browser flow, JWKS fetch, session registration or
    referral handling).
  - Config asset streaming only covers the stores the Rust registry can encode
    (`PhysicalMaterial`, `BlockHitboxes`); `Environment`, `Fluid`, `BlockType`
    and `Item` remain follow-ups.
  - No password challenge, no `readyForGameplay` teleport, no world
    time/gameplay config, and no gameplay/movement loop.
  - The self-spawn entity uses default (empty) model/skin/equipment payloads;
    other-player entity tracking is not implemented.
  - Common-asset blobs are loaded together on a blocking pool per request; no
    per-asset streaming/backpressure.
  - `ServerTags` and translations are empty init packets.
- **Follow-ups:**
  - Add the remaining P0 config asset packets.
  - Add `AddToServerPlayerList`, the self `EntityUpdates` spawn, time/gameplay
    packets and the `ClientReady(readyForGameplay)`/teleport exchange (T31).
  - Feed movement packets into `WorldThread::set_view` and stream updates.
  - Add a real vanilla-client trace for the authenticated branch.

---

## Session 21 — spawn payloads, teleport exchange and the live game loop (2026-10-23)

- **Goal:** complete the client's spawn/join burst and make the session usable
  for gameplay input: world time/gameplay config, `readyForGameplay` teleport,
  an open chunk stream, and movement-driven view updates.
- **Landed:**
  - `WorldThread`: `WorldTimeState` and `WorldThread::world_time`, derived from
    the world config's daytime/nighttime durations and pause flag (Java defaults
    when unset).
  - `hytale-server::session`:
    - `SetGameMode`, `WorldGameplayConfig` (protocol defaults with block
      breaking/gathering/placement enabled), `UpdateTimeSettings`, `UpdateTime`
      after the self entity spawn;
    - `ClientReady(readyForGameplay)` handling and `ClientTeleport` using the
      saved transform;
    - a persistent game loop that keeps the `Chunks` stream open, ticks the
      world thread at 50 ms, forwards `ChunkLoaded`/`ChunkUnloaded` events, and
      processes `ClientMovement`, `ViewRadius`, `ClientDisconnect` and `Ping`;
    - movement computes the new chunk, calls `WorldThread::set_view`, and streams
      the resulting unload/new-chunk batches;
    - `GameLoopReport` (teleport sent/acked, movement packets, view changes,
      loop chunk counters, disconnect) merged into `SessionReport`.
  - `client-bot`:
    - reads the initial snapshot by packet count instead of waiting for FIN (the
      stream now stays open);
    - sends `ClientReady(readyForGameplay)`, receives `ClientTeleport`, sends a
      `ClientMovement` teleport ack + position, and collects the view-update
      packets;
    - `--move-chunk x,z`.
  - `auth_join` loopback test now exercises the teleport ack and a movement to
    the adjacent chunk, asserting `UnloadChunk(0,0)` and `SetColumn(1,0)`.
- **Verified:**
  - Manual auth join with `--move-chunk 1,0`:
    - teleport sent and acked, `movement=1`, `view_changes=1`, loop streamed the
      new chunk (43 packets total);
    - self spawn, gameplay config, time, player list and Game transition all
      reported.
  - Ignored `auth_join` (2 tests) and `dev_join` pass.
  - `cargo test --workspace` (1120 passed, 0 failed, 9 ignored), clippy and fmt
    green.
- **Deviations:**
  - Self spawn uses Java's empty model/skin/equipment forms; cosmetics and
    inventory payloads are not decoded from the player save.
  - `WorldGameplayConfig` uses protocol defaults; the `GameplayConfig` asset is
    not loaded.
  - World time is sent once and does not advance yet.
  - Movement only changes the view; there is no physics, collision, anti-cheat
    or teleport position validation.
  - The game loop ticks the world inline on the connection task; a chunk load
    can briefly block the async loop.
  - World map settings, other players and entity tracking are still absent.
  - One connection/player per `chunk-serve` process; no universe or real
    listener loop yet.
- **Follow-ups:**
  - Load cosmetics/equipment and send the real self-spawn components.
  - Add world clock progression and `GameplayConfig` asset data.
  - Add other-player tracking and broadcast movement/entity updates.
  - Wire block place/break and the persistent multi-connection server runtime.
  - Capture a real vanilla-client trace to validate the packet/channel order.

---

## Session 22 — Environment/Fluid config packets and Rust-owned tags (2026-10-23)

- **Goal:** stream the remaining self-contained world-content stores,
  `Environment` (61) and `Fluid` (83), with a consistent tag-index model.
- **Landed:**
  - `hytale-assets::packets::TagRegistry`: deterministic alphabetical indexes
    for every tag key, value and `key=value` composite in the P0 documents;
    `ServerTags` map and per-asset expanded index sets.
  - `ConfigAssets { packets, tags }`; `config_assets` returns the four packets
    and the tag map; `WorldThread::config_asset_packets` and the session now
    carry both.
  - `AssetRegistry::environment_packet`: id, water tint, tags,
    `rebuildMapGeometry = true`, `maxId`.
  - `AssetRegistry::fluid_packet`: id, max level, textures with normalized
    weights, opacity, shader effects, Java `ColorLight` parsing (3-digit
    nibbles / 6-digit `/17`), particles, draw type, particle color, tags.
  - `ServerTags` now sends the matching non-empty tag map, so the client can
    resolve the Rust-owned indexes.
  - `hytale-assets/tests/asset_packets.rs` adds Environment/Fluid decode tests:
    135 environments and 19 fluids, max ids, all non-tag fields compared with
    the Java goldens, and every used tag index present in `ServerTags`.
  - `client-bot` recognises `UpdateFluids` and `UpdateEnvironments`; the auth
    test expects config packets `[41, 87, 83, 61]`.
- **Verified:**
  - `cargo test -p hytale-assets --test asset_packets` (3 tests).
  - Manual auth join: handshake ids `[11, 13, 20, 223, 24, 21, 25, 26, 28, 41,
    87, 83, 61, 64, 21, 22]`, config packets `[41, 87, 83, 61]`, 21 chunk
    packets, Game/teleport loop unchanged.
  - `cargo test --workspace` (1121 passed, 0 failed, 9 ignored), clippy and fmt
    green.
- **Deviations:**
  - Tag indexes are Rust-owned, not Java's global assignment order. Every
    packet and `ServerTags` share one registry, matching the existing
    Rust-owned block/item id policy.
  - `fluidFXIndex`/`blockSoundSetIndex` are 0 because `FluidFX` and
    `BlockSoundSet` are not loaded/streamed yet.
  - Environment `fluidParticles` is always `None` (the P0 pack has none and the
    FluidFX indexes are unavailable).
  - Environment/Fluid payloads are not byte-identical to Java because of tags
    and the missing child-store indexes; non-tag fields are golden-checked.
  - `BlockType` (40) and `Item` (54) packets remain pending.
- **Follow-ups:**
  - Encode `BlockType`/`Item` from the Rust registry.
  - Add `FluidFX`, `BlockSoundSet` and `BlockParticleSet` for real child indexes.
  - Add `UpdateTagPatterns` for tag-pattern assets.
  - Keep `ServerTags` in sync when block/item/contained tags are added.

---

## Session 23 — BlockType/Item config packets (2026-10-23)

- **Goal:** stream the last two P0 content stores — `BlockType`
  (`UpdateBlockTypes`, 40) and `Item` (`UpdateItems`, 54) — so the join sends
  every self-contained config packet.
- **Landed:**
  - `hytale-assets::registry`:
    - item `State` entries are expanded into Java's contained item assets
      (`*<item>_State_<name>`, recursively); all 3971 Java item keys are now
      present plus the 37 assets the canonical Java dump fails to decode;
    - items resolve parents with the recursive object merge Java's nested
      `BuilderCodec` fields use (`TranslationProperties.Description` etc.);
    - `BlockTypeAsset::container_item` records `BlockType.getItem()`;
    - `AssetRecord::parent` keeps the source `Parent` for inherited block-id
      lookup; `Face::{expand, neighbor}` are exposed to the packet layer.
  - `hytale-assets::packets::block_type`:
    - `UpdateBlockTypes` init from the Rust snapshot: draw/material/opacity,
      textures, model/animation, hitbox + interaction hitbox, shader effects,
      movement settings, tint/biome tint, light, gathering, rail, support and
      supporting maps, interaction hint, group/transition groups, tag indexes,
      and Java `processConfig` defaults;
    - deterministic Rust-owned group ids; `transitionToTag` uses the tag
      registry, which now also collects transition/support tag ids.
  - `hytale-assets::packets::item`:
    - `UpdateItems` init with id/model/texture/icon/translation props,
      stack/level/resource types/categories/set, utility, item entity,
      block id (including inherited item blocks), interaction config with
      priority slots, particles/light/durability/tags and Java defaults;
    - item maps and interaction priorities are `BTreeMap`s.
  - `config_assets()` order is `[41, 87, 83, 61, 40, 54]`; `client-bot`
    records ids 40/54.
  - `crates/hytale-assets/tests/block_item_packets.rs`: decodes both Java
    goldens and compares core fields by key, normalizes Rust-owned ids, checks
    config order, sorted item keys and `ServerTags` membership.
- **Verified:**
  - `cargo test -p hytale-assets` (17 tests), `cargo test --workspace` (all
    green), clippy and fmt clean.
  - `cargo test -p client-bot --test auth_join -- --ignored` passes; the auth
    join reports config packets `[41, 87, 83, 61, 40, 54]` and still completes
    the teleport/movement loop.
  - All 6135 Java block keys and 3971 Java item keys are compared; residual
    differences are limited to Rust-owned ids/groups/tags and the documented
    parent-processed/child-store cases.
- **Deviations:**
  - Rust-owned block ids, group ids and tag indexes.
  - Deferred trees: block `states`, `interactions`, `bench`,
    `connectedBlockRuleSet`, `placementSettings`, `conditionalSounds`,
    `display`; item tool/weapon/armor/glider/ability/builder/selector/trails/
    appearance conditions/pullback/HUD.
  - Child-store indexes remain 0 (`BlockSoundSet`, `AmbientSoundEvent`,
    `ItemSoundSet`, `Quality`, `Reticle`, interaction ids).
  - 818 `itemEntity` values (quality override), 61 `playerAnimationsId`s
    (generated child assets), 3 salad `utility` flags, 24 inherited
    `supporting` maps and a few `processConfig`-inherited colors/hints differ
    from Java until those stores/behaviours are ported.
- **Follow-ups:**
  - Port `ItemQuality`, `ItemSoundSet`, `BlockSoundSet`, `FluidFX`,
    `BlockParticleSet` and `UpdateTagPatterns`.
  - Emit block states/interactions/benches/connected rules and the item
    equipment payloads.
  - Capture a real vanilla-client trace to validate the 40/54 payloads.
