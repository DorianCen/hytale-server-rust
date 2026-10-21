# Golden reference data

These files are produced by the Java "golden dumper" in
[`tools/java-dumper/`](../../tools/java-dumper/). They capture the exact behaviour of
the original Hytale Java world-generation code so the Rust port can be checked
**bit-for-bit**.

Regenerate everything with:

```sh
tools/java-dumper/run.sh
```

The dumper compiles the already-compiled upstream classes (read-only, under
`.hytale-source/`) into `tools/java-dumper/lib/`, compiles itself with Maven, and
writes all nine JSON files into this directory. Output is deterministic: running the
script twice produces byte-identical files.

The world-side dumps (`zoom.json`, `climate.json`, `loader.json`, `chunks.json`)
read the Default world assets from `testdata/Default` (the dumper takes the world
directory as an optional second argument; it defaults to
`<output-directory>/../Default`). The chunk dump boots the world from the
**unpacked** asset tree (`Assets/extracted/Server/World/Default`, byte-identical
to `testdata/Default`) so the directory walk order matches the Rust port.

## Floating-point encoding

All `double`/`float` values are written as Java hex-float strings
(`Double.toHexString` / `Float.toHexString`), e.g. `"0x1.8p3"`. This preserves every bit
and avoids decimal rounding ambiguity. Integer results are plain JSON numbers.

Java hex-float strings are not parseable with Rust's standard `f64::from_str`; the Rust
verifier needs a small hex-float parser (sign, hex mantissa, binary exponent) or can be
switched to raw `doubleToLongBits` if that is preferred later. Special values appear as
the strings `"NaN"`, `"Infinity"` and `"-Infinity"`.

## Files

### `fastrandom.json`
`com.hypixel.hytale.math.util.FastRandom`. Keyed by seed (`0, 1, 2, 3, 4, 123456789`).
For each seed the same `FastRandom` instance is consumed in this order:

- `nextInt_256` – 256 values from `nextInt()`.
- `nextInt_bound100_128` – 128 values from `nextInt(100)`.
- `nextLong_128` – 128 values from the inherited `nextLong()`.
- `nextDouble_128` – 128 values from the inherited `nextDouble()` (hex).
- `nextFloat_128` – 128 values from the inherited `nextFloat()` (hex).

`nextInt(bound)`, `nextLong`, `nextDouble` and `nextFloat` are inherited from
`java.util.Random` and consume the FastRandom LCG through `next(bits)`.

### `hashutil.json`
`com.hypixel.hytale.math.util.HashUtil`. Covers every public method:

- `hash_1`..`hash_4` – `hash(long)`, `hash(long,long)`, `hash(long,long,long)`, `hash(long,long,long,long)`.
- `rehash_1`..`rehash_4` – matching `rehash` overloads.
- `random_1`..`random_4` – matching `random` overloads (hex values in `[0,1]`).
- `randomInt` – `randomInt(l1, l2, l3, bound)` over representative triples and bounds
  `{1, 2, 3, 7, 16, 100, 1000, Integer.MAX_VALUE}`.
- `hashUuid` – `hashUuid(UUID)` for fixed UUIDs (zero, max, v1/v3-style samples).

Inputs come from the integer grid `-1000..1000 step 137` plus boundary values
(`Long.MIN_VALUE`, `Long.MAX_VALUE`, `Integer.MIN_VALUE`, `Integer.MAX_VALUE`, etc.).
Higher-arity calls use cyclic combinations of that list.

### `mathutil.json`
`com.hypixel.hytale.math.util.MathUtil`. One section per method, each an array of
`{...inputs, out}` objects. Covered: `abs`, `floor`, `ceil`, `fastFloor`, `fastCeil`,
`fastRound`, `clamp` (double/float/int/long), `lerp`, `lerpUnclamped`, `seededLerp`,
`floorMod`, `compareAngle`, `shortAngleDistance`, `lerpAngle`, `wrapAngle`,
`halfFloatToFloat`, `halfFloatFromFloat`, `byteCount`, `packInt`, `packLong`,
`lengthSquared`/`length` (2D and 3D), `round`, `within`, `clipToZero`, `closeToZero`,
`getPercentageOf`, `percent`, `saturatedMultiply`, `distanceToLineSq`,
`distanceToInfLineSq`, `sideOfLine`, `mapToRange` and `percentile`.

**Skipped on purpose:** `randomInt`, `randomDouble`, `randomFloat` are nondeterministic
(`ThreadLocalRandom` / `Math.random`) and must not be part of golden data.
`rotateVectorYAxis`, `getRotationForHitNormal` and `getNameForHitNormal` depend on
mutable vector/transform types rather than pure math and are not dumped.

### `noise.json`
Noise functions from `com.hypixel.hytale.procedurallib.logic`. All `NoiseType` enum
values are covered in 2D; 3D is emitted for every type that implements it.

Structure:

