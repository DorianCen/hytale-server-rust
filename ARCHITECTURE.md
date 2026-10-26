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
  and a Java read-back gate. Session 11 composed the three parts into
  `WorldSave` and `hytale-server world-boot`, with one Java gate
  (`run-save-check.sh`) that verifies the config, player and chunk in a
  Rust-written save. Session 12 added demand loading: `WorldSave::ensure_chunks`
  loads stored chunks and generates missing ones around the player, and the
  shared `WorldGenerator` boot is used by both `headless` and `world-boot`.
  Session 13 moved demand onto a dedicated `WorldThread`: a `ChunkView` plan
  orders loads nearest-first, a view change cancels queued loads and unloads
  chunks that left, and `Tick` emits the chunk events the network stream will
  consume. That sits on session 7's headless boot, session 6's BSON chunk
  storage, session 5's P0 asset registry, session 4's runtime chunk packet
  encoders and session 3's full protocol codegen. Session 14 added
  `chunk::stream`: a `ChunkLoaded` event becomes a Java-ordered
  `SetColumn`/`SetChunk`/environment batch, framed with the protocol's
  zstd/frame layer via `WorldThread::chunk_packets`. Session 15 added
  `hytale-net::chunk_stream`: a server-opened uni stream writes those frames and
  the client reader validates/decompresses them; `client-bot` decodes the chunk
  packets into summaries. Session 16 bridged the live world thread to the
  stream: `chunk_streamer::send_view` ticks the `WorldThread`, builds the
  Java-ordered batches and writes them, and `hytale-server chunk-serve` +
  `client-bot --chunks` exercise it end to end over QUIC. Session 17 added
  incremental updates: `send_updates` forwards `ChunkLoaded` batches and
  `UnloadChunk` packets after a view change (`chunk-serve --move-demo`). Session
  18 started the asset packet encoders: `PhysicalMaterial` and
  `BlockBoundingBoxes` now match the Java payloads byte-for-byte. Session 19
  completed the first dev/insecure join: a generic frame stream, the
  Connect → insecure options → ConnectAccept → setup handshake in
  `hytale-server::session`, and `client-bot --dev-join` receiving 21 chunk
  packets. Session 20 added the local EdDSA authentication profile
  (`AuthGrant` → `AuthToken` → `ServerAuthToken`, `cnf.x5t#S256` certificate
  binding), common-asset index streaming, the encodable config asset packets,
  translations, and the `ClearChunks`/`JoinWorld`/`ClientReady` Game transition
  before the chunk snapshot, plus `AddToServerPlayerList` and the joining
  player's self `EntityUpdates` spawn. Session 21 added the initial world
  time/gameplay packets, the `readyForGameplay` → `ClientTeleport` exchange, a
  persistent `Chunks` stream and a 50 ms game loop that turns `ClientMovement`
  into `WorldThread::set_view` updates. Session 22 added the `Environment` and
  `Fluid` config packets with a Rust-owned tag registry and matching
  `ServerTags`. Session 23 added `BlockType`/`Item`, including item `State`
  contained assets and the Rust-owned group registry, so all six self-contained
  P0 config packets stream during setup. The first vertical slice is archived in
  [`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md).
  Session 26 completed the M2 child stores: block decals/particles/sounds,
  FluidFX and SoundEvent; item sounds/qualities/reticles/categories/player
  animations; and the TagPattern store with `UpdateTagPatterns`. All ten child
  packets are byte-identical to Java; tag patterns match Java's op trees with a
  deterministic Rust-owned tag index map. Session 27 wired the config burst
  into the join and session 47/T57b extended it to 21 packets with the entity
  stat/effect stores, caches the encoded `ConfigAssets` once per world thread,
  adds `hytale-server --validate-assets`, and extends `client-bot` to assert the
  full config order. Session 28 landed the shared `hytale-core`
  runtime: typed layered config, text/JSON logging init, the async
  scheduler/shutdown token, the ordered sync/async event bus and the module
  manifest registry. Session 29 added the persistent universe catalog, the
  provider seam and per-world `UniverseRuntime` threads with save-on-unload and
  the `universe` CLI. Session 30 added the persistent multi-session `serve`
  listener: one endpoint/universe, independent session tasks, shared per-world
  config cache, connection limits, idle timeout and graceful shutdown. Session
  31 added the Java-compatible world clock (`WorldClock`), the internal
  `GameplayConfig` store with a tag-resolved `WorldGameplayConfig` join packet,
  per-second `UpdateTime` broadcasts, and `GameTime`/pause persistence through
  `config.json`. Session 32 decoded the saved model/skin/inventory components and
  the insecure handshake skin into the self-spawn `EntityUpdates`, with item-id
  validation and Java empty-slot defaults; a Java equipment fixture covers the
  codecs. Session 33 added the per-world `PlayerRegistry`: unique network ids,
  full player-list snapshots/add/removes, view-filtered entity
  spawn/movement/removal broadcasts, a per-session message queue drained by the
  game loop, and a two-bot loopback test covering mutual spawn, movement,
  disconnect and rejoin. Session 34 completed the v1 edit loop: validated
  `MouseInteraction` break/place, item→block resolution, dirty chunk saves and
  view-filtered `ServerSetBlock` broadcasts. Session 35 added the v1 lifecycle:
  per-session registered chunk views whose loaded set is the union of all live
  views, `ClientDisconnect`/idle/transport teardown that saves the player
  transform and removes the view/entity, per-UUID player snapshots for
  reconnect, and loopback coverage for join → move → edit → disconnect →
  reconnect plus idle-timeout teardown. Session 36 closed M4/v1 with
  `hytale-net::trace`: server-side JSONL packet ids/channels/order/payload
  hashes, a committed lifecycle fixture, a live replay gate, and a real-client
  capture runbook that records the `ViewRadius`-on-Chunks requirement. Session
  37 added the inventory core: the `hytale-inventory` crate (Java-compatible
  item stacks, six player containers, atomic move/split/merge/swap/drop/pickup),
  inventory save/load in player holders, initial inventory/active-slot packets,
  authoritative held-item consumption on placement and inventory persistence
  with the transform. Session 38 added the interaction core: `hytale-interactions`
  loads the Java root/interaction assets with parent inheritance, walks the
  structural/variable graph and dispatches block `Use` actions (doors, state
  changes, containers/benches) before T46 placement. Session 39 added movement
  authority: the internal `MovementConfig` store and join
  `UpdateMovementSettings` packet, `hytale-world::collision` shapes from
  `BlockType`/`BlockBoundingBoxes` (Java rotations + filler origins), the
  `hytale-entities::movement` AABB sweep/step/gravity integrator, bounded
  `ClientMovement` validation and authoritative `ClientTeleport` corrections.
  Session 40 added block simulation: a transient scheduled-tick queue in
  `BlockSection`, `hytale-world::simulation` (deterministic random ticks,
  support loss, falling blocks, default fluid flow/collision, gathering drops
  and transient block health) and one simulation pass per 50 ms in the server
  loop with view-filtered block/fluid broadcasts. Session 41 closed the T52
  parity follow-ups: T52b adds partial-hitbox fluid face coverage, waterlogging,
  finite/fire tickers, disabled tickers and Java ticker inheritance; T52c adds
  support-distance chains, filler/self-face rules, `WAITING_CHUNK`, light/fluid
  `SpreadTo` gates, weighted placement, scheduled growth and a tick-suppression
  seam; T52d stores health/fragility in the Java `BlockHealthSection` component
  with legacy `BlockHealthChunk` migration and pending deltas for T57. T53
  (stats/effects/damage plus the Java entity-gameplay fixture), T54 (time and
  weather), T55 (projectiles plus traversal/mount seams), T56 (prefab
  entities/block components plus the persisted runtime store), T57
  (block-health/runtime-entity/world-map presentation) and T57b's map-image and
  container-window slice are done; M6 starts with T58, whose NPC
  navigation/steering/flock core is done (T58b/T58c). T59's spawn configuration,
  role combat profiles, Java damage ranges and scripted actor are landed, and
  T59b adds the live WorldThread NPC store/persistence/session tick,
  movement-mode spawn-position/environment selection, explicit population
  ticks, loaded-section segment counting and real `BlockSet` floor resolution;
  automatic environment enumeration, beacons and encounters remain
  T59b/T59c. See
  [`plan.md`](plan.md), [`NEXT_STEPS.md`](NEXT_STEPS.md)
  and [`tasks/README.md`](tasks/README.md).
- **Reused base:** `hytale-math`, `procedural`, `worldgen-model`,
  `worldgen-hytale`, `worldgen-cli`, `worldgen-server`.
- **Archived worldgen docs:** [`done/world-generation/ARCHITECTURE.md`](done/world-generation/ARCHITECTURE.md),
  [`done/world-generation/PORTING.md`](done/world-generation/PORTING.md),
  [`done/world-generation/PORTING_HISTORY.md`](done/world-generation/PORTING_HISTORY.md).

Paths are relative to the repository root unless noted.

---

## 1. Workspace layout

The existing worldgen crates stay where they are. The server crates:

```text
crates/
  hytale-protocol/   protocol IR output, packets/structs/enums, registry, framing
  hytale-assets/     asset-pack VFS, store graph, Parent resolver, typed assets,
                     index maps, asset packet encoders
  hytale-core/       config/options, logging, event bus, registry, tasks, modules
  hytale-storage/    IndexedStorageFile, BSON component codecs, backups
  hytale-world/      universe, world, runtime chunks, heightmaps, lighting, player data
  hytale-inventory/  item stacks, containers, player inventory and transactions
  hytale-interactions/ interaction graph loader/executor and block metadata
  hytale-entities/   ECS wrapper, components, systems, player/entity lifecycle
  hytale-npc/        role/flock catalogs, A* navigation, path following, steering,
                     perception, world-spawn selection and combat profiles/actors
  hytale-net/        QUIC transport, streams, sessions, packet state machine
  hytale-auth/       EdDSA JWT keys/claims, certificate binding, local authority;
                     OAuth/session-service adapters planned
  hytale-commands/   command framework, argument parsing, built-in commands
  hytale-server/     binary, startup wiring, session/listener runtime, player
                     registry, module registration

tools/
  protocol-dumper/     generated Java protocol sources → protocol IR
  protocol-gen/        protocol IR → Rust source
  asset-golden-dumper/ Java asset packet / decoded-store fixtures
  client-bot/          Rust scripted client for join/stream/trace tests
```

Planned dependency direction (arrows point from dependent to dependency):

```text
hytale-server
  ├─ hytale-net ── hytale-protocol
  ├─ hytale-auth ─ hytale-core
  ├─ hytale-commands ─ hytale-core
  ├─ hytale-world ── hytale-assets ── hytale-protocol
  │                  hytale-storage ─┘
  │                  hytale-inventory ─ hytale-protocol
  │                  hytale-interactions ─ hytale-assets
  ├─ hytale-entities ─ hytale-world + hytale-protocol
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
  ├─ WorldSave (config.json + players/ + chunk regions)
  ├─ ChunkView plan + demand queue (nearest-first, cancellable)
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
  a packet generator). `hytale-assets::packets` covers all six self-contained
  P0 stores: `PhysicalMaterial` (87) and `BlockBoundingBoxes` (41) are
  byte-exact; `Environment` (61), `Fluid` (83), `BlockType` (40) and `Item`
  (54) are golden-checked by asset key because their tag indexes, block ids and
  group ids are Rust-owned and published consistently (`ServerTags`, the
  deterministic group registry). `FluidFX`/`BlockSoundSet`/`ItemSoundSet`/
  `ItemQuality` child indexes resolve through their loaded stores.
