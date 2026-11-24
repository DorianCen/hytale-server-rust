# Headless feature query

`ChunkGenerator::features_in` answers "what would the generator place in this
rectangle?" without generating a block or a chunk. It runs the world generator's
shared placement pass — the same pass `PrefabPopulator` runs per chunk — against
an arbitrary block rectangle and returns the features it would place there:
biome prefabs, per-seed unique prefabs, spawn points and cave entrances.

Cave entrances are also available directly:

```rust
let entrances = generator.cave_entrances_in(seed, rect);
```

`cave_entrances_in` runs the cave stage's placement predicate (the same
zone/threshold/height filter `CavePopulator` uses) and stops **before**
`get_cave`: no cave graph is compiled and no block is generated. A reported
position is the surface column `(x, height, z)` the predicate accepted; the
name is the cave type and the category is the owning zone.

```rust
use worldgen_hytale::query::{BlockRect, PlacementFilter, PlacementKindMask};

let rect = BlockRect { min_x: -256, min_z: -256, max_x: 255, max_z: 255 };
let filter = PlacementFilter { kinds: PlacementKindMask::all(), ..PlacementFilter::default() };
let placements = generator.features_in(0, rect, &filter);
for placement in placements { /* name, category, position, bounds, kind, … */ }
```

The types live in `worldgen_hytale::query`; the module docs
(`crates/worldgen-hytale/src/query/mod.rs`) are the reference for the semantics
below.

## Contract

- **Pure function of `(seed, rect, filter)`.** The same inputs always return
  the same placements in the same order. The query generates no block and
  mutates no chunk or execution; it reads and fills the generator's caches like
  any other generator query.
- **Deterministic order:** kind (`UniquePrefab`, then `BiomePrefab`, then
  `Spawn`), then x, then z, then y, then name.
- **`rect` is inclusive** on both axes and must be normalized (`min <= max`);
  an inverted rect panics (a caller bug).
- **The filter is applied after placement.** `kinds`, `categories`,
  `name_contains` and `min_size` only drop results; they can never change a
  random draw or the set of candidates the pass evaluates.
- **`name` and `category` are asset names** — the prefab's
  `WorldGenPrefabSupplier::get_prefab_name` and the `PrefabCategory` name, not
  translated display names. Consumers that need a label map them themselves.
- **The query is not O(1).** It scans a rectangle padded by the maximum prefab
  extent and the height-threshold interpolation radius, so cost grows with the
  rectangle's area (see [Cost](#cost)).

## Per-window invalidation, and why a small rect is a subset of a large one

