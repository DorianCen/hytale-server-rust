# CoreServer → Rust rewrite plan

**Status:** proposed; v1 scope frozen as "vanilla client join + world streaming".
**Starting point:** the world-generation port is complete and golden-verified
(archived notes in [`done/ARCHITECTURE.md`](done/ARCHITECTURE.md),
[`done/PORTING_HISTORY.md`](done/PORTING_HISTORY.md),
[`done/REMAINING.md`](done/REMAINING.md)).

**Session docs:** [`NEXT_STEPS.md`](NEXT_STEPS.md) (immediate work),
[`ARCHITECTURE.md`](ARCHITECTURE.md) (crate/runtime design),
[`PORTING.md`](PORTING.md) (porting and subagent rules),
[`GOLDEN_DATA.md`](GOLDEN_DATA.md) (Java oracle and fixtures),
[`REMAINING.md`](REMAINING.md) (deferred scope),
[`BENCHMARKS.md`](BENCHMARKS.md) (measurement matrix),
[`PORTING_HISTORY.md`](PORTING_HISTORY.md) (session log).

This is not a file-by-file translation. The previous port succeeded because it
reproduced observable behavior and proved it with golden data. CoreServer should
be built the same way: port behavior, keep the Java server only as an oracle,
never embed a JVM in the runtime.

---

## 1. Decisions this plan is built on

- **v1 target:** a vanilla client can connect to the Rust server, authenticate,
  receive assets, spawn into a world, stream chunks, move, place/break blocks,
  disconnect/reconnect, and reload the world from disk.
- **Plugins:** Rust-native code plugins plus unmodified Hytale asset packs.
  No Java plugin runtime.
- **Storage:** read/write the vanilla `IndexedStorage` world format, so existing
  worlds are drop-in.
- **Estimates:** omitted; milestones are ordered by dependency and exit criteria.

---

## 2. What "CoreServer" actually includes

`CoreServer/` is 1,843 files / ~303k lines, but it cannot be ported in
isolation. The real dependency closure is:

| Java area | Files | Role in the Rust server |
|---|---:|---|
| `CoreServer/` | 1,843 | server runtime, universe/world, chunks, storage, network, commands, assets, UI |
| `AssetStore/` | 40 | asset pack loading, inheritance, index maps, load-order graph |
| `Codec/` | 135 | JSON/BSON codecs, validation, schema generation |
| `Component/` | 93 | custom archetype ECS |
| `Common/`, `Event/`, `Logger/`, `Metrics/`, `Registry/`, `Storage/` | ~90 | shared infrastructure, event bus, `IndexedStorageFile` |
| `Protocol/` generated sources | 389 packets, 445 structs, 177 enums | wire protocol and asset packets |

The reusable Rust base is solid:

- `crates/hytale-math`, `crates/procedural` — unchanged.
- `crates/worldgen-model` — generation chunk, priority grid, registry seam.
- `crates/worldgen-hytale` — world generation.
- `crates/worldgen-server` — server-facing chunk provider.
- `tools/java-dumper` — golden oracle harness.
- `Assets/` plus `HytaleAssets/Schema/` (120 JSON schemas) — the content contract.

**Important boundary:** `worldgen_model::chunk::GeneratedChunk` is a full-height
flat generation buffer. It is not the runtime chunk and must not become the
storage/network chunk. v1 needs a separate runtime `ChunkColumn` with palettes,
lighting, heightmaps, entities, and block components, with a cheap
`from_generated_chunk` conversion.

---

## 3. v1 definition (the acceptance contract)

A single `hytale-server` binary that:

1. Boots from an unmodified `Assets/Assets.zip` and optional `--assets`/`--mods`
   directories.
2. Loads a universe from `universe/`, loads/creates worlds, and supports
   `Default`, `Flat`, `Void`, and `Dummy` world-gen providers.
3. Produces the same block/fluid/environment registries and asset packet
   payloads as Java for the default pack.
4. Generates chunks through the existing `worldgen-hytale` service and converts
   them into runtime chunks.
5. Serves the matching protocol version over QUIC + mTLS, authenticates through
   the Hytale session/OAuth services, sends setup/assets, spawns the player,
   streams view-radius chunks, and accepts movement/block packets.
6. Saves/loads chunks and player data in Java-compatible `IndexedStorage` + BSON.
7. Exposes console commands (`stop`, `save`, `world`, `tp`, `give`, `help`) and
   basic permissions.
