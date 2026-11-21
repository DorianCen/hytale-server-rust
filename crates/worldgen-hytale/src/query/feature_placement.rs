//! The public types of the headless feature query and the
//! [`ChunkGenerator::features_in`] implementation.
//!
//! The query itself is documented in [`crate::query`]; this leaf module holds
//! the result/filter vocabulary and the placement bookkeeping. It runs the
//! shared placement pass of
//! [`crate::chunk::populator::prefab_placement_pass`] against a rectangle
//! instead of a chunk and never writes a block.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::BitOr;
use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::math_util;

use crate::biome::biome::Biome;
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::populator::prefab_placement_pass::{
    Candidate, PlacementCollector, PlacementContext,
};
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::container::unique_prefab_container::UniquePrefabEntry;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::bounds::IChunkBounds;
use crate::util::transform::Transform;

/// Inclusive block-coordinate rectangle on the XZ plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlockRect {
    /// Inclusive minimum block x.
    pub min_x: i32,
    /// Inclusive minimum block z.
    pub min_z: i32,
    /// Inclusive maximum block x.
    pub max_x: i32,
    /// Inclusive maximum block z.
    pub max_z: i32,
}

impl BlockRect {
    /// Whether this rectangle intersects the inclusive block rectangle
    /// `(min_x, min_z)..=(max_x, max_z)` on the XZ plane.
    const fn intersects(self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> bool {
        max_x >= self.min_x && min_x <= self.max_x && max_z >= self.min_z && min_z <= self.max_z
    }

    /// Whether the block `(x, z)` lies inside this rectangle.
    const fn contains(self, x: i32, z: i32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}

/// Inclusive 3D block bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlockBounds {
    /// Inclusive minimum block x.
    pub min_x: i32,
    /// Inclusive minimum block y.
    pub min_y: i32,
    /// Inclusive minimum block z.
    pub min_z: i32,
    /// Inclusive maximum block x.
    pub max_x: i32,
    /// Inclusive maximum block y.
    pub max_y: i32,
    /// Inclusive maximum block z.
    pub max_z: i32,
}

impl BlockBounds {
    /// The larger horizontal footprint (x or z extent, both inclusive).
    const fn horizontal_size(self) -> i64 {
        let size_x = (self.max_x as i64) - (self.min_x as i64) + 1;
        let size_z = (self.max_z as i64) - (self.min_z as i64) + 1;
        if size_x > size_z {
            size_x
        } else {
            size_z
        }
    }
}

/// Kind of a placed feature.
///
/// Reserved extension point: a `CaveEntrance` variant is planned for a later
/// task, so the enum is `#[non_exhaustive]` — adding the variant is not a
/// breaking change for downstream `match`es.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlacementKind {
    /// A per-seed unique prefab (landmark, monument, spawn location, …).
    UniquePrefab,
    /// A prefab placed by a biome's prefab container.
    BiomePrefab,
    /// A spawn point derived from a unique prefab marked as a spawn location.
    Spawn,
}

impl PlacementKind {
    /// The deterministic output order of the kinds (smaller comes first).
    const fn rank(self) -> u8 {
        match self {
            PlacementKind::UniquePrefab => 0,
            PlacementKind::BiomePrefab => 1,
            PlacementKind::Spawn => 2,
        }
    }
}

/// Bit set of [`PlacementKind`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlacementKindMask(u8);

impl PlacementKindMask {
    /// The unique-prefab bit.
    pub const UNIQUE_PREFAB: PlacementKindMask = PlacementKindMask(1 << 0);
    /// The biome-prefab bit.
    pub const BIOME_PREFAB: PlacementKindMask = PlacementKindMask(1 << 1);
    /// The spawn-point bit.
    pub const SPAWN: PlacementKindMask = PlacementKindMask(1 << 2);

    /// Every kind (the [`Default`]).
    #[must_use]
    pub const fn all() -> PlacementKindMask {
        PlacementKindMask((1 << 0) | (1 << 1) | (1 << 2))
    }

