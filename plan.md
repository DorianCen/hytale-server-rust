# Plan: Rust Rewrite of the Hytale WorldGenerator

Source module: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator` (252 Java files, ~24,500 lines in `server/worldgen/` + ~9,000 lines of JSON loaders).

Data assets: `HytaleAssets/Server/World/Default/` (55 MB — `World.json`, climate mask config, 31 zone folders, 12,079 JSON files, mostly `.prefab.json` structures).

## Locked decisions

| Decision | Choice |
|---|---|
| Fidelity | **Bit-exact parity** — same seed → identical chunks, verified against the Java jar with golden dumps at every phase |
| Scope | **Generation only** — terrain/caves/prefabs/world layout. Modifier plugin & entity pasting deferred; Phase 0 delivers a `REMAINING.md` documenting deferred work |
| Layout | **Multi-crate workspace** — `hytale-math`, `procedural`, `worldgen-model`, `worldgen-hytale`, `worldgen-cli` |
| Concurrency | **Dedicated worker pool** — fixed threads, explicit `&mut GenContext` scratch per task, priority queue, cancellation tokens |
| Serialization | **serde_json** replaces gson + the Hytale `Codec` framework; the 77 Java loader classes become `#[derive(Deserialize)]` structs with faithful transcription of defaults/validation |
| Caches | Port the semantic contract (sized + timed) via `moka` or a small sharded cache: 50k/20 s core data, 5k/30 s caves, 50/5 min unique prefabs |
| CoreServer boundary | Minimal `trait WorldGen` returning our own `GeneratedChunk`; do **not** port `Holder<ChunkStore>`/ECS yet |

## Architecture being ported (verified end-to-end)

1. **Load once per world**: `HytaleWorldGenProvider` → `WorldGenConfig` → `ChunkGeneratorJsonLoader` reads `World.json` → image mask (`Mask.png` → `FuzzyZoom`) **or** procedural climate mask (`Mask.json` → `ClimateNoise` + 512×512 `ClimateGraph` table built by hierarchical weighted-voronoi in (temperature, intensity) space + Dijkstra distance-transform fade) → `Zones.json` → `Zone[]` → `ChunkGenerator`.
2. **Per seed**: unique zones (Spawn, Temple) rolled by scanning/painting colored discs into the mask (image path) or ring-spiral `ClimateSearch` scoring (climate path) → immutable per-seed `ZonePatternGenerator`.
3. **Per column (hot path, cached)**: fuzzy domain-warp jitter → mask sample → RGB → `ZoneColorMapping` → Zone → `BiomePatternGenerator` (jittered voronoi cell-hash → `TileBiome`, optional `CustomBiome` noise overlay with border fade) → height threshold context + heightmap noise → `ZoneBiomeResult`.
4. **Per chunk** (own thread pool `cores*0.75`): prepare `(32+2·5)²` `HeightThresholdInterpolator` table → strict stage order: tint mapping → environment mapping → `BlockPopulator` (terrain fill, static/dynamic layers, covers) → `CavePopulator` (caves = seeded recursive node graphs of shapes, cached whole caves, chunk-bucketed) → `PrefabPopulator` (lattice-collected, category-priority overlap invalidation, recursive child composition depth ≤ 10, unique prefabs) → `WaterPopulator`.
5. **All block writes** funnel through `BlockPriorityChunk` byte grid with priorities `FILLING(1) < LAYER(2) < COVER(3) < WATER(4) < CAVE_COVER(5) < CAVE(6) < CAVE_PREFAB(7) < PREFAB_CAVE(8) < PREFAB(9)` + `FLAG_SUBMERGE`, pluggable modifier (cave variant lets regular caves carve into prefab caves).
6. **Zero-allocation style**: `ThreadLocal<ChunkGeneratorResource>` scratch pools in Java → becomes explicit `&mut GenContext` in Rust.

## Proposed workspace layout

```
hytale-server-rust/
├── crates/
│   ├── hytale-math/        # FastRandom, HashUtil, ChunkUtil, MathUtil, vectors
│   ├── procedural/         # port of ProceduralLibrary (noise, points, conditions, NoiseProperty)
│   ├── worldgen-model/     # chunk data model, block priority grid, registries (block/fluid/env)
│   ├── worldgen-hytale/    # THE port: zones, biomes, climate, caves, prefabs, populators, loaders
│   ├── worldgen-cli/       # headless harness: generate chunks → PNG/JSON dumps, benchmarks
│   └── worldgen-server/    # CoreServer-facing chunk-provider adapter
└── testdata/               # extracted Java assets + golden reference dumps
```

## Phases

### Phase 0 — Scaffolding
- [x] Cargo workspace, CI, clippy/fmt, `worldgen-cli` skeleton
- [x] Extract the Default world assets into `testdata/`
- [x] Write `REMAINING.md` (deferred scope: modifier/event plugin system, entity serialization from prefabs, CoreServer/ECS integration, world map UI)
- [x] Build the Java generator via Maven + a Java "dumper" emitting golden JSON/bytes for fixed seeds (noise samples, zone/biome maps, chunk dumps) — the parity backbone reused by every later phase (`tools/java-dumper/`)

