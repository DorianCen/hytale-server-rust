//! Headless **feature placement query** — "what would be placed here?" without
//! generating a single block.
//!
//! [`ChunkGenerator::features_in`](crate::chunk::chunk_generator::ChunkGenerator::features_in)
//! runs the world generator's shared placement pass (the same pass the
//! [`crate::chunk::populator::PrefabPopulator`] runs per chunk) against an
//! arbitrary rectangle and reports the features the generator would place
//! there:
//!
//! * **biome prefabs** — the candidates the shared pass collects, invalidates
//!   and pastes,
//! * **unique prefabs** — the per-seed entries of
//!   [`ChunkGenerator::get_unique_prefabs`](crate::chunk::chunk_generator::ChunkGenerator::get_unique_prefabs)
//!   whose bounds intersect the rectangle,
//! * **spawn points** — the per-seed
//!   [`ChunkGenerator::get_spawn_points`](crate::chunk::chunk_generator::ChunkGenerator::get_spawn_points)
//!   transforms inside the rectangle,
//! * **cave entrances** — the cave types' entry points that pass the cave
//!   stage's zone/threshold/height predicate
//!   ([`ChunkGenerator::cave_entrances_in`](crate::chunk::chunk_generator::ChunkGenerator::cave_entrances_in)).
//!   The query stops before `get_cave`, so no cave geometry is generated.
//!
//! # Contract
//!
//! The query is a **pure function of `(seed, rect, filter)`**: the same inputs
//! always return the same placements in the same order, and no block, chunk or
//! execution is generated or mutated by the query itself (the generator's
//! caches are read/filled as they are by any generator query).
//!
//! The scan covers the rectangle **padded by the maximum prefab extent** (plus
//! the height-threshold interpolation radius used by the placement pass), so
//! the cost grows with the rectangle's area — this is deliberately **not an
//! O(1)** lookup. A small rectangle is cheap; a large one enumerates every
//! chunk-aligned window it touches.
//!
//! The reported `name` and `category` are the prefab's *asset* name and the
//! prefab category's name (`WorldGenPrefabSupplier::get_prefab_name` and
//! `PrefabCategory::name`) — **not** translated display names. Consumers that
//! want a human label map them themselves.
//!
//! # Per-window invalidation and rect independence
//!
//! Biome-prefab placement is evaluated per **chunk-aligned window**: the
//! candidates owning a window are invalidated against each other exactly like
//! the populator invalidates the candidates of one chunk, and a candidate is
//! reported placed when it survives its window's invalidation *and* its
//! rotated supplier bounds intersect that window (the populator's paste gate).
//! Comparing against a rectangle is then only a filter on the result. This
//! reproduces the populator's **per-chunk** invalidation rather than a global
//! invalidation — and the collection window covers the whole block range of
//! every window it enumerates, so each window's candidate bucket is exactly
//! the list the populator would build for that chunk — which makes a smaller
//! rectangle's result always a subset of a larger rectangle's result, by
//! construction rather than by luck.
//!
//! # Documented deviations from an actual per-chunk generation run
//!
//! 1. **The rectangle's interior biomes are sparse-sampled.** A real chunk runs
//!    the pass with the biomes of its core-data table as the exact interior
//!    set (plus a sparse scan around it). The query has no generated chunk, so
//!    it runs the same sparse scan over the *whole* padded rectangle
//!    (`BIOME_SAMPLE_STEP_SIZE` = 8 blocks). A biome that is smaller than the
//!    sample step can therefore be missed inside the rectangle.
//! 2. **Base-check heights are always computed.** The populator's base check
//!    reads the height through its execution, which returns `NO_HEIGHT` (`-1`)
//!    for columns whose height has not been computed yet; the query reads
//!    [`ChunkGenerator::get_height`](crate::chunk::chunk_generator::ChunkGenerator::get_height)
//!    instead. See the query context (`QueryCtx`) for the reason (the same
//!    deviation the shared pass documents).
//!

mod cave_entrances;
mod feature_placement;

pub use cave_entrances::{CAVE_QUERY_TILE, COLUMN_MEMO_CAP};

pub use feature_placement::{
    BlockBounds, BlockRect, FeaturePlacement, PlacementFilter, PlacementKind, PlacementKindMask,
};