8. Runs headless on Linux with no JVM.

**Explicitly out of v1:** NPC/AI/pathfinding, combat, crafting, projectiles,
full inventory UI, instances, world-map UI, block entities beyond the minimum,
and Java plugins. Those are post-v1 milestones.

---

## 4. Target workspace

```text
crates/
  hytale-protocol/     generated packets/structs/enums, registry, encode/decode
  hytale-assets/       pack VFS, store framework, parent/inheritance, typed assets,
                       index maps, asset packet encoders
  hytale-core/         config/options, logging, event bus, registry, task scheduler,
                       plugin kernel, module manifests
  hytale-storage/      IndexedStorageFile v1, BSON component codecs, backups
  hytale-world/        universe, world, runtime chunk store, heightmaps, lighting,
                       world map data, player data
  hytale-entities/     ECS wrapper, components, systems, player/entity lifecycle
  hytale-net/          QUIC transport, streams, sessions, packet state machine
  hytale-auth/         OAuth device/browser, session service, JWT, credential store
  hytale-commands/     command framework, argument parsing, built-in commands
  hytale-server/       binary, startup wiring, module registration

  hytale-math/         existing
  procedural/          existing
  worldgen-model/      existing
  worldgen-hytale/     existing
  worldgen-cli/        existing
  worldgen-server/     existing
tools/
  protocol-dumper/     Java: generated protocol source → protocol IR
  protocol-gen/        Rust: protocol IR → Rust code
  asset-golden-dumper/ Java: per-store decoded assets + packet bytes
  packet-trace/        Java: capture handshake/join packet sequences
  client-bot/          Rust: scripted client for join/stream tests
```

Layering: `hytale-protocol` ← `hytale-assets` ← `hytale-world` ←
`hytale-net`/`hytale-server`. `hytale-core` is shared infrastructure; no crate
reaches into another's internals.

---

## 5. Technical strategy

### 5.1 Protocol: generate it, do not hand-port it

There are 389 packets, 445 structs, and 177 enums. Hand-writing these is the
single largest avoidable risk. Use a two-stage generator:

1. **`tools/protocol-dumper` (Java)** reads the auto-generated sources under
   `Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/` and
   emits a machine-readable `protocol.json` IR containing:
   - packet id, class name, direction, channel, compression flag, max size;
   - nullable-bitfield size, fixed-block size, variable-field count/start;
   - fields in wire order with fixed offsets, types, array/string bounds, unions.
2. **`tools/protocol-gen` (Rust)** emits:
   - `Packet` trait with `compute_size`, `encode`, `decode`;
   - all packet/struct/enum types;
   - `PacketRegistry` with ids, channels, directions, max sizes;
   - `ProtocolSettings` constants (CRC `-386561135`, version `3`, build `258`).

Wire rules to preserve exactly:

- framing `[u32 LE payload length][u32 LE packet id][payload]`;
- fixed fields little-endian; UUID longs big-endian;
- LEB128-style varints;
- nullable bitfields, fixed ASCII/UTF-8 strings, variable arrays, unions;
- zstd compression for `IS_COMPRESSED` packets, with the zstd frame carrying
  the uncompressed size;
- channel enforcement (`Default`, `Chunks`, `WorldMap`, `Voice`).

The IR is the contract. `protocol-gen` is checked in and CI runs `--check` so
protocol updates produce reviewable diffs. Every packet gets a round-trip test;
every field layout is additionally validated against Java-produced golden bytes.

### 5.2 Asset pipeline: schemas first, hand-written codecs only where necessary

`HytaleAssets/Schema/` already contains 120 JSON schemas, including `Item.json`,
`BlockType.json`, `Environment.json`, `Fluid.json`, and the rest. Do **not**
port the 15k-line Java `Codec` library. Instead:

- Build a VFS over `Assets.zip` and directories.
- Implement the `AssetStore` semantics that matter: load-order graph, pre-added
  assets, `Parent` inheritance, per-field merge rules, index maps, validation,
  and `loadsBefore`/`loadsAfter`.
- Decode through an intermediate `serde_json::Value`/`bson::Document`, resolve
  inheritance and merges, then deserialize into typed Rust structs. This mirrors
  Java's "merge, then decode" behavior and keeps the 120 schemas authoritative.
