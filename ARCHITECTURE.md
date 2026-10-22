# Architecture — CoreServer → Rust

This is the living architecture document for the CoreServer rewrite. The phase
plan and scope live in [`plan.md`](plan.md); this file records the crate layout,
runtime topology, invariants, and the Java→Rust mapping as modules land.

- **Goal:** a Rust server that loads unmodified Hytale assets, hosts worlds,
  speaks the matching Hytale protocol to a vanilla client, and reads/writes
  vanilla `IndexedStorage` saves.
- **Status:** session 8 added runtime world storage: `World`/`WorldStorage` load
  a saved chunk from a region file into the runtime `ChunkColumn` map, track dirty
  chunks, apply block edits and save them back. The reverse storage gate is
  green: the real Java `ChunkStore` codec reads a Rust-mutated region and resolves
  the edited block. Session 9 added `WorldConfig` v4 load/save for the per-world
  `config.json`, preserving unknown fields, with a Java-written fixture and a
  Java read-back probe. Session 10 added player saves
  (`DiskPlayerStorageProvider`): the `EntityStore` holder document with typed
  `UUID`/`Transform`/`WorldGenId`, opaque preservation of every other component,
  and a Java read-back gate. That sits on session 7's headless boot, session 6's
  BSON chunk storage, session 5's P0 asset registry, session 4's runtime chunk
  packet encoders and session 3's full protocol codegen. Wiring config and player
  load into the boot, generation of missing chunks, the join path and the live
  server loop are still open; see [`NEXT_STEPS.md`](NEXT_STEPS.md) and
  [`tasks/README.md`](tasks/README.md).
- **Reused base:** `hytale-math`, `procedural`, `worldgen-model`,
  `worldgen-hytale`, `worldgen-cli`, `worldgen-server`.
- **Archived worldgen docs:** [`done/ARCHITECTURE.md`](done/ARCHITECTURE.md),
  [`done/PORTING.md`](done/PORTING.md),
  [`done/PORTING_HISTORY.md`](done/PORTING_HISTORY.md).

Paths are relative to the repository root unless noted.

---

## 1. Workspace layout

The existing worldgen crates stay where they are. The server adds ten crates:

```text
crates/
  hytale-protocol/   protocol IR output, packets/structs/enums, registry, framing
  hytale-assets/     asset-pack VFS, store graph, Parent resolver, typed assets,
                     index maps, asset packet encoders
  hytale-core/       config/options, logging, event bus, registry, tasks, modules
  hytale-storage/    IndexedStorageFile, BSON component codecs, backups
  hytale-world/      universe, world, runtime chunks, heightmaps, lighting, player data
  hytale-entities/   ECS wrapper, components, systems, player/entity lifecycle
  hytale-net/        QUIC transport, streams, sessions, packet state machine
  hytale-auth/       OAuth device/browser, session service, JWT, credential store
  hytale-commands/   command framework, argument parsing, built-in commands
  hytale-server/     binary, startup wiring, module registration

tools/
  protocol-dumper/     generated Java protocol sources → protocol IR
  protocol-gen/        protocol IR → Rust source
  asset-golden-dumper/ Java asset packet / decoded-store fixtures
  packet-trace/        Java handshake/join packet traces
  client-bot/          Rust scripted client for join/stream tests
```

Planned dependency direction (arrows point from dependent to dependency):

```text
hytale-server
  ├─ hytale-net ── hytale-protocol
  ├─ hytale-auth ─ hytale-core
  ├─ hytale-commands ─ hytale-core
  ├─ hytale-world ── hytale-assets ── hytale-protocol
  │                  hytale-storage ─┘
  ├─ hytale-entities ─ hytale-world
  └─ hytale-core

worldgen-server ── worldgen-hytale ── worldgen-model ── hytale-math
```

Rules:

- `hytale-protocol` is a leaf; it may depend on wire-format crates only
  (`bytes`, `zstd`), never on `hytale-assets` or `hytale-world`.