    /// Whether the mask contains `kind`.
    #[must_use]
    pub const fn contains(self, kind: PlacementKind) -> bool {
        self.0 & (1 << kind.rank()) != 0
    }
}

impl Default for PlacementKindMask {
    fn default() -> Self {
        Self::all()
    }
}

impl BitOr for PlacementKindMask {
    type Output = PlacementKindMask;

    fn bitor(self, rhs: PlacementKindMask) -> PlacementKindMask {
        PlacementKindMask(self.0 | rhs.0)
    }
}

/// What [`ChunkGenerator::features_in`] returns: a filter over the collected
/// placements.
///
/// The filter is applied **after** placement, so changing it never changes any
/// random draw (filtering cannot add, remove or reorder candidates before the
/// invalidation).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlacementFilter {
    /// The kinds to keep. [`PlacementKindMask::all`] by default.
    pub kinds: PlacementKindMask,
    /// Keep only placements whose category name is one of these (exact match,
    /// case-sensitive) when set.
    pub categories: Option<Vec<String>>,
    /// Keep only placements whose prefab name contains this substring
    /// (case-insensitive) when set.
    pub name_contains: Option<String>,
    /// Keep only placements whose larger horizontal footprint is at least this
    /// many blocks when set. Placements without bounds (spawn points) never
    /// match a set `min_size`.
    pub min_size: Option<i32>,
}

impl PlacementFilter {
    /// Whether `placement` passes every configured rule.
    fn matches(&self, placement: &FeaturePlacement) -> bool {
        if !self.kinds.contains(placement.kind) {
            return false;
        }

        if let Some(categories) = &self.categories {
            if !categories.iter().any(|name| name == &placement.category) {
                return false;
            }
        }

        if let Some(needle) = &self.name_contains {
            if !placement
                .name
                .to_lowercase()
                .contains(&needle.to_lowercase())
            {
                return false;
            }
        }

        if let Some(min_size) = self.min_size {
            let Some(bounds) = placement.bounds else {
                return false;
            };
            if bounds.horizontal_size() < i64::from(min_size) {
                return false;
            }
        }

        true
    }
}

/// One feature the generator places in the queried rectangle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeaturePlacement {
    /// What placed the feature.
    pub kind: PlacementKind,
    /// The prefab's asset name (`WorldGenPrefabSupplier::get_prefab_name`) —
    /// not a translated display name.
    pub name: String,
    /// The prefab category's name; see [`Self::name`] for the naming contract.
    pub category: String,
    /// The placement origin `(x, y, z)` in block coordinates.
    pub position: (i32, i32, i32),
    /// Inclusive world bounds; `None` for spawn points.
    pub bounds: Option<BlockBounds>,
    /// The prefab rotation.
    pub rotation: PrefabRotation,
    /// Whether the map should show this feature.
    pub show_on_map: bool,
    /// Whether the feature is a spawn location.
    pub spawn_location: bool,
}