- Generate typed structs from the schemas where practical; keep manual types for
  computed/discriminated assets (`BlockType`, `Item`, `BlockBoundingBoxes`,
  `PrefabListAsset`, etc.).
- Port the 54 asset stores in priority order, not alphabetically:

| Priority | Stores | Why |
|---|---|---|
| P0 | `BlockType`, `Fluid`, `Environment`, `Item`, `BlockBoundingBoxes`, `PhysicalMaterial`, `BlockTypeListAsset`, `PrefabListAsset` | world generation, collision, client join |
| P1 | `BlockSet`, `BlockSoundSet`, `BlockParticleSet`, `BlockBreakingDecal`, `ItemCategory`, `ItemQuality`, `ModelAsset`, `GameplayConfig` | asset packets, rendering, gameplay config |
| P2 | audio, particles, weather, camera, trails, VFX, response curves, tags | full asset streaming |
| P3 | editor/UI/buildertool/misc stores | parity, not join-critical |

- Each store gets a golden dump from Java (`asset-golden-dumper`) containing its
  final asset map and its protocol packet bytes. Rust must match the bytes
  exactly.
- Asset ids: the Java pipeline's id assignment is schedule-dependent for sibling
  blocks (already documented in `done/REMAINING.md`). The Rust server must assign
  one deterministic order, use it consistently for worldgen and client packets,
  and record its own registry snapshot. Bit-exactness against the committed
  golden dump is a test target; matching a *live* server's arbitrary sibling
  order is not required.

### 5.3 Runtime chunk model: sections with palettes, not the generation buffer

Port the Java runtime layout rather than flattening it:

- `ChunkColumn`: 32×32 columns over height sections.
- Per-section components:
  - `BlockSection`: block palette (`Empty`/`Byte`/`HalfByte`/`Short`), filler
    palette, rotation palette, ticking bitsets, local/global light, change
    counters, sky-lit stamp.
  - `FluidSection`: fluid palette + nibble level data.
  - `EnvironmentSection`: run-based environment data.
  - `EntitySection`, `BlockComponentSection`: sparse holder maps.
- `HeightmapColumn` (`Legacy` and `Tree`), tint, world chunk flags/keep-alive.
- `from_generated_chunk`: copy blocks/rotation/filler/tint/environment/fluid
  into sections, choosing palettes.
- Network and storage serialization must operate on the same section bytes Java
  writes:
  - network: `SetChunk`, `SetColumn`, `SetChunkTintmap`,
    `SetChunkEnvironments`, `SetFluids`, `SetBlock`, etc.;
  - storage: `BlockSection.Data`, `FluidSection.Data`,
    `EnvironmentSection.Data`, `EntityChunk.Entities`,
    `BlockComponentChunk.BlockComponents`, `HeightmapColumn.Heightmap`,
    `ChunkColumn.Sections`.
- Golden fixtures: generate chunk packets and saved region blobs in Java for the
  existing 21 golden chunks; Rust must match byte-for-byte.

### 5.4 ECS: hybrid, not a 14k-line custom ECS port

- Use **`bevy_ecs`** for the entity store (players, entities, block entities,
  future NPCs): components, resources, queries, schedules, change detection,
  serialization via a thin registration wrapper.
- Use a **custom chunk store** (slot map keyed by chunk index) for chunks and
  sections. Chunk data is fixed-layout, hot, and network/storage-critical;
  routing 32³ arrays through archetypes adds indirection for no benefit.
- Provide a shared `ComponentRegistry` facade over both stores for module
  registration and save/load, so game modules look uniform.
- Keep a `Holder`-like serializable bundle for load/save/transfer, matching
  Java's "AoS holder, SoA store" model where it matters.
- Do not promise Java ECS API parity. Preserve observable behavior; expose a
  Rust-native component/system API.

### 5.5 Server runtime and threading

- **Tokio** for network, auth, asset loading, and storage IO.
- **One dedicated OS thread per world** for the tick loop, matching Java's
  single-threaded world semantics and avoiding chunk/entity data races.
- Worldgen stays on the existing `WorldGenService` worker pool; results are
  delivered to the world thread.
- Storage reads/writes run on a blocking pool; chunk load/save completions return
  to the world thread.
- The asset registry is immutable after load and shared as `Arc`; hot reload is a
  later copy-on-write feature.
- World tasks, commands, and events are queued onto the owning world thread.