- `hytale-assets` and `hytale-storage` are independent. `hytale-world` may use
  both, but neither may use `hytale-world`.
- `worldgen-*` must not start depending on `hytale-world`; the adapter direction
  is `hytale-world → worldgen-server`.
- No crate reaches into another's private modules. Shared types move down.

---

## 2. Runtime topology

```text
tokio runtime
  ├─ QUIC listener (quinn) ── per-connection tasks ── packet decode
  ├─ auth/session tasks (reqwest, JWT refresh)
  ├─ asset load (blocking pool, once at boot)
  └─ storage pool (blocking reads/writes)

Universe (shared)
  ├─ worlds: name → World
  ├─ players: uuid → PlayerRef
  └─ universe resources / storage

World (one dedicated OS thread each)
  ├─ tick queue / scheduled tasks / commands
  ├─ ChunkStore (custom slot map; runtime sections, heightmaps, lighting)
  ├─ EntityStore (bevy_ecs World)
  ├─ WorldGenRuntime (worker pool + read-only query generator)
  ├─ storage loader/saver
  └─ world map / time / weather state
```

Why a world thread: Java's `World` is a `TickingThread`; commands, entity
systems, chunk loads/saves and network-driven changes are serialized on it.
Keeping one thread per world preserves that model and removes a large class of
races. Network, auth, and storage are async; results cross into the world thread
through channels.

Packet routing: the connection task decodes the frame, identifies the player
session, and dispatches to the owning world thread. Chunk and world-map streams
have their own QUIC streams and channel ids; a packet received on the wrong
channel is a protocol violation.

---

## 3. Protocol layer

Source of truth: the auto-generated Java under
`.hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/`
plus the hand-written `Protocol/src/main/java/com/hypixel/hytale/protocol/io/`
helpers.

Pipeline:

1. `tools/protocol-dumper` scans/parses the generated sources into an IR
   (`protocol.json`): class kind, packet metadata, fields, offsets, bounds,
   enums, unions, nullability.
2. `tools/protocol-gen` emits Rust:
   - `Packet` trait (`compute_size`, `encode`, `decode`);
   - packet/struct/enum modules;
   - `PacketRegistry` (id, direction, channel, max size);
   - `ProtocolSettings` (CRC, version, build, counts).
3. `hytale-protocol` supplies hand-written wire primitives: varint, fixed
   integers, UUID, nullable bitsets, strings, arrays, framing, zstd.

Generated code is committed. CI runs `protocol-gen --check` and diffs; a
protocol version bump is a regeneration, not a manual edit.

Wire invariants:

| Item | Rule |
|---|---|
| Frame | `[u32 LE payload length][u32 LE packet id][payload]` |
| Fixed fields | little-endian |
| UUID | two big-endian `i64` |
| Varint | 7-bit little-endian groups, max 5 bytes |
| Nullable fields | presence bitset at the fixed-block start |
| Fixed strings | unused bytes zero-filled |
| Compressed packets | zstd frame, content size present |
| Channels | `Default`, `Chunks`, `WorldMap`, `Voice` |

The protocol version constants are pinned. Matching the client build is part of
the v1 acceptance contract.

---

## 4. Asset layer

Source of truth: `Assets/Assets.zip` (and unpacked `Assets/extracted/`) plus the
120 JSON schemas under
`.hytale-source/hytale-shared-source/HytaleAssets/Schema/`.

Components:

- **Pack VFS**: read-only zip/directory access; mods overlay packs by key.
- **Store graph**: the 54 `HytaleAssetStore` registrations from
  `CoreServer/.../asset/AssetRegistryLoader.java`, with `loadsBefore` /
  `loadsAfter` ordering and pre-added assets.
- **Parent resolver**: `Parent` inheritance is resolved on an intermediate
  `serde_json::Value`/`bson::Document`; field-level merge rules are per asset
  type. Cycles and missing parents are hard errors in validation mode.
- **Typed assets**: generated from the JSON schemas where practical, with manual
  types for computed/discriminated assets. Do not port the Java `Codec` library.