### Phase 1 — `hytale-math` + `procedural` (~2.5k Java lines)
- [x] `FastRandom`, `HashUtil` (seed hashing/rehashing), `ChunkUtil`, `MathUtil` — exact Java float/int semantics (golden-verified)
- [x] Noise: Simplex, Perlin, Value, Cell (`CellNoise`), Grid, Mesh/Hex, Distance, Point + `ResultBuffer2d/3d` (golden-verified bit-exact, 2d + 3d)
- [x] `NoiseProperty` combinator DSL (fractal, sum, scale, rotate, distort, blend, gradient, curve)
- [x] `IPointGenerator` jittered grids (`nearest2D`, `collect`; 3×3 cell search, 256-entry offset tables), `CoordinateRandomizer` (fuzzy domain warp)
- [x] Conditions: `IIntCondition`, `ICoordinateCondition`, `IHeightThresholdInterpreter`, `DoubleRange`, thresholds
- [x] Golden tests vs Phase 0 dumper output (`crates/*/tests/golden.rs`)

### Phase 2 — World layout: mask → zones → biomes (~2.9k lines)
- [x] `PixelProvider`, `ExactZoom`, `FuzzyZoom`, `PixelDistanceProvider` (8×8 cell spatial index for border distance)
- [x] Climate core: `ClimateType` tree + flag bits (IS_OCEAN/IS_SHORE/IS_ISLAND), `ClimateGraph` (512×512 hierarchical weighted-voronoi), `DistanceTransform` (Dijkstra), `ClimateNoise` (jittered-grid quantized sampling), `ClimateSearch`, `DirectGrid`, `IntMap`/`DoubleMap`
- [x] `MaskProvider`, `ClimateMaskProvider`, `UniqueClimateGenerator` (ported with Subtask 5 + the 7-climate loaders)
- [x] `Zone`, `ZonePatternProvider`/`Generator`, `ZoneColorMapping`, unique-zone candidate scan/painting (zone.rs + loader chain; cross-checked in 7d)
- [x] `Biome`, `TileBiome`, `CustomBiome(Generator)`, `BiomePatternGenerator` (weighted voronoi + `sizeModifier` warp), `BiomeInterpolation`
- [x] serde loaders: `World.json`, mask/climate/biome/zone JSON (Subtask 7 complete, golden-verified via `loader.json` + `climate.json` + `registries.json`)
- [ ] Verification: PNG mask/zone/biome renders pixel-diffed vs Java (superseded by bit-exact JSON golden dumps for the numeric chain; PNG render remains a `worldgen-cli` nicety)

### Phase 3 — Chunk core: terrain (~2.6k lines)
- [x] `BlockPriorityChunk`, priority enum + flags, `setBlock/setFluid` arbitration
- [x] `ChunkGenerator` skeleton: bounds clamping (`getLargestFeatureChunkExtents`), worker pool (own chunk model: private cache twins, real pool lands in Phase 6), caches, `getZoneBiomeResultAt`, `generateHeight`
- [x] `HeightThresholdInterpolator` + `CoreDataCacheEntry` + `InterpolatedBiomeCountList`
- [x] `BlockPopulator` + `LayerPopulator` (static/dynamic layers), covers, `WaterPopulator`, tint/environment mapping
- [x] Block/fluid/environment registry subset extracted from what the Default world JSON references (golden `registries.json` from the Java dumper)
- [x] Verification: full-chunk block dumps + heightmap PNGs vs Java — the Java `ChunkDumper`/`chunks.json` landed; all 21 chunks / 21,504 columns are bit-exact (terrain, layers, covers, water, caves, cave covers, prefabs)

### Phase 4 — Caves (~4.6k lines incl. loaders)
- [x] `CaveGenerator` recursive node-graph growth (yaw/pitch walk, `CaveBiomeMaskFlags` GENERATE/POPULATE/CONTINUE, anchors, radius inheritance), `Cave` + chunk-bucket compile (4a + 4b)
- [x] Shapes: pipe, cylinder, ellipsoid, empty-line, prefab-as-node (structure), then the **distorted family** (extrusion/body math, interpolation curves, pitch compensation, width/floor/ceiling distortion noise) (4c + 4d)
- [x] `CaveNodeShapeUtils` (anchors, covers, support invalidation), fillings/fluids, `CaveBlockPriorityModifier` — the block-support data gap is CLOSED: the golden `registries.json` now carries one `containsKey(DOWN/UP)` entry per Java `RotationTuple` per block (`RegistryDumper.blockSupportField`), and `BlockTypeInfo::get_support` honors the rotation index
- [x] Cave JSON loaders with cross-zone node references (`CaveNodeTypeStorage`) — 4e done; all 28 Default zones load their `Cave/Caves.json`; entry-point seed chains bit-exact vs `loader.json`
- [x] Verification: chunk slice PNGs + block-histogram invariants vs Java — the cave **node graph** (types, seeds, bounds, ends, prefabs), per-node floor/ceiling/replace geometry and every carve are bit-exact on the golden chunk grid (the last cave-cover seed edge was the quoted cave-type seed suffix, see PORTING_HISTORY.md)