### 5.6 World generation integration

`worldgen-server` currently exposes a blocking `ChunkProvider` with per-worker
`Rc` generators. The server needs more than chunks:

- Add a **query handle** for `get_zone_biome_result_at`, height, spawn points,
  and unique-prefab data. The current per-worker `Rc` generators cannot be
  shared across threads; give the world a dedicated read-only query instance (or
  make the generator cache shareable) and document the memory cost.
- Add `WorldGenRuntime` in `hytale-world` that owns the worker pool, the query
  handle, chunk priority/cancellation, and conversion to runtime chunks.
- Close the deferred worldgen integration gaps needed by CoreServer:
  - port `WorldGenerator/builtin/worldgen/modifier` (34 files) and its
    `ModifyEvents` dispatch, or at minimum define the modifier/event seam and
    port the content ops;
  - port `BiomeDataSystem` behavior for player zone/biome tracking;
  - prefab entities/block components remain deferred, but the prefab spawner
    path must be designed so they can be added without changing storage.

### 5.7 Storage: exact `IndexedStorage` compatibility

The default Java storage provider is `IndexedStorage`, not RocksDB. It is
tractable to match:

- Port `Storage/IndexedStorageFile` v1:
  - magic `"HytaleIndexedStorage"`, version `1`, blob count `1024`, segment
    size `4096`;
  - zstd-compressed blobs, per-blob index table, segment chain;
  - atomic write/flush behavior and recovery semantics.
- Region files: `<regionX>.<regionZ>.region.bin`; blob index
  `ChunkUtil.indexColumn(localX, localZ)`.
- Chunk payload: BSON document of ECS components using the Java names and `Data`
  byte arrays. Use the `bson` crate and explicit serde field names; no need to
  port the Java `Codec` library.
- Player data: match `DiskPlayerStorageProvider` layout and BSON/JSON shape.
- World config: match `World.json`/`config` fields used by `WorldConfig` (seed,
  uuid, generator, storage, spawn, time, gameplay config).
- Add a compatibility test: load a Java-created world fixture, mutate a block,
  save, reload in Rust, then load the same save in Java and compare.
- RocksDB support is out of scope for v1; add a migration tool later if needed.

### 5.8 Auth and transport

- `quinn` + `rustls` QUIC server. ALPN `hytale/<protocol version>` (and previous
  version for graceful rejection).
- mTLS required. Accept self-signed client certificates, extract the
  certificate, and validate the JWT certificate binding (`cnf.x5t#S256`),
  mirroring `QUICTransport`/`ServerAuthManager`.
- Port:
  - `InitialPacketHandler` → `AuthenticationPacketHandler` →
    `SetupPacketHandler` → `GamePacketHandler`;
  - OAuth browser/device flows, session service, token refresh, encrypted
    credential store;
  - `--auth-mode`, `--session-token`, `--identity-token`, singleplayer token
    path;
  - offline/dev mode for local testing.
- Abstract the Hytale services behind traits so a service change does not stall
  development.
- Keep the exact packet-order state machine; it is part of the client contract.

### 5.9 Plugins, modules, events

- Core modules become Rust crates registered in a startup manifest with explicit
  dependencies, mirroring `PluginManifest` ordering.
- Third-party code plugins: a versioned Rust plugin trait/registry for v1; a
  dynamic-library or WASM ABI is a later decision. Asset packs (`mods/` with
  `manifest.json`) are supported from the start.
- Typed event bus with priorities and cancellable events, modeled on
  `EventBus`/`EventRegistry`.
- Java plugins are not loadable. Provide asset-mod compatibility and a migration
  guide; do not build a JVM bridge unless the scope explicitly changes.

---

## 6. Verification: Java remains the oracle

Every subsystem gets a golden-data gate before it is considered done.

| Layer | Java oracle | Rust gate |
|---|---|---|
| Registries | existing `tools/java-dumper` | already passed; keep green |
| Assets | new per-store decoded dump + packet bytes | byte-identical asset packets |
| Chunk generation | existing golden chunks | already passed; keep green |
| Chunk network packets | `SetChunk`/`SetColumn`/`SetChunkTintmap` for golden chunks | byte-identical payloads |
| Chunk storage | Java-written region files + BSON | read → write → Java reads back |
| Protocol | every packet serialized from Java | round-trip + byte-identical |
| Handshake/join | packet trace from Java/bot client | same packet sequence and deterministic fields |
| World behavior | scripted Java scenarios | same blocks/entities/positions after replay |