- **Index maps**: `DefaultAssetMap`, `IndexedAssetMap`,
  `IndexedLookupTableAssetMap`, `BlockTypeAssetMap` equivalents. Id assignment is
  deterministic and owned by the Rust pipeline. `hytale-assets::registry`
  reproduces the Java load order for file-backed stores (including the
  `ConcurrentHashMap` waiting-asset wave); block types contained in item
  documents use the Rust snapshot's own order after the preloads and standalone
  block files.
- **Packet encoders**: one per store, matching the Java `toPacket()` output; the
  39 asset packet types are golden-tested byte-for-byte (one per store that has
  a packet generator).

Asset ids must be assigned once at boot and used by worldgen, network packets,
and storage. The registry snapshot is the server's own; it is never re-derived
from a golden dump at runtime.

---

## 5. World and chunk layer

`worldgen-model::GeneratedChunk` is the **generation buffer**, not the runtime
chunk. The runtime model follows Java's section layout:

```text
ChunkColumn
  ├─ WorldChunk flags / keep-alive
  ├─ BlockChunk: tint, legacy height, environment sections
  ├─ HeightmapColumn (Legacy or Tree)
  ├─ sections[10]
  │    ├─ BlockSection: block/filler/rotation palettes, ticking, light
  │    ├─ FluidSection: fluid palette + nibble levels
  │    ├─ EnvironmentSection
  │    ├─ EntitySection
  │    └─ BlockComponentSection
  └─ EntityChunk / BlockComponentChunk
```

Each section is 32×32×32. Palettes are `Empty`, `Byte`, `HalfByte`, `Short`;
the serialized byte layout must match Java exactly because it is used both on
the wire (`SetChunk`) and on disk (`BlockSection.Data`).

`from_generated_chunk` copies the generation arrays into sections, selecting
palettes. It is the only bridge between `worldgen-model` and `hytale-world`.

Chunk lifecycle:

1. Player view radius requests chunks.
2. `ChunkStore` returns a loaded chunk or schedules a storage load.
3. Missing chunks are generated by `WorldGenRuntime`.
4. The world thread installs the chunk, updates heightmaps/lighting, and sends
   the relevant packets.
5. Dirty chunks are saved through `IndexedStorageFile`; unload follows the
   keep-alive rules.

Network and storage encodings are tested against the same golden chunk fixtures.

The first headless boot (`hytale-server headless`) wires the pieces together for
one chunk: P0 asset registry → `worldgen-model::Registries` → `worldgen-server`
worker → `GeneratedChunk` → runtime `ChunkColumn` → BSON codec → region file.
`WorldGenOptions.registries` carries the in-memory snapshot so generation,
storage and (later) packets use the same ids.

`hytale-world::world` closes the loop from disk: `WorldStorage` caches region
files and moves chunks in and out of `IndexedStorageFile` blobs, while `World`
owns the loaded columns, dirty set and `get_block`/`set_block`. The
`load-chunk` CLI loads, edits and re-saves one stored chunk. Missing-chunk
generation, unload/keep-alive and the live world thread are not wired yet.

---

## 6. Entity layer

- `hytale-entities` wraps `bevy_ecs`.
- Each `World` owns an `EntityStore` (`bevy_ecs::World` plus resources).
- Components are Rust structs; a registration wrapper provides stable type ids,
  serialization, and copy/clone semantics.
- Systems are scheduled on the world thread. Java's `CommandBuffer`/`Ref`
  behavior is mapped to deferred commands and entity ids; exact ECS API parity is
  not a goal.
- `PlayerRef` is a handle containing UUID, username, connection, world, and
  entity reference.
- Chunk components stay in `hytale-world`; block entities that need ECS
  behavior live in the entity store and reference a block position.

The initial component set is deliberately small: transform, UUID, player,
persistent display name, world-gen id, movement/velocity, and the components
needed by block placement. Gameplay components arrive in M5.

---

## 7. Storage layer