/// The [`PlacementContext`] of the feature query: an arbitrary rectangle.
///
/// The window is the **padded collection range** (`features_in` grows the
/// caller's rect by the maximum prefab extent, rounded out to whole chunks,
/// so every enumerated window's bucket is complete — see `features_in`),
/// there are no generated core-data biomes (the sparse region scan covers the
/// whole window), the region scan skips nothing, and the base-check height is
/// the **computed** [`ChunkGenerator::get_height`].
///
/// The populator's [`ChunkCtx`](crate::chunk::populator::prefab_populator::ChunkCtx)
/// reads the height through the execution instead: a not-yet-computed table
/// column yields `NO_HEIGHT` (`-1`) there (a transcribed Java staging
/// artifact). The query has no execution and deliberately wants the idealized
/// height. In the current Default fixture no prefab pattern declares
/// `BaseChecks`, so the two agree on all fixture data.
///
/// # The zone-biome memo
///
/// The pass evaluates every (biome, entry) pair's lattice over the whole
/// window; a lattice point of every pair asks for the same column's zone
/// biome. The generator's own column cache holds only
/// `GENERATOR_CACHE_MAX_SIZE` (50,000) entries, so on a large rectangle the
/// pairs evict each other and a cold column costs tens of microseconds each
/// time. The memo keeps the generator's snapshot per column for the lifetime
/// of one query, deliberately *outliving* the generator's column cache: one
/// query never recomputes a column it already resolved. It is bounded by the
/// query's distinct density-passing lattice points (the pass only queries a
/// column after the noise-density filter), and the pass reads the result only
/// through `get_biome`, so a snapshot is exact.
struct QueryCtx<'a> {
    /// The generator the rectangle is queried against.
    generator: &'a ChunkGenerator,
    /// The padded collection window (see [`ChunkGenerator::features_in`]).
    rect: BlockRect,
    /// Memoized zone-biome results of the query, keyed by the column `(x, z)`
    /// (the seed is fixed for one query).
    zone_biomes: RefCell<HashMap<(i32, i32), ZoneBiomeResult>>,
}

impl PlacementContext for QueryCtx<'_> {
    fn window_min_x(&self) -> i32 {
        self.rect.min_x
    }

    fn window_min_z(&self) -> i32 {
        self.rect.min_z
    }

    fn window_max_x(&self) -> i32 {
        self.rect.max_x
    }

    fn window_max_z(&self) -> i32 {
        self.rect.max_z
    }

    fn generator(&self) -> &ChunkGenerator {
        self.generator
    }

    fn zone_biome_result_at(&self, seed: i32, x: i32, z: i32) -> ZoneBiomeResult {
        if let Some(result) = self.zone_biomes.borrow().get(&(x, z)) {
            return result.clone();
        }
        let result = self.generator.get_zone_biome_result_at(seed, x, z);
        self.zone_biomes.borrow_mut().insert((x, z), result.clone());
        result
    }

    fn core_biomes(&self) -> Vec<Rc<dyn Biome>> {
        // No chunk exists — the sparse scan of the pass covers the whole
        // window (see the module docs, deviation 1).
        Vec::new()
    }

    fn interior_skip_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }

    fn base_check_height(&self, seed: i32, x: i32, z: i32) -> i32 {
        self.generator.get_height(seed, x, z)
    }
}

