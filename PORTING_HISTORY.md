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
