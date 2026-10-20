# Next steps — continuation roadmap (updated 2026-10-20)

Status: Phase 0 + Phase 1 complete and golden-verified. Phase 2 zoom/climate
golden-verified (Subtask 6). Subtasks 1–4 + 6 DONE; Phase 3 terrain/water
populators DONE. This file lists the remaining work in dependency order,
sized for ~200k-context subagent runs. Free-model pool (subagents):
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
  discoveryConfig only — `caveGenerator` Phase 4, `biomePatternGenerator` +
  `uniquePrefabContainer` Subtasks 4/5 append fields).
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
- ZoneColorMapping / ZonePatternProvider / Generator / Cache landed with
  Subtask 3; Zone record still lacks caveGenerator (Phase 4) and
  uniquePrefabContainer (Phase 5) — those porters append the fields.
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

## Subtask 7 — serde loaders: World.json / Mask.json / Zones.json / zone files (Phase 2 close) [NEXT UP]
Java sources: `worldgen/loader/**` (77 classes) — sized as 2–3 subagent runs.
- 7a. Core config path: `WorldGenConfig`, `ChunkGeneratorJsonLoader`,
  `MaskProviderJsonLoader`/`ClimateMaskJsonLoader`, `ZoneJsonLoader`,
  `ZoneColorMappingJsonLoader` → `#[derive(Deserialize)]` structs in
  `crates/worldgen-hytale/src/loader/`. Transcribe defaults/validation
  faithfully (plan risk #3); flexible gson semantics: untyped maps,
  weighted-map shorthands (`"!Rock_Bedrock"`, `"*"` in masks) need custom
  serde deserializers; `SeedString` seed labels (`"RANDOMIZER"`) reproduce
  the dumper's verified convention (`hash("<seed>.<label>") * 114512143`).
- 7b. Registries wiring: extend `Registries::from_golden` with the NAME → id
  direction (`BlockType.getAssetMap().getIndex(name)` + case-insensitive
  lookup, rotation suffixes via `BlockPattern.BlockEntry.decode` —
  `"Name@rotation"`), switch `ChunkGeneratorExecution`'s default registry
  from `Registries::empty()` to the golden-loaded one (a `Registries::load()`
  reading `testdata/golden/registries.json` — path configurable later).
- 7c. Content loaders the biome/zone configs reference: NoiseBlockArray /
  ResolvedBlockArray / ResolvedVariantsBlockArray JSON loaders, tint/env
  mapping, cover/layer/water/prefab container loaders, cave config loaders
  (with the Phase-4 cave types), `Prefabs.json` + `.prefab.json` parsing
  (10,050 files under `Assets/extracted/Server/Prefabs/` — the `PrefabStore:
  ASSETS` store), environments.
- 7d. Loader golden tests: Java dumper extension loading a REAL Zone config
  and dumping the resolved containers/biomes; Rust asserts equality. This
  also cross-checks the Subtask-4 unique-zone wiring note.

## Subtask 5 — ClimateMaskProvider + UniqueClimateGenerator [after 7a/7b]
- `chunk/MaskProvider` (the chunk-level one the ChunkGenerator uses) +
  `ClimateMaskProvider` + `UniqueClimateGenerator` (unique-zone placement via
  ring-spiral `ClimateSearch` scoring — the zoom/ unique-placement helpers
  already ported cover the image path; this is the climate path).
- Finishes the climate package; needs the loaders (7a) for the mask configs.

## Phase 4 — Caves (~4.6k Java lines, largest phase) [after 5]
Port slices in order, each ending green + reviewed:
- 4a. Core value types: `Cave`, `CaveType`, `CaveNodeType`,
  `CaveBiomeMaskFlags`, `CaveGeneratorCache` (+ ExtendedCoordinateCache use),
  `CaveBlockPriorityModifier` (the modifier that lets regular caves carve
  into prefab caves); Zone record gains the `caveGenerator` field;
  ChunkGenerator gains `getCave`/`generateCave` + `cave_generator_cache`.
- 4b. Node-graph growth: `CaveGenerator` recursive yaw/pitch walk, anchors,
  radius inheritance, GENERATE/POPULATE/CONTINUE flags, chunk-bucket compile.
- 4c. Shapes: pipe, cylinder, ellipsoid, empty-line, prefab-as-node.
- 4d. The distorted family (largest): extrusion/body math, interpolation
  curves, pitch compensation, width/floor/ceiling distortion noise;
  `CaveNodeShapeUtils` (anchors, covers, support invalidation), fillings/
  fluids.
- 4e. Cave JSON loaders (cross-zone node references, `CaveNodeTypeStorage`)
  + golden verification: Java dumper dumps whole caves for fixed seeds →
  chunk-slice PNGs + block-histogram invariants vs Java.

## Phase 5 — Prefabs (~2.5k lines + format work) [after 4a]
- 5a. `PrefabPasteUtil` + reusable paste buffer (block-mask gating,
  per-column heightmap fitting, environments, recursive child composition
  depth ≤ 10, seed composition); parse `.prefab.json` (10k files, data
  ready under `Assets/extracted/Server/Prefabs/`); `PrefabRotation` (also
  unlocks the deferred IChunkBounds rotated-bounds methods).
- 5b. `PrefabPopulator`: sparse 8-block biome sampling, lattice collection,
  weighted picks, rotation, deep-search heights, `PrefabBaseCheck` evenness,
  AABB invalidation by category priority; Zone gains `uniquePrefabContainer`;
  ChunkGenerator gains the prefab caches.
- 5c. Unique prefabs: `UniquePrefabConfiguration`, exclusion radii,
  zone-border exclusion, inverse domain-warp placement, spawn points
  (`getSpawnPoints`), `ValidationUtil`, `UniquePrefabContainer` port.
- Entities: stubbed (deferred per scope); `getExtents`/`PrefabContainerEntry`
  seams replaced with the real types.
- Verification: prefab placement coordinate lists + structure maps vs Java.

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
