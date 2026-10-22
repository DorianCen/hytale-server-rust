# Porting history — CoreServer rewrite

Append-only log of landed sessions. Do not read this for planning; use
[`NEXT_STEPS.md`](NEXT_STEPS.md) for current work. Read this when debugging a
regression or verifying why a design choice was made.

The worldgen port history is archived in
[`done/PORTING_HISTORY.md`](done/PORTING_HISTORY.md).

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