- **Gameplay config**: `hytale-assets::gameplay` reads the internal
  `Server/GameplayConfigs` `DefaultAssetMap` (no numeric ids, no packet
  generator). It models `World`/`Combat`/`Player`, fills absent sections with
  Java `GameplayConfig.DEFAULT` values, resolves block tags through
  `ServerTags`, and builds `WorldGameplayConfig` for the join. The Java asset
  boot cannot validate these files without the camera-effect store; Rust reads
  them directly and falls back per world-config id.
- **Common assets**: `hytale-assets::common` reads the pack-root
  `CommonAssetsIndex.hashes`, advertises the hash/name pairs in
  `WorldSettings.requiredAssets`, and lazily loads + SHA-256-verifies the files
  under `Common/` when the client asks for them. Blob bytes never enter the
  registry snapshot; only the index metadata is kept at boot.

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
owns the loaded columns, dirty set and `get_block`/`set_block`. `WorldSave`
adds the rest of a saved world — `config.json` and `players/` — and is the
object the boot path owns. `hytale-server world-boot` loads or creates the
config/player, loads every stored chunk, applies an optional edit and saves
everything. `hytale-server::world_thread::WorldThread` owns the save, the
shared `WorldGenerator` and the demand queue on one OS thread. A `ChunkView`
plan orders missing chunks nearest-first; `SetView` replaces the base view
(cancelling pending loads) and unloads chunks that left; `Tick` processes a
bounded number of chunks and emits `WorldEvent::ChunkLoaded/ChunkUnloaded`.
Session-owned views registered with `add_view` share one loaded set (the union
of all live views): moving or leaving one player only unloads chunks no other
view needs, and the first session view replaces the startup base view so the
last session leaving releases session-owned terrain. `load-chunk` remains the
one-chunk editor. Chunk streaming and the live tick loop are wired; chunk
load/unload events still use one world-wide queue (per-view filtering is
follow-up work).

