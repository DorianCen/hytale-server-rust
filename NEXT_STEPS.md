# Next steps — Phase 2/3 continuation roadmap

Status: Phase 0 + Phase 1 complete and golden-verified. Phase 2 partially done
(`zoom/` + climate core in `crates/worldgen-hytale`). This file lists the next
subtasks in dependency order, sized for ~200k-context subagent runs
(`opencode/glm-5.3-flash-free` / `opencode/deepseek-v4.1-flash-free`).

## Verified so far (do not regress — `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` must stay green)
- `hytale-math`: FastRandom, HashUtil, MathUtil, ChunkUtil, TrigMathUtil, ranges — golden tests vs `testdata/golden/{fastrandom,hashutil,mathutil}.json` (bit-exact).
- `procedural`: all 11 noise types — golden tests vs `testdata/golden/noise.json` (bit-exact 2d+3d); NoiseProperty DSL; random/ (coordinate randomizers + rotators); point/ (IPointGenerator family); condition/ package.
- `worldgen-hytale`: zoom/ (PixelProvider, PixelDistanceProvider, ExactZoom, FuzzyZoom, unique-zone placement helpers); climate/ core (ClimateType, ClimateGraph, DistanceTransform, ClimateNoise, ClimateSearch, DirectGrid, IntMap/DoubleMap).

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

## Subtask 5 — ClimateMaskProvider + UniqueClimateGenerator [needs 3+4]
- Finishes the climate package

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
- 23 new tests (374 workspace total).

## Subtask 6 — Java dumper extension (Phase 2 golden data)
- Extend `tools/java-dumper` to dump: ExactZoom.generate over a test grid from the real Default-world Mask, ClimateNoise.generate maps, ClimateGraph table/fade maps for fixed seeds → `testdata/golden/{zoom,climate}.json`
- Then wire `crates/worldgen-hytale/tests/golden.rs`

## Subtask 7 — serde loaders for World.json / Zones.json / mask config (Phase 2 close)
- 77 Java loader classes become #[derive(Deserialize)] structs; transcribe defaults/validation faithfully (plan risk #3)
- Flexible gson semantics: untyped maps, weighted-map shorthands (`"!Rock_Bedrock"`, `"*"` in masks)

Then Phase 3 (populators) per plan.md.
