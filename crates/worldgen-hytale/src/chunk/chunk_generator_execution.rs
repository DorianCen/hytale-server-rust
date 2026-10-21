// Ported from: `.hytale-source/.../worldgen/chunk/ChunkGeneratorExecution.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.ChunkGeneratorExecution` —
//! the per-chunk generation context: it owns the chunk buffer, the
//! block-priority grid and the per-chunk
//! [`HeightThresholdInterpolator`], and it arbitrates every block/fluid
//! write.
//!
//! # What is *not* ported (see `REMAINING.md`)
//!
//! * **The ECS boundary**: `chunk.setState(x, y, z, holder)` and the
//!   `Holder<ChunkStore>` section machinery are CoreServer concepts and are
//!   not ported (see `plan.md`) — the calls are no-ops here.
//! * **The block physics**: `BlockPhysics.setSupportValue` / `clear` /
//!   `reset` need the block-type registry and the chunk sections — no-ops
//!   here.
//! * **The entity chunk** (`chunk.getEntityChunk()`) — not part of the
//!   generation-only scope.
//! * **The block/fluid registries**: Java reads them from the static
//!   CoreServer asset maps. The Rust port carries a data-driven seam
//!   ([`worldgen_model::registry`]) filled by the loader from the golden
//!   registry dump (see `REMAINING.md` § 6).
//!
//! The full stage order (tint → environment → blocks → caves → prefabs →
//! water) is ported and the generated chunk grid is **bit-exact** against the
//! Java pipeline on all 21 golden chunks (`tests/chunks_golden.rs`).
//!
//! # Deviations
//!
//! * **The priority grid is allocated per execution** instead of being taken
//!   from the thread-local resource pool. The grid is reset before every
//!   use, so the values are identical; only the allocation is repeated.
//! * **The Java method overloads are collapsed** into distinct Rust
//!   functions (`set_block`, `set_block_entry`, `set_block_with_environment`,
//!   …) because Rust has no overloading.
//! * **`execute` takes the resource** (`execute(&mut self, seed, resource)`)
//!   because Java reaches the reused scratch buffers through the static
//!   `ChunkGenerator.getResource()`.
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use hytale_math::chunk_util;

use worldgen_model::block_priority::{
    BlockPriorityChunk, BlockPriorityModifier, NoneModifier, FLAG_MASK, HEIGHT, MASK, NO_CHANGE,
};
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

use crate::cache::core_data_cache_entry::CoreDataCacheEntry;
use crate::chunk::chunk_generator::{ChunkGenerator, TINT_INTERPOLATION_RADIUS};
use crate::chunk::chunk_generator_resource::ChunkGeneratorResource;
use crate::chunk::height_threshold_interpolator::HeightThresholdInterpolator;
use crate::chunk::populator::{BlockPopulator, CavePopulator, PrefabPopulator, WaterPopulator};
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::util::block_fluid_entry::BlockFluidEntry;

/// Java `AssetMapWithIndexes.NOT_FOUND` = `Integer.MIN_VALUE` — the value
/// Java returns for a fluid lookup outside the world height.
const FLUID_NOT_FOUND: i32 = i32::MIN;

/// Java `WorldGenTimingsCollector` — the per-chunk stage timings (the Rust
/// port records them per execution instead of feeding one shared collector;
/// [`crate::world_gen::WorldGenService`] aggregates them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageTimings {
    /// The tint-mapping stage.
    pub tint_nanos: u64,
    /// The environment-mapping stage.
    pub environment_nanos: u64,
    /// The `BlockPopulator` stage (terrain / layers / covers).
    pub blocks_nanos: u64,
    /// The `CavePopulator` stage.
    pub caves_nanos: u64,
    /// The `PrefabPopulator` stage.
    pub prefabs_nanos: u64,
    /// The `WaterPopulator` stage.
    pub water_nanos: u64,
    /// The whole `execute` call.
    pub total_nanos: u64,
}