Weather (`hytale-world::weather`) loads the `Weather` store (`Unknown` at 0),
parses each environment's 24-hour weighted forecast, applies
`WorldConfig.ForcedWeather`, and reselects deterministically on an in-game hour
boundary using the world seed and forecast seed key. `WorldThread::weather_at`
exposes the effective index for a position; sessions send `UpdateWeather` with
the Java join/change transition durations. `WorldClock::current_hour` and
`moon_phase` complete the time model; presentation-only weather documents stay
with T60.

`WorldClock` (`hytale-world::world_clock`) follows Java
`WorldTimeResource`: game time is an epoch instant, the configured daytime and
nighttime durations are real seconds that map onto the fixed 60/40 game-day
split, and pause freezes progression. `WorldThread` owns one clock, advances it
from wall-clock deltas on each command, and persists `GameTime` plus
`IsGameTimePaused` in `config.json` on save/shutdown. Sessions send
`UpdateTimeSettings`/`UpdateTime` from the shared snapshot; `UpdateTime` is
broadcast once per real second while time runs.

Block edits enter through the client `MouseInteraction` packet. The session
validates the pressed left/right button, block target, reach and held item, then
resolves an item with a `BlockType` to its block id and applies the edit through
`WorldThread` (`set_block` / `place_block`), which marks the chunk dirty for the
normal region save. Placement consumes one item from the authoritative active
hotbar stack (legacy sessions without saved inventory keep the packet-item
fallback) and resyncs `UpdatePlayerInventory`. `PlayerRegistry::broadcast_block`
sends the authoritative `ServerSetBlock` on each interested session's chunks
stream, including the editor. Concurrent writes are serialized by the single
world thread; network broadcasts are queued per session.