Target: vanilla `IndexedStorage` compatibility.

- `IndexedStorageFile` v1: magic `"HytaleIndexedStorage"`, version 1, 1024
  blobs, 4096-byte segments, zstd-compressed blob payloads.
- Region files: `<regionX>.<regionZ>.region.bin`; blob index is
  `ChunkUtil.indexColumn(localX, localZ)`.
- Chunk payloads: BSON documents of ECS components, using the Java component
  names and `Data` byte arrays. The section bytes are the exact palette blobs
  from §5. Implemented by `hytale_world::chunk::storage::ChunkCodec` and
  `RegionFile`; block/fluid/environment palettes store asset names, and the
  legacy heightmap uses `ShortBytePalette`.
- Player data: `DiskPlayerStorageProvider` layout in
  `hytale_world::player_data`. `<world>/players/<uuid>.json` is the
  `EntityStore` holder (`{"Components": ...}`), with typed `UUID`, `Transform`
  and `WorldGenId` and opaque preservation of all other components. The
  Java-written fixture and the Java read-back probe are the gates.
- World config: `WorldConfig` v4 (`config.json`) lands in
  `hytale_world::world_config`: typed server-owned fields, opaque provider
  sections, Java extended-JSON UUIDs, `.bak` + atomic save, and unknown-field
  preservation. The Java-written fixture and the Java read-back probe are the
  gates.

A Java-written world fixture is the compatibility gate in both directions.
Rust reads the committed `testdata/golden/server/storage/java-world/` fixture;
`crates/hytale-world/tests/world_storage.rs` and the `load-chunk` CLI mutate a
chunk and write it back; `tools/java-dumper/run-reload-check.sh` then reads the
Rust-written region with the real Java `ChunkStore` codec and checks the edited
block. RocksDB support is not part of v1.

---

## 8. Network and auth

- QUIC via `quinn`; TLS via `rustls`.
- ALPN `hytale/<protocol version>`; accept the previous version only to send a
  structured “outdated” disconnect.
- mTLS required. Self-signed client certificates are accepted; the JWT
  certificate binding (`cnf.x5t#S256`) is the actual identity check.
- Connection states: `Initial` → `Authentication` → `Setup` → `Game`.
  The exact packet order is part of the client contract and is covered by
  packet-trace fixtures.
- Auth providers: OAuth browser, OAuth device, external session, singleplayer
  token, offline/dev. All behind a trait so service changes are isolated.

---

## 9. Modules, events, plugins

- Core modules are Rust crates registered in a startup manifest with explicit
  dependencies, mirroring `PluginManifest` ordering.
- Event bus: typed events, priorities, cancellable events, sync/async dispatch.
- Asset packs are the supported mod format from day one.
- Third-party code plugins use a Rust plugin trait in v1. A dynamic-library or
  WASM ABI is a later decision; Java plugins are not supported.
- Hot reload is out of v1, except for asset packs if the file watcher lands
  early.

---

## 10. Java → Rust mapping (living table)

This table starts at the package level and will be split per class as modules
land. “Planned” means the crate exists in the architecture but is not implemented.

