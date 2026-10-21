// Ported from: `.hytale-source/.../worldgen/ChunkGeneratorResource.java`
//! Java `com.hypixel.hytale.server.worldgen.ChunkGeneratorResource` — the
//! per-thread scratch buffers of the world generator.
//!
//! Java keeps the resource in a `ThreadLocal` so the hot paths can reuse the
//! buffers without allocating. The Rust port follows the plan's "explicit
//! `&mut` scratch per task" model: the resource is an ordinary value that
//! the worker owns (one per task) instead of a thread-local.
//!
//! # What is *not* part of the resource (see `REMAINING.md`)
//!
//! | Java member                        | Why it is absent                           |
//! |------------------------------------|-------------------------------------------|
//! | `prefabs` (timeout cache), `getPrefab` | the Rust [`crate::prefab::prefab_store::PrefabStore`] caches the prefab buffers by path |
//! | `prefabBuffer`                    | the paste buffer borrows the execution mutably (see below) |
//! | `bounds2d`, `resultBuffer2d`, `resultBuffer3d` | the procedural result buffers are not used by the chunk core |
//! | `cacheVector2d`                   | the joml vector is only a scratch key for the Java caches; the Rust caches build their keys directly |
//!
//! # Deviations
//!
//! * **`release()` is omitted** — the only thing it shuts down is the prefab
//!   timeout cache (Phase 5).
//! * **`init(ChunkGenerator)` is omitted** — the Java method only stores the
//!   generator for the (deferred) prefab accessor.
//! * **`coverArray` is a `Vec<i32>`** — the Java `IntList` is a growable int
//!   list; the Rust port uses the natural equivalent.
//! * **The prefab *buffer* cache lives in the prefab store** — the Java
//!   `prefabs` timeout cache maps a supplier to its buffer; the Rust
//!   [`crate::prefab::prefab_store::PrefabStore`] caches the buffers by their
//!   path (the supplier's `equals` is the path too), so the per-thread cache
//!   is redundant.
//! * **The prefab paste buffer is not part of the resource** — the Rust
//!   [`crate::prefab::prefab_paste_util::PrefabPasteBuffer`] borrows the
//!   execution mutably, so it cannot live in the (unborrowed) resource; the
//!   populator allocates one per paste (the buffer is fully re-seeded, so the
//!   reuse is not observable).
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use worldgen_model::block_priority::BlockPriorityChunk;

use crate::cache::coordinate_cache::CoordinateKey;
use crate::chunk::populator::prefab_populator::PrefabPopulator;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::climate::climate_noise::Buffer as ClimateBuffer;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `worldgen.ChunkGeneratorResource`.
pub struct ChunkGeneratorResource {
    /// Java `public final Random random` — a `FastRandom` seeded with 0.
    pub random: FastRandom,
    /// Java `public final Random random2` — a `FastRandom` seeded with 0.
    pub random2: FastRandom,
    /// Java `public final IntList coverArray` — an `IntArrayList(5)`.
    pub cover_array: Vec<i32>,
    /// Java `public final BlockPriorityChunk priorityChunk`.
    pub priority_chunk: BlockPriorityChunk,
    /// Java `public final CoordinateCache.CoordinateKey cacheCoordinateKey`.
    pub cache_coordinate_key: CoordinateKey,
    /// Java `public final ZoneBiomeResult zoneBiomeResult` — created with a
    /// fresh `ZoneGeneratorResult` (`zoneBiomeResult.setZoneResult(new
    /// ZoneGeneratorResult())`).
    pub zone_biome_result: ZoneBiomeResult,
    /// Java `public final ClimateNoise.Buffer climateBuffer`.
    pub climate_buffer: ClimateBuffer,
    /// Java `public final PrefabPopulator prefabPopulator` — the stateful
    /// populator of the prefab stage (Phase 5b).
    pub prefab_populator: PrefabPopulator,
}

impl ChunkGeneratorResource {
    /// Java `ChunkGeneratorResource()`.
    pub fn new() -> Self {
        Self {
            random: FastRandom::new(0),
            random2: FastRandom::new(0),
            priority_chunk: BlockPriorityChunk::new(),
            cover_array: Vec::with_capacity(5),
            cache_coordinate_key: CoordinateKey::new(0, 0, 0),
            zone_biome_result: {
                let mut result = ZoneBiomeResult::new();
                result.set_zone_result(Some(Rc::new(ZoneGeneratorResult::new())));
                result
            },
            climate_buffer: ClimateBuffer::default(),
            prefab_populator: PrefabPopulator::new(),
        }
    }

    /// Java `@Nonnull Random getRandom()`.
    pub fn get_random(&mut self) -> &mut FastRandom {
        &mut self.random
    }
}

impl Default for ChunkGeneratorResource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_randoms_are_seeded_with_zero() {
        let mut resource = ChunkGeneratorResource::new();
        // The Java constructor seeds both randoms with 0.
        let mut reference = FastRandom::new(0);
        assert_eq!(resource.random.next_int(), reference.next_int());
        let mut reference2 = FastRandom::new(0);
        assert_eq!(resource.random2.next_int(), reference2.next_int());
    }

    #[test]
    fn the_cover_array_starts_empty() {
        let resource = ChunkGeneratorResource::new();
        assert!(resource.cover_array.is_empty());
        assert!(resource.cover_array.capacity() >= 5);
    }

    #[test]
    fn the_priority_grid_starts_empty() {
        let mut resource = ChunkGeneratorResource::new();
        resource
            .priority_chunk
            .set(1, 2, 3, worldgen_model::block_priority::PREFAB);
        assert_eq!(
            resource.priority_chunk.get(1, 2, 3),
            worldgen_model::block_priority::PREFAB
        );
        resource.priority_chunk.reset();
        assert_eq!(
            resource.priority_chunk.get(1, 2, 3),
            worldgen_model::block_priority::NONE
        );
    }

    #[test]
    fn the_zone_biome_result_carries_a_scratch_zone_result() {
        let resource = ChunkGeneratorResource::new();
        assert!(resource.zone_biome_result.get_zone_result().is_some());
        assert!(resource.zone_biome_result.get_biome().is_none());
    }

    #[test]
    fn the_coordinate_key_is_reusable() {
        let mut resource = ChunkGeneratorResource::new();
        resource.cache_coordinate_key.set_location(7, 3, -2);
        assert_eq!(resource.cache_coordinate_key.seed(), 7);
        assert_eq!(
            resource.cache_coordinate_key.coord(),
            hytale_math::chunk_util::index_chunk(3, -2)
        );
    }
}
