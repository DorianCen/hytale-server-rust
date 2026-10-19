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

## Subtask 3 — chunk core skeleton (~1.3k lines) [needs 1+2]
- chunk/: BlockPriorityChunk (69 lines, priority enum per plan), BlockPriorityModifier, ZoneBiomeResult, MaskProvider (59), ValidationUtil, HeightThresholdInterpolator (+ cache/CoreDataCacheEntry, cache/InterpolatedBiomeCountList, ChunkGeneratorExecution, ChunkGeneratorCache)
- ChunkGenerator skeleton: getCoreData, generateInterpolatedHeightNoise,
  static generateInterpolatedThreshold, generateLowest/HighestThresholdDependent
  (needs ZonePatternProvider seam until Subtask 4)
- Registries: block/fluid/environment string→int maps extracted from `testdata/Default` JSON usage

## Subtask 4 — zone package + unique zones (~524 lines) [needs 1+2+3]
- zone/: full Zone class (replaces the partial `zone/zone.rs` slice; appends
  biomePatternGenerator + caveGenerator handle), ZoneColorMapping,
  ZonePatternProvider/Generator(+Cache)
- ZoneGeneratorResult gains the real Zone payload; ZoneBiomeResult port

## Subtask 5 — ClimateMaskProvider + UniqueClimateGenerator [needs 3+4]
- Finishes the climate package

## Subtask 6 — Java dumper extension (Phase 2 golden data)
- Extend `tools/java-dumper` to dump: ExactZoom.generate over a test grid from the real Default-world Mask, ClimateNoise.generate maps, ClimateGraph table/fade maps for fixed seeds → `testdata/golden/{zoom,climate}.json`
- Then wire `crates/worldgen-hytale/tests/golden.rs`

## Subtask 7 — serde loaders for World.json / Zones.json / mask config (Phase 2 close)
- 77 Java loader classes become #[derive(Deserialize)] structs; transcribe defaults/validation faithfully (plan risk #3)
- Flexible gson semantics: untyped maps, weighted-map shorthands (`"!Rock_Bedrock"`, `"*"` in masks)

Then Phase 3 (populators) per plan.md.