Biome-prefab placement is evaluated per **chunk-aligned window**, exactly like
the populator: the candidates owning a window are invalidated against each
other (a higher-priority overlapping candidate wins), and a candidate is
reported placed when it survives its window's invalidation *and* its rotated
supplier bounds intersect that window (the populator's paste gate). Comparing
against the caller's rectangle is then only a filter on the result.

That is what makes rect independence structural rather than lucky: a smaller
rectangle's result is always a subset of a larger rectangle's result, because
both enumerate the same per-window buckets and the larger query adds only
windows and candidates the smaller one never asks about. The collection window
covers the whole block range of every enumerated window, so no window's bucket
is truncated at the rectangle's edge.

## Documented deviations from a real per-chunk run

1. **The rectangle's interior biomes are sparse-sampled.** A real chunk knows
   its core-data biomes exactly and scans around them. The query has no chunk,
   so it runs the pass's sparse scan over the whole padded rectangle at
   `BIOME_SAMPLE_STEP_SIZE` = 8 blocks. A biome smaller than the sample step can
   be missed inside the rectangle.
2. **Base-check heights are always computed.** The populator reads the base
   check height through its execution, which returns `NO_HEIGHT` (`-1`) for a
   column whose height has not been computed yet (a transcribed Java staging
   artifact). The query reads `ChunkGenerator::get_height` instead. The Default
   fixture declares no `BaseChecks` prefab patterns, so on fixture data the two
   agree; a pack with `BaseChecks` can differ in the same way the shared pass
   documents.

## Cave entrances

`ChunkGenerator::cave_entrances_in(seed, rect)` (also reachable through
`features_in` with `PlacementKind::CAVE_ENTRANCE`) reports the cave entry-point
candidates whose surface column is inside `rect`. It runs the cave stage's
shared predicate — the column resolves to the iterated zone by identity, the
cave type's entry threshold matches, and the height condition matches
`seed + 0xF5ACDEB5` — and returns `(x, height, z)` sorted by `(x, z, y, name)`.
The name is the cave type, the category the zone.

The collection is over the rect padded by the point generator's interval, not
by the cave type's `maximum_size`: the query reports entry points inside the
rect, and the populator's larger reach exists to find caves crossing into a
chunk, not anchors inside a query rect. The query never calls `get_cave`.

The Default world's cave point generators are **dense**: about 0.2 candidate
columns per block², so a 512-block rect yields ~53,000 entrances. Measured on
the Default fixture, seed 0, release build, one fresh query per rect:

| rect (blocks per side) | entrances | seconds |
|---|---:|---:|
| 32 (one chunk) | 219 | 0.008 |
| 128 | 3,311 | 0.08 |
| 256 | 13,257 | 5.5 |
| 512 | 52,941 | 78.6 |

A 10,000-block rect is therefore a batch job (≈6.6–8 hours and ≈20 M
placements at the measured density), not an overlay query. The explorer's
adapter caps the overlay's cave kind at 256 blocks per side; the other kinds
continue over the full rect. Larger cave views belong to T33's tiling/streaming
work.

## Cost

Cost is driven by the padded scan and by the per-lattice-point placement
predicates, both of which scale with the rectangle's area — not with the number
of returned placements. The collection window spans every biome the rectangle
touches, and every collected biome's prefab grid is enumerated across the whole
window; each lattice point that passes the noise-density filter evaluates the
column's zone biome (memoized for the lifetime of one query), the height, the
parent block and the base checks.

Measured on the Default fixture, seed 0, release build, one run per rect, rects
centred on the origin (the rects are nested, so later runs re-cover earlier
ground and start warmer). The probe's filter excludes cave entrances
(T27 measures those separately; the cave kind's density would otherwise
dominate the table). Command:

```sh
cargo run --release -p worldgen-cli --bin feature_query_bench
```

| rect (blocks per side) | seconds | placements (unique / biome / spawn) |
|---|---|---|
| 256 | 10.421 | 586 (0 / 586 / 0) |
| 512 | 20.134 | 1,311 (0 / 1,311 / 0) |
| 1024 | 73.330 | 6,015 (0 / 6,015 / 0) |
| 2048 | 326.308 | 35,378 (2 / 35,375 / 1) |
| 5000 | 1,599.468 | 262,608 (2 / 262,605 / 1) |
| 10,000 | ≈6,400 (extrapolated) | not measured |

The 256–5000 rows are measured, one fresh `features_in` call each. The 10,000
row is an **extrapolation**, not a measurement: the leg was started but stopped
after ≈57 minutes (its own peak RSS had reached ≈7.9 GB by then), and the
measured growth is linear in area from 512 blocks up, so 10,000 (4× the area of
5000) extrapolates to ≈4 × 1,599 s ≈ 6,400 s ≈ 1.8 hours.

Doubling the side length cost 1.9× (256 → 512), 3.6× (512 → 1024), 4.5×
(1024 → 2048) and 4.9× (2048 → 5000, for 5.96× the area): from 512 blocks up,
cost is essentially **linear in the rectangle's area** (a doubling of the side
is ≈4× the cost), with a fixed overhead that dominates the smallest rects.

Process peak RSS for the measured ladder: `VmHWM` 5,221,188 kB (≈5.0 GiB) at the
5000×5000 leg (read from `/proc`, after that leg completed; the probe prints
`peak_rss_kb` only at the end of the whole ladder, which the stopped 10,000 leg
prevented). Most of that is the generator's caches and the prefab store, plus
the query's per-column zone-biome memo, which lives for one query and grows with
the rectangle's distinct density-passing lattice points.

For search use cases (the explorer's ring search, T24) this is the sizing rule:

- **Keep ring rectangles small.** A ring is a set of rectangles; each call pays
  the whole interior scan, and a doubling of the side costs ≈4× once the rect is
  past a few hundred blocks, so a 1024×1024 region is ~7× a 256×256 one and a
  5000×5000 one is ~27 minutes.
- **Query the smallest rectangle that can answer the question.** The query has
  no cross-call cache: overlapping rings re-scan their shared area. Prefer one
  rect the search needs over several overlapping ones, and keep a ring's rect
  in the low hundreds of blocks per side until a measured budget says otherwise.
- **Reuse results client-side.** A smaller rect's placements are exactly the
  subset of a larger rect's placements (by the subset property above), so a
  search that already fetched a larger rect can filter it rather than re-query
  the smaller one.
- **Budget memory with the rect.** The per-chunk buckets keep candidate
  references/indices for every window a candidate can reach, and the zone-biome
  memo keeps one snapshot per distinct lattice column; the 5000×5000 leg peaked
  at ≈5 GiB, and a 10,000×10,000 query needs ≈4× that. A ring search on a client
  should stay well under that.

## Benchmark

```sh
cargo run --release -p worldgen-cli --bin feature_query_bench -- [world-dir] [seed]
```

Defaults: `testdata/Default`, seed 0. Release only — the probe prints a warning
when built in debug. It loads the generator once, warms it on a disjoint 64×64
rect near (4096, 4096), then measures one `features_in` call per square rect
(256, 512, 1024, 2048, 5000, 10,000 blocks), printing
`rect_<size>_seconds`, `rect_<size>_placements` and the per-kind counts
(`_unique`, `_biome`, `_spawn`), followed by `peak_rss_kb` from `VmHWM`. The
rects are nested, so a later run re-covers ground the earlier ones touched; each
timed run is still one fresh call. The largest leg costs hours; see the
extrapolated 10,000 row in [Cost](#cost).