/// Java `chunk.ChunkGeneratorExecution`.
pub struct ChunkGeneratorExecution {
    /// Java `private final ChunkGenerator chunkGenerator`.
    chunk_generator: Rc<ChunkGenerator>,
    /// Java `private final GeneratedChunk chunk` (the Java class holds the
    /// block chunk, the entity chunk and the sections separately; the Rust
    /// model folds them into one buffer).
    chunk: GeneratedChunk,
    /// Java `private final BlockPriorityChunk priorityChunk`.
    priority_chunk: BlockPriorityChunk,
    /// Java `private final HeightThresholdInterpolator interpolator`.
    interpolator: HeightThresholdInterpolator,
    /// Java `private BlockPriorityModifier blockPriorityModifier`.
    block_priority_modifier: Rc<dyn BlockPriorityModifier>,
    /// Java `BlockType.getAssetMap()` / `Fluid.getAssetMap()` — the block and
    /// fluid registries.
    ///
    /// Deviation: Java reads the asset maps from the static CoreServer
    /// registries; the Rust port carries them on the execution so the
    /// populators can reach them (the asset packs themselves are not part of
    /// the shared source tree — see `REMAINING.md` § 6).
    registry: Rc<Registries>,
    /// The stage timings of the last [`Self::execute`] call.
    timings: StageTimings,
}

impl ChunkGeneratorExecution {
    /// Java `ChunkGeneratorExecution(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, GeneratedChunk chunk)`.
    ///
    /// Deviation: the Java class reads the block/fluid registries from the
    /// static CoreServer asset maps; this constructor keeps the test default
    /// [`Registries::empty`] (unit tests construct executions without the
    /// asset pipeline), while every loader/generation path uses
    /// [`Self::with_registry`] with the registry dump (see `REMAINING.md` § 6).
    pub fn new(seed: i32, chunk_generator: Rc<ChunkGenerator>, chunk: GeneratedChunk) -> Self {
        Self::with_registry(seed, chunk_generator, chunk, Rc::new(Registries::empty()))
    }

    /// Java `ChunkGeneratorExecution(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, GeneratedChunk chunk)` — with an explicit registry.
    pub fn with_registry(
        seed: i32,
        chunk_generator: Rc<ChunkGenerator>,
        chunk: GeneratedChunk,
        registry: Rc<Registries>,
    ) -> Self {
        // Java: `ChunkGenerator.getResource().priorityChunk.reset()` — the
        // Rust port allocates a fresh (already reset) grid instead of
        // recycling the pooled one.
        let priority_chunk = BlockPriorityChunk::new();

        let chunk_x = chunk.get_x();
        let chunk_z = chunk.get_z();
        let mut interpolator =
            HeightThresholdInterpolator::new(Rc::clone(&chunk_generator), chunk_x, chunk_z);
        interpolator.populate(seed);

        Self {
            chunk_generator,
            chunk,
            priority_chunk,
            interpolator,
            block_priority_modifier: Rc::new(NoneModifier::NONE),
            registry,
            timings: StageTimings::default(),
        }
    }

    /// The block and fluid registries of this execution.
    pub fn registry(&self) -> &Rc<Registries> {
        &self.registry
    }

    /// Java `public void execute(int seed)`.
    ///
    /// Deviation: the Java method reaches the reused scratch buffers through
    /// the static `ChunkGenerator.getResource()`; the Rust port takes the
    /// [`ChunkGeneratorResource`] as an explicit parameter.
    ///
    /// The stage order is the Java one: tint → environment → blocks → caves
    /// → prefabs → water. The generated chunk is bit-exact against the Java
    /// pipeline on the golden chunk grid (`tests/chunks_golden.rs`).
    pub fn execute(&mut self, seed: i32, resource: &mut ChunkGeneratorResource) {
        self.execute_cancellable(seed, resource, None);
    }

    /// Java `public void execute(int seed)` — with a cooperative cancellation
    /// token. Returns `false` when the token was set before a stage (the
    /// caller discards the partially generated chunk); the timings still
    /// record the completed stages.
    pub fn execute_cancellable(
        &mut self,
        seed: i32,
        resource: &mut ChunkGeneratorResource,
        cancel: Option<&AtomicBool>,
    ) -> bool {
        let started = Instant::now();
        macro_rules! stage {
            ($field:ident, $body:expr) => {{
                if cancel.is_some_and(|token| token.load(Ordering::Relaxed)) {
                    self.timings.total_nanos = started.elapsed().as_nanos() as u64;
                    return false;
                }
                let stage_start = Instant::now();
                $body;
                self.timings.$field = stage_start.elapsed().as_nanos() as u64;
            }};
        }

        // Generate the tint map
        stage!(tint_nanos, self.generate_tint_mapping(seed));

        // Generate the environment map
        stage!(environment_nanos, self.generate_environment_mapping(seed));

        // Generate filling + layers + covers
        stage!(blocks_nanos, BlockPopulator::populate(seed, self, resource));

        // Generate caves
        stage!(caves_nanos, CavePopulator::populate(seed, self));

        // Generate prefabs
        stage!(
            prefabs_nanos,
            PrefabPopulator::populate(seed, self, resource)
        );

        // Generate water
        stage!(water_nanos, WaterPopulator::populate(seed, self));

        self.timings.total_nanos = started.elapsed().as_nanos() as u64;
        true
    }