impl ChunkGenerator {
    /// The features this generator places in `rect` for `seed`, filtered by
    /// `filter` and sorted deterministically (kind, x, z, y, name).
    ///
    /// The query runs the shared placement pass without generating any block;
    /// see the [`crate::query`] module docs for the contract, the cost and the
    /// documented deviations from a real per-chunk run.
    ///
    /// # Panics
    ///
    /// Panics when `rect` is not normalized (`min > max` on an axis): that is
    /// a caller bug, not a data condition.
    #[must_use]
    pub fn features_in(
        &self,
        seed: i32,
        rect: BlockRect,
        filter: &PlacementFilter,
    ) -> Vec<FeaturePlacement> {
        assert!(
            rect.min_x <= rect.max_x && rect.min_z <= rect.max_z,
            "the rect is normalized (min <= max on both axes)"
        );

        let unique_prefabs = self.get_unique_prefabs(seed);

        let mut placements = Vec::new();

        // The biome-prefab pass is the expensive part; when the kind mask
        // does not ask for it, skip it entirely. This is safe: the RNG is
        // re-seeded per lattice point, so the populator's stream never
        // depends on which candidates a query collects (the filters are
        // documented to apply after placement; skipping a kind's collection
        // removes only placements the mask would have discarded anyway).
        if filter.kinds.contains(PlacementKind::BiomePrefab) {
            // The collection window must cover the *whole block range* of
            // every chunk the bucketing below enumerates (the rect padded by
            // the maximum prefab extent, rounded out to whole chunks). A
            // window's bucket is the populator's per-chunk candidate list
            // only if the window lies fully inside the collection window;
            // a bare-rect window would truncate the edge chunks' buckets and
            // make the per-window invalidation depend on the rect (breaking
            // the small-rect ⊆ large-rect property structurally).
            let max_extent = self.get_zone_pattern_provider().get_max_extent();
            let min_cx = chunk_util::chunk_coordinate_i32(rect.min_x - max_extent);
            let max_cx = chunk_util::chunk_coordinate_i32(rect.max_x + max_extent);
            let min_cz = chunk_util::chunk_coordinate_i32(rect.min_z - max_extent);
            let max_cz = chunk_util::chunk_coordinate_i32(rect.max_z + max_extent);
            let window = BlockRect {
                min_x: chunk_util::min_block(min_cx),
                min_z: chunk_util::min_block(min_cz),
                max_x: chunk_util::max_block(max_cx),
                max_z: chunk_util::max_block(max_cz),
            };

            let mut collector = PlacementCollector::new();
            collector.reset();
            // The pass derives the zone generator and the per-lattice-point
            // seeds from `world_seed`; it must be set before the biome
            // collection.
            collector.world_seed = seed;
            collector.min_priority = PrefabCategory::MAX_PRIORITY;
            {
                let ctx = QueryCtx {
                    generator: self,
                    rect: window,
                    zone_biomes: RefCell::new(HashMap::new()),
                };
                collector.collect_biomes(&ctx);
                collector.collect_prefabs(&ctx, &unique_prefabs);
            }
            // The result filter stays the caller's rect; the padded window
            // only feeds the collection.
            self.collect_biome_prefabs(&collector, rect, &mut placements);
        }

        if filter.kinds.contains(PlacementKind::UniquePrefab) {
            for entry in &unique_prefabs {
                Self::push_unique_prefab(entry, rect, &mut placements);
            }
        }

        if filter.kinds.contains(PlacementKind::Spawn) {
            for transform in self.get_spawn_points(seed) {
                Self::push_spawn_point(&transform, rect, &mut placements);
            }
        }

        let mut placements: Vec<FeaturePlacement> = placements
            .into_iter()
            .filter(|placement| filter.matches(placement))
            .collect();
        placements.sort_by(|a, b| {
            (
                a.kind.rank(),
                a.position.0,
                a.position.2,
                a.position.1,
                a.name.as_str(),
            )
                .cmp(&(
                    b.kind.rank(),
                    b.position.0,
                    b.position.2,
                    b.position.1,
                    b.name.as_str(),
                ))
        });
        placements
    }