```text
grid2d            { size:32, start:-0x1.0p3, step:0x1.0p-1, layout:"values[y][x]" }
grid3d            { size:8,  start:-0x1.f99999999999ap1, step:0x1.0p-1, layout:"values[z][y][x]" }
noise2d.<TYPE>    { class, config, seeds: { "1": [[hex,...] x32] x32, "42": ... } }
noise3d.<TYPE>    { class, config, seeds: { "1": [[[hex,...] x8] x8] x8, "42": ... } }
```

`NoiseFunction.get(seed, offsetSeed, x, y[, z])` is called with `offsetSeed = seed`
(most implementations hash `offsetSeed` and ignore `seed`, so passing the same value
makes the two seed sets genuinely different). 2D grid is `x,y in [-8,8)` step `0.5`;
3D grid is `8x8x8` starting at `-3.95` step `0.5` (an irrational-free but non-lattice
start, so hash/gradient noise is not sampled only at lattice points).

Since `NoiseType` maps to implementations through the JSON loaders, the dumper
instantiates the same classes directly with the loaders' default values where a
default exists. Deviations are recorded in each entry's `config`:

| Type | Implementation | Notes |
| --- | --- | --- |
| `SIMPLEX` | `SimplexNoise.INSTANCE` | singleton |
| `OLD_SIMPLEX` | `OldSimplexNoise.INSTANCE` | singleton |
| `VALUE` | `ValueNoise` | interpolation `QUINTIC` (loader default) |
| `PERLIN` | `PerlinNoise` | interpolation `QUINTIC` (loader default) |
| `CELL` | `CellNoise` | SQUARE grid, EUCLIDEAN, `CELL_VALUE`, jitter 1 |
| `DISTANCE` | `DistanceNoise` subclass | CENTRE_DISTANCE, EUCLIDEAN, `SUB` |
| `CONSTANT` | `ConstantNoise` | value `0.5` (loader default) |
| `GRID` | `GridNoise` | thickness `0.1`; **no loader default exists**, representative value |
| `MESH` | `MeshNoise` | thickness `0.1`; loader requires it, representative value. `NoiseType.MESH` maps to the square mesh (`CellType.SQUARE`); `HexMeshNoise` is not reachable from the enum |
| `BRANCH` | `BranchNoise` | defaults except `parentFade = 1.0` (loader default `0.0` would make the output constant `-1`); representative line thickness/scale `0.1` |
| `POINT` | `PointNoise` | representative `centre=(1.5,-2,0.5)`, `innerRadius=1`, `outerRadius=8`; loader defaults are all zero |

**Skipped in 3D:** `MESH` and `BRANCH`. Both throw
`UnsupportedOperationException` from their 3D `get` methods, so no 3D golden data
exists for them. `CONSTANT` and `GRID` 3D values are emitted but are near-degenerate
by construction (constant value / axis-aligned grid planes).

Sanity checks performed by the dumper: every emitted noise value is checked to be
finite (no `NaN`/`Infinity`); observed ranges are within roughly `[-1, 1]`.

### `zoom.json`
The world mask of the Default world (`com.hypixel.hytale.server.worldgen.zoom.FuzzyZoom`).

The Default world uses the **procedural climate mask**: `World.json` points at
`Mask.json`, and the legacy `Mask.png` image mask no longer exists. The dumper
therefore

- loads the real configuration through the real loaders
  (`ClimateNoiseJsonLoader`, `CoordinateRandomizerJsonLoader`) and
- rasterises the real `Noise.Continent` property into a 128×128 land/ocean mask
  (one pixel = 32 world blocks, `zoomX = zoomY = 32`, offsets `0/0`).

The rasterised pixels are dumped per seed (in `PixelProvider` array order,
`index = x * height + y`) so the Rust port can rebuild the exact same
`PixelProvider`; they are *input* data, not expected results.

Per seed (`0`, `1`, `42`, `1234567`) the dump contains, for 281 world coordinates
(`{-1000,-333,0,333,1000}²` plus a 16×16 grid around the origin, step 32):

- `getX` / `getY` — the fuzzy domain warp of the real `Mask.json` randomizer
  (`SIMPLEX`, scale `0.01`, amplitude `16.0`, seed label `RANDOMIZER`).
- `mask` — `FuzzyZoom.generate(rx, rz) & 0xFFFFFF` at the **warped** coordinates,
  exactly like `ZonePatternGenerator.generate` does.
- `inBounds` — `FuzzyZoom.inBounds(rx, rz)`.
- `distance` — the distance the generator uses (`Infinity` when out of bounds) and
  `distanceRaw` — the unconditional `FuzzyZoom.distance(rx, rz)`.
- `uniqueZoneCandidates` — two `ExactZoom.generateUniqueZoneCandidates` runs
  (`maxPositions = 64`): one scanning for the land colour (radius `4`, padding `2`)
  and one for the ocean colour (radius `2`, padding `0`), with the viable
  positions of each entry.

`construction.randomizerConstruction` additionally dumps the **resolved** randomizer
(the generator noise property with the integer seed offset the loader derived from
the seed string).

### `climate.json`
The procedural climate mask of the Default world
(`com.hypixel.hytale.server.worldgen.climate.ClimateGraph` / `ClimateNoise`).

