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
writes all four JSON files into this directory. Output is deterministic: running the
script twice produces byte-identical files.

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