    /// Buckets the collected candidates into the chunk-aligned windows they
    /// belong to, invalidates each window's bucket, and keeps the candidates
    /// whose rotated supplier bounds intersect their window (the populator's
    /// paste gate).
    fn collect_biome_prefabs(
        &self,
        collector: &PlacementCollector,
        rect: BlockRect,
        placements: &mut Vec<FeaturePlacement>,
    ) {
        let candidates = collector.candidates();
        if candidates.is_empty() {
            return;
        }

        // The windows that can own a candidate reaching the rect: the rect
        // padded by the maximum prefab extent.
        let max_extent = self.get_zone_pattern_provider().get_max_extent();
        let min_cx = chunk_util::chunk_coordinate_i32(rect.min_x - max_extent);
        let max_cx = chunk_util::chunk_coordinate_i32(rect.max_x + max_extent);
        let min_cz = chunk_util::chunk_coordinate_i32(rect.min_z - max_extent);
        let max_cz = chunk_util::chunk_coordinate_i32(rect.max_z + max_extent);

        // A candidate belongs to every window whose lattice range (the window
        // padded by the entry extents) contains its centre. That is exactly
        // the populator's per-chunk collection region, so a window's bucket is
        // the candidate list the populator would invalidate for that chunk.
        //
        // The buckets hold borrowed candidates and their original indices: the
        // index keeps the cross-window dedup, and the reference keeps a bucket
        // from cloning the candidate (the invalidation only borrows them).
        let mut buckets: HashMap<(i32, i32), Vec<(usize, &Candidate)>> = HashMap::new();
        for (index, candidate) in candidates.iter().enumerate() {
            let extents = candidate.entry.get_extents();
            let low_cx = chunk_util::chunk_coordinate_i32(candidate.x - extents);
            let high_cx = chunk_util::chunk_coordinate_i32(candidate.x + extents);
            let low_cz = chunk_util::chunk_coordinate_i32(candidate.z - extents);
            let high_cz = chunk_util::chunk_coordinate_i32(candidate.z + extents);
            for cx in low_cx..=high_cx {
                if cx < min_cx || cx > max_cx {
                    continue;
                }
                for cz in low_cz..=high_cz {
                    if cz < min_cz || cz > max_cz {
                        continue;
                    }
                    buckets
                        .entry((cx, cz))
                        .or_default()
                        .push((index, candidate));
                }
            }
        }

        let mut placed = vec![false; candidates.len()];
        let mut placed_indices = Vec::new();
        for cx in min_cx..=max_cx {
            for cz in min_cz..=max_cz {
                let Some(entries) = buckets.get(&(cx, cz)) else {
                    continue;
                };

                // `invalidate_candidates` takes a slice of candidate
                // references; the bucket's indices stay available for the
                // paste gate and the cross-window dedup below.
                let bucket: Vec<&Candidate> =
                    entries.iter().map(|&(_, candidate)| candidate).collect();
                let min_priority = bucket
                    .iter()
                    .map(|candidate| candidate.priority)
                    .min()
                    .unwrap_or(PrefabCategory::MAX_PRIORITY);
                let mut invalidated = vec![false; bucket.len()];
                PlacementCollector::invalidate_candidates(&bucket, &mut invalidated, min_priority);

                for (bit, &(index, _)) in entries.iter().enumerate() {
                    if invalidated[bit] {
                        continue;
                    }
                    if !Self::candidate_pastes_into(&candidates[index], cx, cz) {
                        continue;
                    }
                    if !placed[index] {
                        placed[index] = true;
                        placed_indices.push(index);
                    }
                }
            }
        }

        for index in placed_indices {
            let candidate = &candidates[index];
            let (min_x, min_y, min_z, max_x, max_y, max_z) =
                PlacementCollector::candidate_bounds(candidate);
            if !rect.intersects(min_x, min_z, max_x, max_z) {
                continue;
            }
            placements.push(FeaturePlacement {
                kind: PlacementKind::BiomePrefab,
                name: candidate.supplier.get_prefab_name(),
                category: candidate.generator.get_category().name.clone(),
                position: (candidate.x, candidate.y, candidate.z),
                bounds: Some(BlockBounds {
                    min_x,
                    min_y,
                    min_z,
                    max_x,
                    max_y,
                    max_z,
                }),
                rotation: candidate.rotation,
                // Biome prefab patterns carry no map/spawn flags — only the
                // unique-prefab configurations do.
                show_on_map: true,
                spawn_location: false,
            });
        }
    }

    /// The populator's `isMatchingChunkBounds` paste gate, on rotated supplier
    /// bounds (not on the buffer-access bounds the result reports).
    fn candidate_pastes_into(candidate: &Candidate, chunk_x: i32, chunk_z: i32) -> bool {
        let bounds = candidate.supplier.get_bounds();
        let rotation = candidate.rotation;
        let min_x = candidate
            .x
            .wrapping_add(bounds.get_low_bound_x_rotated(rotation));
        let min_z = candidate
            .z
            .wrapping_add(bounds.get_low_bound_z_rotated(rotation));
        let max_x = candidate
            .x
            .wrapping_add(bounds.get_high_bound_x_rotated(rotation));
        let max_z = candidate
            .z
            .wrapping_add(bounds.get_high_bound_z_rotated(rotation));

        chunk_util::max_block(chunk_x) >= min_x
            && chunk_util::min_block(chunk_x) <= max_x
            && chunk_util::max_block(chunk_z) >= min_z
            && chunk_util::min_block(chunk_z) <= max_z
    }

