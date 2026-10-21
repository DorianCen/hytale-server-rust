# Architecture — Hytale WorldGenerator → Rust

This document maps the original Java world generator (`.hytale-source/hytale-shared-source/HytaleServer/{Math,ProceduralLibrary,WorldGenerator,CoreServer}`) onto the Rust workspace in `crates/`.

- **Goal:** bit-exact generation — same seed + same world assets + same registry dump ⇒ identical chunk bytes.
- **Status:** Phases 0–5 complete and golden-verified; Phase 6 (integration) implemented — `WorldGen` trait, worker-pool service, provider config, stage timings, CLI and criterion benches. The post-port backlog (real-server registry id check, golden-regeneration gate, remaining structural seams, server adapter) is tracked in [`NEXT_STEPS.md`](NEXT_STEPS.md#what-comes-next-post-port-backlog-priority-order).
- **Companion docs:** [`plan.md`](plan.md) (phase plan), [`PORTING.md`](PORTING.md) (porting rules), [`NEXT_STEPS.md`](NEXT_STEPS.md) (history and remaining work), [`REMAINING.md`](REMAINING.md) (deferred scope), [`testdata/golden/README.md`](testdata/golden/README.md) (golden formats).

Paths in the tables are relative to the repository root. A row maps one Java class, or a tightly coupled class family where the Rust port merges them; grouped rows list **every** Rust file they cover.

---

## 1. Workspace layout

```
hytale-server-rust/
├── Cargo.toml                     # workspace: resolver 2, shared deps
├── crates/
│   ├── hytale-math/               # Java `Math` subset used by generation
│   │   ├── src/{fast_random,hash_util,math_util,chunk_util,
│   │   │        trig_math_util,number_util,range}.rs
│   │   └── tests/golden.rs
│   ├── procedural/                # Java `ProceduralLibrary`
│   │   ├── src/noise/             # noise functions, cell noise, properties,
│   │   │                          # point generators, conditions, suppliers
│   │   ├── src/json/              # serde loaders for the same DSL
│   │   └── tests/{golden,loader_golden}.rs
│   ├── worldgen-model/            # CoreServer boundary / chunk data model
│   │   └── src/{chunk,block_priority,registry,block_key}.rs
│   ├── worldgen-hytale/           # the port of `WorldGenerator`
│   │   ├── src/{world_gen,world_gen_provider,world_gen_config}.rs
│   │   ├── src/{zoom,climate,zone,biome,container,cache,chunk,
│   │   │        cave,prefab,loader,util,weighted_map}/
│   │   ├── benches/{cache,prefab_weights}.rs
│   │   └── tests/                 # golden, loader, chunk and service tests
│   └── worldgen-cli/              # headless harness (`crates/worldgen-cli/src/main.rs`)
├── testdata/
│   ├── Default/                   # Default world JSON assets
│   └── golden/                    # Java-produced golden dumps
├── tools/java-dumper/             # Java golden-data producer (`run.sh`)
└── Assets/extracted/              # optional unpacked asset pack (prefabs)
```

Brace groups in the tree are shorthand for sibling files (for example the `src/` group lists the modules `fast_random` and `range` as two files); the mapping tables below use literal paths only.

| Crate | Depends on | Role |
|---|---|---|
| `hytale-math` | — | `FastRandom`, hashing, index math, exact float/int helpers |
| `procedural` | `hytale-math` | Noise DSL, cell noise, point generators, conditions |
| `worldgen-model` | `hytale-math` | Chunk buffer, priority grid, golden registries, block keys |
| `worldgen-hytale` | all three | World loading, zones/biomes/climate, caves, prefabs, populators, service |
| `worldgen-cli` | `procedural`, `worldgen-hytale`, `worldgen-model` | `generate`, `dump-chunk`, `render-map`, `validate`, `bench` |

Layering is strictly one-way: `worldgen-hytale` never leaks into `worldgen-model`, and `procedural` has no dependency on the world generator.

---

## 2. Java → Rust mapping

### 2.1 Math (`crates/hytale-math`)

Java root: `Math/src/main/java/com/hypixel/hytale/math/**`.

| Java class | Rust file | Responsibility |
|---|---|---|
| `math/util/FastRandom.java` | `crates/hytale-math/src/fast_random.rs` | Java LCG and inherited `Random.next(bits)` streams; the generator-wide RNG. |
| `math/util/HashUtil.java` | `crates/hytale-math/src/hash_util.rs` | Static coordinate hashing, `rehash` chains, `randomInt`/`random` helpers. |
| `math/util/MathUtil.java` | `crates/hytale-math/src/math_util.rs` | Bit-faithful floor/ceil/round/clamp/lerp/pack/percent helpers. |
| `math/util/ChunkUtil.java` | `crates/hytale-math/src/chunk_util.rs` | Chunk/column/block index arithmetic and `SIZE`/`HEIGHT`/`MIN_Y` constants. |
| `math/util/TrigMathUtil.java` | `crates/hytale-math/src/trig_math_util.rs` | Riven lookup-table sin/cos and cardinal-angle overrides. |
| `math/util/NumberUtil.java` | `crates/hytale-math/src/number_util.rs` | `short` arithmetic with Java's silent overflow. |
| `math/Range.java`, `math/range/FloatRange.java`, `math/range/IntRange.java` | `crates/hytale-math/src/range.rs` | Range value types used by loaders and cave configuration. |

### 2.2 ProceduralLibrary (`crates/procedural`)

Java root: `ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/**`.

#### Noise core

| Java class | Rust file | Responsibility |
|---|---|---|
| `NoiseFunction.java`, `NoiseFunction2d.java`, `NoiseFunction3d.java`, `NoiseFunctionPair.java`, `NoiseType.java` | `crates/procedural/src/noise/function.rs` | Noise traits, type enum and per-type dispatch. |
| `logic/SimplexNoise.java` | `crates/procedural/src/noise/simplex_noise.rs` | 2D/3D simplex gradient noise with Java seed hashing. |
| `logic/OldSimplexNoise.java` | `crates/procedural/src/noise/old_simplex_noise.rs` | Legacy simplex variant kept for old world configs. |
| `logic/PerlinNoise.java` | `crates/procedural/src/noise/perlin_noise.rs` | Perlin gradient noise. |
| `logic/ValueNoise.java` | `crates/procedural/src/noise/value_noise.rs` | Lattice value noise with the selected interpolation. |
| `logic/GeneralNoise.java` | `crates/procedural/src/noise/general_noise.rs` | Shared hashing, gradient tables, interpolation and output limiting. |
| `logic/ConstantNoise.java` | `crates/procedural/src/noise/constant_noise.rs` | Constant-value noise source. |
| `logic/GridNoise.java` | `crates/procedural/src/noise/grid_noise.rs` | Axis-aligned grid-plane noise. |
| `logic/MeshNoise.java` | `crates/procedural/src/noise/mesh_noise.rs` | Square/honeycomb mesh distance noise. |
| `logic/HexMeshNoise.java` | `crates/procedural/src/noise/hex_mesh_noise.rs` | Hexagonal mesh variant. |
| `logic/DistanceNoise.java` | `crates/procedural/src/noise/distance_noise.rs` | Distance-to-centre/edge noise. |
| `logic/BranchNoise.java` | `crates/procedural/src/noise/branch_noise.rs` | Branching corridor noise. |
| `logic/PointNoise.java` | `crates/procedural/src/noise/point_noise.rs` | Spherical falloff around a static point (inner/outer radius). |
| `logic/CellNoise.java` | `crates/procedural/src/noise/cell_noise.rs` | Voronoi/cell noise with evaluator and jitter hooks. |
| `logic/CellularNoise.java` | `crates/procedural/src/noise/cellular_noise.rs` | Shared 2D/3D cellular sampling helpers. |
| `logic/ResultBuffer.java` | `crates/procedural/src/noise/result_buffer.rs` | `ResultBuffer2d/3d` cell query results (fresh allocation instead of Java's static pool). |
| `logic/DoubleArray.java` | `crates/procedural/src/noise/double_array.rs` | Small fixed-size `double` tuple types. |

#### Cell noise

| Java class | Rust file | Responsibility |
|---|---|---|
| `logic/cell/CellType.java`, `MeasurementMode.java`, `DistanceCalculationMode.java` | `crates/procedural/src/noise/cell/cell_type.rs`, `crates/procedural/src/noise/cell/measurement_mode.rs`, `crates/procedural/src/noise/cell/distance_calculation_mode.rs` | Cell topology and distance-mode enums. |
| `logic/cell/CellDistanceFunction.java`, `PointDistanceFunction.java`, `BorderDistanceFunction.java`, `GridCellDistanceFunction.java`, `HexCellDistanceFunction.java` | `crates/procedural/src/noise/cell/cell_distance_function.rs`, `crates/procedural/src/noise/cell/point_distance_function.rs`, `crates/procedural/src/noise/cell/border_distance_function.rs`, `crates/procedural/src/noise/cell/grid_cell_distance_function.rs`, `crates/procedural/src/noise/cell/hex_cell_distance_function.rs` | Distance metric traits plus square-grid and hex-grid kernels. |
| `logic/cell/evaluator/*.java` (`PointEvaluator`, `Border`, `Density`, `Distance`, `Jitter`, `Normal`, `Branch`, `SkipCell`) | `crates/procedural/src/noise/cell/evaluator.rs` | Evaluator trait/dispatch; concrete implementations in the sibling `evaluator/` submodules. |
| `logic/cell/jitter/*.java` (`CellJitter`, `ConstantCellJitter`, `DefaultCellJitter`) | `crates/procedural/src/noise/cell/jitter.rs` | Jitter trait and concrete offset tables. |

#### NoiseProperty DSL

| Java class | Rust file | Responsibility |
|---|---|---|
| `property/NoiseProperty.java` | `crates/procedural/src/noise/support.rs` | Object-safe `NoiseProperty` trait and formula seam. |
| `property/NoisePropertyType.java` | `crates/procedural/src/noise/property/noise_property_type.rs` | Enum of property kinds used by the loaders. |
| `property/SingleNoiseProperty.java` | `crates/procedural/src/noise/property/single_noise_property.rs` | Single-source property. |
| `property/FractalNoiseProperty.java` | `crates/procedural/src/noise/property/fractal_noise_property.rs` | Fractal octave summation. |
| `property/ScaleNoiseProperty.java`, `OffsetNoiseProperty.java`, `RotateNoiseProperty.java`, `SumNoiseProperty.java`, `MinNoiseProperty.java`, `MaxNoiseProperty.java`, `MultiplyNoiseProperty.java`, `NoiseFormulaProperty.java`, `NormalizeNoiseProperty.java`, `InvertNoiseProperty.java` | `crates/procedural/src/noise/property/scale_noise_property.rs`, `crates/procedural/src/noise/property/offset_noise_property.rs`, `crates/procedural/src/noise/property/rotate_noise_property.rs`, `crates/procedural/src/noise/property/sum_noise_property.rs`, `crates/procedural/src/noise/property/min_noise_property.rs`, `crates/procedural/src/noise/property/max_noise_property.rs`, `crates/procedural/src/noise/property/multiply_noise_property.rs`, `crates/procedural/src/noise/property/noise_formula_property.rs`, `crates/procedural/src/noise/property/normalize_noise_property.rs`, `crates/procedural/src/noise/property/invert_noise_property.rs` | Arithmetic/algebra combinators and range normalization/inversion. |
| `property/BlendNoiseProperty.java`, `CurveNoiseProperty.java`, `GradientNoiseProperty.java`, `DistortedNoiseProperty.java` | `crates/procedural/src/noise/property/blend_noise_property.rs`, `crates/procedural/src/noise/property/curve_noise_property.rs`, `crates/procedural/src/noise/property/gradient_noise_property.rs`, `crates/procedural/src/noise/property/distorted_noise_property.rs` | Blend, curve/gradient remap and domain-warp combinators. |

#### Points, conditions, randomizers, suppliers

| Java class | Rust file | Responsibility |
|---|---|---|
| `logic/point/IPointGenerator.java`, `PointGenerator.java`, `OffsetPointGenerator.java`, `ScaledPointGenerator.java`, `DistortedPointGenerator.java` | `crates/procedural/src/noise/point/mod.rs` | Jittered-grid nearest/transition/collect point generation (3×3 cell search). |
| `condition/IIntCondition.java`, `IDoubleCondition.java`, `IDoubleThreshold.java`, `IHeightThresholdInterpreter.java` | `crates/procedural/src/noise/support.rs` | Shared condition/threshold traits (also used by the noise core). |
| `condition/ConstantIntCondition.java`, `DefaultDoubleCondition.java`, `SingleDoubleCondition.java`, `DoubleThreshold.java`, `HeightCondition.java`, `HeightThresholdCoordinateCondition.java`, `NoiseMaskCondition.java`, `ConstantBlockFluidCondition.java`, `BasicHeightThresholdInterpreter.java`, `NoiseHeightThresholdInterpreter.java` | `crates/procedural/src/noise/condition.rs` | Concrete value/coordinate/threshold/block conditions and both height-threshold interpreters. |
| `random/ICoordinateRandomizer.java`, `CoordinateRandomizer.java`, `RotatedCoordinateRandomizer.java`, `CoordinateRotator.java`, `CoordinateOriginRotator.java` | `crates/procedural/src/noise/random.rs` | Fuzzy domain-warp and coordinate rotation primitives. |
| `supplier/IDoubleRange.java`, `DoubleRange.java`, `DoubleRangeNoiseSupplier.java`, constant/double-range coordinate hash suppliers | `crates/procedural/src/noise/supplier.rs` | Range suppliers used by cave and container configs. |

#### JSON loaders (procedural)

| Java class(es) | Rust file(s) | Responsibility |
|---|---|---|
| `json/JsonLoader.java`, `JsonResourceLoader.java`, `JsonLoaderBase` surface | `crates/procedural/src/json/mod.rs` | Shared JSON cursor/error/seed helpers and loader trait. |
| `json/SeedString.java`, `SeedResource.java` | `crates/procedural/src/json/seed_string.rs` | Seed-label hashing (`hash("<label>")`) used for seed offsets. |
| `json/NoiseTypeJson.java`, `NoiseFunctionJsonLoader.java` | `crates/procedural/src/json/noise_type_json.rs`, `crates/procedural/src/json/noise_function_json_loader.rs` | Noise type/config decode. |
| `json/SimplexNoiseJsonLoader.java`, `OldSimplexNoiseJsonLoader.java`, `PerlinNoiseJsonLoader.java`, `ValueNoiseJsonLoader.java`, `ConstantNoiseJsonLoader.java`, `CellNoiseJsonLoader.java`, `PointNoiseJsonLoader.java`, `GridNoiseJsonLoader.java`, `DistanceNoiseJsonLoader.java`, `MeshNoiseJsonLoader.java`, `BranchNoiseJsonLoader.java` | `crates/procedural/src/json/simplex_noise_json_loader.rs`, `crates/procedural/src/json/old_simplex_noise_json_loader.rs`, `crates/procedural/src/json/perlin_noise_json_loader.rs`, `crates/procedural/src/json/value_noise_json_loader.rs`, `crates/procedural/src/json/constant_noise_json_loader.rs`, `crates/procedural/src/json/cell_noise_json_loader.rs`, `crates/procedural/src/json/point_noise_json_loader.rs`, `crates/procedural/src/json/grid_noise_json_loader.rs`, `crates/procedural/src/json/distance_noise_json_loader.rs`, `crates/procedural/src/json/mesh_noise_json_loader.rs`, `crates/procedural/src/json/branch_noise_json_loader.rs` | Basic and structured noise loaders. |
| `json/AbstractCellJitterJsonLoader.java`, `CellDistanceFunctionJsonLoader.java`, `CellBorderDistanceFunctionJsonLoader.java`, `PointEvaluatorJsonLoader.java` | `crates/procedural/src/json/abstract_cell_jitter_json_loader.rs`, `crates/procedural/src/json/cell_distance_function_json_loader.rs`, `crates/procedural/src/json/cell_border_distance_function_json_loader.rs`, `crates/procedural/src/json/point_evaluator_json_loader.rs` | Cell noise sub-configuration loaders. |
| `json/NoisePropertyJsonLoader.java`, `BlendNoisePropertyJsonLoader.java`, `CurveNoisePropertyJsonLoader.java`, `GradientNoisePropertyJsonLoader.java` | `crates/procedural/src/json/noise_property_json_loader.rs`, `crates/procedural/src/json/blend_noise_property_json_loader.rs`, `crates/procedural/src/json/curve_noise_property_json_loader.rs`, `crates/procedural/src/json/gradient_noise_property_json_loader.rs` | Property-dispatch and compound property loaders. |
| `json/DoubleConditionJsonLoader.java`, `DoubleRangeJsonLoader.java`, `DoubleThresholdJsonLoader.java`, `HeightThresholdInterpreterJsonLoader.java`, `BasicHeightThresholdInterpreterJsonLoader.java`, `NoiseHeightThresholdInterpreterJsonLoader.java`, `NoiseMaskConditionJsonLoader.java`, `FloatRangeJsonLoader.java` | `crates/procedural/src/json/double_condition_json_loader.rs`, `crates/procedural/src/json/double_range_json_loader.rs`, `crates/procedural/src/json/double_threshold_json_loader.rs`, `crates/procedural/src/json/height_threshold_interpreter_json_loader.rs`, `crates/procedural/src/json/basic_height_threshold_interpreter_json_loader.rs`, `crates/procedural/src/json/noise_height_threshold_interpreter_json_loader.rs`, `crates/procedural/src/json/noise_mask_condition_json_loader.rs`, `crates/procedural/src/json/float_range_json_loader.rs` | Condition, threshold and range loaders. |
| `json/PointGeneratorJsonLoader.java`, `CoordinateRandomizerJsonLoader.java`, `CoordinateRotatorJsonLoader.java` | `crates/procedural/src/json/point_generator_json_loader.rs`, `crates/procedural/src/json/coordinate_randomizer_json_loader.rs`, `crates/procedural/src/json/coordinate_rotator_json_loader.rs` | Point-generator and randomizer loaders. |

### 2.3 WorldGenerator (`crates/worldgen-hytale`)

Java root: `WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/**`.

#### Load path and integration boundary

| Java class | Rust file | Responsibility |
|---|---|---|
| `HytaleWorldGenProvider.java` | `crates/worldgen-hytale/src/world_gen_provider.rs` | `Name`/`Version`/`Path` defaults, legacy `World.json` rule, path-traversal guard, generator load. |
| `WorldGenConfig.java` | `crates/worldgen-hytale/src/world_gen_config.rs` | World-gen folder + name + version record produced by the provider. |
| `loader/ChunkGeneratorJsonLoader.java` | `crates/worldgen-hytale/src/loader/chunk_generator_json_loader.rs` | Top-level load: `World.json` → mask → zones → `ChunkGenerator`; asset/registry roots. |
| `loader/ZonesJsonLoader.java` | `crates/worldgen-hytale/src/loader/zones_json_loader.rs` | Reads `Zones.json` into the zone array. |
| `loader/context/FileContextLoader.java`, `FileContext.java`, `FileLoadingContext.java` | `crates/worldgen-hytale/src/loader/context/file_context_loader.rs`, `crates/worldgen-hytale/src/loader/context/file_context.rs`, `crates/worldgen-hytale/src/loader/context/file_loading_context.rs` | Zone-folder walk, file-context ids and shared loading state. |
| `loader/context/ZoneFileContext.java`, `BiomeFileContext.java`, `CaveFileContext.java` | `crates/worldgen-hytale/src/loader/context/zone_file_context.rs`, `crates/worldgen-hytale/src/loader/context/biome_file_context.rs`, `crates/worldgen-hytale/src/loader/context/cave_file_context.rs` | Per-zone/per-biome/per-cave file resolution. |
| `SeedStringResource.java` | `crates/worldgen-hytale/src/loader/seed_string_resource.rs` | Java-style seed-string resource and hashing. |
| `loader/WorldGenPrefabLoader.java`, `loader/WorldGenPrefabSupplier.java` | `crates/worldgen-hytale/src/loader/world_gen_prefab_loader.rs`, `crates/worldgen-hytale/src/prefab/prefab_supplier.rs` | Resolve prefab names to files/suppliers for the world-gen root. |
| `loader/MaskProviderJsonLoader.java` | `crates/worldgen-hytale/src/loader/mask_provider_json_loader.rs` | Chooses the climate (`Mask.json`) or image (`Mask.png`) mask path. |
| CoreServer `world/worldgen/IWorldGen.java`, `WorldGenTimingsCollector.java` + `ChunkGenerator.generate/executor` | `crates/worldgen-hytale/src/world_gen.rs` | `WorldGen` trait, `WorldGenService` worker pool, priority queue, cancellation, stats/timings, shutdown. |

#### Zoom / mask

| Java class | Rust file | Responsibility |
|---|---|---|
| `zoom/PixelProvider.java` | `crates/worldgen-hytale/src/zoom/pixel_provider.rs` | Raster mask storage and indexed pixel access. |
| `zoom/ExactZoom.java` (+ its private unique-zone statics) | `crates/worldgen-hytale/src/zoom/exact_zoom.rs`, `crates/worldgen-hytale/src/unique_zone_placement.rs` | Nearest-pixel mask sampling, unique-zone candidate scan and disc painting. |
| `zoom/FuzzyZoom.java` | `crates/worldgen-hytale/src/zoom/fuzzy_zoom.rs` | Bilinear mask sampling, warp integration, bounds and distance. |
| `zoom/PixelDistanceProvider.java` | `crates/worldgen-hytale/src/zoom/pixel_distance_provider.rs` | 8×8-cell spatial index for border distances. |

#### Climate

| Java class | Rust file | Responsibility |
|---|---|---|
| `climate/ClimateColor.java`, `ClimatePoint.java` | `crates/worldgen-hytale/src/climate/climate_color.rs`, `crates/worldgen-hytale/src/climate/climate_point.rs` | Climate colors and graph points. |
| `climate/ClimateType.java` | `crates/worldgen-hytale/src/climate/climate_type.rs` | Climate tree node + ocean/shore/island flag bits. |
| `climate/ClimateGraph.java` | `crates/worldgen-hytale/src/climate/climate_graph.rs` | 512×512 hierarchical weighted-voronoi table and fade mode. |
| `climate/ClimateNoise.java` | `crates/worldgen-hytale/src/climate/climate_noise.rs` | Jittered-grid quantized climate sampling (continent/temperature/intensity). |
| `climate/ClimateSearch.java` | `crates/worldgen-hytale/src/climate/climate_search.rs` | Ring-spiral search and rule scoring for unique climates. |
| `climate/DirectGrid.java`, `climate/util/IntMap.java`, `DoubleMap.java` | `crates/worldgen-hytale/src/climate/direct_grid.rs`, `crates/worldgen-hytale/src/climate/maps.rs` | Flat grid and primitive maps. |
| `climate/util/DistanceTransform.java` | `crates/worldgen-hytale/src/climate/distance_transform.rs` | Dijkstra distance transform for the climate fade. |
| `climate/ClimateMaskProvider.java`, `UniqueClimateGenerator.java` | `crates/worldgen-hytale/src/climate/climate_mask_provider.rs`, `crates/worldgen-hytale/src/climate/unique_climate_generator.rs` | Climate-path mask provider and unique-climate placement. |

#### Zones

| Java class | Rust file | Responsibility |
|---|---|---|
| `zone/Zone.java` (record + nested `UniqueEntry`/`UniqueCandidate`/`Unique`) | `crates/worldgen-hytale/src/zone/zone.rs`, `crates/worldgen-hytale/src/zone/unique.rs` | Zone record (id/name, biome pattern, cave generator, unique prefabs) and its unique-zone value types. |
| `zone/ZoneColorMapping.java` | `crates/worldgen-hytale/src/zone/zone_color_mapping.rs` | Mask RGB → zone resolution. |
| `zone/ZonePatternProvider.java` | `crates/worldgen-hytale/src/zone/zone_pattern_provider.rs` | World-level zone list, max extent and per-seed generator lookup. |
| `zone/ZonePatternGenerator.java` | `crates/worldgen-hytale/src/zone/zone_pattern_generator.rs` | Per-seed unique-zone painting and zone lookup (mask warp + borders). |
| `zone/ZonePatternGeneratorCache.java` | `crates/worldgen-hytale/src/zone/zone_pattern_generator_cache.rs` | Per-seed generator memoization. |
| `zone/ZoneDiscoveryConfig.java`, `ZoneGeneratorResult.java` (+ CoreServer `EventTitleUtil` constants) | `crates/worldgen-hytale/src/zone/discovery_config.rs`, `crates/worldgen-hytale/src/zone/zone_generator_result.rs` | Discovery titles and the scratch result object. |

#### Biomes

| Java class | Rust file | Responsibility |
|---|---|---|
| `biome/Biome.java` | `crates/worldgen-hytale/src/biome/biome.rs` | Object-safe biome trait (Java abstract class). |
| `biome/TileBiome.java` | `crates/worldgen-hytale/src/biome/tile_biome.rs` | Tile biome with tint/environment/layer/cover/water containers. |
| `biome/CustomBiome.java`, `CustomBiomeGenerator.java` | `crates/worldgen-hytale/src/biome/custom_biome.rs`, `crates/worldgen-hytale/src/biome/custom_biome_generator.rs` | Noise-overlay biome and its generator. |
| `biome/BiomePatternGenerator.java` | `crates/worldgen-hytale/src/biome/biome_pattern_generator.rs` | Jittered voronoi tile selection + custom-biome overlay. |
| `biome/BiomeInterpolation.java` | `crates/worldgen-hytale/src/biome/biome_interpolation.rs` | Biome radius/interpolation metadata. |

#### Containers

| Java class | Rust file | Responsibility |
|---|---|---|
| `container/FadeContainer.java` | `crates/worldgen-hytale/src/container/fade_container.rs` | Border-distance fade values. |
| `container/TintContainer.java` | `crates/worldgen-hytale/src/container/tint_container.rs` | Per-biome tint mapping and defaults. |
| `container/EnvironmentContainer.java` | `crates/worldgen-hytale/src/container/environment_container.rs` | Environment id/name mapping. |
| `container/WaterContainer.java` | `crates/worldgen-hytale/src/container/water_container.rs` | Water block + fluid level config. |
| `container/CoverContainer.java` | `crates/worldgen-hytale/src/container/cover_container.rs` | Weighted surface covers and cover parts. |
| `container/LayerContainer.java` | `crates/worldgen-hytale/src/container/layer_container.rs` | Static/dynamic terrain layers. |
| `container/PrefabContainer.java` | `crates/worldgen-hytale/src/container/prefab_container.rs` | Weighted prefab entries and pattern generators. |
| `container/UniquePrefabContainer.java` | `crates/worldgen-hytale/src/container/unique_prefab_container.rs` | Unique-prefab configuration and placement. |

#### Caches

| Java class | Rust file | Responsibility |
|---|---|---|
| `cache/CoreDataCacheEntry.java`, `cache/InterpolatedBiomeCountList.java` | `crates/worldgen-hytale/src/cache/core_data_cache_entry.rs`, `crates/worldgen-hytale/src/cache/interpolated_biome_count_list.rs` | Per-column core data (zone, biome, height, tint, environment) and the running biome counts. |
| `cache/CoordinateCache.java`, `ExtendedCoordinateCache.java` | `crates/worldgen-hytale/src/cache/coordinate_cache.rs`, `crates/worldgen-hytale/src/cache/extended_coordinate_cache.rs` | Coordinate-keyed cache with Java key packing. |
| `cache/ChunkGeneratorCache.java` | `crates/worldgen-hytale/src/cache/chunk_generator_cache.rs` | Sized/timeout core-data cache wrapper. |
| `cache/UniquePrefabCache.java`, `cache/CaveGeneratorCache.java` | `crates/worldgen-hytale/src/cache/unique_prefab_cache.rs`, `crates/worldgen-hytale/src/cave/cave_generator_cache.rs` | Unique-prefab and per-cave-type generator caches. |

#### Chunk core and populators

| Java class | Rust file | Responsibility |
|---|---|---|
| `chunk/ChunkGenerator.java` | `crates/worldgen-hytale/src/chunk/chunk_generator.rs` | Per-world facade: zone/biome/height queries, cave/unique-prefab/spawn APIs, caches. |
| `chunk/ChunkGeneratorExecution.java` | `crates/worldgen-hytale/src/chunk/chunk_generator_execution.rs` | Per-chunk context, priority arbiter, stage order, `StageTimings`, cancellation. |
| `ChunkGeneratorResource.java` | `crates/worldgen-hytale/src/chunk/chunk_generator_resource.rs` | Reusable scratch buffers (`&mut GenContext`). |
| `chunk/HeightThresholdInterpolator.java` | `crates/worldgen-hytale/src/chunk/height_threshold_interpolator.rs` | `(32+2r)²` threshold interpolation table. |
| `chunk/MaskProvider.java` | `crates/worldgen-hytale/src/chunk/mask_provider.rs` | Common mask interface over image/climate providers. |
| `chunk/ZoneBiomeResult.java` | `crates/worldgen-hytale/src/chunk/zone_biome_result.rs` | Column result: zone handle, biome, border distance, height. |
| `chunk/ValidationUtil.java` | `crates/worldgen-hytale/src/chunk/validation_util.rs` | Recursive prefab/cave-node validation for `worldgen-cli validate`. |
| `chunk/populator/BlockPopulator.java` | `crates/worldgen-hytale/src/chunk/populator/block_populator.rs` | Terrain fill, static/dynamic layers and covers. |
| `chunk/populator/CavePopulator.java` | `crates/worldgen-hytale/src/chunk/populator/cave_populator.rs` | Places compiled caves and cave covers into the chunk. |
| `chunk/populator/PrefabPopulator.java` | `crates/worldgen-hytale/src/chunk/populator/prefab_populator.rs` | Lattice prefab collection, height fitting, overlap invalidation. |
| `chunk/populator/WaterPopulator.java` | `crates/worldgen-hytale/src/chunk/populator/water_populator.rs` | Fills water up to the configured level. |

#### Caves

| Java class | Rust file | Responsibility |
|---|---|---|
| `cave/Cave.java` | `crates/worldgen-hytale/src/cave/cave.rs` | Compiled cave: nodes bucketed per chunk and sorted by priority. |
| `cave/CaveGenerator.java` | `crates/worldgen-hytale/src/cave/cave_generator.rs` | Recursive node-graph growth with biome masks and depth limits. |
| `cave/CaveType.java`, `CavePrefabPlacement.java` | `crates/worldgen-hytale/src/cave/cave_type.rs`, `crates/worldgen-hytale/src/cave/cave_prefab_placement.rs` | Cave type config (biome mask, fluids, yaw/pitch, entry nodes) and prefab placement modes. |
| `cave/CaveNodeType.java` | `crates/worldgen-hytale/src/cave/cave_node_type.rs` | Node type, children, covers and shape generator. |
| `cave/CaveBiomeMaskFlags.java`, `CaveYawMode.java` | `crates/worldgen-hytale/src/cave/cave_biome_mask_flags.rs`, `crates/worldgen-hytale/src/cave/cave_yaw_mode.rs` | Generate/populate/continue flags and yaw modes. |
| `cave/CaveBlockPriorityModifier.java` | `crates/worldgen-hytale/src/cave/cave_block_priority_modifier.rs` | Lets regular caves carve into prefab caves. |
| `cave/element/CaveElement.java`, `CaveNode.java`, `CavePrefab.java` | `crates/worldgen-hytale/src/cave/element/cave_element.rs`, `crates/worldgen-hytale/src/cave/element/cave_node.rs`, `crates/worldgen-hytale/src/cave/element/cave_prefab.rs` | Cave graph elements and their shape/prefab payloads. |
| `cave/prefab/CavePrefabContainer.java` | `crates/worldgen-hytale/src/cave/prefab/cave_prefab_container.rs` | Weighted cave-prefab container. |
| `cave/shape/CaveNodeShape.java`, `CaveNodeShapeEnum.java`, `AbstractCaveNodeShape.java` | `crates/worldgen-hytale/src/cave/shape/cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/cave_node_shape_enum.rs`, `crates/worldgen-hytale/src/cave/shape/abstract_cave_node_shape.rs` | Shape trait, enum and shared base. |
| `cave/shape/CaveNodeShapeUtils.java` | `crates/worldgen-hytale/src/cave/shape/cave_node_shape_utils.rs` | Anchors, covers and support invalidation. |
| `cave/shape/PipeCaveNodeShape.java`, `CylinderCaveNodeShape.java`, `EllipsoidCaveNodeShape.java`, `EmptyLineCaveNodeShape.java`, `PrefabCaveNodeShape.java` | `crates/worldgen-hytale/src/cave/shape/pipe_cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/cylinder_cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/ellipsoid_cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/empty_line_cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/prefab_cave_node_shape.rs` | Basic cave shapes. |
| `cave/shape/DistortedCaveNodeShape.java`, `ShapeDistortion.java` | `crates/worldgen-hytale/src/cave/shape/distorted_cave_node_shape.rs`, `crates/worldgen-hytale/src/cave/shape/shape_distortion.rs` | Distorted-shape dispatcher and distortion settings. |
| `cave/shape/distorted/*.java` (`AbstractDistortedShape`, `AbstractDistortedExtrusion`, `AbstractDistortedBody`, `DistortedShape`, `DistortedShapes`, `DistortedPipeShape`, `DistortedCylinderShape`, `DistortedEllipsoidShape`) | `crates/worldgen-hytale/src/cave/shape/distorted/abstract_distorted_shape.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/abstract_distorted_extrusion.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/abstract_distorted_body.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/distorted_shape.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/distorted_shapes.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/distorted_pipe_shape.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/distorted_cylinder_shape.rs`, `crates/worldgen-hytale/src/cave/shape/distorted/distorted_ellipsoid_shape.rs` | Extrusion/body math, curves, floor/ceiling distortion. |

#### Prefabs

| Java class | Rust file | Responsibility |
|---|---|---|
| `prefab/PrefabCategory.java` | `crates/worldgen-hytale/src/prefab/prefab_category.rs` | Prefab overlap categories and priorities. |
| `prefab/PrefabBaseCheck.java` | `crates/worldgen-hytale/src/prefab/prefab_base_check.rs` | Evenness/base-fit checks before pasting. |
| `prefab/PrefabPatternGenerator.java` | `crates/worldgen-hytale/src/prefab/prefab_pattern_generator.rs` | Placement grid, rotations, displacement, height conditions. |
| `prefab/PrefabPasteUtil.java` | `crates/worldgen-hytale/src/prefab/prefab_paste_util.rs` | Block-mask gating, per-column height fit, recursive child composition. |
| `prefab/PrefabStoreRoot.java` + CoreServer `prefab/PrefabStore.java`, `selection/buffer/PrefabLoader.java`, WorldGenerator `prefab/PrefabLoadingCache.java` | `crates/worldgen-hytale/src/prefab/prefab_store_root.rs`, `crates/worldgen-hytale/src/prefab/prefab_store.rs` | `ASSETS`/`WORLD_GEN` selection and the name → file → parsed buffer store. |
| `prefab/unique/UniquePrefabConfiguration.java` | `crates/worldgen-hytale/src/prefab/unique/unique_prefab_configuration.rs` | Exclusion radii, zone-border rules and placement config. |
| `prefab/unique/UniquePrefabGenerator.java` | `crates/worldgen-hytale/src/prefab/unique/unique_prefab_generator.rs` | Inverse domain-warp unique-prefab placement. |
| CoreServer `prefab/PrefabRotation.java` | `crates/worldgen-hytale/src/prefab/prefab_rotation.rs` | Rotation enum and rotated-bounds helpers. |
| CoreServer `prefab/PrefabWeights.java` | `crates/worldgen-hytale/src/prefab/prefab_weights.rs` | Weighted prefab selection. |
| CoreServer `selection/buffer/PrefabBuffer.java`, `PrefabBufferBlockEntry.java`, `PrefabBufferColumn.java`, `PrefabBufferCall.java`, fastutil `Int2ObjectOpenHashMap` | `crates/worldgen-hytale/src/prefab/buffer/prefab_buffer.rs`, `crates/worldgen-hytale/src/prefab/buffer/prefab_buffer_block_entry.rs`, `crates/worldgen-hytale/src/prefab/buffer/prefab_buffer_column.rs`, `crates/worldgen-hytale/src/prefab/buffer/prefab_buffer_call.rs`, `crates/worldgen-hytale/src/prefab/buffer/int_object_open_hash_map.rs` | Reusable paste buffer and its accessors. |
| CoreServer `selection/mask/BlockMask.java`, `MultiBlockMask.java`, `BlockFilter.java` | `crates/worldgen-hytale/src/prefab/mask/block_mask.rs`, `crates/worldgen-hytale/src/prefab/mask/multi_block_mask.rs`, `crates/worldgen-hytale/src/prefab/mask/block_filter.rs` | Placement masks/filters. |
| CoreServer `selection/buffer/BsonPrefabBufferDeserializer.java` | `crates/worldgen-hytale/src/prefab/prefab_json.rs` | `.prefab.json` parser (blocks, fluids, anchors; entities/components skipped). |

#### Util and weighted maps

| Java class | Rust file | Responsibility |
|---|---|---|
| `util/ArrayUtli.java` | `crates/worldgen-hytale/src/util/array_utli.rs` | Java-compatible array shuffle/fill helpers. |
| `util/BlockArray.java` | `crates/worldgen-hytale/src/util/block_array.rs` | Sparse/dense block arrays with default entries. |
| `util/bounds/*.java` (`IChunkBounds`, `ChunkBounds`, `IWorldBounds`, `WorldBounds`) | `crates/worldgen-hytale/src/util/bounds.rs` | Chunk/world AABBs and random-offset helpers. |
| `util/BlockFluidEntry.java` | `crates/worldgen-hytale/src/util/block_fluid_entry.rs` | Block + fluid pair used by populators. |
| `util/NoiseBlockArray.java`, `ResolvedBlockArray.java` | `crates/worldgen-hytale/src/util/noise_block_array.rs`, `crates/worldgen-hytale/src/util/resolved_block_array.rs` | Noise-driven and register-resolved block arrays. |
| `util/ListPool.java`, `ObjectPool.java` | `crates/worldgen-hytale/src/util/list_pool.rs`, `crates/worldgen-hytale/src/util/object_pool.rs` | Scratch pools (Drop-guard = Java `close()`). |
| `util/ConstantNoiseProperty.java`, `LogUtil.java` | `crates/worldgen-hytale/src/util/constant_noise_property.rs`, `crates/worldgen-hytale/src/util/log_util.rs` | Constant property defaults and the `WorldGenerator` log target. |
| `util/function/*.java` (`ICoordinateDoubleSupplier`, constant/random suppliers) | `crates/worldgen-hytale/src/util/function.rs` | Coordinate double suppliers. |
| `util/cache/*.java` (`Cache`, `TimeoutCache`, `SizedTimeoutCache`, `ConcurrentSizedTimeoutCache`) | `crates/worldgen-hytale/src/util/cache.rs` | Sized + time-to-idle cache contract on `moka`. |
| `util/condition/*.java`, `util/condition/flag/*.java` | `crates/worldgen-hytale/src/util/condition/mod.rs`, `crates/worldgen-hytale/src/util/condition/flag.rs` | Block/fluid/int conditions, masks and 2-flag logic. |
| `Math/vector/Rotation3f.java`, `Math/vector/Transform.java` | `crates/worldgen-hytale/src/util/transform.rs` | Rotation/position transform used by prefabs and spawn points. |
| `Common/map/IWeightedElement.java`, `IWeightedMap.java`, `WeightedMap.java`, `DefaultMap.java` | `crates/worldgen-hytale/src/weighted_map/mod.rs`, `crates/worldgen-hytale/src/weighted_map/default_map.rs` | Weighted map, builder, singleton semantics, seeded selection and default map. |

#### World JSON loaders

| Java class(es) | Rust file(s) | Responsibility |
|---|---|---|
| `loader/biome/*.java` (`BiomeJsonLoader`, `TileBiomeJsonLoader`, `CustomBiomeJsonLoader`, `CustomBiomeGeneratorJsonLoader`, `BiomeMaskJsonLoader`, `BiomeInterpolationJsonLoader`, `BiomePatternGeneratorJsonLoader`) | `crates/worldgen-hytale/src/loader/biome/biome_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/tile_biome_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/custom_biome_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/custom_biome_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/biome_mask_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/biome_interpolation_json_loader.rs`, `crates/worldgen-hytale/src/loader/biome/biome_pattern_generator_json_loader.rs` | Biome/tile/custom-biome and pattern loaders. |
| `loader/cave/CaveGeneratorJsonLoader.java`, `CaveTypeJsonLoader.java`, `CaveTypesJsonLoader.java`, `CaveNodeTypeJsonLoader.java`, `CaveNodeTypeStorage.java`, `CaveNodeChildEntryJsonLoader.java`, `CaveNodeCoverEntryJsonLoader.java`, `CaveBiomeMaskJsonLoader.java`, `FluidLevelJsonLoader.java` | `crates/worldgen-hytale/src/loader/cave/cave_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_type_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_types_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_node_type_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_node_type_storage.rs`, `crates/worldgen-hytale/src/loader/cave/cave_node_child_entry_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_node_cover_entry_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_biome_mask_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/fluid_level_json_loader.rs` | Cave graph/type/node configuration and cross-zone node storage. |
| `loader/cave/CavePrefabConfigJsonLoader.java`, `CavePrefabContainerJsonLoader.java`, `CavePrefabEntryJsonLoader.java` | `crates/worldgen-hytale/src/loader/cave/cave_prefab_config_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_prefab_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/cave_prefab_entry_json_loader.rs` | Cave-prefab configuration/container/entries. |
| `loader/cave/shape/*.java` (`Cylinder`, `Distorted`, `Ellipsoid`, `EmptyLine`, `Pipe`, `Prefab` shape generators + `ShapeDistortionJsonLoader`) | `crates/worldgen-hytale/src/loader/cave/shape/cylinder_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/distorted_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/ellipsoid_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/empty_line_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/pipe_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/prefab_cave_node_shape_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/cave/shape/shape_distortion_json_loader.rs` | Shape-generator dispatch and distortion settings. |
| `loader/climate/*.java` (`ClimateColor`, `ClimateGraph`, `ClimateGrid`, `ClimateMask`, `ClimateNoise`, `ClimatePoint`, `ClimateRule`, `ClimateType`, `ContinentThresholds`, `UniqueClimateGenerator`, `UniqueClimate`) | `crates/worldgen-hytale/src/loader/climate/climate_color_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_graph_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_grid_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_mask_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_noise_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_point_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_rule_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/climate_type_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/continent_thresholds_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/unique_climate_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/climate/unique_climate_json_loader.rs` | Climate tree/graph/noise/unique-climate loaders. |
| `loader/container/*.java` (fade, tint, environment, water, cover, layer, prefab, unique prefab) | `crates/worldgen-hytale/src/loader/container/fade_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/tint_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/environment_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/water_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/cover_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/layer_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/prefab_container_json_loader.rs`, `crates/worldgen-hytale/src/loader/container/unique_prefab_container_json_loader.rs` | Container loaders shared by biomes and zones. |
| `loader/zone/*.java` (`ZoneJsonLoader`, `ZoneBiomesJsonLoader`, `ZoneCustomBiomesJsonLoader`, `ZoneColorMappingJsonLoader`, `ZonePatternProviderJsonLoader`, `ZoneRequirementJsonLoader`, `UniqueZoneEntryJsonLoader`) | `crates/worldgen-hytale/src/loader/zone/zone_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/zone_biomes_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/zone_custom_biomes_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/zone_color_mapping_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/zone_pattern_provider_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/zone_requirement_json_loader.rs`, `crates/worldgen-hytale/src/loader/zone/unique_zone_entry_json_loader.rs` | Zone graphs, color maps and unique-zone entries. |
| `loader/prefab/BlockPlacementMaskJsonLoader.java`, `PrefabPatternGeneratorJsonLoader.java`, `WeightedPrefabMapJsonLoader.java`, `unique/UniquePrefabConfigurationJsonLoader.java` | `crates/worldgen-hytale/src/loader/prefab/block_placement_mask_json_loader.rs`, `crates/worldgen-hytale/src/loader/prefab/prefab_pattern_generator_json_loader.rs`, `crates/worldgen-hytale/src/loader/prefab/weighted_prefab_map_json_loader.rs`, `crates/worldgen-hytale/src/loader/prefab/unique/unique_prefab_configuration_json_loader.rs` | Prefab placement masks, weights, patterns and unique config. |
| `loader/util/NoiseBlockArrayJsonLoader.java`, `ResolvedBlockArrayJsonLoader.java`, `ResolvedVariantsBlockArrayLoader.java`, `Vector2dJsonLoader.java`, `Vector3dJsonLoader.java`, `ColorUtil.java`, `PathUtil` helpers | `crates/worldgen-hytale/src/loader/util/noise_block_array_json_loader.rs`, `crates/worldgen-hytale/src/loader/util/resolved_block_array_json_loader.rs`, `crates/worldgen-hytale/src/loader/util/resolved_variants_block_array_loader.rs`, `crates/worldgen-hytale/src/loader/util/vector2d_json_loader.rs`, `crates/worldgen-hytale/src/loader/util/vector3d_json_loader.rs`, `crates/worldgen-hytale/src/loader/color_util.rs`, `crates/worldgen-hytale/src/loader/util/path_util.rs`, `crates/worldgen-hytale/src/loader/util/float_range.rs` | Shared block-array, vector, color, path and float-range helpers. |

### 2.4 CoreServer boundary (`crates/worldgen-model`)

Java root: `CoreServer/src/main/java/com/hypixel/hytale/server/core/**` plus the priority grid from `WorldGenerator`.

| Java class | Rust file | Responsibility |
|---|---|---|
| `WorldGenerator/.../worldgen/chunk/BlockPriorityChunk.java` | `crates/worldgen-model/src/block_priority.rs` | Byte priority grid with `MASK`/`FLAG_MASK`/`FLAG_SUBMERGE` and `NO_CHANGE..PREFAB` ladder. |
| `WorldGenerator/.../worldgen/chunk/BlockPriorityModifier.java` | `crates/worldgen-model/src/block_priority.rs` | Pluggable write arbitration (identity `NoneModifier`, cave modifier in `worldgen-hytale`). |
| `world/universe/world/worldgen/GeneratedChunk.java`, `GeneratedBlockChunk.java`, `GeneratedChunkSection.java`, `GeneratedColumn.java`, `GeneratedEnvironment.java` | `crates/worldgen-model/src/chunk.rs` | Flat chunk buffer: blocks, rotation, filler, tint, environment column/per-block. |
| `world/chunk/section/FluidSection.java` | `crates/worldgen-model/src/chunk.rs` | Fluid id + level arrays flattened into the chunk. |
| `asset/type/blocktype/config/BlockType.java`, `Protocol/.../DrawType.java` | `crates/worldgen-model/src/registry.rs` | Block registry: name→id, draw type, opacity, per-rotation support map. |
| `asset/type/fluid/Fluid.java`, `FluidTicker.java` | `crates/worldgen-model/src/registry.rs` | Fluid registry: name→id, max level, solid predicate. |
| `asset/type/environment/Environment.java` | `crates/worldgen-model/src/registry.rs` | Environment registry: name→id. |
| CoreServer `prefab/selection/mask/BlockPattern.java` (`BlockEntry`), `BlockType/Rotation.java`, `RotationTuple.java`, `util/FillerBlockUtil.java` | `crates/worldgen-model/src/block_key.rs` | `Name\|Yaw=…\|Pitch=…\|Roll=…\|Filler=x,y,z` decoding and rotation/filler types. |

### 2.5 CLI and benches (Rust-only, no 1:1 Java class)

| Rust file | Responsibility |
|---|---|
| `crates/worldgen-cli/src/main.rs` | clap CLI: `generate`, `dump-chunk`, `render-map`, `validate`, `bench`; world resolution via the provider semantics. |
| `crates/worldgen-hytale/benches/cache.rs` | Criterion port of the Java `CacheBenchmark` (sized/TTI cache churn). |
| `crates/worldgen-hytale/benches/prefab_weights.rs` | Criterion port of the Java `PrefabWeightsBenchmark`. |

---

## 3. Data flow

### 3.1 World load (once per world, once per worker)

```
HytaleWorldGenProvider {Name, Version, Path}
        │ resolve_path() + legacy <folder>/World.json rule
        ▼
WorldGenConfig {path, name, version}
        │
        ▼
ChunkGeneratorJsonLoader::load()
  ├─ World.json ── Width/Height/Offset, Randomizer, Masks, PrefabStore
  ├─ Mask.json  → ClimateMaskProvider
  │                ClimateNoise (continent/temperature/intensity)
  │                  → ClimateGraph 512×512 weighted-voronoi
  │                  → DistanceTransform (fade)
  │   or Mask.png → MaskProvider → FuzzyZoom + PixelDistanceProvider
  ├─ Zones.json → ZonesJsonLoader → ZoneJsonLoader
  │                → Zone { biomePatternGenerator, caveGenerator,
  │                         tint/fade/env/water/cover/layer/prefab containers }
  └─ WorldGenPrefabLoader → PrefabStore (parsed .prefab.json buffers)
        ▼
ChunkGenerator {ZonePatternProvider, caches, cave/unique/spawn APIs}
```

The default world in this repository uses the **procedural climate** path; the image-mask path is ported and exercised by the loader tests, but the Default assets no longer ship `Mask.png`.

### 3.2 Per-seed setup

`ZonePatternProvider::get_generator(seed)` memoizes one `ZonePatternGenerator` per seed. The generator:

1. derives its seed offsets from the world seed through `SeedString`/`HashUtil`;
2. paints/resolves unique zones (Spawn, Temple, …) either into the mask copy or by `ClimateSearch` scoring;
3. answers `get_zone(x, z)` after applying the domain-warp randomizer and mask sampling.

### 3.3 Per-column core data (hot path, cached)

`ChunkGenerator::get_zone_biome_result_at(seed, x, z)`:

```
CoordinateRandomizer (fuzzy warp)
  → ZonePatternGenerator::generate(x, z)
      mask RGB → ZoneColorMapping → Zone + border distance
  → BiomePatternGenerator
      jittered voronoi cell hash → TileBiome
      optional CustomBiome noise overlay with border fade
  → HeightThresholdInterpolator / height noise
  → ZoneBiomeResult { zone, biome, border distance, height, tint, environment }
```

Results are memoized in `CoreDataCacheEntry` / `ChunkGeneratorCache` (and the biome-count list) keyed by seed + chunk/column coordinates. Caches are **pure memoization**: a cold or warm cache must produce the same result and must not change RNG consumption.

### 3.4 Per-chunk stages

`ChunkGeneratorExecution::execute_cancellable(seed, resource, cancel)`:

```
allocate GeneratedChunk + BlockPriorityChunk
build HeightThresholdInterpolator table: (32 + 2·MAX_RADIUS)²

1. tint mapping        ── environment-independent column tint
2. environment mapping ── per-column/per-block environment ids
3. BlockPopulator      ── FILLING(1) / LAYER(2) / COVER(3)
4. CavePopulator       ── CAVE(6) / CAVE_COVER(5) / CAVE_PREFAB(7)
5. PrefabPopulator     ── PREFAB(9) / PREFAB_CAVE(8)
6. WaterPopulator      ── WATER(4)

every write → BlockPriorityChunk arbitration (+ BlockPriorityModifier)
final buffer → GeneratedChunk
```

Stage durations are recorded in `StageTimings` (per execution) and aggregated by `WorldGenService` into `WorldGenStats`. Cancellation is checked **before each stage**; a cancelled request returns `WorldGenError::Cancelled` and its partial chunk is dropped.

### 3.5 Concurrency model

```
caller ──WorldGenRequest{seed,x,z,priority,cancel}──► WorldGenService
                                                        │  BinaryHeap<Job>
                                                        │  (priority desc, FIFO within)
                                                        ▼
                                     worker thread i: own ChunkGenerator
                                                      own ChunkGeneratorResource
                                                        │
                                                        ├─ generate_chunk()
                                                        └─ StageTimings → Stats
```

- Default worker count = `max(2, ceil(cores × 0.75))` (Java `ChunkGenerator.POOL_SIZE`).
- Each worker loads its own generator + registry once at startup; a load failure aborts the pool.
- `shutdown()` drains queued jobs, joins workers, and is idempotent; `Drop` calls it.
- The direct path (`generate_chunk`) shares the exact same execution code; tests use it, while the CLI goes through the service.

---

## 4. Determinism and bit-exactness contract

**Contract:** for a fixed world asset tree, fixed `"ChunkGenerator"` seed label, fixed seed and fixed golden registry dump, `generate_chunk` returns byte-identical `GeneratedChunk` data to the Java pipeline. Concurrency affects *scheduling*, never *content*.

Rules that must hold:

1. **Precision:** Java `double`→`f64`, `float`→`f32`; never round-trip through another precision.
2. **Integer semantics:** `i32`/`i64` arithmetic uses wrapping ops; `>>>` is `((x as u32) >> n) as i32`; Java `floorMod` uses `rem_euclid`, `%` stays truncating.
3. **RNG:** `FastRandom` and `HashUtil` are transcribed bit-for-bit; no `rand` crate in generation code. Any code path that consumes a random draw must consume exactly the same number/order as Java (e.g. singleton weighted maps must not draw).
4. **Ordering:** Java maps whose iteration order is observable are replaced by ordered structures or explicit sort keys; block writes go through the priority ladder, so insertion order cannot leak into the output.
5. **Caches:** keyed by seed/coordinates/type; mutable shared state never changes results, only speed. Cache timeouts and sizes are semantic only.
6. **Data:** registry ids are part of the contract for block dumps; the golden dump pins them (see §6).
7. **No hand-crafted expectations:** every expected value comes from `tools/java-dumper` or a transcribed Java unit test.

Verification matrix (must stay green):

| Test | Golden data | Coverage |
|---|---|---|
| `crates/hytale-math/tests/golden.rs` | `fastrandom.json`, `hashutil.json`, `mathutil.json` | RNG streams, hashes, math helpers |
| `crates/procedural/tests/golden.rs` | `noise.json` | All noise types (2D; 3D where supported), bit-exact |
| `crates/procedural/tests/loader_golden.rs` | `loader.json` (climate noise construction) | Seed chains and property trees |
| `crates/worldgen-hytale/tests/golden.rs` | `zoom.json`, `climate.json` | Mask warp, climate graph/fade |
| `crates/worldgen-hytale/tests/loader_climate_golden.rs` | `climate.json` | Climate chain over real `Mask.json` |
| `crates/worldgen-hytale/tests/loader_golden_dump.rs` | `loader.json` | Zones, biomes, containers, caves, seed offsets |
| `crates/worldgen-hytale/tests/loader_caves_golden.rs` | `loader.json` (`caves`) | Cave entry-point seed chains |
| `crates/worldgen-hytale/tests/loader_zones.rs`, `crates/worldgen-hytale/tests/loader_caves.rs`, `crates/worldgen-hytale/tests/loader_context.rs`, `crates/worldgen-hytale/tests/prefab_files.rs`, `crates/worldgen-hytale/tests/unique_prefabs.rs` | world assets | Focused loader/store checks |
| `crates/worldgen-hytale/tests/chunks_golden.rs` | `chunks.json` | **21 chunks / 21,504 columns bit-exact** (tint, zone, biome, environments, every block/fluid/priority voxel incl. caves/covers/prefabs) |
| `crates/worldgen-hytale/tests/world_gen_service.rs` | — | Worker-pool output == direct output; cancellation; shutdown |
| `crates/worldgen-hytale/tests/generation_smoke.rs` | — | End-to-end smoke generation |

Run everything with:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## 5. Documented deviations from Java

These are intentional and are repeated in module-level `//!` notes where relevant.

| Area | Java | Rust port | Why / impact |
|---|---|---|---|
| Shared caches | One generator shared by all chunk threads; `ThreadLocal<ChunkGeneratorResource>` | `Rc` object graph; one `ChunkGenerator` + `ChunkGeneratorResource` **per worker** | The generator graph is `Rc`/`!Send`; output is identical because caches are keyed and loading is deterministic. Cost: N world loads at startup. |
| Core-data cache | `ChunkGeneratorCache` shared across threads | Private cache twin inside `ChunkGenerator` | The ported `moka` cache closures are `Send + Sync`, incompatible with the `Rc` graph; same value semantics. |
| Chunk representation | `GeneratedChunk` → 10 lazy `GeneratedChunkSection`s with palettes | Flat `Vec` block/rotation/filler/tint/environment/fluid arrays laid out y-major, then z, then x | Same observable values, no ECS `Holder<ChunkStore>`/palette machinery. |
| ECS / entities | `GeneratedEntityChunk`, entity components, `BlockPhysics.setSupportValue`, `BiomeDataSystem` | Not ported; entity chunk and physics are no-ops | Generation-only scope (`plan.md`, `REMAINING.md`). Prefab entities/components are parsed and skipped. |
| Registries | Live CoreServer asset pipeline (`BlockType.getAssetMap()`, `Fluid.getAssetMap()`) | `Registries` loaded from `testdata/golden/registries.json` (`Registries::load/from_golden`) | The base-game pack is not in the source tree. Ids are pinned to the dumper's canonical single-threaded load order; diff against a real-server dump when available. |
| Serialization | gson + Hytale `Codec` framework | `serde_json` + hand-written validation/defaults | The 77 Java loader classes become plain Rust loaders; explicit defaults preserve Java behavior. |
| Modifier plugin | `WorldGenModifier`, events, content ops | Not ported | Deferred scope (`REMAINING.md`). |
| World map | `map/GeneratorChunkWorldMap.java` | Not ported; `render-map` samples `get_height`/`get_zone_biome_result_at` directly | Visualisation only, no generation impact. |
| Prefab cache | `.lpf` binary cache + block-id migrations | `.prefab.json` parsed directly; no migrations/components/entities | JSON is the source of truth; the vanilla pack ships no `.lpf`. |
| Threading API | `ThreadPoolExecutor` + `CompletableFuture` + `PriorityBlockingQueue` | OS threads + `BinaryHeap` + `mpsc` + `Condvar`; blocking `WorldGen::generate` | Simpler ownership; priority/FIFO and graceful drain preserved. Cancellation is cooperative between stages, not mid-shape. |
| Timings | `WorldGenTimingsCollector` averages after `WARMUP=100`, reports `-Infinity` before | `WorldGenStats` reports `0.0` before warm-up, then averages measured chunks | Reporting difference only; stage boundaries identical. |
| Cache utility | `TimeoutCache`, `SizedTimeoutCache`, `ConcurrentSizedTimeoutCache` with destroyers/computeKey | `moka` sized + time-to-idle wrappers with explicit `cleanup()` | Semantic contract preserved; explicit cleanup instead of background threads. |
| Provider path safety | CoreServer trusted-root registry (`PathUtil.isInTrustedRoot`) | Lexical `..` guard + `starts_with` containment | No trusted-root registry exists in the port; same rejection of traversal. |
| Procedural seams | Full `procedurallib.condition`/`supplier`/`property` object graph | Reduced traits in `procedural::noise::support`; full supplier port in `crates/procedural/src/noise/supplier.rs`; float-range/supplier seam in `worldgen-hytale::supplier` | Avoids a dependency cycle; behavior transcribed exactly, integration of the seams remains open. |
| Cave shapes | `TetrahedronCaveNodeShape` exists in Java | Not ported (not reachable from `CaveNodeShapeEnum`) | Dead code in the Java enum. |
| Loader mask registry | Global `BlockPlacementMaskRegistry` pools value-equal masks | Fresh value-equal masks; no global pool | Pooling is unobservable; less shared mutable state. |
| Prefab store | Searches every asset pack; caches expire | Single configured root; unbounded in-memory maps | World-generator scope; finite prefab set. |
| CLI | CoreServer commands/UI | `worldgen-cli` with clap; `bench` prints stage averages, criterion benches cover cache/weights | Headless harness; no server dependency. |

---

## 6. Regenerating golden data

All golden files are produced by the Java dumper; never edit them by hand.

```sh
tools/java-dumper/run.sh
```

What the script does:

1. Packages the already-compiled upstream `target/classes` trees (`Math`, `ProceduralLibrary`, `WorldGenerator`, plus the CoreServer modules needed by the registry dump) into `tools/java-dumper/lib/hytale-*.jar`. Missing/unresolved class trees are recompiled with Maven first.
2. Compiles the dumper project (`tools/java-dumper/pom.xml`).
3. Runs `com.hypixel.hytale.tools.golden.GoldenDumper` with `-Djava.util.concurrent.ForkJoinPool.common.parallelism=1` and writes `testdata/golden/*.json`.

Prerequisites:

- JDK 25 and Maven 3.10+; a populated `~/.m2` repository (the script resolves gson, joml, fastutil, sentry, bson, zstd, …).
- The read-only Java tree at `.hytale-source/hytale-shared-source/HytaleServer`.
- `Assets/Assets.zip` for the **registry** dump (not part of the source tree).
- `testdata/Default/` for the world dumps; the chunk dump boots the unpacked `Assets/extracted/Server/World/Default` tree so directory-walk order matches Rust.

Outputs and consumers:

| File | Producer | Consumed by |
|---|---|---|
| `fastrandom.json`, `hashutil.json`, `mathutil.json` | `GoldenDumper` | `crates/hytale-math/tests/golden.rs` |
| `noise.json` | `GoldenDumper` | `crates/procedural/tests/golden.rs` |
| `zoom.json`, `climate.json` | `Phase2Dumper` | `crates/worldgen-hytale/tests/golden.rs`, `crates/worldgen-hytale/tests/loader_climate_golden.rs` |
| `registries.json` | `RegistryDumper` | `Registries::load`, loader/chunk tests |
| `loader.json` | `LoaderDumper` | `crates/worldgen-hytale/tests/loader_golden_dump.rs`, `crates/worldgen-hytale/tests/loader_caves_golden.rs`, `crates/procedural/tests/loader_golden.rs` |
| `chunks.json` | `ChunkDumper` (fast path: `ChunkProbe`) | `crates/worldgen-hytale/tests/chunks_golden.rs` |

Determinism notes:

- The asset pipeline decodes in parallel and stores results in a `ConcurrentHashMap`; forcing one fork-join worker makes iteration/load order — and therefore block/fluid/environment ids — reproducible.
- All floats are written as Java hex-float strings (`Double.toHexString` / `Float.toHexString`); the Rust comparisons use exact bit parsing, not decimal.
- Re-running the script twice must produce byte-identical files. If ids or chunk dumps change, the parity tests will fail loudly.

After regenerating, run `cargo test --workspace` (and the clippy command above). The chunk test is the final gate: it compares all 21 golden chunks column-by-column, including every voxel's block, rotation, filler, fluid, environment and priority.

---

## 7. Navigation quick reference

| I want to… | Start here |
|---|---|
| See how a world is loaded | `crates/worldgen-hytale/src/loader/chunk_generator_json_loader.rs` |
| Change config/path semantics | `crates/worldgen-hytale/src/world_gen_provider.rs` |
| Inspect the per-chunk stage pipeline | `crates/worldgen-hytale/src/chunk/chunk_generator_execution.rs` |
| Debug zone/biome selection | `crates/worldgen-hytale/src/zone/zone_pattern_generator.rs`, `crates/worldgen-hytale/src/biome/biome_pattern_generator.rs` |
| Debug cave carving | `crates/worldgen-hytale/src/cave/cave_generator.rs`, `crates/worldgen-hytale/src/cave/shape/` |
| Debug prefab placement | `crates/worldgen-hytale/src/chunk/populator/prefab_populator.rs`, `crates/worldgen-hytale/src/prefab/prefab_paste_util.rs` |
| Understand write arbitration | `crates/worldgen-model/src/block_priority.rs` |
| See what blocks/fluids exist | `crates/worldgen-model/src/registry.rs`, `testdata/golden/registries.json` |
| Run a headless generation | `crates/worldgen-cli/src/main.rs` |
| Add a golden check | `testdata/golden/README.md`, then the matching suite under `crates/worldgen-hytale/tests/` |