Block `Use` interactions are resolved from the merged `BlockType` JSON and the
loaded root/interaction catalog before placement. The executor supports
conditions, replacement variables, serial/parallel/chaining/repeat and inline
nodes; door/state/container/bench nodes become `WorldAction`s. Door/state
actions apply a `State.Definitions` variant through `WorldThread` and broadcast
the authoritative block. Container opens send `SetPage(Bench)` plus
`OpenWindow(Container)` backed by the persisted block container; close and
teardown restore the `CloseWindow` state. Unsupported node types are inert and
fall through to T46 placement. Bench crafting windows, selectors, effects and
exact door/filler logic remain later work.

Inventory state lives in `hytale-inventory`: `ItemStack` mirrors Java's codec,
`SimpleItemContainer` implements the Java merge/move/remove transaction rules,
and `PlayerInventory` owns hotbar/utility/armor/storage/tools/backpack with
active-slot clamping. `PlayerData::inventory()` loads modern `*Inventory`
components (legacy `Inventory` fallback), and teardown writes the live transform
and inventory together. Sessions send `UpdatePlayerInventory` plus the
hotbar/utility/tools `SetActiveSlot` packets after self-spawn; active-slot and
held-item changes fan out as equipment entity updates. Block container
inventories persist with the world and are exposed through the minimal
`OpenWindow(Container)`/`CloseWindow` packet round trip; `MoveItemStack`
transactions, crafting windows and world item entities remain later gameplay
work.