### Phase 5 — Prefabs (~2.5k lines + format work)
- [x] Parse `.prefab.json` (12k files); investigate `.lpf` cache format only if needed for speed (JSON is source of truth)
- [x] `PrefabPasteUtil` + reusable paste buffer: block-mask gating, per-column heightmap fitting, environments, recursive child composition (depth ≤ 10) with seed composition
- [x] `PrefabPopulator`: sparse 8-block biome sampling, lattice collection, weighted picks, rotation, deep-search heights, `PrefabBaseCheck` evenness, AABB invalidation by category priority
- [x] Unique prefabs: `UniquePrefabConfiguration`, exclusion radii, zone-border exclusion, inverse domain-warp placement, spawn points (`getSpawnPoints`), `ValidationUtil`, `UniquePrefabContainer` port
- [x] Entities: stubbed (deferred per scope)
- [x] Verification: prefab placement coordinate lists + structure maps vs Java — the Java `ChunkDumper` landed (`testdata/golden/chunks.json`, deterministic, incl. the seed-0 unique-prefab chunks (4,-5)/(9,4)) and `tests/chunks_golden.rs` is bit-exact on every `PREFAB`/`PREFAB_CAVE`/`CAVE_PREFAB` voxel

### Phase 6 — Integration & polish
- [x] Final `trait WorldGen` (blocking `generate(seed, chunk coords, priority, cancel)`), dedicated worker pool (`WorldGenService`, priority queue, per-worker generator + scratch resource, shared registry snapshot), graceful shutdown, per-stage timing metrics (`StageTimings` / `WorldGenStats`)
- [x] Server chunk-provider adapter (`worldgen-server`: `ChunkProvider` / `WorldGenChunkProvider`, `stillNeeded`, float priority, cancellation token)
- [x] `worldgen-cli` commands: `generate`, `render-map`, `dump-chunk`, `validate` (recursive prefab load), `bench` (timing harness; criterion ports of the two JMH benchmarks in `crates/worldgen-hytale/benches/`)
- [x] `HytaleWorldGenProvider` config semantics (Name/Version/Path, legacy `World.json` rule, traversal guard) for world-config compatibility
- [x] Architecture doc: Java class → Rust module mapping (`ARCHITECTURE.md`)

### Post-port backlog (outside the parity claim)
Ordered list lives in [`NEXT_STEPS.md`](NEXT_STEPS.md):
1. Verify the dumped registry ids against a real vanilla-server load.
2. Visual-regression PNGs; release-mode benchmarks.
3. Deferred by design (see `REMAINING.md`): modifier/event plugins, entities/components/ECS, world-map UI, `.lpf`.

Closed after the port: the golden-regeneration determinism gate
(`tools/java-dumper/check.sh`), the procedural/supplier seams and the
duplicate per-worker registry load, and the server chunk-provider adapter
(`crates/worldgen-server`).

## Test strategy (cross-cutting)

- **Golden parity harness** (Phase 1 onward): Java side dumps JSON/bytes for fixed inputs; Rust asserts equality — the backbone of bit-exact verification.
- **Visual regression**: PNG renders (top-down blocks, heightmap, cave slices, zone/biome maps).
- **Property tests**: priority arbitration monotonicity, prefab invalidation, cache correctness under concurrency.
- **Benchmarks**: criterion ports of the two JMH benchmarks (`PrefabWeightsBenchmark`, `CacheBenchmark`).

## Key risks

1. **Bit-exactness of noise/math** — mitigated by porting Phase 1 first with the dumper harness before any world logic exists.
2. **`.lpf` prefab cache format unknowns** — fallback: `.prefab.json` directly.
3. **Loader implicit defaults** — the 77 Java loader classes contain validation/defaulting logic that must be transcribed, not just field names; parity tests catch omissions.
4. **Flexible gson JSON** (untyped maps, weighted-map shorthands like `"!Rock_Bedrock"` / `"*"` in masks) needs careful serde custom deserializers.

## Effort distribution

| Phase | Java LOC basis | Relative effort |
|---|---|---|
| 0 Scaffolding | — | small |
| 1 Math + noise | ~2.5k | medium (exactness is the challenge) |
| 2 Zones/biomes/climate | ~2.9k | medium |
| 3 Chunk core | ~2.6k | medium |
| 4 Caves | ~4.6k | **large** |
| 5 Prefabs | ~2.5k | **large** (format + recursion) |
| 6 Integration | — | small-medium |

Expected Rust size: ~12–18k lines (serde replacing loader boilerplate); Phases 4–5 hold most of the complexity.