- `construction.graph` — the climate type tree (names, colours, island colours,
  points with their modifier, children) plus the graph parameters
  (`resolution = 512`, `fadeMode = CHILDREN`, `fadeRadius = 50`, `fadeDistance = 100`).
- `graph.table` — the 512×512 climate-type id table (flat row-major array,
  `index = y * width + x`).
- `graph.fade` — the 512×512 raw distance-transform map (hex doubles).
- `noise.seeds.<seed>.construction` — the **resolved** `ClimateNoise`:
  the grid, the thresholds and the three noise properties (continent, temperature,
  intensity) as trees with the integer seed offsets the loaders derived.
- `noise.seeds.<seed>.{id,continent,temperature,intensity,fade}` — the
  `ClimateNoise.generate` results on a 16×16 grid over `[-2048, 2048)` blocks
  (step 256), flat row-major (`index = z * width + x`).

The noise-property trees mirror the Java objects 1:1
(`SINGLE`, `FRACTAL`, `NORMALIZE`, `SCALE`, `OFFSET`, `DISTORTED`, `BLEND`, `SUM`
and the `SIMPLEX` / `CONSTANT` / `POINT` / `GRID` noise functions) so the Rust port
can rebuild the identical graph — and therefore compare bit patterns.

### `registries.json`
The **real** block, fluid and environment registries of the base-game asset pack
(`Assets/Assets.zip`, which is not part of the source tree).

The block types are not standalone assets: they are the `"BlockType"` objects
*embedded* in the `Server/Item/Items/**` item assets, which `Item.CODEC` registers as
child assets of the BlockType store through
`ContainedAssetCodec(BlockType.class, BlockType.CODEC, Mode.INHERIT_ID_AND_PARENT)`
— the block id is the item key and the parent is inherited from the item's
`"Parent"`. The numeric ids are handed out by `BlockTypeAssetMap.putAll0` in
asset-load order, so they can only be obtained by running the real asset pipeline.

`RegistryDumper.java` therefore boots the vanilla CoreServer asset pipeline
(`AssetRegistryLoader`) over the pack (mounted as a zip `FileSystem`, exactly like
the vanilla server) and dumps:

- `meta` — the boot description, the load-order strategy, the id-order caveat and
  the known gaps.
- `blocks.count` / `blocks.assets[]` — `{ name, index, drawType, drawTypeId,
  opacity, unknown, support? }` per registered block type, plus `preLoadOrder`
  and the `emptyId` / `unknownId` / `debugCubeId` / `debugModelId` constants (the
  four hard-coded block types are ids 0…3). `opacity` is the Java
  `BlockType.getOpacity()` (the surface-limited cave height uses it); `support`
  is the per-`RotationTuple` `containsKey(DOWN/UP)` map (absent for the Java
  `null` support map).
- `fluids.count` / `fluids.assets[]` — `{ name, index, maxFluidLevel }` per fluid.
- `environments.count` / `environments.assets[]` — `{ name, index }` per environment
  (the world generator resolves the environment names through
  `Environment.getAssetMap().getIndex(name)`).

The dump is deterministic: it contains no timestamps, the asset files are collected
with the vanilla `Files.walkFileTree` (the zip entry order) and the dumper runs the
pipeline with a single worker thread of the common fork join pool, because the
vanilla pipeline collects the parallel decode results in a `ConcurrentHashMap` whose
iteration order would otherwise depend on the thread scheduling.

The ids are pinned to the load order this dumper observed; they are only identical
to a real server's ids when the real server loads the same pack in the same order.

### `loader.json`
The real Default-world loader chain (`ChunkGeneratorJsonLoader` →
`ZonePatternProviderJsonLoader` → `FileContextLoader` → `ZoneJsonLoader` →
`BiomePatternGeneratorJsonLoader` → biome/container/cave loaders) for the world
seeds `0 / 1 / 42 / 1234567`, including the file-context ids, the seed chains,
the biome containers and the cave entry-point seed offsets. The Rust
`tests/loader_golden_dump.rs` compares every value bit-exactly (the noise-derived
samples are now **0** deviations).

### `chunks.json`
Whole generated chunks of the Default world through the **real** Java pipeline
(`ChunkGeneratorExecution.execute`): seeds `0 / 1 / 42` × chunks
`(0,0), (4,-5), (9,4), (7,3), (-12,-25), (16,-8), (30,15)`. Per voxel the dump
records block id + rotation + filler, fluid id + level, environment id and the
`BlockPriorityChunk` priority byte, compactly run-length encoded per column
(`columnDefs`; `cols[i]` indexes them in `ChunkUtil.indexColumn` order =
`(z << 5) | x`). The column def also carries the tint, biome/zone ids, the raw
parent tile biome, the zone border distance, the biome-pattern index/cell and the
raw tint as **debug references**. The definitive Rust parity test is
`tests/chunks_golden.rs` (currently `#[ignore]`d: only 150 / 21,504 columns still
differ, all in cave-cover `CAVE` / `CAVE_COVER` voxels).