Additional test layers:

- `cargo-fuzz` on the protocol decoder and asset parser.
- `client-bot` (Rust) for join/stream/soak tests without a human client.
- Differential harness: same seed/assets, Java vs Rust, compare chunks and
  packet payloads.
- Performance budgets: boot time, asset load, chunk generation, tick time, join
  time, memory.
- CI: `cargo test`, `clippy -D warnings`, `fmt --check`, `protocol-gen --check`,
  golden diffs.

---

## 7. Milestones

### M0 — Scope and scaffolding

- Freeze the v1 contract and non-goals above.
- Create crate skeletons and CI.
- Audit the 120 asset schemas against the 54 stores; list manual codecs.
- Spike protocol IR extraction and `IndexedStorageFile` decoding.
- Add golden-fixture directories and naming conventions.

**Exit:** every v1 subsystem has a crate, an owner, and a golden-data plan.

### M1 — Protocol and transport

- `protocol-dumper` + `protocol-gen`; all 389 packets/445 structs/177 enums
  generated.
- Framing, varints, nullable fields, zstd, channels, registry.
- `hytale-net`: quinn server, ALPN, mTLS, client-cert capture, stream/channel
  mapping.
- Handshake state machine skeleton; bot sends `Connect` and reaches auth/setup.

**Exit:** all packets round-trip; bot completes the protocol handshake; protocol
golden tests pass.

### M2 — Asset foundation

- Asset pack VFS, load-order graph, `Parent` inheritance, index maps, validation.
- Schema-driven typed assets and P0/P1 stores.
- Asset packet encoders for P0/P1; golden byte comparison.
- `--validate-assets` CLI.

**Exit:** Rust produces Java-identical `UpdateBlockTypes`/`UpdateItems`/etc. for
the default pack; registries match the live/golden snapshot.

### M3 — Runtime, world, storage, worldgen

- `hytale-core` runtime: config, logging, events, tasks, module manifests.
- `hytale-world`: universe/world lifecycle, runtime chunk sections/palettes,
  heightmaps, lighting, player data.
- `hytale-storage`: `IndexedStorageFile`, BSON chunk/player codecs,
  backup/recovery.
- Integrate `worldgen-server`, add the query handle and `WorldGenRuntime`.
- Implement `Default`/`Flat`/`Void`/`Dummy` providers.
- Headless CLI boots, creates/loads a world, generates chunks, saves/reloads.

**Exit:** headless server round-trips a Java world; generated chunks convert to
runtime chunks; save/load matches Java.

### M4 — Client join and streaming

- `hytale-auth` and the full connection state machine.
- Setup, asset streaming, world settings, player spawn.
- Chunk tracker/view radius, chunk packet streaming, lighting packets, unload.
- Player movement and basic block place/break.
- Reconnect and clean disconnect.

**Exit:** matching vanilla client connects, spawns, sees terrain, moves, edits
blocks, disconnects, reconnects, and the world persists. This is **v1**.

### M5 — Core gameplay parity

- Inventory/items, interactions, block physics/ticks/fluids, collision/physics,
  entity stats/effects, time/weather, projectiles, prefab spawner, block health,
  UI basics.
- Port the core `modules/` subset (entity, interaction, collision, physics,
  block, item, time, weather, etc.).

**Exit:** scripted gameplay scenarios match Java; no regressions in v1 gates.

### M6 — Builtin modules and NPC

- `hytale-builtin` feature-gated modules and `hytale-npc`: AI, pathfinding,
  flocking, spawning, combat, encounters, instances, adventure plugins, world
  map/UI/editor.
- `builtin/HytaleGenerator` (the density/assignment worldgen engine, 783 files)
  is a separate project; do not fold it into CoreServer v1.

**Exit:** selected builtin modules run with parity tests; Java plugin support
remains out of scope.

### M7 — Production hardening

- Security review, fuzzing, rate limits, auth failure handling.
- Performance tuning and memory ceilings.
- Cross-platform builds, packaging, docs, plugin/mod API.
- Long-running soak tests and crash recovery.

---

## 8. Workstreams and dependency order