Block simulation is owned by `hytale-world::simulation`. `BlockSection` carries
a transient `(tick, index)` scheduled queue; `begin_tick` moves due requests
into the ticking set and `finish_tick` applies Java's `BlockTickStrategy`. One
simulation pass drains changed blocks (scheduling the 3×3×3 neighbourhood),
processes fluid/support/scheduled ticks, runs deterministic random ticks,
advances one-cell falling blocks and ticks health/fragility. `SimulationTables`
parses `Support`/`Supporting` (including `MaxSupportDistance`, filler and
`SupportsRequiredFor`), `SupportDropType`, `Gathering`, the full
`RandomTickProcedure`/`TickProcedure` set, `FallingBlockSettings`, the
`BlockBoundingBoxes` hitboxes and the `Default`/`Finite`/`Fire` fluid tickers
from the merged assets. The server calls `simulate(0.05)` each 50 ms game-loop
tick, sets the clock's sunlight factor for light gates and broadcasts
block/fluid changes to viewers. Per-section health/fragility lives in the Java
`BlockHealthSection` v0 component (legacy `BlockHealthChunk` is migrated on
read). T57 turns the health deltas into section `UpdateBlockDamage(s)` packets
and writes the load snapshot after each chunk; replacing a block clears health
and fragility. Sound playback and the builtin module lifecycle remain T60; T56
lands fixed-item drop entities, prefab entities/block components and their
persistence.

---

## 6. Entity layer

- `hytale-entities` provides deterministic gameplay models (stats/effects/
  damage/projectiles/movement); a general `bevy_ecs` store is still a later
  seam, not the current implementation.
- Each `World` owns `hytale_world::runtime::RuntimeEntityStore`,
  `BlockComponentStore` and `containers::BlockContainerStore`: stable runtime
  ids, transforms, prefab/item/projectile kinds, the original component JSON
  documents and per-block container inventories. All commit to versioned,
  atomically written JSON sidecars (`entities.json`, `block-components.json`,
  `block-containers.json`) with the world save. Java's ECS codec is not ported,
  so these sidecars are not part of the Java-readable chunk regions.
- Chunk world-map images are rendered on demand by `hytale-server::map_image`
  from the loaded chunk's heightmap, tint, block and fluid assets, cached per
  chunk in `WorldThread`, and sent as `UpdateWorldMap.chunks` images/unloads
  when the session view changes.
- Systems run on the world thread. Java's `CommandBuffer`/`Ref` behavior maps
  to deferred commands and runtime ids; exact ECS API parity is not a goal.
- `PlayerRef` is a handle containing UUID, username, connection, world, and the
  player entity/runtime identity.
- Chunk sections (blocks/fluids/environment/light/health) stay in
  `hytale-world`; T56 block components are keyed by world position in the
  runtime store and removed when the block is replaced.

The initial component set is deliberately small: transform, UUID, player,
persistent display name, world-gen id, movement/velocity, and the components
needed by block placement. Session 42 started the gameplay layer:
`hytale-entities::stats` owns `EntityStatType`/values/maps with clamps, static
min/max modifiers, regeneration and reset behavior; `effects` owns active
effects, overlap/expiry and invulnerability aggregation; `damage` owns causes,
resistance/outgoing modifiers, health subtraction and the death result. The
asset registry loads `Server/Entity/Stats` and item stat arrays resolve names to
ids. T57 adds the runtime-entity and block-health presentation; stat/effect/
damage packets, condition evaluation and the full damage-calculator graph
remain T59. `hytale-entities::movement` owns the player AABB
(`0.65 × 1.85 × 0.65`), the Y→X→Z collision sweep with one-block step-up, and
the deterministic gravity/jump/fly/climb integrator used by tests and later NPC
work. `hytale-world::collision` turns `BlockType` materials and
`BlockBoundingBoxes` into indexed world shapes (64 Java rotation slots, filler
origins); the server validates network movement against them and sends an
authoritative `ClientTeleport` only when a target is corrected.
`hytale-world::player_data` also decodes the saved
`Model` reference and inventory containers into a server-side
`PlayerAppearance`; the session maps that plus the auth skin into the self-spawn
`ModelUpdate`/`PlayerSkinUpdate`/`EquipmentUpdate`. Gameplay components arrive
in M5.

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
  `EntityStore` holder (`{"Components": ...}`), with typed `UUID`, `Transform`,
  `WorldGenId`, saved `Model` (`ModelReference`), optional `PlayerSkin` and the
  modern armor/hotbar/utility inventory containers (plus the legacy `Inventory`
  fallback). Other components remain opaque. The Java-written core/equipment
  fixtures and the Java read-back probes are the gates.
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
block. `tools/java-dumper/run-save-check.sh` is the whole-save gate: it runs the
config, player and chunk probes against one Rust-written save. RocksDB support is
not part of v1.

