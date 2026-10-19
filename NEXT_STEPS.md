# Next steps — Phase 2/3 continuation roadmap

Status: Phase 0 + Phase 1 complete and golden-verified. Phase 2 partially done
(`zoom/` + climate core in `crates/worldgen-hytale`). This file lists the next
subtasks in dependency order, sized for ~200k-context subagent runs
(`opencode/glm-5.3-flash-free` / `opencode/deepseek-v4.1-flash-free`).

## Verified so far (do not regress — `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` must stay green)
- `hytale-math`: FastRandom, HashUtil, MathUtil, ChunkUtil, TrigMathUtil, ranges — golden tests vs `testdata/golden/{fastrandom,hashutil,mathutil}.json` (bit-exact).
- `procedural`: all 11 noise types — golden tests vs `testdata/golden/noise.json` (bit-exact 2d+3d); NoiseProperty DSL; random/ (coordinate randomizers + rotators); point/ (IPointGenerator family); condition/ package.
- `worldgen-hytale`: zoom/ (PixelProvider, PixelDistanceProvider, ExactZoom, FuzzyZoom, unique-zone placement helpers); climate/ core (ClimateType, ClimateGraph, DistanceTransform, ClimateNoise, ClimateSearch, DirectGrid, IntMap/DoubleMap).

## Subtask 1 — weighted map + worldgen/util (~1.9k Java lines) [independent]
- Common/map: IWeightedMap, WeightedMap, DefaultMap, IWeightedElement → `crates/worldgen-hytale/src/weighted_map/`
- worldgen/util: ArrayUtli, BlockArray, NoiseBlockArray, ResolvedBlockArray, BlockFluidEntry, ListPool, ObjectPool, ConstantNoiseProperty, LogUtil, function/, bounds/, cache/ (use moka), condition/
- Skip ChunkThreadPoolExecutor/ChunkWorkerThreadFactory (different worker design)

## Subtask 2 — biome package (~508 lines) [needs subtask 1: container + IWeightedMap]
- biome/: Biome, TileBiome, CustomBiome, CustomBiomeGenerator, BiomePatternGenerator, BiomeInterpolation
- Needs chunk/HeightThresholdInterpolator (port alongside — 129 lines)

## Subtask 3 — chunk core skeleton (~1.3k lines) [needs subtask 1]
- chunk/: BlockPriorityChunk (69 lines, priority enum per plan), BlockPriorityModifier, ZoneBiomeResult, MaskProvider (59), ValidationUtil, HeightThresholdInterpolator
- Registries: block/fluid/environment string→int maps extracted from `testdata/Default` JSON usage

## Subtask 4 — zone package + unique zones (~524 lines) [needs 1+2+3]
- zone/: full Zone class, ZoneColorMapping, ZonePatternProvider/Generator(+Cache), ZoneGeneratorResult, ZoneDiscoveryConfig
- Replaces the partial `src/zone.rs` value types

## Subtask 5 — ClimateMaskProvider + UniqueClimateGenerator [needs 3+4]
- Finishes the climate package

## Subtask 6 — Java dumper extension (Phase 2 golden data)
- Extend `tools/java-dumper` to dump: ExactZoom.generate over a test grid from the real Default-world Mask, ClimateNoise.generate maps, ClimateGraph table/fade maps for fixed seeds → `testdata/golden/{zoom,climate}.json`
- Then wire `crates/worldgen-hytale/tests/golden.rs`

## Subtask 7 — serde loaders for World.json / Zones.json / mask config (Phase 2 close)
- 77 Java loader classes become #[derive(Deserialize)] structs; transcribe defaults/validation faithfully (plan risk #3)
- Flexible gson semantics: untyped maps, weighted-map shorthands (`"!Rock_Bedrock"`, `"*"` in masks)

Then Phase 3 (populators) per plan.md.