| Workstream | Starts | Depends on | Critical path |
|---|---|---|---|
| Protocol + transport | M1 | — | yes |
| Asset pipeline | M2 | protocol types for asset packets | yes |
| Runtime/ECS | M3 | — | yes |
| World/chunk/storage | M3 | assets (registries), runtime | yes |
| Auth/join/streaming | M4 | protocol, assets, world, runtime | yes |
| Gameplay modules | M5 | join/streaming | no |
| Builtin/NPC | M6 | gameplay | no |
| Plugin API | M6/M7 | runtime | no |

Parallel tracks: protocol/net and runtime/ECS can start immediately; asset work
can start once protocol types exist; world/chunk can start with a
`Registries::empty` stub and swap in the real asset pipeline later.

---

## 9. Main risks and mitigations

1. **Protocol codegen fidelity.** Nullable bitfields, unions, bit-packed fields,
   and per-field string bounds are easy to get subtly wrong. Mitigation: IR
   generated once, every packet round-tripped and byte-compared, property tests,
   fuzzing.
2. **Asset codec semantics.** `Parent` inheritance and per-codec merge rules are
   the most under-documented part. Mitigation: schema-driven intermediate
   representation, golden dumps per store, port stores in dependency order, fail
   closed with validation.
3. **Chunk serialization.** Palette layouts and block-migration versions must
   match. Mitigation: port `BlockSection`/`FluidSection`/`EnvironmentSection`
   serialization directly from Java, binary fixtures, Java-read-back test.
4. **IndexedStorage compatibility.** Header, zstd blobs, segment chains, and BSON
   component names must match exactly. Mitigation: port `IndexedStorageFile`
   first, then chunk/player codecs; use Java-written fixtures.
5. **Auth service dependence.** OAuth/session endpoints can change or be
   unavailable. Mitigation: trait-based auth, offline/dev mode, test against a
   real client early.
6. **World tick/ECS semantic drift.** A hybrid ECS changes component lifecycle
   details. Mitigation: unit and scenario tests around load/save/transfer, keep
   chunk data outside archetypes, avoid promising Java ECS API parity.
7. **Scope creep.** `builtin/` and `NPC/` together are larger than CoreServer.
   Mitigation: freeze v1 at join + streaming + block edits; gate everything else
   behind M5/M6.
8. **Performance.** Asset loading and chunk streaming are the likely
   bottlenecks. Mitigation: parallel pack decode, `Arc` registries, pre-encoded
   `CachedPacket` equivalents, chunk packet batching, benchmarks from M2 onward.
9. **Mod ecosystem expectations.** Java plugins will not run. Mitigation:
   asset-pack compatibility, clear migration docs, and a Rust plugin API; no
   silent promises of Java compatibility.

---

## 10. Immediate next steps

1. **Protocol IR spike:** add `tools/protocol-dumper`, parse
   `Protocol/target/generated-sources/...`, emit `protocol.json`, and confirm the
   counts (389/445/177) against `ProtocolSettings`.
2. **Protocol codegen prototype:** add `crates/hytale-protocol`; generate
   `Connect`, `Ping`, `Disconnect`, `ServerDisconnect`; prove encode/decode
   round-trip and framing.
3. **QUIC transport spike:** add `crates/hytale-net` with `quinn`, ALPN
   `hytale/3`, mTLS, client-cert extraction, and the length/id framer.
4. **Asset golden dump:** extend `tools/java-dumper` to emit `UpdateBlockTypes`,
   `UpdateItems`, `UpdateEnvironments`, `AssetInitialize`/`AssetPart`, and
   per-store decoded maps for the default pack.
5. **Asset loader spike:** implement `hytale-assets` for `BlockType`, `Item`,
   `Fluid`, `Environment`; compare against `testdata/golden/registries.json` and
   the new asset golden.
6. **Runtime chunk spike:** define `ChunkColumn`/sections with palettes; implement
   `from_generated_chunk`; produce a Java `SetChunk` golden fixture and match it.
7. **Storage spike:** port `IndexedStorageFile` and read one Java region file;
   decode `WorldChunk`/`BlockChunk` BSON.
8. **Client bot skeleton:** send `Connect`, log the handshake, and use it as the
   first end-to-end gate.

The first real proof point is not "CoreServer compiles"; it is **a Rust binary
that loads the vanilla asset pack, writes a Java-compatible region file, and
serves one matching client through spawn and chunk streaming**. Everything else
in this plan is built to make that proof point reliable and then widen it.