| Java area | Rust target | Status |
|---|---|---|
| `WorldGenerator/**` | `worldgen-hytale` | ported, golden-verified |
| `Math/**` (generation subset) | `hytale-math` | ported |
| `ProceduralLibrary/**` (generation subset) | `procedural` | ported |
| `CoreServer/.../world/worldgen/{Generated*}` | `worldgen-model::chunk` | ported as generation buffer |
| `CoreServer/.../world/worldgen/IWorldGen` | `worldgen-server` adapter | ported |
| `Protocol/**` generated | `hytale-protocol` | full codegen landed (M1): 389 packets, registry, `ProtocolSettings`, round-trip gate |
| `Protocol/.../io/*` wire helpers | `hytale-protocol::io` | varint/fixed/UUID/strings/bitsets/framing/zstd + big-endian storage accessors landed (M1) |
| `AssetStore/**`, `Codec/**` | `hytale-assets` | pack VFS + `Parent` resolver + store order landed; P0 asset-packet goldens dumped (M2); P0 registry/typed stores and the `worldgen-model` bridge landed (M2) |
| `CoreServer/.../asset/**` | `hytale-assets` | 54-store inventory landed (M2); P0 loader, contained block-type extraction and deterministic ids landed (M2) |
| `Storage/IndexedStorageFile` | `hytale-storage` | v1 container landed (M3) |
| `CoreServer/.../universe/world/chunk/**` | `hytale-world::chunk` | palette, block/fluid/environment sections, `ChunkColumn::from_generated_chunk`, heightmap, tint palette and the `SetColumn`/`SetChunk`/tint/environment/fluid packet builders landed (M3); all 46 `chunks-pkts` payloads match Java byte-for-byte |
| `CoreServer/.../universe/{Universe,World}` | `hytale-world` | runtime chunk model and one-chunk headless boot landed (M3); `World`/`WorldStorage` load/mutate/save (M3) and `WorldConfig` v4 load/save (M3) landed; universe lifecycle, boot-time config wiring, missing-chunk generation and player load pending |
| `CoreServer` entry point / `Options` | `hytale-server` | library + `headless` CLI landed (M3); network/auth/command wiring pending |
| `Component/**` | `hytale-entities` wrapper on `bevy_ecs` | planned (M3) |
| `CoreServer/.../universe/world/storage/**` | `hytale-storage` + `hytale-world::{chunk::storage, world, world_config, player_data}` | `IndexedStorageFile` container, BSON chunk holder codec, named-palette storage, legacy heightmap, region files, `WorldConfig` v4 and player holder documents landed (M3); Java fixture read gates and all Rust write → Java read gates green (M3). Boot-time load/save wiring pending |
| `CoreServer/.../io/**`, `transport/**` | `hytale-net` | QUIC mTLS spike + `PeerIdentity` landed (M4); packet decoding/session pending |
| `CoreServer/.../auth/**` | `hytale-auth` | planned (M4) |
| `CoreServer/.../command/**` | `hytale-commands` | planned (M4) |
| `CoreServer/.../permissions/**` | `hytale-commands`/`hytale-core` | planned (M4) |
| `CoreServer/modules/**` | `hytale-world` (core subset), later module crates | planned (M5) |
| `builtin/**` | `hytale-builtin` feature crates | deferred (M6) |
| `NPC/**` | `hytale-npc` | deferred (M6) |

Deliberate exclusions are in [`REMAINING.md`](REMAINING.md).

---

## 11. Invariants

1. **No JVM at runtime.** Java is an oracle and a fixture producer only.
2. **No Java `Codec` port.** Use schemas + serde + explicit merge rules.
3. **No Java ECS API parity.** Preserve observable behavior; expose Rust-native
   components/systems.
4. **No hand-written protocol packets** except the small codegen bootstrap set.
5. **No guessed bytes.** Every binary format is validated against Java output.
6. **No `unsafe`** without an explicit task and a documented safety argument.
7. **Worldgen stays deterministic.** Existing golden chunk tests must stay green.
8. **One writer per shared file.** Protocol/asset/chunk formats have a single
   owning module; other crates consume them.

---

## 12. Data flow summary

```text
Assets.zip ── hytale-assets ── registries ──┬─ hytale-world/worldgen
                                             ├─ hytale-assets packet encoders
                                             └─ hytale-storage (block ids)

worldgen-hytale ── GeneratedChunk ── from_generated_chunk ── runtime ChunkColumn
                                                               ├─ SetChunk/SetColumn → client
                                                               └─ BlockSection.Data → IndexedStorage

client ── QUIC frames ── hytale-protocol decode ── hytale-net session
                                                        └─ world thread ── ECS/chunk changes
```

The next concrete step is in [`NEXT_STEPS.md`](NEXT_STEPS.md). Keep this file
updated when a crate gains its first real module or a Java package is mapped to
specific Rust files.