    /// The stage timings of the last [`Self::execute`] call (Java
    /// `WorldGenTimingsCollector`).
    pub fn get_timings(&self) -> StageTimings {
        self.timings
    }

    /// Consumes the execution and hands out the generated chunk (the Java
    /// `ChunkGenerator.generate` result).
    pub fn into_chunk(self) -> GeneratedChunk {
        self.chunk
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               GETTERS / SETTERS                                               ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `@Nonnull ChunkGenerator getChunkGenerator()`.
    pub fn get_chunk_generator(&self) -> &Rc<ChunkGenerator> {
        &self.chunk_generator
    }

    /// Java `GeneratedBlockChunk getChunk()` — the Rust chunk buffer.
    pub fn get_chunk(&self) -> &GeneratedChunk {
        &self.chunk
    }

    /// Java `GeneratedBlockChunk getChunk()` — the mutable Rust chunk buffer.
    pub fn get_chunk_mut(&mut self) -> &mut GeneratedChunk {
        &mut self.chunk
    }

    /// Java `@Nonnull BlockPriorityChunk getPriorityChunk()`.
    pub fn get_priority_chunk(&self) -> &BlockPriorityChunk {
        &self.priority_chunk
    }

    /// Java `@Nonnull HeightThresholdInterpolator getInterpolator()`.
    pub fn get_interpolator(&self) -> &HeightThresholdInterpolator {
        &self.interpolator
    }

    /// Java `ZoneBiomeResult zoneBiomeResult(int cx, int cz)`.
    pub fn zone_biome_result(&self, cx: i32, cz: i32) -> ZoneBiomeResult {
        self.interpolator.zone_biome_result(cx, cz)
    }

    /// Java `interpolator.getHeightNoise(int cx, int cz)`.
    pub fn get_height_noise(&self, cx: i32, cz: i32) -> f64 {
        self.interpolator.get_height_noise(cx, cz)
    }

    /// Java `interpolator.getHeightThreshold(int seed, int x, int z, int y)`.
    pub fn get_height_threshold(&self, seed: i32, x: i32, z: i32, y: i32) -> f32 {
        self.interpolator.get_height_threshold(seed, x, z, y)
    }

    /// Java `@Nonnull CoreDataCacheEntry[] getCoreDataEntries()`.
    pub fn get_core_data_entries(&self) -> &[Option<Rc<CoreDataCacheEntry>>] {
        self.interpolator.get_entries()
    }

    /// Java `long getIndex()`.
    pub fn get_index(&self) -> i64 {
        self.chunk.get_index()
    }

    /// Java `int getX()`.
    pub fn get_x(&self) -> i32 {
        self.chunk.get_x()
    }

    /// Java `int getZ()`.
    pub fn get_z(&self) -> i32 {
        self.chunk.get_z()
    }

    /// Java `void setPriorityModifier(BlockPriorityModifier
    /// blockPriorityModifier)`.
    pub fn set_priority_modifier(
        &mut self,
        block_priority_modifier: Rc<dyn BlockPriorityModifier>,
    ) {
        self.block_priority_modifier = block_priority_modifier;
    }

    /// Java `int getHeight(final int seed, final int worldX, final int
    /// worldZ)`.
    ///
    /// Deviation: the Java method returns the *raw* `height` field of the
    /// table entry, which is `CoreDataCacheEntry.NO_HEIGHT` (`-1`) until the
    /// height of that column has been computed. The Rust port keeps that
    /// behaviour (the value is not recomputed here).
    pub fn get_height(&self, seed: i32, world_x: i32, world_z: i32) -> i32 {
        if chunk_util::is_inside_chunk(self.get_x(), self.get_z(), world_x, world_z) {
            let cx = world_x & chunk_util::SIZE_MASK;
            let cz = world_z & chunk_util::SIZE_MASK;
            return self
                .interpolator
                .table_entry(cx, cz)
                .height
                .load(std::sync::atomic::Ordering::SeqCst);
        }
        self.chunk_generator.get_height(seed, world_x, world_z)
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                                  TINT MAPPING                                                 ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private void generateTintMapping(int seed)`.
    pub fn generate_tint_mapping(&mut self, seed: i32) {
        let radius = TINT_INTERPOLATION_RADIUS;
        let mut raw_tint = vec![
            0i32;
            ((chunk_util::SIZE + radius * 2) * (chunk_util::SIZE + radius * 2))
                as usize
        ];
        {
            // gather the raw tint color for each block column
            let m = chunk_util::SIZE + radius;
            for cx in -radius..m {
                for cz in -radius..m {
                    raw_tint[Self::tint_index_local(cx, cz) as usize] = self
                        .zone_biome_result(cx, cz)
                        .get_biome()
                        .expect("generateTintMapping: the column has no biome (Java: NPE)")
                        .get_tint_container()
                        .get_tint_color_at(seed, self.global_x(cx), self.global_z(cz));
                }
            }
        }

        // generate the average
        let radius2 = radius * radius;
        for cx in 0..chunk_util::SIZE {
            for cz in 0..chunk_util::SIZE {
                let mut r = 0i32;
                let mut g = 0i32;
                let mut b = 0i32;
                let mut counter = 0i32;
                for ix in -radius..=radius {
                    for iz in -radius..=radius {
                        if ix * ix + iz * iz <= radius2 {
                            let c = raw_tint[Self::tint_index_local(cx + ix, cz + iz) as usize];
                            r += (c >> 16) & 0xFF;
                            g += (c >> 8) & 0xFF;
                            b += c & 0xFF;
                            counter += 1;
                        }
                    }
                }
                if counter > 0 {
                    r /= counter;
                    g /= counter;
                    b /= counter;
                    let tint = 0xFF00_0000u32 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    self.chunk.set_tint(cx, cz, tint as i32);
                } else {
                    // use red as error color
                    self.chunk
                        .set_tint(cx, cz, 0xFF00_0000u32 as i32 | 0x00FF_0000);
                }
            }
        }
    }

    /// Java `public static int tintIndexLocal(int x, int z)`.
    pub fn tint_index_local(x: i32, z: i32) -> i32 {
        (x + TINT_INTERPOLATION_RADIUS) * (chunk_util::SIZE + TINT_INTERPOLATION_RADIUS * 2)
            + (z + TINT_INTERPOLATION_RADIUS)
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               ENVIRONMENT MAPPING                                             ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private void generateEnvironmentMapping(int seed)`.
    pub fn generate_environment_mapping(&mut self, seed: i32) {
        for cx in 0..chunk_util::SIZE {
            for cz in 0..chunk_util::SIZE {
                let env_id = self
                    .zone_biome_result(cx, cz)
                    .get_biome()
                    .expect("generateEnvironmentMapping: the column has no biome (Java: NPE)")
                    .get_environment_container()
                    .get_environment_at(seed, self.global_x(cx), self.global_z(cz));
                self.chunk.set_environment_column(cx, cz, env_id);
            }
        }
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                            BLOCK MANIPULATION                                                 ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `public int getBlock(int x, int y, int z)`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk.get_block(x, y, z)
    }

    /// Java `public int getRotationIndex(int x, int y, int z)`.
    pub fn get_rotation_index(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk.get_rotation_index(x, y, z)
    }

    /// Java `public void setEnvironment(int x, int y, int z, int environment)`.
    pub fn set_environment(&mut self, x: i32, y: i32, z: i32, environment: i32) {
        if environment == worldgen_model::chunk::ENVIRONMENT_NOT_SET {
            return;
        }
        self.chunk.set_environment(x, y, z, environment);
    }

    /// Java `public boolean setBlock(int x, int y, int z, byte type, int
    /// block)`.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, type_: i8, block: i32) -> bool {
        self.set_block_with(x, y, z, type_, block, -1, 0, 0)
    }

    /// Java `public boolean setBlock(int x, int y, int z, byte type,
    /// BlockFluidEntry entry)`.
    pub fn set_block_entry(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        entry: &BlockFluidEntry,
    ) -> bool {
        self.set_block_with(x, y, z, type_, entry.block_id, -1, entry.rotation, 0)
    }

    /// Java `public boolean setBlock(int x, int y, int z, byte type, int
    /// block, int environment)`.
    pub fn set_block_with_environment(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        block: i32,
        environment: i32,
    ) -> bool {
        if self.set_block(x, y, z, type_, block) {
            self.set_environment(x, y, z, environment);
            return true;
        }
        false
    }

    /// Java `public boolean setBlock(int x, int y, int z, byte type,
    /// BlockFluidEntry entry, int environment)`.
    pub fn set_block_entry_with_environment(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        entry: &BlockFluidEntry,
        environment: i32,
    ) -> bool {
        if self.set_block_entry(x, y, z, type_, entry) {
            self.set_environment(x, y, z, environment);
            return true;
        }
        false
    }

    /// Java `public boolean setBlock(int x, int y, int z, final byte type,
    /// int block, @Nullable Holder<ChunkStore> holder, int supportValue, int
    /// rotation, int filler)`.
    ///
    /// The Java `holder` (the ECS chunk store) and the block-physics calls
    /// are not ported (see the module docs) — the writes below are the
    /// observable part of the method. The `support_value` is only consumed by
    /// the (deferred) `BlockPhysics` handling, so it is unused here.
    #[allow(clippy::too_many_arguments)]
    pub fn set_block_with(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        block: i32,
        _support_value: i32,
        rotation: i32,
        filler: i32,
    ) -> bool {
        if !(chunk_util::MIN_Y..HEIGHT).contains(&y) {
            return false;
        }

        let mut new_priority = type_ & MASK;
        let new_flags = type_ & FLAG_MASK;

        let old_priority = self.priority_chunk.get(x, y, z);
        let old_modified = self
            .block_priority_modifier
            .modify_current(old_priority, new_priority);

        if type_ == NO_CHANGE {
            new_priority = old_modified;
        }

        if new_priority >= old_modified {
            new_priority = self
                .block_priority_modifier
                .modify_target(old_priority, new_priority)
                | new_flags;
            self.priority_chunk.set(x, y, z, new_priority);
            self.chunk.set_block(x, y, z, block, rotation, filler);
            // Java: `chunk.setState(x, y, z, holder)` — the ECS chunk store
            // is not ported (see `plan.md`).
            // Java: the `BlockPhysics` support handling needs the block type
            // registry and the chunk sections — not ported either.

            return true;
        }

        false
    }

    /// Java `public boolean setFluid(int x, int y, int z, byte type, int
    /// fluid)`.
    ///
    /// The fluid level defaults to `Fluid.getAssetMap().getAssetOrDefault
    /// (fluid, Fluid.EMPTY).getMaxFluidLevel()`.
    pub fn set_fluid_defaulted(&mut self, x: i32, y: i32, z: i32, type_: i8, fluid: i32) -> bool {
        let fluid_level = self.registry.fluids.get_max_fluid_level(fluid);
        self.set_fluid(x, y, z, type_, fluid, fluid_level)
    }

    /// Java `public boolean setFluid(int x, int y, int z, byte type, int
    /// fluid, int environment)`.
    ///
    /// ⚠ The Java overload takes the **environment**, not a fluid level — it
    /// delegates to the five-argument overload, so the fluid level is
    /// defaulted from the fluid registry.
    pub fn set_fluid_with_environment(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        fluid: i32,
        environment: i32,
    ) -> bool {
        if self.set_fluid_defaulted(x, y, z, type_, fluid) {
            self.set_environment(x, y, z, environment);
            return true;
        }
        false
    }

    /// Java `public boolean setFluid(int x, int y, int z, final byte type,
    /// int fluid, byte fluidLevel)`.
    pub fn set_fluid(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        fluid: i32,
        fluid_level: i8,
    ) -> bool {
        if !(chunk_util::MIN_Y..HEIGHT).contains(&y) {
            return false;
        }

        let mut new_priority = type_ & MASK;
        let new_flags = type_ & FLAG_MASK;

        let old_priority = self.priority_chunk.get(x, y, z);
        let old_modified = self
            .block_priority_modifier
            .modify_current(old_priority, new_priority);

        if type_ == NO_CHANGE {
            new_priority = old_modified;
        }

        if new_priority >= old_modified {
            new_priority = self
                .block_priority_modifier
                .modify_target(old_priority, new_priority)
                | new_flags;
            self.priority_chunk.set(x, y, z, new_priority);
            self.chunk.set_fluid(x, y, z, fluid, fluid_level);
            return true;
        }

        false
    }

    /// Java `public boolean setFluid(int x, int y, int z, byte type, int
    /// fluid, byte fluidLevel, int environment)`.
    ///
    /// ⚠ There is no such Java overload: the Java environment variant takes
    /// an `int environment` and *defaults* the fluid level (see
    /// [`ChunkGeneratorExecution::set_fluid_with_environment`]). This
    /// function is the explicit-level variant the Rust port keeps for
    /// callers that know the level.
    #[allow(clippy::too_many_arguments)]
    pub fn set_fluid_with_level_and_environment(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        fluid: i32,
        fluid_level: i8,
        environment: i32,
    ) -> bool {
        if self.set_fluid(x, y, z, type_, fluid, fluid_level) {
            self.set_environment(x, y, z, environment);
            return true;
        }
        false
    }

    /// Java `public int getFluid(int x, int y, int z)`.
    pub fn get_fluid(&self, x: i32, y: i32, z: i32) -> i32 {
        if !(chunk_util::MIN_Y..HEIGHT).contains(&y) {
            return FLUID_NOT_FOUND;
        }
        self.chunk.get_fluid_id(x, y, z)
    }

    /// Java `public void overrideBlock(int x, int y, int z, byte type, int
    /// block, Holder<ChunkStore> holder, int rotation, int filler)`.
    pub fn override_block(&mut self, x: i32, y: i32, z: i32, type_: i8, block: i32) {
        self.override_block_with(x, y, z, type_, block, 0, 0);
    }

    /// Java `public void overrideBlock(int x, int y, int z, byte type,
    /// BlockFluidEntry entry)`.
    pub fn override_block_entry(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        entry: &BlockFluidEntry,
    ) {
        self.override_block_with(x, y, z, type_, entry.block_id, entry.rotation, 0);
    }

    /// Java `public void overrideBlock(int x, int y, int z, byte type, int
    /// block, Holder<ChunkStore> holder, int rotation, int filler)`.
    #[allow(clippy::too_many_arguments)]
    pub fn override_block_with(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        block: i32,
        rotation: i32,
        filler: i32,
    ) {
        self.priority_chunk.set(x, y, z, type_);
        self.chunk.set_block(x, y, z, block, rotation, filler);
        // Java: `chunk.setState(x, y, z, holder)` and the `BlockPhysics`
        // handling — not ported (see the module docs).
    }

    /// Java `public void overrideFluid(int x, int y, int z, byte type, int
    /// fluid)`.
    pub fn override_fluid(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        type_: i8,
        fluid: i32,
        fluid_level: i8,
    ) {
        if !(chunk_util::MIN_Y..HEIGHT).contains(&y) {
            return;
        }

        self.priority_chunk.set(x, y, z, type_);
        self.chunk.set_fluid(x, y, z, fluid, fluid_level);
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                                     MISC                                                      ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `protected int localX(int x)`.
    pub fn local_x(&self, x: i32) -> i32 {
        x - self.get_x() * chunk_util::SIZE
    }

    /// Java `protected int localZ(int z)`.
    pub fn local_z(&self, z: i32) -> i32 {
        z - self.get_z() * chunk_util::SIZE
    }

    /// Java `public int globalX(int localX)`.
    pub fn global_x(&self, local_x: i32) -> i32 {
        chunk_util::min_block(self.get_x()) + local_x
    }

    /// Java `public int globalZ(int localZ)`.
    pub fn global_z(&self, local_z: i32) -> i32 {
        chunk_util::min_block(self.get_z()) + local_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::chunk::chunk_generator::test_stubs::{ConstantNoise, StubInterpreter};
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zone::zone_pattern_provider::ZonePatternProvider;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
    use worldgen_model::block_priority::{CAVE, FILLING, FLAG_SUBMERGE, LAYER, PREFAB};

    struct FixedPointGenerator;

    impl procedural::noise::point::IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            ResultBuffer2d::new()
        }

        fn nearest_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_2d(&self, _s: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn collect(
            &self,
            _seed: i32,
            _min_x: f64,
            _min_y: f64,
            _max_x: f64,
            _max_y: f64,
            _consumer: &mut dyn FnMut(f64, f64),
        ) {
            unimplemented!("not exercised by these tests")
        }

        fn get_interval(&self) -> f64 {
            unimplemented!("not exercised by these tests")
        }
    }

    fn tint_container(color: i32) -> Rc<TintContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(color, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(TintContainer::new(
            DefaultTintContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    fn environment_container(environment: i32) -> Rc<EnvironmentContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(environment, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(EnvironmentContainer::new(
            DefaultEnvironmentContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    fn fade_container() -> Rc<FadeContainer> {
        Rc::new(FadeContainer::new(
            0.0,
            1.0,
            0.0,
            1.0,
            FadeContainer::NO_FADE_HEIGHTMAP,
        ))
    }

    fn tile_biome(id: i32, tint: i32, environment: i32) -> Rc<TileBiome> {
        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint_container(tint),
            environment_container(environment),
            Rc::new(WaterContainer::new(Vec::new())),
            fade_container(),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ))
    }

    fn generator(tint: i32, environment: i32) -> Rc<ChunkGenerator> {
        generator_for_biome(tile_biome(7, tint, environment))
    }

    /// A world whose every column resolves to `biome`.
    fn generator_for_biome(biome: Rc<TileBiome>) -> Rc<ChunkGenerator> {
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(Rc::clone(&biome), 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
        let zone = Rc::new(Zone {
            id: 3,
            name: "zone-3".to_string(),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            cave_generator: None,
            unique_prefab_container: Rc::new(
                crate::container::unique_prefab_container::UniquePrefabContainer::new(
                    0,
                    Vec::new(),
                ),
            ),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                Rc::new(FixedPointGenerator),
                tile_biomes,
                Vec::new(),
            )),
        });
        let mut mapping = ZoneColorMapping::new();
        mapping.add(0x00FF00, Rc::clone(&zone));
        let provider = ZonePatternProvider::new(
            Rc::new(FixedPointGenerator),
            vec![zone],
            Vec::new(),
            crate::chunk::mask_provider::MaskProvider::new(FuzzyZoom::new(
                empty_randomizer(),
                PixelProvider::from_pixels(4, 4, vec![0x00FF00; 16]),
                1.0,
                1.0,
                0,
                0,
            )),
            mapping,
        );
        ChunkGenerator::new(provider, None)
    }

    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    #[test]
    fn the_tint_index_is_the_padded_column_index() {
        // Java: `(x + 4) * 40 + (z + 4)`.
        assert_eq!(ChunkGeneratorExecution::tint_index_local(0, 0), 4 * 40 + 4);
        assert_eq!(ChunkGeneratorExecution::tint_index_local(-4, -4), 0);
        assert_eq!(
            ChunkGeneratorExecution::tint_index_local(35, 35),
            39 * 40 + 39
        );
    }

    #[test]
    fn the_tint_map_is_the_average_of_the_biome_tint() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        execution.generate_tint_mapping(42);

        // Every column holds the same tint, so the average is that tint.
        assert_eq!(execution.get_chunk().get_tint(0, 0), 0xFF11_2233u32 as i32);
        assert_eq!(
            execution.get_chunk().get_tint(31, 31),
            0xFF11_2233u32 as i32
        );
    }

    #[test]
    fn the_environment_map_is_the_biome_environment() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        execution.generate_environment_mapping(42);

        assert_eq!(execution.get_chunk().get_environment(0, 0, 0), 5);
        assert_eq!(execution.get_chunk().get_environment(31, 319, 31), 5);
    }

    #[test]
    fn the_block_priority_arbitration_follows_the_java_rules() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        // A filling block is written ...
        assert!(execution.set_block(1, 10, 1, FILLING, 42));
        assert_eq!(execution.get_block(1, 10, 1), 42);
        assert_eq!(execution.get_priority_chunk().get(1, 10, 1), FILLING);

        // ... a lower priority is rejected ...
        assert!(!execution.set_block(1, 10, 1, worldgen_model::block_priority::NONE, 43));
        assert_eq!(execution.get_block(1, 10, 1), 42);

        // ... a higher priority wins ...
        assert!(execution.set_block(1, 10, 1, LAYER, 44));
        assert_eq!(execution.get_block(1, 10, 1), 44);
        assert_eq!(execution.get_priority_chunk().get(1, 10, 1), LAYER);

        // ... the flag bits are kept in the raw value ...
        assert!(execution.set_block(2, 10, 2, PREFAB | FLAG_SUBMERGE, 45));
        assert_eq!(execution.get_priority_chunk().get(2, 10, 2), PREFAB);
        assert_eq!(
            execution.get_priority_chunk().get_raw(2, 10, 2),
            PREFAB | FLAG_SUBMERGE
        );

        // ... and a write outside the world height is rejected.
        assert!(!execution.set_block(1, HEIGHT, 1, PREFAB, 46));
        assert!(!execution.set_block(1, -1, 1, PREFAB, 46));
    }

    #[test]
    fn the_no_change_priority_keeps_the_current_one() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        assert!(execution.set_block(1, 10, 1, CAVE, 42));
        // `NO_CHANGE` (-1) means "keep the current priority" — the write is
        // accepted and the block is replaced.
        assert!(execution.set_block(1, 10, 1, worldgen_model::block_priority::NO_CHANGE, 43));
        assert_eq!(execution.get_block(1, 10, 1), 43);
        assert_eq!(execution.get_priority_chunk().get(1, 10, 1), CAVE);
    }

    #[test]
    fn the_fluid_write_follows_the_same_arbitration() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        assert!(execution.set_fluid(1, 10, 1, worldgen_model::block_priority::WATER, 7, 8));
        assert_eq!(execution.get_fluid(1, 10, 1), 7);
        assert!(!execution.set_fluid(1, 10, 1, worldgen_model::block_priority::FILLING, 9, 4));
        assert_eq!(execution.get_fluid(1, 10, 1), 7);
        // Outside the world height the fluid lookup reports "not found".
        assert_eq!(execution.get_fluid(1, HEIGHT, 1), FLUID_NOT_FOUND);
    }

