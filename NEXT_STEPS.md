# Next steps — continuation roadmap (updated 2026-10-21)

Status (2026-10-21, end of the chunk-parity session): Phase 0 + Phase 1
complete and golden-verified. Phase 2 zoom/climate golden-verified (Subtask 6).
Subtasks 1–4, 6 and 7 (incl. 7d) DONE; Phase 3 terrain/water populators DONE;
Phase 4 caves DONE (4a–4e); Phase 5a/5b/5c DONE (the prefab package, the
`PrefabPopulator` and the unique prefabs; the full chunk generation places real
`PREFAB` blocks). The chunk-level verification dump landed and is deterministic
(`testdata/golden/chunks.json`) and the Rust parity test now shows **150 / 21,504
mismatching columns**, all in the cave stage: tint, raw tint, zone, biome,
parent tile biome, biome-pattern index/cell, environment base mapping, terrain
layers, covers, water and prefab blocks are **bit-exact**; the remaining voxels
are cave-cover `NoiseMask` map-condition edge cases. The loader golden is now
**100% bit-exact (0 noise-derived deviations)**. `cargo test --workspace` and
`cargo clippy --workspace --all-targets -- -D warnings` are green
(`tests/chunks_golden.rs` is `#[ignore]`d with the precise remaining diff).
Remaining before Phase 6: fix the last cave-cover condition edges, then
un-ignore `tests/chunks_golden.rs` and run Phase 6. This file lists the
remaining work in dependency order, sized for ~200k-context subagent runs.
Free-model pool (subagents):
`opencode/minimax-m3.1-flash-free`, `opencode/longcat-2.5-preview-free`,
`opencode/deepseek-v4.1-flash-free`, `opencode/glm-5.3-flash-free` — note
they intermittently rate-limit / IP-block; retry with a different model or
fall back to porting in-session (each retry costs ~1 min, a full port ~10–20
min — don't burn more than ~3 retries before switching).

## Verified so far (do not regress — `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` must stay green)
- `hytale-math`: FastRandom, HashUtil, MathUtil, ChunkUtil, TrigMathUtil, ranges — golden tests vs `testdata/golden/{fastrandom,hashutil,mathutil}.json` (bit-exact).
- `procedural`: all 11 noise types — golden tests vs `testdata/golden/noise.json` (bit-exact 2d+3d); NoiseProperty DSL; random/ (coordinate randomizers + rotators); point/ (IPointGenerator family); condition/ package.
- `worldgen-hytale`: zoom/ (PixelProvider, PixelDistanceProvider, ExactZoom, FuzzyZoom, unique-zone placement helpers); climate/ core (ClimateType, ClimateGraph, DistanceTransform, ClimateNoise, ClimateSearch, DirectGrid, IntMap/DoubleMap) — golden-verified against `testdata/golden/{zoom,climate}.json` (Phase-2 dumper, Subtask 6).

## Subtask 1 — weighted map + worldgen/util (~1.9k Java lines) [DONE]
- Common/map: IWeightedElement, IWeightedMap, WeightedMap (Builder +
  SingletonWeightedMap unified into one struct), DefaultMap →
  `crates/worldgen-hytale/src/weighted_map/` (20 unit tests). `resolveKeys`
  is a free `resolve_keys` fn; singleton `forEachEntry` yields weight 1.0;
  `get` uses a NaN-propagating `java_math_min` (Java `Math.min` semantics).
- worldgen/util: ArrayUtli, BlockArray, BlockFluidEntry, NoiseBlockArray,
  ResolvedBlockArray, ListPool (Drop-guard = Java `close()`), ObjectPool,
  ConstantNoiseProperty, LogUtil (`log` crate, target `WorldGenerator`),
  function/ (ICoordinateDoubleSupplier family), bounds/ (all four classes;
  PrefabRotation rotated-bounds methods deferred to Phase 5),
  cache/ (semantic contract on moka: sized + time-to-idle, lazy eviction +
  explicit `cleanup()`, use-after-shutdown panics; destroyer/computeKey →
  Rust Drop/moka internals, documented), condition/ (+ flag/).
- Skip ChunkThreadPoolExecutor/ChunkWorkerThreadFactory (different worker
  design). ResolvedBlockArray global registries arrive with Phase-2 loaders.
- Also ported (prerequisite of NoiseBlockArray): `procedurallib.supplier`
  → `procedural::noise::supplier` (IDoubleRange with 2d/3d coordinate
  overloads, DoubleRange Constant/Normal/Multiple, DoubleRangeNoiseSupplier,
  NoisePropertySupplier2d/3d method-reference adapters). The reduced seams in
  `procedural::noise::support` stay for the noise core; integration of those
  seams is deferred.

## Subtask 2 — biome package (~508 lines) + containers + zone [DONE]
- zone/: package split. `ZoneDiscoveryConfig` (+ EventTitleUtil constants),
  `ZoneGeneratorResult` (border_distance + nullable zone handle, Java-style
  setters for the scratch-reuse pattern), `Zone` record PARTIAL (id/name/
  discoveryConfig only — `biomePatternGenerator` Subtask 3 / `caveGenerator`
  Phase 4 (4a + 4e) / `uniquePrefabContainer` Subtask 5 append fields).
- container/: all seven containers 1:1 (fade, tint, environment, water,
  cover, layer with Layer<T> generic), PrefabContainer with documented
  Phase-5 seams (`PrefabSupplier` empty trait + `pattern_generator_max_size`);
  UniquePrefabContainer deferred (needs ChunkGenerator + prefab.unique.*).
  Java static ListPools are `thread_local!` entries (Rc != Send).
- chunk/: constants MAX_RADIUS / MAX_RADIUS2 only; the full
  HeightThresholdInterpolator (+ CoreDataCacheEntry, InterpolatedBiomeCountList,
  ZoneBiomeResult, ChunkGenerator skeleton) ports with Subtask 3.
- biome/: Biome (object-safe trait; Java abstract class), TileBiome,
  CustomBiome, CustomBiomeGenerator, BiomePatternGenerator (tiles-then-customs
  ordering, getBiomeIndex via nearest2D + random3, getBiomeDirect via
  doubleToLongBits + random3, requireNonNullElse → panic on empty tile map),
  BiomeInterpolation (empty-map → MAX_RADIUS² default, populated → radius²).
  Tile/Custom biomes implement PartialEq/Hash **by id** (Java hashCode()==id +
  identity equals ⇒ equivalent since ids are unique per world).
- weighted_map fix: trait-default `get_random` now returns the singleton key
  WITHOUT consuming a random draw (Java SingletonWeightedMap.get(Random)
  semantics) — matters for cover picks in Phase 3.
- Bit-exactness caveat for Phase 3: Java `IWeightedMap.get(Random)` on a
  singleton must not shift FastRandom streams; covered by a regression test.

## Subtask 3 — chunk core skeleton (~1.3k lines) [DONE]
- worldgen-model crate (locked "own chunk model" decision): `block_priority.rs`
  (BlockPriorityChunk 1:1 — byte grid, priority ladder NO_CHANGE..PREFAB,
  MASK/FLAG_MASK/FLAG_SUBMERGE with exact `~31` byte math, BlockPriorityModifier
  trait + identity NoneModifier) and `chunk.rs` (GeneratedChunk — flat
  `y<<10|z<<5|x` grid proven identical to Java's 10-lazy-section layout;
  block/rotation/filler/tint/environment-column+per-block/fluid id+level;
  "air write into a fresh section is a no-op" reproduced; palette/Holder/ECS
  not ported per plan). 17 tests.
- zone/ remainder: ZoneColorMapping, ZonePatternProvider (+ maxExtent fold),
  ZonePatternGenerator (exact generate/getZone incl. `hash3 & 0x7FFF_FFFF % len`
  and the in-bounds POSITIVE_INFINITY quirk on solid masks — regression-tested),
  ZonePatternGeneratorCache (computeIfAbsent -> RwLock<HashMap> + Java error
  wrapping). Zone record: `biome_pattern_generator` appended; UniqueEntry/
  UniqueCandidate restructured to carry the zone handle.
- cache/: CoreDataCacheEntry (interior mutability for moka-shared entries),
  InterpolatedBiomeCountList (+ AverageCollector.add = val - val/n + v/n
  inlined + attributed), CoordinateCache/CoordinateKey, ExtendedCoordinateCache,
  ChunkGeneratorCache (compute-once sized + time-to-idle over moka).
- chunk/: ZoneBiomeResult, MaskProvider, HeightThresholdInterpolator (full
  populate/interpolation; holds Rc<ChunkGenerator> + chunk x/z instead of the
  execution — cycle-break, identical arithmetic), ChunkGeneratorResource
  partial (GenContext scratch), ChunkGeneratorExecution partial (tint/env
  mapping EXACT; setBlock/setFluid/override arbitration exact over the
  worldgen-model grid; BlockPhysics/Holder no-ops documented; execute() stage
  list appends when the populators land — NOT BIT-EXACT until then, in-code
  flag), ChunkGenerator skeleton (all core-data/height hot paths 1:1 incl.
  f32-vs-f64 threshold compare; owns a private cache twin because the ported
  cache closures are Send-bound while the worldgen graph is Rc-based).
- Deferred w/ doc notes: registries (string→int maps -> Subtask 7, needs
  asset-map ordering), ValidationUtil (Phase 5), cave/unique-prefab/spawn/
  world-map/executor (Phases 4-6). 45 new tests in the slice.

## Subtask 4 — zone package unique-zone wiring [mostly absorbed into Subtask 3]
- ZoneColorMapping / ZonePatternGenerator / Generator / Cache landed with
  Subtask 3; the Zone record still lacks `uniquePrefabContainer` (Phase 5) —
  that porter appends the field. (`caveGenerator` landed with 4a/4e.)
- Remaining: cross-check unique-zone candidate/painting semantics against the
  loader JSON during Subtask 7.

## Phase 3 populator slice (Block + Covers + Layers + Water) [DONE]
- `worldgen-model::registry` — the registry SEAM: `DrawType` (protocol enum
  1:1, default `Cube`), `BlockTypeInfo`/`BlockRegistry`, `FluidRegistry`
  (`maxFluidLevel` default 8), `Registries::empty()`; `FluidTicker.isSolid`
  (Cube | CubeWithModel) ported as a free fn.
- `chunk::populator::block_populator.rs` — BlockPopulator + private
  LayerPopulator: one FastRandom per chunk seeded with
  `hash4(seed, chunkX, chunkZ, 0x4e5facef_afd03ace)`, shared across the 32×32
  column walk (draw order = parity); threshold walk quirks transcribed
  (`threshold as f64 > noise || threshold == 1.0` f32 compare; the
  `if (empty) add(y)` boundary add at `min-1`; static-layer `--layerY` inside
  getBlock; `if (layerY <= min) return` exits the WHOLE static-layer method —
  Java quirk kept; labelled `layerLoop` break; env-only branch with clamped
  min/max; surface-list in-place `set(i, maxY)`), cover draw-order chain
  exact (parent → column-draw → height-draw), the Java always-false
  `parentCondition == ConstantIntCondition.DEFAULT_*` dead code documented,
  not ported.
- `chunk::populator::water_populator.rs` — submerge logic exact (raw
  priority + FLAG_SUBMERGE gate, block override only below WATER priority,
  registry null/solid skip).
- `ChunkGeneratorExecution`: `registry: Rc<Registries>` field +
  `execute(seed, &mut resource)` now runs tint → env → BlockPopulator →
  WaterPopulator (cave/prefab stages = Phase 4/5; terrain+water path is
  bit-exact modulo missing stages).
- ⚠ DISCOVERED BLOCKER (documented in REMAINING.md §6): the game's BlockType/
  Fluid asset packs are NOT in `.hytale-source` (only schema + name lists).
  Java itself resolves block names at load time and throws on unknown names —
  so Java cannot load the Default world without the pack either; chunk-level
  golden parity is blocked on the pack (or obtaining it). Until then the
  registry seam is data-driven and empty (water writes nothing).
  → **RESOLVED**: the base-game pack was provided (`Assets/Assets.zip`) and the
  block definitions turned out to be the `"BlockType"` objects **embedded in the
  `Server/Item/Items/**` item assets** (registered as child assets by
  `ContainedAssetCodec` with mode `INHERIT_ID_AND_PARENT`). The real pipeline
  now runs in the Java dumper, so the registries arrive as golden data — see
  the registry subtask below.
- 23 new tests (374 workspace total).

## Subtask 6 — Java dumper extension (Phase 2 golden data) [DONE]
- `tools/java-dumper` (`Phase2Dumper.java`) now dumps the Default world mask
  (`testdata/golden/zoom.json`: the real `Mask.json` randomizer warp over the
  rasterised continent mask — `getX/getY`, `generate`, `distance`, `inBounds` for
  the seeds 0/1/42/1234567) and the procedural climate mask
  (`testdata/golden/climate.json`: the 512×512 `ClimateGraph` table + fade map,
  the resolved noise construction and the `ClimateNoise.generate` sample grids).
  Both are wired into `crates/worldgen-hytale/tests/golden.rs`
  (`zoom_golden_parity`, `climate_golden_parity` — bit-exact).

## Subtask 7 — serde loaders: World.json / Mask.json / Zones.json / zone files (Phase 2 close) [DONE — 7-foundation / 7-procedural / 7-climate / 7a / 7b / 7c / 7d all DONE]
Java sources: `worldgen/loader/**` (77 classes) — sized as 2–3 subagent runs.
- **7-foundation [DONE]**: `procedural::json` framework — `SeedString`
  (javaStringHash × 114512143, `append`/`appendToOriginal`/`alternateOriginal`),
  `JsonLoaderBase` (`has`/`get`/`getRaw` with the `"File": "X.Y.Z"` →
  `X/Y/Z.json` reference resolution + traversal check, `must_get_*` with
  verbatim error messages, gson `getAs*` semantics incl. `(int)` casts),
  serde_json `preserve_order` (gson insertion order). **Bit-exact verified**:
  the real `testdata/Default/Mask.json` continent config loads through
  `NoisePropertyJsonLoader` and all 256 samples × 4 seeds of
  `testdata/golden/climate.json` `noise.seeds.*.continent` match
  (`crates/procedural/tests/loader_golden.rs`), including the pinned
  `seedOffset` 203152061 = hash("0.NoiseProperty.NoiseProperty.NoiseProperty")×
  114512143.
- **7-procedural [DONE]**: ALL `procedurallib.json` loaders — the 11-noise
  dispatch (`NoiseTypeJson`), `NoisePropertyJsonLoader` (full composition
  order incl. the `type != X && has(...)` filter quirks), coordinate
  randomizer/rotator (the `LoadedRotator` enum makes Java's "origin rotator
  dropped through concrete rotateX calls" quirk explicit), double/float
  ranges, double conditions/thresholds, noise-mask conditions, height-threshold
  interpreters (basic/noise), point generators/evaluators, cell sub-loaders
  (jitter/distance/border/evaluators), branch/grid/mesh/distance loaders.
  73 new tests; workspace 432 tests green. Seams: `FloatRange` supplier type
  doesn't exist yet (loader panics; full parse transcribed as
  `load_parsed()`); `impl support::IDoubleRange for supplier::DoubleRange`
  bridge until the support seams retire.
- **7-climate [DONE]**: `loader/climate/**` (11 loaders, none append seed
  suffixes except `MaskProviderJsonLoader`'s `".MaskProvider"`),
  `MaskProviderJsonLoader` (PNG image path via the `png` crate),
  `PrefabStoreRoot`, `ChunkGeneratorJsonLoader` core (world json / override
  folder / world size+offset / the `Masks` weighted pick /
  `LoadedMaskProvider::{Climate,Image}` enum for the Java subclass split).
  **Golden gate passed**: the full climate chain (`ClimateNoise.generate`
  id/continent/temperature/intensity/fade) is 1024/1024 samples bit-exact vs
  `testdata/golden/climate.json` over the real Mask.json
  (`crates/worldgen-hytale/tests/loader_climate_golden.rs` — also pins the
  grid seedOffset, the climate-type tree construction and the Masks pick).
  Seams: the `ZonePatternProviderJsonLoader` construction point panics
  (`SEAM(subtask-7a)`); `PrefabStoreRoot::resolve_prefab_store(ASSETS)`
  needs the CoreServer prefab store; the Java asset-file-system /
  `EventHandler` / `setPrefabConfig` boot block is out of loader scope.
- **7a-next [DONE]**: `ZonePatternProviderJsonLoader` (+ `ensureMaskIntegrity`
  both branches, both unique-candidate paths), `ZonesJsonLoader`,
  `ZoneJsonLoader` (the cave generator is the real 4e loader now, the
  unique prefabs stay at `SEAM(phase-5)`), the biome structural chain
  (8 loaders under `loader/biome/`), and the FULL
  `ChunkGeneratorJsonLoader::load()` wiring
  (world json → mask → zone pattern provider → `FileContextLoader` → zones →
  `ChunkGenerator::new`). Mask polymorphism resolved by converting
  `chunk::MaskProvider` into an `Image(FuzzyZoom)`/`Climate(Rc<ClimateMaskProvider>)`
  enum with the same method surface (`get_climate` for the buffer-passing
  climate call; `instanceof` checks become matches).
  - **End-to-end verified** (`crates/worldgen-hytale/tests/loader_zones.rs`):
    the real Default world walks the entire chain — the cave configuration of
    every zone is loaded (the Phase-4 cave seam is gone) — and stops at the
    *biome prefab container* (the Phase-5 prefab package); a synthetic
    minimal world loads completely through `ChunkGenerator::new` and checks
    the containers against the golden registry ids. The cave chain itself is
    verified end-to-end in `crates/worldgen-hytale/tests/loader_caves.rs`.
  - **Discovered quirk**: real zone files carry a UTF-8 BOM — gson is
    lenient, so `loader::load_json_object` strips `\uFEFF` (serde_json would
    reject).
- 7a. Core config path: `WorldGenConfig` [DONE], `ChunkGeneratorJsonLoader` [DONE],
  `MaskProviderJsonLoader`/`ClimateMaskJsonLoader`, `ZoneJsonLoader`,
  `ZoneColorMappingJsonLoader` [DONE]. Transcribe defaults/validation
  faithfully (plan risk #3); flexible gson semantics: untyped maps,
  weighted-map shorthands (`"!Rock_Bedrock"`, `"*"` in masks) need custom
  serde deserializers; `SeedString` seed labels (`"RANDOMIZER"`) reproduce
  the dumper's verified convention (`hash("<seed>.<label>") * 114512143`) —
  DONE (see 7-foundation).
- **7b+containers [DONE]** (subagent, deepseek-v4.1-flash-free):
  registries name→id direction (case-insensitive, `Registries::load()` from
  the golden dump, `BlockPattern.BlockEntry.decode` block-key syntax),
  `ChunkGeneratorExecution` registry wiring, `loader/util/`
  (NoiseBlockArray / ResolvedBlockArray / ResolvedVariantsBlockArray /
  Vector2d/3d loaders) and `loader/container/` (cover/layer/tint/environment/
  water/fade + prefab/unique-prefab at Phase-5 seams); the biome
  `require_container_absent` gates are removed. The registry snapshot is
  threaded `ChunkGeneratorJsonLoader` → `ZonesJsonLoader` → zone → biome.
  Known deviations: unknown names panic (Java dynamically registers an
  unknown asset), the `ResolvedVariants` child variants fall back to the base
  block (the golden dump has no parent/child data), and the global
  `ResolvedBlockArray` caches are omitted (unobservable).
- 7b. Registries wiring **[DONE]**: `Registries::from_golden` carries the
  NAME → id direction (`BlockType.getAssetMap().getIndex(name)` +
  case-insensitive lookup, rotation suffixes via
  `BlockPattern.BlockEntry.decode`); `Registries::load()` reads
  `testdata/golden/registries.json`. `ChunkGeneratorJsonLoader` loads the
  registries and threads `Rc<Registries>` down to the biome loaders.
  Deviation: `ChunkGeneratorExecution::new()` keeps `Registries::empty()`
  (unit-test callers construct executions without the asset pipeline); the
  loader path always supplies the loaded registries.
- 7c. Content loaders the biome/zone configs reference
  **[PARTIAL — biome/zone containers DONE]**: NoiseBlockArray /
  ResolvedBlockArray / ResolvedVariantsBlockArray / Vector2d/3d JSON loaders,
  tint/environment mapping, cover/layer/water/fade container loaders and the
  Phase-5 seam placeholders for prefab/unique-prefab containers. Still open:
  `Prefabs.json` + `.prefab.json` parsing (10,050 files under
  `Assets/extracted/Server/Prefabs/` — the
  `PrefabStore: ASSETS` store), and the real prefab/unique-prefab generators.
  (The cave config loaders are done — see 4e.)
- 7d. Loader golden tests [DONE]: `tools/java-dumper` (`LoaderDumper.dumpLoader`)
  loads the REAL Default world through the real Java loader chain
  (`ChunkGeneratorJsonLoader` → `ZonePatternProviderJsonLoader` →
  `FileContextLoader` → `ZoneJsonLoader` → `BiomePatternGeneratorJsonLoader` →
  the biome/container loaders) for the seeds 0/1/42/1234567 and dumps
  `testdata/golden/loader.json` (Masks pick, zone requirement, file context,
  MaskMapping, per-zone summaries, the phase-4 cave `EntryPoints` seed chain,
  biome-pattern grids + sampled biomes, the full seed-0 biome containers).
  Rust parity: `crates/worldgen-hytale/tests/loader_golden_dump.rs` —
  bit-exact. Known deviations (documented in the test header): the biome
  `Prefabs` containers are stubbed to null on BOTH sides (phase-5 seam, the
  dump records `hasPrefabs` so the 5b port can assert against it), and the
  Rust `BiomePatternGeneratorJsonLoader` is fed the resolved `GridGenerator`
  child element (the same input Java uses internally). The Subtask-4
  unique-zone wiring cross-check passed (unique-zone candidates/painting
  match the loader JSON on both sides).

## Subtask 5 — ClimateMaskProvider + UniqueClimateGenerator [after 7a/7b]
- `chunk/MaskProvider` (the chunk-level one the ChunkGenerator uses) +
  `ClimateMaskProvider` + `UniqueClimateGenerator` (unique-zone placement via
  ring-spiral `ClimateSearch` scoring — the zoom/ unique-placement helpers
  already ported cover the image path; this is the climate path).
- **CORE [DONE]**: `climate/unique_climate_generator.rs` (Entry/Unique records,
  `generate`, `get_candidates`, both `apply` forms with the MAX_PARENT_DEPTH
  retry loop; positions resolved synchronously instead of
  `CompletableFuture`) and `climate/climate_mask_provider.rs` (EMPTY_ZOOM
  base, `get` with the unique-color override, fade distance, the candidates
  split between the climate and image paths). Needs the 7-climate loaders for
  the mask configs — in flight.

## Phase 4 — Caves (~4.6k Java lines, largest phase) [after 5]
Port slices in order, each ending green + reviewed:
- 4a. Core value types [DONE]: `Cave`, `CaveType`, `CaveNodeType`,
  `CaveBiomeMaskFlags`, `CaveYawMode`, `CavePrefabPlacement`,
  `CaveGeneratorCache`, `CaveBlockPriorityModifier`, `cave/element/**`
  (`CaveElement`, `CaveNode`, `CavePrefab`); the `Zone` record gains the
  nullable `caveGenerator` component; `ChunkGenerator` gains
  `getCave`/`generateCave` + the `cave_generator_cache`.
  - New `crates/worldgen-hytale/src/cave/` package (1:1 with
    `worldgen.cave`), `crate::supplier` (the procedural `IFloatRange` /
    `IDoubleCoordinateHashSupplier` seam — the `procedural` crate is out of
    scope for this slice), `crate::prefab::prefab_rotation` (the CoreServer
    rotation enum the yaw mode needs).
  - Seams left for the later slices (4a): `cave::shape::CaveNodeShape` +
    `CaveNodeShapeGenerator` (the concrete shapes — 4c/4d),
    `cave::prefab::CavePrefabSupplier` (the prefab buffers — Phase 5).
    The recursive growth (`cave::cave_generator`) landed in 4b.
    The cave cache is a private twin of `ExtendedCoordinateCache` (the
    ported cache is `moka`-backed and therefore `Send + Sync`, the cave
    graph is `Rc`-based) — the same pattern as the core-data cache.
- 4b. Node-graph growth [DONE]: `cave::cave_generator` (`CaveGenerator`
  trait + `CaveGeneratorImpl` — the recursive `generate`/`startCave`/
  `continueNode` yaw/pitch walk with anchors, child-entry shuffle,
  repeat/chance/child-count limits, GENERATE/POPULATE/CONTINUE flags, the
  per-node height conditions, the prefab-placement pass and the chunk-bucket
  compile) and `chunk::populator::cave_populator` (the full zone →
  entry-point → cave → node/prefab loop). The cave stage is now active in
  `ChunkGeneratorExecution::execute` (the priority modifier is installed and
  the nodes are visited in bucket order). The concrete shapes stay 4c/4d:
  they are the documented `SEAM(4c)` and the tree remains **not bit-exact**.
- 4c. Shapes [DONE]: `cave::shape::abstract_cave_node_shape` (the Java
  `AbstractCaveNodeShape` base class — the shared `populateChunk` that carves
  the shape into the chunk and places the covers),
  `cave::shape::pipe_cave_node_shape`, `cave::shape::cylinder_cave_node_shape`,
  `cave::shape::ellipsoid_cave_node_shape`,
  `cave::shape::empty_line_cave_node_shape` and
  `cave::shape::prefab_cave_node_shape` (structure only) plus the
  4c-relevant part of `cave::shape::cave_node_shape_utils` (the anchor
  projections, the child offset / end radius and the shared population
  helpers). The generators reproduce the Java draw order exactly (the
  `getValue(random.nextDouble())` overload always consumes a draw, the
  `getValue(random)` overload does not for a `Constant` range).
  - Seams left: the prefab paste / per-column height map are
    `SEAM(phase-5)`; the block-type support data of `invalidateCover` is
    ported but the golden block dump carries no support map (so every block
    reports "no support" until the CoreServer block assets are dumped).
  - The `CaveNodeShape::get_prefab_rotation` seam of 4a was renamed to
    `as_prefab_rotation` (the concrete shape keeps the Java
    `getPrefabRotation()`).
- 4d. The distorted family [DONE]: `cave::shape::distorted_cave_node_shape`
  (the Java `DistortedCaveNodeShape` — it has its own `populateChunk`, the
  floor / ceiling distortion and the `getEndRadius` width/height selector) +
  `cave::shape::distorted` (the `DistortedShape` interface, the
  `AbstractDistortedShape` / `AbstractDistortedExtrusion` /
  `AbstractDistortedBody` bases, the cylinder / pipe / ellipsoid shapes, the
  `DistortedShapes` factory registry) + `cave::shape::shape_distortion`
  (the width / floor / ceiling noise triple). The extrusion / body math, the
  interpolation curves, the pitch compensation and the factory draw order
  (`getValue(random)`, not `getValue(random.nextDouble())`) are 1:1 with the
  Java.
  - Seams left: the block-type support *data* (the golden dump has none) and
    the 4e JSON loader (which supplies the `InterpolationFunction`, the shape
    factory and the distortion noises).
- 4e. Cave JSON loaders [DONE]: `crates/worldgen-hytale/src/loader/cave/**`
  (`CaveGeneratorJsonLoader` → `CaveTypes` → `CaveType` → `CaveNodeType` +
  `CaveNodeTypeStorage` → child/cover entries → biome mask → prefab config →
  all seven shape-generator loaders) + `CaveFileContext`, the
  `BlockPlacementMaskJsonLoader`, the `SeedStringResource` unique-id counters
  and the `PathUtil.resolvePathWithinDir` traversal guard. The
  `SEAM(phase-4)` panic in `ZoneJsonLoader` is GONE: all 28 Default zones
  load their `Cave/Caves.json` end-to-end (Oceans: `Caldera`/`Copper`/`Iron`,
  the `{"File": "Ores.Zone0.CopperPlacement"}` reference and the
  `Caldera.Entry` → `Caldera.Type_0.Entry` cross-file node reference
  resolve).
  - Golden gate: `tests/loader_caves_golden.rs` reproduces every
    entry-point-seed offset the dumper pins in `testdata/golden/loader.json`
    (`<seed>.Zones.Zone-<name>.CaveGenerator-<type>.CaveType` chain,
    bit-exact, seeds 0/1/42/1234567).
  - **Discovered data quirks** (not in the Java sources): 5,588 vanilla
    `*.node.json` files carry gson-lenient JSON COMMENTS and some a UTF-8
    BOM → `procedural::json` now strips both (gson semantics); the
    procedural noise loader was missing the `FractalMode::MIN/MAX` modes
    (used by the cave node types; adding them also closed a biome-port gap —
    the 7d golden test now compares the 31 previously-skipped biome
    containers: pinned-column tolerance 5.0, deviation count 2590).
  - Quirks transcribed: seed suffix chain (`.CaveGenerator`, `-<type>`,
    `.CaveType`, `.CaveNodeType-<name>`, `.Child-<i>`, `-cover#<i>`, …), the
    GLOBAL unique-name counter (load-order dependent), a per-cave-type node
    storage, the `"Zones."`-prefixed cross-zone file reference, wildcards at
    the END of block-mask entry arrays, `PrefabRotation.valueOf` (strict,
    `"90"` invalid), verbatim error wrapping (`Error while loading caves…`).
  - Server-side deviation: `CaveNodeType.children` is a `RefCell` (Java sets
    children after construction; cycles keep node types alive) — this
    required a `clippy::mutable_key_type` allow on the cave generator cache.
  - Seams left: the modifier/event dispatch is skipped (REMAINING.md §1);
    the cave prefab ENTRIES parse fully but resolving the prefab name into a
    buffer is `SEAM(phase-5)` (`NamedPrefabSupplier`); the biome
    `Prefabs` containers are the remaining end-of-chain seam (Phase 5a). **[DONE — the
  loaders; the chunk-slice PNGs are still open]**
  - New `crates/worldgen-hytale/src/loader/cave/` package (1:1 with the
    Java `loader.cave`): `CaveGeneratorJsonLoader`, `CaveTypesJsonLoader`,
    `CaveTypeJsonLoader`, `CaveNodeTypeJsonLoader`, `CaveNodeTypeStorage`,
    `CaveNodeChildEntryJsonLoader`, `CaveNodeCoverEntryJsonLoader`,
    `CaveBiomeMaskJsonLoader`, `CavePrefabConfigJsonLoader`,
    `CavePrefabContainerJsonLoader`, `CavePrefabEntryJsonLoader`,
    `FluidLevelJsonLoader` + the `loader/cave/shape/` family (the pipe,
    cylinder, ellipsoid, empty-line, prefab and distorted shape generators +
    the shape distortion).
  - New `loader/context/cave_file_context.rs` (`CaveFileContext`), the
    shared `loader/util/path_util.rs` (the Java `PathUtil.resolvePathWithinDir`
    path-traversal guard) and `loader/seed_string_resource.rs` (the Java
    `SeedStringResource.uniqueIds` counters — the inline cave node types
    get their name *and* their seed from them).
  - `loader/prefab/block_placement_mask_json_loader.rs` (the Java
    `BlockPlacementMaskJsonLoader` — the cave types / cave prefabs /
    prefab cave nodes need the block mask).
  - The `ZoneJsonLoader` cave seam is **cleared**: the zone now loads its
    `Cave/Caves.json` through the real `CaveGeneratorJsonLoader` (a zone
    without that file gets **no** cave generator, like the Java `null`).
  - **Golden verification** (`crates/worldgen-hytale/tests/loader_caves_golden.rs`):
    the Java dumper's `caves` section of `testdata/golden/loader.json` pins
    the entry-point **seed offsets** of every cave type of all 28 zones for
    the seeds 0/1/42/1234567 — the Rust port reproduces every one of them
    (the seed chain
    `<seed>.Zones.Zone-<zone>.CaveGenerator-<type>.CaveType` is bit-exact).
  - **Seams left**: the modifier / event plugin system (`REMAINING.md` §1 —
    the `ModifyEvents.CaveTypes` / `ModifyEvents.CaveCovers` dispatches are
    skipped), the prefab *file* resolution (the prefab store is Phase 5 —
    the cave prefab entries and the prefab cave nodes parse the prefab
    *names* and hand a `NamedPrefabSupplier` to the cave package; its bounds
    are the documented `SEAM(phase-5)`), and the block-type support data of
    `invalidateCover` (the golden block dump carries none).
  - **Discovered quirks**: the vanilla cave node files use gson's lenient
    JSON comments (`//` and `/* */`) and some carry a UTF-8 BOM — the
    `procedural::json::strip_json_comments` helper and the BOM stripping
    reproduce gson's lenient mode; the `MIN` / `MAX` fractal noise modes
    were missing from the `procedural` noise loader (the cave node types
    use them) and are now mapped.

## Phase 5 — Prefabs (~2.5k lines + format work) [DONE — 5a, 5b, 5c all DONE]
- 5a. `PrefabPasteUtil` + reusable paste buffer (block-mask gating,
  per-column heightmap fitting, environments, recursive child composition
  depth ≤ 10, seed composition); parse `.prefab.json` (10k files, data
  ready under `Assets/extracted/Server/Prefabs/`); `PrefabRotation` (also
  unlocks the deferred IChunkBounds rotated-bounds methods). **[DONE]** —
  detailed notes below.
- 5b. `PrefabPopulator`: sparse 8-block biome sampling, lattice collection,
  weighted picks, rotation, deep-search heights, `PrefabBaseCheck` evenness,
  AABB invalidation by category priority; Zone gains `uniquePrefabContainer`;
  ChunkGenerator gains the prefab caches. **[DONE]** (the populator is ACTIVE
  in `ChunkGeneratorExecution::execute`; the Default world chunk (0,0) seed 0
  contains 750 PREFAB-priority blocks, deterministic — see §5b notes at the
  end of the 5a subsection).
- 5c. Unique prefabs: `UniquePrefabConfiguration`, exclusion radii,
  zone-border exclusion, inverse domain-warp placement, spawn points
  (`getSpawnPoints`), `ValidationUtil`, `UniquePrefabContainer` port.
  **[DONE]** (zero `SEAM(phase-5)` markers left; the Default world's Spawn/
  Temple unique containers load and really paste — `Zone1_Spawn` /
  `Zone1_Temple`; per-seed unique lists for seeds 0/1/42/1234567; detailed
  notes in the 5c subsection below).
- Entities: stubbed (deferred per scope); `getExtents`/`PrefabContainerEntry`
  seams replaced with the real types.
- Verification: prefab placement coordinate lists + structure maps vs Java.
  **OPEN — merged into the Phase 4/5 chunk-level golden dump** (`ChunkDumper`
  Java side → `testdata/golden/chunks.json` → Rust golden test; the dump must
  include at least chunk (0,0) [terrain+caves+regular prefabs] AND the seed-0
  unique-prefab chunks (4,-5) [spawn room] and (9,4) [temple portal] over the
  seeds {0,1,42}; the entity/component writes are excluded per
  `REMAINING.md` §2).

### 5a — the prefab package [DONE]

Java sources: `worldgen/prefab/**` (`PrefabPasteUtil`, `PrefabPatternGenerator`,
`PrefabBaseCheck`, `PrefabCategory`, `PrefabStoreRoot`, `PrefabLoadingCache`),
`worldgen/loader/{WorldGenPrefabLoader,WorldGenPrefabSupplier}`,
`worldgen/loader/prefab/{PrefabPatternGeneratorJsonLoader,WeightedPrefabMapJsonLoader}`,
`CoreServer/.../prefab/{PrefabFormat,PrefabEntry,PrefabWeights,PrefabStore}` and
`CoreServer/.../prefab/selection/{buffer,mask}/**`.

- **The prefab buffer** (`crates/worldgen-hytale/src/prefab/buffer/`):
  `PrefabBuffer` (anchor, bounds, columns, child prefabs), the builder
  (the Java validation: the short range of the coordinates, the duplicate
  column, the sequential `y` values, the 0xFFFF offset limit), the accessor
  (`getMinX/MaxX(rotation)`, `getMinYAt`/`getMaxYAt`, `getBlockId`,
  `getFiller`, `getRotationIndex`, `forEach`) and the **fastutil column
  map** (`int_object_open_hash_map.rs`).
  - **The fastutil map is reproduced exactly** — the table size, the
    `HashCommon.mix` hash, the linear probing, the rehash (from the back of
    the old table to the front), the `0` = "null key" slot and the
    **descending** iteration order. It was validated against the real
    fastutil 8.5.12 jar (100+ random column sets of up to 400 columns: the
    table layout *and* the iteration order matched). The iteration order is
    observable through the per-block chance rolls of the paste.
  - **Layout deviation**: the Java stores a column in a packed byte segment
    (a `short` mask per block + the variable-width id / offset / chance /
    rotation / filler / fluid fields). The Rust port stores the same
    values in a plain `Vec<PrefabBufferBlockEntry>`; every height / index
    computation of the Java buffer is reproduced (the `y` of the first /
    last entry of a column, the rotated corner bounds, the rotated *query*
    of `getMinYAt` — the Java `// TODO I think we need to inverse the
    rotation here..` quirk).
- **The `.prefab.json` format** (`prefab/prefab_json.rs`): the version
  check (`Prefab version is too old/new`), the anchor, the blocks (the
  chance is encoded in the block name — `"50%Rock_Stone"`), the fluids, the
  support / filler / rotation fields, the short range validation and the
  duplicate-block check. The block id migrations are **not** ported (they
  need the CoreServer `BlockMigration` assets).
- **The prefab store** (`prefab/prefab_store.rs` + `loader/world_gen_prefab_loader.rs`):
  the name → file resolution (the dotted names, the `".*"` wildcards, the
  `.lpf` before the `.prefab.json`), the lazy buffer cache (the Java
  `PrefabLoadingCache`) and the `PrefabStoreRoot::ASSETS` root
  (`<assets>/Server/Prefabs`).
- **The paste** (`prefab/prefab_paste_util.rs`): `PrefabPasteBuffer`,
  `generate` / `generate0` / `generateChild`, the column predicate (the
  chunk test and the `fitHeightmap` offset), the block mask gating, the
  environment / fluid writes, the recursive child composition (depth ≤ 10)
  and the **seed composition** (the Java's *inverted* `inheritSeed` logic
  is reproduced verbatim, including the `HashUtil.hash(parent, cx, cy, cz)`
  derivation and the "increment on collision" fix-up).
- **The masks** (`prefab/mask/`): `BlockMask`, `MultiBlockMask`,
  `BlockFilter` (the 13 filter types, the prefixes, the `AboveBlock` /
  `BelowBlock` naming quirk) and `BlockPattern` (the `"50%Name"` chances,
  the `Filler=` triplet).
- **The weights** (`prefab/prefab_weights.rs`): the cumulative weight
  selection, the `Math.min(value, WeightedMap.EPSILON)` clamp, the
  `"Name=weight, Name=weight"` mapping string.
- **The loaders** (`loader/prefab/`): `WeightedPrefabMapJsonLoader` (the
  weight is divided by the number of the matching files) and
  `PrefabPatternGeneratorJsonLoader` (the grid generator, the height
  conditions, the block mask, the rotations, the displacement, the base
  checks).
- **The caves are wired**: `NamedPrefabSupplier` (the Phase-5 seam) is
  gone — the cave loaders resolve the prefab names through the prefab
  store and hand the suppliers of the matching files to the cave
  package. `PrefabCaveNodeShape.getFloorPosition` / `getCeilingPosition`
  read the per-column height map of the prefab and `populateChunk` pastes
  the prefab (`PREFAB_CAVE` priority); `CavePopulator.populatePrefab`
  pastes the cave prefabs (`CAVE_PREFAB` priority, with the
  `FLAG_SUBMERGE` flag when the cave type submerges).
  - The prefab cave node's bounds use the **recursive** chunk bounds of
    the prefab (`WorldGenPrefabSupplier.getBounds`) for X/Z and the prefab
    buffer for Y — the Java distinction is reproduced (the rotated bounds
    methods of `IChunkBounds` are ported with them).
- **The biome prefab containers load** (`loader/container/prefab_container_json_loader.rs`):
  the entries hold the real `PrefabPatternGenerator` and the real
  suppliers. The Default world therefore loads **completely** (the only
  remaining seam is the *unique* prefab container — 5c — which is skipped
  with a warning instead of aborting the load).
- **Verification**: `tests/prefab_files.rs` (the real files: the cave
  formation with 265 blocks / 21 columns and three amber trees with their
  extents, the name resolution, the version validation) and
  `tests/generation_smoke.rs` (**the Default world generates one chunk
  through the real `ChunkGeneratorExecution::execute`** — the chunk
  contains cave blocks (`CAVE`), cave prefabs (`CAVE_PREFAB`) and prefab
  cave nodes (`PREFAB_CAVE`), so the Phase-4 + 5a chain is real).
- **Seams that remain**:
  - **5b** — the `PrefabPopulator` (the biome prefab *placement*) is not
    ported: the biome prefab containers are loaded but nothing places
    them yet. `ChunkGeneratorExecution::execute` still has the
    `TODO(Phase 5): PrefabPopulator::populate` marker.
  - **5c** — the unique prefab container (`ZoneJsonLoader.loadUniquePrefabContainer`
    now *warns* instead of panicking; the unique prefabs are neither
    loaded nor placed).
  - The block **components** (the ECS state of a prefab block) and the
    prefab **entities** are stubbed (`REMAINING.md` § 2); the number of the
    skipped components / entities is reported by `PrefabJsonStats`.
  - The **`.lpf`** binary format is not ported (`REMAINING.md` § 5) — the
    asset pack ships no `.lpf` file.
- **Open deviations** (with their Java lines):
  - `PrefabStore.stripPrefabSuffix` (CoreServer `PrefabStore.java:96`)
    keeps the name of a `.prefab.json.lpf` file (it is not a prefab file);
    the *loading* strips it — the Rust port follows the Java.
  - `BlockPattern.parse` (CoreServer `BlockPattern.java:141`): the
    "filler fix" is a **no-op** in Java (the `;` → `,` normalisation undoes
    it) — the Rust port reproduces the Java behaviour (the filler triplet
    *is* split).
  - `BlockFilter` (CoreServer `BlockFilter.java:241`): the *item* store
    lookup (an item that places a fluid) is not ported — the Rust port has
    no item store, so only the fluid names resolve to fluids.
  - `PrefabWeights` (CoreServer `PrefabWeights.java:262`): the mapping
    string is built from a `HashMap` (the Java uses a fastutil map, so its
    iteration order is the hash order).
  - `PrefabStore` (CoreServer `PrefabStore.java:30`): the store has a
    single root (the Java searches every asset pack) and the caches are
    unbounded (the Java name cache expires after 30 s).
  - The prefab loader lives in the **root loading context** (the Java
    `SeedStringResource.loader`): the Rust `SeedString` does not carry the
    resource, so the loader is installed on the `FileLoadingContext` (which
    every loader of the chain can reach).
  - `PrefabStoreRoot::resolvePrefabStore` takes the assets root as an
    argument (the Java reads the CoreServer asset store).


### 5b — the `PrefabPopulator` [DONE]

Java sources: `worldgen/chunk/populator/PrefabPopulator.java` (580 lines),
`worldgen/chunk/ChunkGeneratorExecution.java` (the stage), `worldgen/chunk/
ChunkGenerator.java` (the prefab caches), `worldgen/ChunkGeneratorResource.
java` (the populator + the paste buffer), `worldgen/container/
UniquePrefabContainer.java` (the entry record).

- **The populator** (`crates/worldgen-hytale/src/chunk/populator/
  prefab_populator.rs`, 1:1 with the Java class): `run` → `collectBiomes` →
  `collectPrefabs` → `invalidatePrefabs` → `generatePrefabs` →
  `generateUniquePrefabs`, plus the helpers `collectBiome`, `collectPrefab`,
  `isBaseValid`, `intersects`, `isWithinUniquePrefabExclusionRange`,
  `getHeight`, `generateRotation`, `generatePrefabAt`, `isMatchingBiome`,
  `isMatchingChunkBounds`, `isMatchingHeight`, `isMatchingNoiseDensity`,
  `isMatchingParentBlock`, `getCoverInGroundAt`, `isMatchingCover`.
  - The populator is a **stateful object** in the Java (it lives in the
    thread-local `ChunkGeneratorResource`); the Rust port keeps the same
    shape: `ChunkGeneratorResource::prefab_populator`.
  - **The draw order is transcribed literally** (the populator is one long
    random stream): the stream is **re-seeded per lattice point**
    (`setSeed(hash3(prefabSeed, x, z) * 0x5feb8caf)`), then the weighted
    prefab pick (`nextDouble` — the *singleton* map draws nothing), then the
    rotation (`nextInt(rotations.length)`), then the height condition of the
    candidate, then the cover rolls of the parent-block check
    (`nextDouble() < coverDensity`). Two unit tests pin the order
    (`the_prefab_pick_and_the_rotation_follow_the_java_draw_order`,
    `a_singleton_prefab_map_does_not_consume_a_draw`).
  - **The sparse biome sampling** (`BIOME_SAMPLE_STEP_SIZE = 8`): the biomes
    of the chunk's own core-data entries plus a sample of the region around
    the chunk (the chunk padded by `HeightThresholdInterpolator.MAX_RADIUS`
    (5) and by the zone pattern provider's `getMaxExtent()`); the samples
    inside the interpolation region are skipped, and the custom biomes of a
    sampled zone are collected when their parent biome matches.
  - **The lattice collection**: the entry's grid generator collects the
    points of the chunk padded by `PrefabContainerEntry.getExtents()` (the
    maximum length of the *recursive* prefab bounds of the entry).
  - **The AABB invalidation**: a candidate invalidates the *lower priority*
    candidates whose rotated AABB overlaps it; a candidate whose priority is
    not above the minimum priority of the whole set never invalidates
    anything (`candidate.priority <= minPriority`).
  - **The base check** (`PrefabBaseCheck`): the *first* check whose
    `prefabSize` fits the prefab wins (the Java `return` inside the loop);
    the four probes of the plus (+) pattern around the body centre; the
    probes are skipped when `rad <= BASE_CHECK_MIN_RADIUS` (3); the radius
    is `fastFloor((size >> 1) * baseScale)` — a **float** multiplication.
  - **The placement**: `PrefabPasteUtil.generate` with the `PREFAB` priority
    (plus `FLAG_SUBMERGE` when the pattern submerges), the entry's
    environment, the pattern's mask, the height condition and the
    `fitHeightmap` flag. The paste seed is
    `HashUtil.hash(x, z) * 0xc0e394ca`.
- **The stage is ACTIVE** in `ChunkGeneratorExecution::execute` (tint →
  environment → blocks → caves → **prefabs** → water — the Java order).
- **The prefab caches**:
  - `ChunkGenerator::unique_prefab_cache` — a private twin of the Java
    `UniquePrefabCache` (50 entries, 5 min) in
    `crates/worldgen-hytale/src/cache/unique_prefab_cache.rs`;
    `ChunkGenerator::get_unique_prefabs(seed)` is the accessor.
  - The Java `prefabLoadingCache` (the loaded prefab buffers) is the
    `PrefabStore`'s buffer cache (the Java cache is keyed by the supplier,
    whose `equals` is the path — the same key).
- **`Zone` gains `uniquePrefabContainer`** (the Java record component) — the
  container keeps its Java seed offset, but its *generators* are still the
  5c seam (the `UniquePrefabContainerJsonLoader` skips the entries with a
  warning instead of aborting the world load).
- **`UniquePrefabContainer.UniquePrefabEntry`** (the Java record) is ported
  (the bounds are computed from the prefab buffer and the recursive prefab
  bounds) so that the populator can place the unique prefabs and exclude the
  regular ones around them.
- **Verification**:
  - `crates/worldgen-hytale/src/chunk/populator/prefab_populator.rs` — 15
    unit tests (the biome ordering, the intersection test, the rotation
    pick, the base check, the draw order, the lattice bounds, the AABB
    invalidation priority).
  - `tests/generation_smoke.rs` — the Default world chunk (0, 0) of the
    seed 0 now contains **PREFAB**-priority blocks (the regular prefab
    categories) and the generation is **deterministic** (the block grid and
    the priority histogram of two runs are identical). The priority
    histogram of that chunk (seed 0) is
    `NONE 204369 · FILLING 2636 · LAYER 93694 · COVER 359 · CAVE_COVER 407
    · CAVE 16099 · CAVE_PREFAB 1315 · PREFAB_CAVE 8051 · **PREFAB 750**`
    — the last number is the 5b contribution (the unique prefabs are still
    missing).
- **Open deviations**:
  - The paste buffer is allocated **per paste** (the Java reuses the
    thread-local `prefabBuffer`; the Rust buffer borrows the execution
    mutably, so it cannot live in the resource). The buffer is re-seeded
    before every paste, so the reuse is not observable.
  - The `atMostEvery(1, TimeUnit.SECONDS)` rate limit of the base-check log
    message is not ported.
  - The two `ConstantIntCondition` identity checks of `isMatchingParentBlock`
    are **dead code** in the Java (the parent condition is an
    `IBlockFluidCondition`, the constant is an `IIntCondition`) — not
    ported, exactly like the `BlockPopulator` does it.
  - The benchmark block (`getBenchmark().registerPrefab`) is Phase 6.
- **Seams that remain (5c)** — *line precise* — **NONE** (5c is DONE, see
  the section below):
  - ~~`crates/worldgen-hytale/src/chunk/chunk_generator.rs:648-656` —
    `ChunkGenerator::generate_unique_prefabs` returns an empty list.~~ →
    ported (5c).
  - ~~`crates/worldgen-hytale/src/loader/container/
    unique_prefab_container_json_loader.rs:72-95` — the *generators* of the
    container are skipped with a warning.~~ → ported (5c).
  - ~~`crates/worldgen-hytale/src/loader/zone/zone_json_loader.rs:223-248` —
    `ZoneJsonLoader::load_unique_prefab_container`.~~ → ported (5c).
  - ~~`crates/worldgen-hytale/src/container/unique_prefab_container.rs` — the
    `UniquePrefabGenerator` placeholder.~~ → ported (5c).
  - ~~`ChunkGenerator::get_spawn_points` (the Java `getSpawnPoints`) is not
    ported yet.~~ → ported (5c).
- **Chunk-level golden parity (for the next session)**:
  - The chunk grid **changed** in 5b: the prefab stage is active, so the
    chunk now carries `PREFAB`-priority blocks (and the terrain/cave blocks
    under them are *not* replaced — the prefab priority wins).
  - The Default world chunk **(0, 0) with the seed 0** is the current smoke
    test (`tests/generation_smoke.rs`); the Java dumper must dump the *same*
    chunk list (chunk coordinates + seed) for the parity test.
  - The `PREFAB` blocks are the *regular* prefab categories only — the
    unique prefabs (5c) are not placed yet, so the parity test must be redone
    after 5c (the unique prefabs overwrite a large part of the spawn and
    temple zones).


### 5c — the unique prefabs [DONE]

Java sources: `worldgen/prefab/unique/{UniquePrefabConfiguration,UniquePrefabGenerator}.java`
(149 + 231 lines), `worldgen/loader/prefab/unique/UniquePrefabConfigurationJsonLoader.java`
(228 lines), `worldgen/container/UniquePrefabContainer.java` (the `generate`
method), `worldgen/chunk/ValidationUtil.java` (183 lines) and the unique
prefab parts of `worldgen/chunk/ChunkGenerator.java` (`getUniquePrefabs`,
`generateUniquePrefabs`, `getSpawnPoints`).

- **The configuration** (`crates/worldgen-hytale/src/prefab/unique/
  unique_prefab_configuration.rs`, 1:1 with the Java class): the height
  threshold condition, the block placement mask, the rotations, the biome
  mask, the noise (map) condition, the parent block condition, the anchor,
  the spawn offset, the max distance, the fit-heightmap / submerge /
  on-water flags, the environment id, the max attempts, the **squared**
  exclusion radius, the spawn flag, the zone-border exclusion and the
  show-on-map flag.
- **The generator** (`…/prefab/unique/unique_prefab_generator.rs`, 1:1): the
  per-seed placement of one unique prefab.
  - `generate(seed, position, chunkGenerator, random, maxFailed, entries)`:
    the *unique zone* placement (the inverse domain warp) or the *random
    search* around the anchor (with the fallback to the anchor).
  - `tryPlacement` — the draw order is `nextDouble` (the distance) and
    `nextFloat` (the angle) per attempt; then the exclusion, the noise
    density, the zone id, the biome mask, the zone-border exclusion, the
    height and the parent-block checks.
  - `forceGeneration` — the fallback (the floored anchor).
  - `forceUniqueZonePlacement` — the **inverse domain warp**: eight
    iterations of `x -= (floor(mask.getX(seed, x, z)) - target.x) / 2`
    (the Java integer division truncates toward zero).
  - `getHeight` — the water height when the biome has water *and* the
    configuration is `OnWater`, the terrain height otherwise.
  - `isMatchingParentBlock` — the cover in the ground + the top block of the
    layer container (the same checks the `PrefabPopulator` does).
- **The container** (`container/unique_prefab_container.rs`): the Java
  `generate(seed, position, chunkGenerator)` — one `FastRandom(seed +
  seedOffset)` for the whole container, the *placement* of a generator
  **before** its prefab pick and its rotation pick (all three share the
  stream), and the entry list that the exclusion check consults.
- **The loaders** (`loader/prefab/unique/unique_prefab_configuration_json_loader.rs`
  and `loader/container/unique_prefab_container_json_loader.rs`): the real
  generators (the `SEAM(phase-5)` warning-skips are **gone**).
  - The seed chain: `…UniquePrefabContainer-<n>.UniquePrefabGenerator` for
    the *n*-th entry; the configuration loader does **not** append a suffix
    (it shares the generator's seed); the weighted prefab map appends
    `.WeightedPrefabMap`; the biome mask appends `.BiomeMask-UniquePrefab`;
    the block mask appends `.BlockMaskCondition`; the parent block array
    appends `ResolvedBlockArray`.
  - The verbatim error messages: `Could not find anchor for Unique prefab
    generator`, `Error while looking up environment "%s"!`.
- **`ChunkGenerator::generate_unique_prefabs`** (the Java
  `generateUniquePrefabs`): the *unique zones* place their prefabs at the
  center of the zone (the position is passed to the container); the other
  zones place theirs around their anchor (the position is `null`).
- **`ChunkGenerator::get_spawn_points`** (the Java `getSpawnPoints`): the
  spawn position is `position + rotate(spawnOffset) + (0.5, 0, 0.5)`, the
  yaw is `-rotation.getYaw()`; the fallback is the chunk center with
  `HeightmapColumn.NO_HEIGHT`; the list is shuffled with
  `FastRandom(seed * 0x59122134)`.
- **`ValidationUtil`** (`chunk/validation_util.rs`): the validation of every
  prefab (and every prefab child) of the world — the unique prefab
  generators, the biome prefab containers and the cave node types (the cave
  node types are visited once per name).
- **Quirks transcribed**:
  - The **exclusion distance is measured to the *previous entry*** (not to
    the anchor) and it is compared against **both** the previous entry's
    exclusion radius **and** the configuration's own exclusion radius — so
    the configuration's radius acts as a *minimum* exclusion radius against
    every previous entry.
  - The **draw order** of a container: the placement of the generator, then
    the prefab pick (`IWeightedMap.get(Random)` — the singleton map does
    **not** consume a draw), then the rotation pick
    (`nextInt(rotations.length)`).
  - The **`maxAttempts`** semantics: the search makes `maxAttempts + 1`
    attempts (`failed` is incremented *after* the failed attempt).
  - The `int` overflow of `seed + seedOffset` (the Rust port uses the
    wrapping addition).
  - The `Zone.Unique` placement: the unique zones are *painted* into the
    mask per seed, so the spawn/temple prefabs are placed through the
    inverse domain warp and *not* through the random search.
- **Verification**:
  - `crates/worldgen-hytale/src/prefab/unique/*` — 16 unit tests (the
    exclusion math, the inverse domain warp, the border exclusion, the biome
    mask, the draw order, the determinism).
  - `crates/worldgen-hytale/src/container/unique_prefab_container.rs` — the
    container generation (the draw order, the seed offset, the determinism).
  - `crates/worldgen-hytale/src/chunk/chunk_generator.rs` — the unique
    prefabs, the cache and the spawn points.
  - `crates/worldgen-hytale/src/chunk/validation_util.rs` — the validation
    (a valid world, a world with a broken prefab).
  - `crates/worldgen-hytale/tests/unique_prefabs.rs` — the **Default
    world**: the Spawn / Temple containers load, the per-seed generation is
    deterministic, the spawn points follow the unique prefabs and the spawn
    room is really **pasted** into its chunk (the block ids and the `PREFAB`
    priority).
  - `crates/worldgen-hytale/tests/generation_smoke.rs` — the priority
    histogram of the chunk (0, 0) is now **pinned** (it did *not* change:
    the unique prefabs of the seed 0 land in the chunks (4, -5) and (9, 4),
    not in (0, 0)).
- **Open deviations**:
  - The `UniquePrefabGenerator` holds the weighted map of the prefab
    suppliers *by value* (the Java holds the references); the generated
    entry therefore gets a fresh shared handle of the picked supplier (the
    supplier is a value object — its `equals` is the path).
  - The `Transform` / `Rotation3f` of the CoreServer are not ported; the
    Rust port has a minimal `util::transform` (the position + the rotation
    components) — the spawn point computation is 1:1.
  - The Java validation runs asynchronously on the chunk generator's
    executor; the Rust port validates synchronously (the same result).
  - The Java logs the *path* of a prefab; the Rust port logs the *prefab
    name* of the child prefabs (the `PrefabSupplier` trait does not expose
    the path).
- **Seams that remain** — *none* in the prefab package. The documented
  deferred items are the prefab **entities** and the prefab **components**
  (`REMAINING.md` § 2) and the `.lpf` binary format (`REMAINING.md` § 5).
  The `SEAM(phase-5)` markers that are left in the tree are the
  `SEAM(prefab-loader)` guards (the prefab loader is not configured when the
  world generator was not built through the loader chain) — they are not
  Phase-5 gaps.
- **Chunk-level golden parity (for the next session)**:
  - The chunk grid **changed** in 5b (the prefab stage is active) and
    **again in 5c** for the chunks that contain a unique prefab (the spawn
    room and the temple portal overwrite a large part of their chunks).
  - The Default world chunk **(0, 0) with the seed 0** is the current smoke
    test (`tests/generation_smoke.rs`); the Java dumper must dump the *same*
    chunk list (chunk coordinates + seed) for the parity test.
  - The parity test should dump the chunk **(4, -5)** as well — it holds the
    spawn room of the seed 0.

## Chunk-level golden parity — IN PROGRESS (150 / 21,504 columns)

The Java dump (`testdata/golden/chunks.json`) and the Rust parity test are in
place and deterministic. The current state after this session's fixes:

- **Exact**: tint (interpolated + raw), zone, biome, parent tile biome,
  biome-pattern index/cell, border distance, the base environment mapping and
  all terrain / layer / cover / water / prefab writes.
- **Remaining**: 150 columns (147 block voxels + 3 environment runs), all cave
  stage: 146 `CAVE` voxels and 1 `CAVE_COVER` voxel (per-y count). The first
  example is seed 0, chunk (0,0), column (0,30): Java places the
  `Plant_Roots_Cave_Small` ceiling cover at y=63, Rust leaves the cave air
  there because the cover's `NoiseMask` **map condition** evaluates false (the
  height and density conditions and the parent check are all true).
- **Next step**: compare the cover map-condition noise value/threshold between
  Java and Rust for the cover files of the affected cave node types (Copper /
  Grasshopper/Deep / Volcanic children — the cover is a `NoiseMaskCondition`
  loaded by `CaveNodeCoverEntryJsonLoader`). This looks like one more
  noise-edge/threshold case, not a structural cave-tree difference: the cave
  node graph (types, seeds, bounds, ends, prefab lists/positions) and the
  per-node floor/ceiling/replace geometry were verified to be **identical**
  for seed 0 chunks (0,0) and (7,3).

### What was wrong in the previous session (important)

The earlier "mount-root blocker" diagnosis was incomplete; the real blockers
were test/dumper bugs plus two procedural noise bugs:

1. **Dump column order**: `ChunkDumper` wrote `columnRefs[ref++]` in the
   `for x { for z }` order while the documented (and test-used) order is
   `ChunkUtil.indexColumn(x, z) = (z << 5) | x`. The dump now writes
   `columnRefs[ChunkUtil.indexColumn(x, z)]`.
2. **Test chunk index**: the Rust tests built `GeneratedChunk` with
   `with_coordinates(0, x, z)` for every chunk. `execution.get_index()` was
   therefore `0` for the non-origin chunks, so `CavePopulator` never found a
   cave containing the chunk. All tests now pass
   `hytale_math::chunk_util::index_chunk(x, z)`.
3. **`FractalMode::OLDSCHOOL` first octave**: Rust incremented `offsetSeed`
   before the first `noise.get`; Java passes the original seed to the first
   octave (`offsetSeed++` reads before incrementing). This alone caused 2,590
   loader-golden deviations (and the biome/terrain divergence initially
   attributed to file order).
4. **`DensityPointEvaluator.randomDensityCondition`**: Java's `0x9afc33ae` is
   an **int** literal (negative, `-1693957202`) widened to `long`; Rust used
   the positive `i64` `2601006094`. The sign extension is fixed.
5. **`SeedString.appendToOriginal`** must keep the original and only rebuild
   the seed (`original + suffix`); the port had replaced both.
6. **`BiomePatternGeneratorJsonLoader`** must feed the `GridGenerator` child
   to the point-generator loader (Java `get(KEY_GRID_GENERATOR)`), not the
   whole `BiomeGenerator` element.
7. The dumper now boots the world from the **disk** tree
   (`Assets/extracted/Server/World/Default`) instead of the zip mount (kept,
   even though the walk order was not the root cause).
8. The registries dump now carries `BlockType.getOpacity()` (the
   `GeneratedBlockChunk.getHeight` test used by surface-limited caves) in
   addition to the per-rotation support probes.

### The dump format and commands

- `ChunkDumper.java` (+ `ChunkProbe.java`, a fast-iteration runner:
  `java -Djava.util.concurrent.ForkJoinPool.common.parallelism=1 -cp <cp>
  com.hypixel.hytale.tools.golden.ChunkProbe <outDir> <worldDir> <repoRoot>` —
  mkdir the outDir first; the probe writes `chunks.json` there; ~5 s). Wired
  into `GoldenDumper.main` as the "Phase 8" dump
  (`testdata/golden/chunks.json`).
- Per (seed, chunk): `cols[i]` references `columnDefs` in
  `ChunkUtil.indexColumn` order; a column def is
  `[tint, biomeId, zoneId, envRuns, blockRuns, parentBiomeId, borderDistance,
  biomeIndex, cellX, cellY, rawTint]`. The last six are debug references; the
  parity target is the first five. Env runs = `[minYInclusive, maxYInclusive,
  environmentId]`; block runs = `[yStart, yEnd, blockId, rotation, filler,
  fluidId, fluidLevel, priorityByte]`. The ids resolve through the palettes.
- The reference histogram of seed-0 chunk (0,0) from the current dump:
  `NONE 204871 · FILLING 2493 · LAYER 93930 · COVER 511 · CAVE_COVER 187 ·
  CAVE 15892 · CAVE_PREFAB 1312 · PREFAB_CAVE 8462 · PREFAB 22`.
- The test takes ~2 min (one world load per seed; 21 chunk executions). The
  dumper writes ~14 MB; `ChunkProbe` is the fast path for dump changes.

## Phase 6 — Integration & polish [last]
- Final `trait WorldGen` (async `generate(seed, index, x, z, cancel, priority)`),
  worker pool (replaces the private cache twin with the real concurrent
  design), graceful shutdown, per-stage timing metrics.
- `worldgen-cli`: `generate`, `render-map`, `dump-chunk`, `validate`, `bench`
  (criterion ports of the two JMH benchmarks).
- `HytaleWorldGenProvider` config semantics (Name/Version/Path); the serde
  loaders read an asset root (defaults to `Assets/extracted/` layout).
- Architecture doc: Java class → Rust module mapping (1:1 mapping table).

## Cross-cutting / open risks (tracked, do not forget)
- **Id-order caveat**: the dumped registry ids are pinned to the dumper's
  canonical single-threaded load order; a real-server dump should be diffed
  when available (see `testdata/golden/registries.json` `meta`).
- `procedural::noise::support` reduced seams still shadow the full
  `supplier`/`condition` ports — integrate them when a loader phase touches
  those files.
- Chunk-level golden parity now depends on the FULL Java pipeline in the
  dumper (blocks resolve!): extend `RegistryDumper`'s boot to run the actual
  Default-world generation for fixed seeds/chunks and dump block grids —
  the definitive Phase-3 verification (was blocked on the asset pack).
- `ChunkGeneratorExecution::execute()` still lacks the cave/prefab stages —
  the in-code NOT-BIT-EXACT flag comes off at Phase 4/5.