    /// One unique prefab, when its absolute horizontal bounds intersect the
    /// rect.
    fn push_unique_prefab(
        entry: &UniquePrefabEntry,
        rect: BlockRect,
        placements: &mut Vec<FeaturePlacement>,
    ) {
        // The entry's bounds are absolute world coordinates (its constructor
        // adds the position); do not add the position again here.
        if !rect.intersects(
            entry.get_low_bound_x(),
            entry.get_low_bound_z(),
            entry.get_high_bound_x(),
            entry.get_high_bound_z(),
        ) {
            return;
        }

        let position = entry.get_position();
        placements.push(FeaturePlacement {
            kind: PlacementKind::UniquePrefab,
            name: entry.get_name().to_string(),
            category: entry.get_category().name.clone(),
            position,
            // The entry records no exact y extent beyond its low/high bound
            // fields; the reported y range is the placement origin.
            bounds: Some(BlockBounds {
                min_x: entry.get_low_bound_x(),
                min_y: position.1,
                min_z: entry.get_low_bound_z(),
                max_x: entry.get_high_bound_x(),
                max_y: position.1,
                max_z: entry.get_high_bound_z(),
            }),
            rotation: entry.get_rotation(),
            show_on_map: entry.is_show_on_map(),
            spawn_location: entry.is_spawn_location(),
        });
    }