    #[test]
    fn the_override_writes_bypass_the_arbitration() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        assert!(execution.set_block(1, 10, 1, PREFAB, 42));
        // An override replaces the block and the priority unconditionally.
        execution.override_block(1, 10, 1, worldgen_model::block_priority::NONE, 43);
        assert_eq!(execution.get_block(1, 10, 1), 43);
        assert_eq!(
            execution.get_priority_chunk().get(1, 10, 1),
            worldgen_model::block_priority::NONE
        );
    }

    #[test]
    fn the_environment_write_ignores_the_not_set_marker() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);

        execution.set_environment(1, 10, 1, worldgen_model::chunk::ENVIRONMENT_NOT_SET);
        assert_ne!(
            execution.get_chunk().get_environment(1, 10, 1),
            worldgen_model::chunk::ENVIRONMENT_NOT_SET
        );
        execution.set_environment(1, 10, 1, 9);
        assert_eq!(execution.get_chunk().get_environment(1, 10, 1), 9);
    }

    #[test]
    fn the_local_and_global_coordinates_match_the_chunk() {
        let generator = generator(0x112233, 5);
        let chunk = GeneratedChunk::with_coordinates(0, 2, -1);
        let execution = ChunkGeneratorExecution::new(42, generator, chunk);

        assert_eq!(execution.local_x(70), 70 - 2 * 32);
        assert_eq!(execution.local_z(-40), -40 - (-32));
        assert_eq!(execution.global_x(5), 2 * 32 + 5);
        assert_eq!(execution.global_z(5), -32 + 5);
    }

    #[test]
    fn the_execute_pipeline_fills_the_tint_the_environment_and_the_terrain() {
        // A biome whose filling block is a real block (42) so that the
        // terrain is observable.
        let biome = Rc::new(TileBiome::new(
            7,
            "biome-7".to_string(),
            Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(42, 0, Vec::new(), Vec::new())),
            None,
            tint_container(0x112233),
            environment_container(5),
            Rc::new(WaterContainer::new(Vec::new())),
            fade_container(),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ));
        let generator = generator_for_biome(biome);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, generator, chunk);
        let mut resource = ChunkGeneratorResource::new();

        execution.execute(42, &mut resource);

        // The tint map.
        assert_eq!(execution.get_chunk().get_tint(0, 0), 0xFF11_2233u32 as i32);
        // The environment map. (The per-block environment of the filled
        // blocks is the *filling* environment of the layer container, which
        // overrides the column environment — exactly like the Java
        // `setBlock(…, filling, fillingEnvironment)`.)
        assert_eq!(execution.get_chunk().environment_column()[0], 5);
        assert_eq!(execution.get_chunk().get_environment(0, 0, 0), 0);
        // The terrain (the stub threshold is 0.75, the height noise is 0.5).
        assert_eq!(execution.get_block(0, 0, 0), 42);
        assert_eq!(execution.get_block(0, 63, 0), 42);
        assert_eq!(execution.get_block(0, 64, 0), 0);
        assert_eq!(
            execution.get_priority_chunk().get(0, 10, 0),
            worldgen_model::block_priority::FILLING
        );
    }
}