---

## 8. Network and auth

- QUIC via `quinn`; TLS via `rustls`.
- ALPN `hytale/<protocol version>`; accept the previous version only to send a
  structured “outdated” disconnect.
- mTLS required. Self-signed client certificates are accepted; the JWT
  certificate binding (`cnf.x5t#S256`) is the actual identity check.
- Connection states: `Initial` → `Authentication` → `Setup` → `Game`.
  The exact packet order is part of the client contract and is covered by
  packet-trace fixtures. `hytale-server::session` implements both the
  development branch (`RequestInsecurePlayerOptions` → `InsecurePlayerOptions`
  → `ConnectAccept`) and the local authenticated branch
  (`Connect(identityToken)` → `AuthGrant` → `AuthToken` → `ServerAuthToken`).
  Both converge on `WorldSettings`/`ServerInfo` → assets → `SetupFinalize`, then
  `ClearChunks`/`JoinWorld`/`ClientReady` and the chunk stream.
- Local authentication (`hytale-auth`): Ed25519 EdDSA JWTs with the Java claim
  names, issuer/audience/scope checks, clock skew and mandatory
  `cnf.x5t#S256` binding to the client certificate. The signing authority is
  local (PKCS#8 key) so the flow works without the live session service; the
  validator/issuer seam is isolated so OAuth/JWKS can replace it.
- Setup assets: common assets are streamed as `AssetInitialize` +
  `WorldLoadProgress` + `AssetPart` + `AssetFinalize` per asset (2,621,440-byte
  parts, matching Java `MAX_FRAME`), followed by
  `RequestCommonAssetsRebuild`; config asset packets and an
  `UpdateTranslations(Init)` follow, then world progress/finished. Unknown
  requested hashes are rejected.
- Generic framed streams live in `hytale-net::frame_stream`; both the setup
  stream and the `Chunks` stream use it. The setup stream is the client-opened
  bidirectional QUIC stream; the chunk stream is server-opened unidirectional.
- Chunk stream: `hytale-net::chunk_stream` opens a server→client unidirectional
  QUIC stream and writes the frames from `ChunkPacketBatch`. The reader validates
  the registry id/max size and zstd-decompresses compressed packets. The
  committed trace fixes the setup/chunks channel rules (notably `ViewRadius` on
  `Chunks`); one stream per connection remains the model.
- Session bridge: `hytale-server::chunk_streamer::send_view` sets the CLI/base
  view on `WorldThread`, ticks until the demand queue drains, and writes the
  loaded view as nearest-first initial batches. `send_view_registered` does the
  same for a session-owned view. `send_updates` forwards later `ChunkLoaded`
  batches and `UnloadChunk` packets after a view change.
  `hytale-server::session` keeps the `Chunks` stream open after the snapshot,
  ticks the world thread on a 50 ms loop, and calls
  `update_view`/`send_updates` from `ClientMovement` so a moving player loads
  and unloads chunks. Teardown always saves the session transform, removes its
  view and unregisters the player. A reconnect loads the per-UUID player
  snapshot before the registry join.
  `hytale-server::serve` now owns the persistent listener: one universe/world
  thread, one independent task per connection, a shared per-world config cache,
  one shared `PlayerRegistry`, connection limits, idle timeout and graceful
  shutdown. The registry fans player-list and view-filtered entity
  spawn/movement/removal messages into each session's queue; the game loop
  drains that queue on its 50 ms tick.
- Packet tracing: `hytale-net::trace` is a `PacketTraceSink` hook on both framed
  streams. `serve --trace <path>` writes one JSONL record per packet with the
  session, stream, direction, packet id/name, registry channel, wire payload
  length and SHA-256. Raw payloads are never stored. The normalized
  `testdata/golden/server/trace/v1-lifecycle.jsonl` fixture and the live
  `client-bot` trace gate replay the join/move/edit/disconnect/reconnect order;
  `testdata/golden/server/trace/README.md` is the real-client capture runbook.
- Auth providers: the current `AuthService` is the local EdDSA authority used by
  the private/offline profile. OAuth browser/device, external session and
  singleplayer token providers remain planned adapters behind the same
  issuer/validator seam.

---

## 9. Modules, events, plugins

- `hytale-core` owns the shared runtime seams: layered `ServerConfig`, tracing
  initialization, `ShutdownToken`/`TaskScheduler`, `EventBus<E>` and
  `ModuleRegistry`.
- Core modules are Rust crates registered in a startup manifest with explicit
  dependencies, mirroring `PluginManifest` ordering. `ModuleRegistry` resolves
  the enabled set dependency-first; module lifecycle hooks are deferred to T60.
- Event bus: typed events with deterministic per-subscriber ordering, sync
  callbacks and bounded async channels; full async channels drop with an
  accounting report rather than blocking the publisher.
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
| `AssetStore/**`, `Codec/**` | `hytale-assets` | pack VFS + `Parent` resolver + store order landed; P0 asset-packet goldens dumped (M2); P0 registry/typed stores and the `worldgen-model` bridge landed (M2); `PhysicalMaterial`/`BlockHitboxes` packet encoders byte-match Java (M2) |
| `CoreServer/.../asset/**` | `hytale-assets` | 54-store inventory landed (M2); P0 loader, contained block-type extraction and deterministic ids landed (M2); common-asset index + verified blob loading landed and stream in setup (M4) |
| `Storage/IndexedStorageFile` | `hytale-storage` | v1 container landed (M3) |
| `CoreServer/.../universe/world/chunk/**` | `hytale-world::chunk` + `simulation` | palette, block/fluid/environment sections, `ChunkColumn::from_generated_chunk`, heightmap, tint palette, packet builders and the `chunk::stream` batch/framer landed (M3/M4); all 46 `chunks-pkts` payloads and the batch order match Java. T52 adds the scheduled tick queue, support/random/fluid/falling passes and transient block health (M5); Java `BlockHealthSection` storage and full builtin tickers remain later work |
| `CoreServer/.../universe/{Universe,World}` | `hytale-world` + `hytale-server` | runtime chunk model, headless boot, `World`/`WorldStorage`, `WorldConfig` v4, `WorldSave`, generation-backed demand loading, the `WorldThread` view/scheduling loop, per-session registered views and the chunk packet batch/framer landed (M3/M4); universe lifecycle and writing frames on a connection landed |
| `CoreServer` entry point / `Options` | `hytale-server` | library + `headless` CLI landed (M3); network/auth/command wiring pending |
| `CoreServer/.../asset/type/gameplay/**` | `hytale-assets::gameplay` | internal `GameplayConfig` `DefaultAssetMap` (8 assets), Java defaults, tag-resolved `WorldGameplayConfig` landed (M4) |
| `CoreServer/.../modules/time/**` | `hytale-world::world_clock` + `hytale-server::{world_thread, session}` | segmented day/night progression, pause, range wrap, ISO game-time persistence, per-second `UpdateTime`, current hour and moon phase landed (M4/T54); weather state/forecasts, `WorldThread::weather_at` and session `UpdateWeather` landed (T54 done; full `UpdateWeathers` asset packet is a split follow-up) |
| `CoreServer/.../entity/{entities/Player,UUIDComponent}` + `inventory/{InventoryComponent,ItemStack}` | `hytale-inventory` + `hytale-world::player_data` + `hytale-server::{session,players}` | core holder codec, item stacks/containers, six player inventories, legacy `Inventory` fallback, active slots, held-item consumption, inventory packets and save/reconnect landed (M4/M5); block-container persistence and the `OpenWindow`/`CloseWindow` round trip landed (T57b); `MoveItemStack` routing and world item entities deferred |
| `CoreServer/.../modules/interaction/**` | `hytale-interactions` + `hytale-server::{session,world_thread}` | root/interaction asset loading with parent inheritance, structural/variable graph walking, block `Interactions` metadata, door/state/container/bench outcomes, door state broadcasts and container windows landed (M5); selectors/effects/cooldowns, item/entity roots and bench crafting windows deferred |
| `CoreServer/.../modules/entity/**` | `hytale-entities` + `hytale-world::{collision,player_data}` | saved model reference and inventory components decoded (M4); `movement` AABB sweep, one-block step, bounds, gravity/jump/fly/climb integrator, slide/roll/mantle traversal state, mount seam and the internal `MovementConfig` join packet landed (M5); full `ModelAsset` protocol expansion, ECS lifecycle, fluids/triggers, entity collisions and mount entities deferred |
| `CoreServer/.../entity/entities/ProjectileComponent`, `modules/projectile/{config,system}`, `modules/physics/{SimplePhysicsProvider,PhysicsMath}` | `hytale-entities::projectile` + `hytale-assets` | legacy `Projectile`/new `ProjectileConfig` stores (86/188), Java launch offset/angle math, gravity/terminal cap, swept block/entity collision, bounce/stick/rest, hit/miss/death timers, TTL and the T53 damage handoff landed (M5, T55 done); density/drag/buoyancy/rolling, predicted/pierce projectiles, explosions and spawn packets deferred |
| `CoreServer/.../modules/entitystats/**`, `asset/type/entityeffect/**`, `modules/entity/damage/**` | `hytale-entities::{stats,effects,damage,components}` | stat types/values/maps, static modifiers, regeneration/reset, active effects with overlap/expiry/invulnerability, damage causes/resistance/outgoing, health/death core and the `EntityGameplay` component seam landed (M5, T53 done); the `EntityStatType`/`EntityEffect`/`DamageCause` stores and the Java `entity/entity-gameplay.json` fixture gate the data; conditions, calculators, DoT and per-entity/config stat/effect packets deferred to T59 |
| `Component/**` | `hytale-entities` wrapper on `bevy_ecs` | planned (M3) |
| `CoreServer/.../universe/world/storage/**` | `hytale-storage` + `hytale-world::{chunk::storage, world, world_config, player_data}` | `IndexedStorageFile` container, BSON chunk holder codec, named-palette storage, legacy heightmap, region files, `WorldConfig` v4 and player holder documents landed (M3); Java fixture read gates and all Rust write → Java read gates green (M3). Boot-time load/save wiring pending |
| `CoreServer/.../io/**`, `transport/**` | `hytale-net` + `hytale-server::{session, chunk_streamer, serve, players}` | QUIC mTLS spike + `PeerIdentity`, generic framed streams, the framed `Chunks` writer/reader, the world-thread → stream bridge, incremental updates, the dev/insecure join, local EdDSA auth, common/config asset streaming (all P0/P1 stores), the `Game` transition, the joining player's list/self-spawn, the initial and periodic time/gameplay packets, the teleport exchange, movement-driven view updates, the persistent multi-session listener, the per-world player registry with view-filtered entity broadcasts, validated block place/break with `ServerSetBlock` broadcasts, and the JSONL packet-trace capture/gate (M3/M4) landed |
| `CoreServer/.../auth/**` | `hytale-auth` | Ed25519 keys, EdDSA JWT claims/signature/time/scope validation, `cnf.x5t#S256` certificate binding and the local issuer/validator authority landed (M4); OAuth device/browser, session-service JWKS and credential-store adapters pending |
| `CoreServer/.../command/**` | `hytale-commands` | planned (M4) |
| `CoreServer/.../permissions/**` | `hytale-commands`/`hytale-core` | planned (M4) |
| `CoreServer/modules/**` | `hytale-world` (core subset), later module crates | planned (M5) |
| `builtin/**` | `hytale-builtin` feature crates | deferred (M6) |
| `NPC/**` | `hytale-npc` | role/flock catalogs, reference/parameter resolution with the builder expression subset, `Modify` flattening, Java direction tables, block-grid A*, real-world `WorldGrid` walk/fly planning, `PathFollower`, pursue/evade/rotate steering, group steering, view-cone perception, `Server/NPC/Spawn/World` selection, role combat profiles/damage ranges, a scripted `NpcActor`, the live `WorldThread` NPC store/spawn/tick/persistence, movement-mode spawn-position search, density/expected-count accounting, explicit population ticks, loaded-section environment segment counting and real `BlockSet` floor resolution landed (M6, T58/T58b/T58c/T59 core and T59b runtime/position/controller/population/segments/block-sets); the full builder graph, half-step motion-controller A*, wander/avoidance, blackboard sensors, automatic environment enumeration, beacons/markers, fluid-tag/light/suppression gates, encounter evaluator and the NPC ECS remain T59b/T59c/T60 |

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
                                                               ├─ ChunkPacketBatch → zstd frames → Chunks stream → client-bot
                                                               └─ BlockSection.Data → IndexedStorage

client ── QUIC frames ── hytale-protocol decode ── hytale-net session
                                                        └─ world thread ── ECS/chunk changes
```

The next concrete step is in [`NEXT_STEPS.md`](NEXT_STEPS.md). Keep this file
updated when a crate gains its first real module or a Java package is mapped to
specific Rust files.