    /// One spawn point, when its floored position lies inside the rect.
    fn push_spawn_point(
        transform: &Transform,
        rect: BlockRect,
        placements: &mut Vec<FeaturePlacement>,
    ) {
        let position = transform.get_position();
        let x = math_util::floor(position.x);
        let y = math_util::floor(position.y);
        let z = math_util::floor(position.z);
        if !rect.contains(x, z) {
            return;
        }

        placements.push(FeaturePlacement {
            kind: PlacementKind::Spawn,
            name: "spawn".to_string(),
            category: "spawn".to_string(),
            position: (x, y, z),
            bounds: None,
            // Spawn transforms carry a yaw rotation, not a prefab rotation;
            // report the identity prefab rotation.
            rotation: PrefabRotation::VALUES[0],
            show_on_map: true,
            spawn_location: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::rc::Rc;

    use worldgen_model::chunk::GeneratedChunk;
    use worldgen_model::registry::Registries;

    use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
    use crate::chunk::populator::prefab_populator::ChunkCtx;

    use super::*;

    /// The real Default world (the same loader chain as the golden chunks).
    fn default_world() -> Rc<ChunkGenerator> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/Default");
        let config = crate::world_gen_config::WorldGenConfig::new(
            root,
            "Default".to_string(),
            "0.0.0".to_string(),
        );
        let loader = crate::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new("ChunkGenerator"),
            config,
        );
        loader.load()
    }

    /// What the populator pastes for `(seed, chunk)`, without running the
    /// terrain pipeline (the interpolator's `populate` is enough for the
    /// context).
    ///
    /// The returned tuples are `(name, x, y, z, rotation)`.
    fn populator_placements(
        generator: &Rc<ChunkGenerator>,
        registries: &Rc<Registries>,
        seed: i32,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Vec<(String, i32, i32, i32, PrefabRotation)> {
        let chunk = GeneratedChunk::with_coordinates(
            chunk_util::index_chunk(chunk_x, chunk_z),
            chunk_x,
            chunk_z,
        );
        let execution = ChunkGeneratorExecution::with_registry(
            seed,
            Rc::clone(generator),
            chunk,
            Rc::clone(registries),
        );

        let unique_prefabs = generator.get_unique_prefabs(seed);
        let mut collector = PlacementCollector::new();
        collector.reset();
        collector.world_seed = seed;
        collector.min_priority = PrefabCategory::MAX_PRIORITY;
        {
            let ctx = ChunkCtx::new(&execution);
            collector.collect_biomes(&ctx);
            collector.collect_prefabs(&ctx, &unique_prefabs);
        }
        collector.invalidate_prefabs();

        let mut placements = Vec::new();
        for index in 0..collector.candidates().len() {
            if collector.invalidated(index) {
                continue;
            }
            let candidate = &collector.candidates()[index];
            // The populator's paste gate (isMatchingChunkBounds).
            let bounds = candidate.supplier.get_bounds();
            let min_x = candidate
                .x
                .wrapping_add(bounds.get_low_bound_x_rotated(candidate.rotation));
            let min_z = candidate
                .z
                .wrapping_add(bounds.get_low_bound_z_rotated(candidate.rotation));
            let max_x = candidate
                .x
                .wrapping_add(bounds.get_high_bound_x_rotated(candidate.rotation));
            let max_z = candidate
                .z
                .wrapping_add(bounds.get_high_bound_z_rotated(candidate.rotation));
            if !(chunk_util::max_block(chunk_x) >= min_x
                && chunk_util::min_block(chunk_x) <= max_x
                && chunk_util::max_block(chunk_z) >= min_z
                && chunk_util::min_block(chunk_z) <= max_z)
            {
                continue;
            }

            placements.push((
                candidate.supplier.get_prefab_name(),
                candidate.x,
                candidate.y,
                candidate.z,
                candidate.rotation,
            ));
        }
        placements.sort_by(|a, b| {
            (a.0.as_str(), a.1, a.2, a.3, a.4.get_degrees()).cmp(&(
                b.0.as_str(),
                b.1,
                b.2,
                b.3,
                b.4.get_degrees(),
            ))
        });
        placements
    }

    #[test]
    fn query_biome_prefabs_match_the_chunk_placement_pass() {
        let generator = default_world();
        let registries = Rc::new(Registries::load());
        let filter = PlacementFilter {
            kinds: PlacementKindMask::BIOME_PREFAB,
            ..PlacementFilter::default()
        };

        // Two fixture chunks that actually contain biome prefab placements
        // (the assertions below keep the test from passing vacuously).
        let mut total = 0usize;
        for &(seed, chunk_x, chunk_z) in &[(0, 0, 0), (0, 4, -5)] {
            let expected = populator_placements(&generator, &registries, seed, chunk_x, chunk_z);
            assert!(
                !expected.is_empty(),
                "seed {seed} chunk ({chunk_x},{chunk_z}) has no biome prefab paste"
            );
            total += expected.len();

            let rect = BlockRect {
                min_x: chunk_util::min_block(chunk_x),
                min_z: chunk_util::min_block(chunk_z),
                max_x: chunk_util::max_block(chunk_x),
                max_z: chunk_util::max_block(chunk_z),
            };
            let actual = generator.features_in(seed, rect, &filter);
            let mut actual: Vec<(String, i32, i32, i32, PrefabRotation)> = actual
                .iter()
                .map(|placement| {
                    (
                        placement.name.clone(),
                        placement.position.0,
                        placement.position.1,
                        placement.position.2,
                        placement.rotation,
                    )
                })
                .collect();
            actual.sort_by(|a, b| {
                (a.0.as_str(), a.1, a.2, a.3, a.4.get_degrees()).cmp(&(
                    b.0.as_str(),
                    b.1,
                    b.2,
                    b.3,
                    b.4.get_degrees(),
                ))
            });
            assert_eq!(
                actual, expected,
                "seed {seed} chunk ({chunk_x},{chunk_z}): the query must match the chunk pass"
            );
        }
        assert!(total > 0, "the fixture chunks contained no placements");
    }
}
