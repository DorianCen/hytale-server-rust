// Ported from: `.hytale-source/.../worldgen/chunk/populator/BlockPopulator.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.populator.BlockPopulator` —
//! the terrain filling, the static/dynamic block layers and the covers.
//!
//! The Java class holds a private nested `LayerPopulator` class; the Rust
//! port mirrors it with the private [`layer_populator`] module.
//!
//! # Deviations
//!
//! * **The scratch list is an explicit parameter.** Java reaches the reused
//!   `IntList` through the static `ChunkGenerator.getResource().coverArray`;
//!   the Rust port passes the [`ChunkGeneratorResource`] down explicitly
//!   (`populate(seed, execution, resource)`), following the plan's
//!   "explicit `&mut` scratch per task" model.
//! * **The dead `ConstantIntCondition` comparisons are not ported** — see
//!   [`is_matching_parent_cover`].
//! * **The `random` is a [`FastRandom`]** (Java uses the `java.util.Random`
//!   type but always instantiates a `FastRandom`).
//!
//! # Java quirks that are transcribed *on purpose*
//!
//! 1. The single `FastRandom` is created **once per chunk** and shared by
//!    all 1024 columns (the column loop is `cx` outer, `cz` inner), so the
//!    draw order of the whole chunk matters.
//! 2. The *conditional* part of the terrain walks **downwards** from
//!    `getHighestNonZero` to `getLowestNonOne` (inclusive) and records the
//!    **first** hit of every "empty stretch" in the surface list.
//! 3. The *guaranteed* part adds the **current** `y` (which is
//!    `min - 1` after the conditional loop) to the surface list when no hit
//!    was recorded, and then fills the column down to `y = 0`.
//! 4. The static-layer loop `return`s from the **whole method** when
//!    `layerY <= min` — every remaining static layer is skipped.
//! 5. The cover draw order is parity-critical (see [`generate_covers`]).
use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;

use worldgen_model::block_priority::{COVER, FILLING, HEIGHT, LAYER};
use worldgen_model::chunk::ENVIRONMENT_NOT_SET;

use crate::biome::biome::Biome;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::chunk::chunk_generator_resource::ChunkGeneratorResource;
use crate::container::cover_container::CoverContainerEntry;
use crate::container::layer_container::LayerEntry;
use crate::util::block_fluid_entry::BlockFluidEntry;

/// Java `BlockType.EMPTY_ID` = `0` — air.
const BLOCK_TYPE_EMPTY_ID: i32 = 0;

/// Java `Fluid.EMPTY_ID` = `0` — no fluid.
const FLUID_EMPTY_ID: i32 = 0;

/// Java `com.hypixel.hytale.server.worldgen.chunk.populator.BlockPopulator`.
pub struct BlockPopulator;

impl BlockPopulator {
    /// Java `public static void populate(int seed, @Nonnull
    /// ChunkGeneratorExecution execution)`.
    ///
    /// Deviation: the Java method reads the reused surface scratch list from
    /// the static `ChunkGenerator.getResource()`; the Rust port takes the
    /// [`ChunkGeneratorResource`] as an explicit parameter.
    ///
    /// ⚠ The `FastRandom` is created **once** and shared by every column —
    /// the draw order of the whole chunk depends on it.
    pub fn populate(
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        resource: &mut ChunkGeneratorResource,
    ) {
        // Java: `new FastRandom(HashUtil.hash(seed, execution.getX(),
        // execution.getZ(), 0x4e5facef_afd03aceL))`.
        let mut random = FastRandom::new(hash_util::hash4(
            seed as i64,
            execution.get_x() as i64,
            execution.get_z() as i64,
            0x4e5facef_afd03aceu64 as i64,
        ));

        for cx in 0..chunk_util::SIZE {
            for cz in 0..chunk_util::SIZE {
                generate_block_column(seed, execution, cx, cz, &mut random, resource);
            }
        }
    }
}

/// Java `private static void generateBlockColumn(int seed, @Nonnull
/// ChunkGeneratorExecution execution, int cx, int cz, @Nonnull Random random)`.
fn generate_block_column(
    seed: i32,
    execution: &mut ChunkGeneratorExecution,
    cx: i32,
    cz: i32,
    random: &mut FastRandom,
    resource: &mut ChunkGeneratorResource,
) {
    // Java: `final IntList surfaceBlockList = ChunkGenerator.getResource()
    // .coverArray` — the reused scratch list.
    let surface_block_list = &mut resource.cover_array;

    let biome: Rc<dyn Biome> = execution
        .zone_biome_result(cx, cz)
        .get_biome()
        .cloned()
        .expect("generateBlockColumn: the column has no biome (Java: NullPointerException)");
    let layer_container = biome.get_layer_container();
    let x = execution.global_x(cx);
    let z = execution.global_z(cz);
    let heightmap_noise = execution.get_height_noise(cx, cz);

    let filling = layer_container.get_filling();
    let filling_environment = layer_container.get_filling_environment();
    let mut highest = 0i32;
    let min = execution.get_interpolator().get_lowest_non_one(cx, cz);
    let mut y = execution.get_interpolator().get_highest_non_zero(cx, cz);
    let mut empty = true;
    {
        // this part of terrain is not guaranteed to appear and depends on thresholds
        while y >= min {
            let threshold = execution.get_height_threshold(seed, x, z, y);
            // Java: `threshold > heightmapNoise` widens the float to a double;
            // `threshold == 1.0` is an exact float comparison.
            if (threshold as f64) > heightmap_noise || threshold == 1.0 {
                if y > highest {
                    highest = y;
                }
                execution.set_block_entry_with_environment(
                    cx,
                    y,
                    cz,
                    FILLING,
                    &filling,
                    filling_environment,
                );
                if empty {
                    surface_block_list.push(y);
                    empty = false;
                }
            } else {
                empty = true;
            }
            y = y.wrapping_sub(1);
        }
    }
    {
        // this part is guaranteed to appear
        // ⚠ Java quirk: the *current* `y` is added — after the loop above it
        // is `min - 1` (or the untouched `getHighestNonZero` when the loop
        // never ran).
        if empty {
            surface_block_list.push(y);
        }
        if y > highest {
            highest = y;
        }
        while y >= 0 {
            execution.set_block_entry(cx, y, cz, FILLING, &filling);
            y = y.wrapping_sub(1);
        }
    }

    execution
        .get_chunk_generator()
        .put_height(seed, x, z, highest);

    layer_populator::generate_layers(seed, execution, cx, cz, x, z, &biome, surface_block_list);
    generate_covers(
        seed,
        execution,
        cx,
        cz,
        x,
        z,
        random,
        &biome,
        surface_block_list,
    );
    surface_block_list.clear();
}

/// Java `private static void generateCovers(int seed, @Nonnull
/// ChunkGeneratorExecution execution, int cx, int cz, int x, int z, @Nonnull
/// Random random, @Nonnull Biome biome, @Nonnull IntList surfaceBlockList)`.
#[allow(clippy::too_many_arguments)]
fn generate_covers(
    seed: i32,
    execution: &mut ChunkGeneratorExecution,
    cx: i32,
    cz: i32,
    x: i32,
    z: i32,
    random: &mut FastRandom,
    biome: &Rc<dyn Biome>,
    surface_block_list: &[i32],
) {
    // Covers like grass / flowers and little rocks
    let cover_container = biome.get_cover_container();

    let size = surface_block_list.len();

    // check if we have surface blocks. No would me we have no generated surface and therefore no need to generate covers
    if size == 0 {
        return;
    }

    {
        // Covers on Water
        let water_container = biome.get_water_container();
        for water_container in water_container.get_entries() {
            for cover_container_entry in cover_container.get_entries() {
                if !cover_container_entry.is_on_water() {
                    continue; // we don't care about normal covers
                }
                if !is_matching_cover_column(seed, cover_container_entry, random, x, z) {
                    continue;
                }

                // add 1 as getMax is inclusive for water
                let y = water_container.get_max(seed, x, z).wrapping_add(1);
                if !is_matching_cover_height(seed, cover_container_entry, random, x, y, z) {
                    continue;
                }
                if !is_matching_parent_cover(
                    execution,
                    cover_container_entry,
                    cx,
                    cz,
                    y,
                    water_container.get_block(),
                    water_container.get_fluid(),
                ) {
                    continue;
                }

                let cover_entry = cover_container_entry.get(random).expect(
                    "generateCovers: the cover entry has no part (Java: NullPointerException)",
                );
                execution.set_block_entry(
                    cx,
                    y.wrapping_add(cover_entry.get_offset()),
                    cz,
                    COVER,
                    &cover_entry.get_entry(),
                );
                execution.set_fluid_defaulted(
                    cx,
                    y.wrapping_add(cover_entry.get_offset()),
                    cz,
                    COVER,
                    cover_entry.get_entry().fluid_id,
                );
            }
        }
    }

    // Covers on the layers
    for &surface_y in surface_block_list.iter().take(size) {
        let y = surface_y.wrapping_add(1);
        for cover_container_entry in cover_container.get_entries() {
            // ⚠ Java quirk: the evaluation order of the `&&` chain is
            // parity-critical — the parent check (no draws) comes **first**,
            // then the column check (one draw) and finally the height check
            // (which may draw as well).
            if !cover_container_entry.is_on_water()
                && is_matching_parent_cover(
                    execution,
                    cover_container_entry,
                    cx,
                    cz,
                    y,
                    BLOCK_TYPE_EMPTY_ID,
                    FLUID_EMPTY_ID,
                )
                && is_matching_cover_column(seed, cover_container_entry, random, x, z)
                && is_matching_cover_height(seed, cover_container_entry, random, x, y, z)
            {
                let cover_entry = cover_container_entry.get(random).expect(
                    "generateCovers: the cover entry has no part (Java: NullPointerException)",
                );
                execution.set_block_entry(
                    cx,
                    y.wrapping_add(cover_entry.get_offset()),
                    cz,
                    COVER,
                    &cover_entry.get_entry(),
                );
                execution.set_fluid_defaulted(
                    cx,
                    y.wrapping_add(cover_entry.get_offset()),
                    cz,
                    COVER,
                    cover_entry.get_entry().fluid_id,
                );
            }
        }
    }
}

/// Java `private static boolean isMatchingParentCover(@Nonnull
/// ChunkGeneratorExecution execution, @Nonnull CoverContainer
/// .CoverContainerEntry coverContainerEntry, int cx, int cz, int y, int
/// defaultId, int defaultFluidId)`.
fn is_matching_parent_cover(
    execution: &ChunkGeneratorExecution,
    cover_container_entry: &CoverContainerEntry,
    cx: i32,
    cz: i32,
    y: i32,
    default_id: i32,
    default_fluid_id: i32,
) -> bool {
    if y <= chunk_util::MIN_Y || y >= HEIGHT {
        return false;
    }

    let parent_condition = cover_container_entry.get_parent_condition();

    // ⚠ The Java method has two more checks:
    //
    // ```java
    // if (parentCondition == ConstantIntCondition.DEFAULT_TRUE) return true;
    // if (parentCondition == ConstantIntCondition.DEFAULT_FALSE) return false;
    // ```
    //
    // They are **dead code**: `parentCondition` is declared as an
    // `IBlockFluidCondition` and `ConstantIntCondition` implements
    // `IIntCondition`, so a `ConstantIntCondition` can never be stored in
    // that field — the `==` comparisons can never be true. The checks are
    // therefore not ported.

    let mut block = execution.get_chunk().get_block(cx, y.wrapping_sub(1), cz);
    // Use defaultId if block has not been set (water covers provide the id of the block that will be placed in the empty space during water population)
    if block == BLOCK_TYPE_EMPTY_ID {
        block = default_id;
    }
    let mut fluid = execution.get_fluid(cx, y.wrapping_sub(1), cz);
    if fluid == FLUID_EMPTY_ID {
        fluid = default_fluid_id;
    }

    parent_condition.eval(block, fluid)
}

/// Java `private static boolean isMatchingCoverColumn(int seed, @Nonnull
/// CoverContainer.CoverContainerEntry coverContainerEntry, @Nonnull Random
/// random, int x, int z)`.
///
/// ⚠ The random draw happens **first** — the map condition is only evaluated
/// when the density roll passed.
fn is_matching_cover_column(
    seed: i32,
    cover_container_entry: &CoverContainerEntry,
    random: &mut FastRandom,
    x: i32,
    z: i32,
) -> bool {
    random.next_double() < cover_container_entry.get_cover_density()
        && cover_container_entry.get_map_condition().eval2(seed, x, z)
}

/// Java `private static boolean isMatchingCoverHeight(int seed, @Nonnull
/// CoverContainer.CoverContainerEntry coverContainerEntry, Random random, int x,
/// int y, int z)`.
fn is_matching_cover_height(
    seed: i32,
    cover_container_entry: &CoverContainerEntry,
    random: &mut FastRandom,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    cover_container_entry
        .get_height_condition()
        .eval(seed, x, z, y, random)
}

/// Java `private static class BlockPopulator.LayerPopulator`.
mod layer_populator {
    use super::*;

    /// Java `static void generateLayers(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, int cx, int cz, int x, int z, @Nonnull
    /// Biome biome, @Nonnull IntList surfaceBlockList)`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn generate_layers(
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cx: i32,
        cz: i32,
        x: i32,
        z: i32,
        biome: &Rc<dyn Biome>,
        surface_block_list: &mut [i32],
    ) {
        generate_static_layers(seed, execution, cx, cz, x, z, biome);
        generate_dynamic_layers(seed, execution, cx, cz, x, z, biome, surface_block_list);
    }

    /// Java `private static void generateDynamicLayers(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, int cx, int cz, int x, int z, @Nonnull
    /// Biome biome, @Nonnull IntList surfaceBlockList)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_dynamic_layers(
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cx: i32,
        cz: i32,
        x: i32,
        z: i32,
        biome: &Rc<dyn Biome>,
        surface_block_list: &mut [i32],
    ) {
        let layers = biome.get_layer_container();
        for slot in surface_block_list.iter_mut() {
            let surface_y = *slot;
            let mut y = surface_y;
            let mut max_y = surface_y;
            'layer_loop: for layer in layers.get_dynamic_layers() {
                let entry = match layer.get_active_entry(seed, x, z) {
                    Some(entry) => entry,
                    None => continue,
                };

                let environment_id = layer.get_environment_id();
                y = y.wrapping_add(layer.get_offset(seed, x, z));
                max_y = max_y.max(y);
                let block_array = entry.get_block_array();
                for block_array_entry in block_array.get_entries() {
                    let repetitions = block_array_entry.get_repetitions(seed, x as f64, z as f64);
                    for _ in 0..repetitions {
                        if y > surface_y || execution.get_block(cx, y, cz) != 0 {
                            let block = block_array_entry.get_block_entry();
                            execution.set_block_entry_with_environment(
                                cx,
                                y,
                                cz,
                                LAYER,
                                &block,
                                environment_id,
                            );
                            execution.set_fluid_with_environment(
                                cx,
                                y,
                                cz,
                                LAYER,
                                block.fluid_id,
                                environment_id,
                            );
                            y = y.wrapping_sub(1);
                        } else {
                            // Java: `break layerLoop;` — the labelled break
                            // leaves the **whole** layer loop.
                            break 'layer_loop;
                        }
                    }
                }
            }
            if max_y > surface_y {
                *slot = max_y;
            }
        }
    }

    /// Java `private static void generateStaticLayers(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, int cx, int cz, int x, int z, @Nonnull
    /// Biome biome)`.
    fn generate_static_layers(
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cx: i32,
        cz: i32,
        x: i32,
        z: i32,
        biome: &Rc<dyn Biome>,
    ) {
        let layers = biome.get_layer_container();
        for layer in layers.get_static_layers() {
            let entry = match layer.get_active_entry(seed, x, z) {
                Some(entry) => entry,
                None => continue,
            };

            let environment_id = layer.get_environment_id();
            let block_entries = entry.get_block_array().get_entries();
            let min = entry.get_min_int(seed, x, z).max(0); // Inclusive [min,
            let max = entry.get_max_int(seed, x, z).min(HEIGHT); // Exclusive ,max)

            // ⚠ Java calls `getMaxInt` twice: the clamped value above and the
            // *unclamped* start of the layer walk.
            let mut layer_y = entry.get_max_int(seed, x, z);
            let mut last_block: Option<BlockFluidEntry> = None;
            for block_entry in block_entries {
                let repetitions = block_entry.get_repetitions(seed, x as f64, z as f64);
                if repetitions > 0 {
                    let block = block_entry.get_block_entry();
                    last_block = Some(block);
                    for _ in 0..repetitions {
                        // ⚠ The pre-decrement happens inside the call, i.e. for
                        // **every** repetition.
                        layer_y = layer_y.wrapping_sub(1);
                        let current_block = execution.get_block(cx, layer_y, cz);
                        if current_block != 0 {
                            execution.set_block_entry_with_environment(
                                cx,
                                layer_y,
                                cz,
                                LAYER,
                                &block,
                                environment_id,
                            );
                            execution.set_fluid_with_environment(
                                cx,
                                layer_y,
                                cz,
                                LAYER,
                                block.fluid_id,
                                environment_id,
                            );
                        }
                        if layer_y <= min {
                            // ⚠ Java quirk: this `return` leaves the **whole**
                            // `generateStaticLayers` method — every remaining
                            // static layer is skipped.
                            return;
                        }
                    }
                }
            }

            // Layer is configured to only set/override the environment, not blocks.
            if block_entries.is_empty() && environment_id != ENVIRONMENT_NOT_SET {
                let mut y = max - 1;
                while y >= min {
                    execution.set_environment(cx, y, cz, environment_id);
                    y = y.wrapping_sub(1);
                }
            }

            let last_block = match last_block {
                Some(last_block) => last_block,
                None => continue, // continue to avoid nesting
            };
            while layer_y > min {
                layer_y = layer_y.wrapping_sub(1);
                let current_block = execution.get_block(cx, layer_y, cz);
                if current_block != 0 {
                    execution.set_block_entry(cx, layer_y, cz, LAYER, &last_block);
                    execution.set_fluid_defaulted(cx, layer_y, cz, LAYER, last_block.fluid_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use hytale_math::fast_random::FastRandom;
    use hytale_math::hash_util;

    use procedural::noise::condition::{
        DefaultCoordinateCondition, IBlockFluidCondition, ICoordinateCondition,
        ICoordinateRndCondition,
    };
    use procedural::noise::supplier::{DoubleRange, IDoubleCoordinateSupplier};

    use worldgen_model::block_priority::{COVER, FILLING as FILLING_PRIORITY};
    use worldgen_model::chunk::GeneratedChunk;

    use crate::chunk::populator::test_stubs::{generator_for_biome, tile_biome};
    use crate::container::cover_container::{
        CoverContainer, CoverContainerEntry, CoverContainerEntryPart,
    };
    use crate::container::layer_container::{
        DynamicLayer, DynamicLayerEntry, LayerContainer, StaticLayer, StaticLayerEntry,
    };
    use crate::container::water_container::WaterContainer;
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use crate::util::noise_block_array::{Entry as NoiseBlockArrayEntry, NoiseBlockArray};
    use crate::weighted_map::{IWeightedMap, WeightedMap};

    use super::*;

    /// The filling block id used by the test biomes.
    const FILLING: i32 = 42;

    /// The seed the populator tests use.
    const SEED: i32 = 42;

    /// The Java `HashUtil.hash(seed, chunkX, chunkZ, 0x4e5facef_afd03aceL)`
    /// seed of the populator's `FastRandom`.
    fn populator_random_seed(seed: i32, chunk_x: i32, chunk_z: i32) -> i64 {
        hash_util::hash4(
            seed as i64,
            chunk_x as i64,
            chunk_z as i64,
            0x4e5facef_afd03aceu64 as i64,
        )
    }

    /// A constant double supplier (the min/max/offset suppliers of the
    /// layers).
    struct ConstantSupplier(f64);

    impl IDoubleCoordinateSupplier for ConstantSupplier {
        fn get2(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get3(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    /// A map condition that records the order in which it is called.
    #[derive(Clone)]
    struct RecordingMapCondition {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl ICoordinateCondition for RecordingMapCondition {
        fn eval2(&self, _seed: i32, _x: i32, _y: i32) -> bool {
            self.log.borrow_mut().push("map");
            true
        }
        fn eval3(&self, _seed: i32, _x: i32, _y: i32, _z: i32) -> bool {
            self.eval2(0, 0, 0)
        }
    }

    /// A height condition that records the call order and consumes exactly one
    /// random draw.
    #[derive(Clone)]
    struct RecordingHeightCondition {
        log: Rc<RefCell<Vec<&'static str>>>,
        draws: Rc<RefCell<Vec<f64>>>,
    }

    impl ICoordinateRndCondition for RecordingHeightCondition {
        fn eval(&self, _seed: i32, _x: i32, _z: i32, _y: i32, random: &mut FastRandom) -> bool {
            self.log.borrow_mut().push("height");
            self.draws.borrow_mut().push(random.next_double());
            true
        }
    }

    /// A parent condition that records the call order.
    #[derive(Clone)]
    struct RecordingParentCondition {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl IBlockFluidCondition for RecordingParentCondition {
        fn eval(&self, _block: i32, _fluid: i32) -> bool {
            self.log.borrow_mut().push("parent");
            true
        }
    }

    /// A block array with `repetitions` copies of `block_id`.
    fn block_array(block_id: i32, repetitions: i32) -> Rc<NoiseBlockArray> {
        Rc::new(NoiseBlockArray::new(vec![NoiseBlockArrayEntry::new(
            "stone".to_string(),
            BlockFluidEntry::new(block_id, 0, 0),
            Rc::new(DoubleRange::constant(repetitions as f64)),
            crate::util::constant_noise_property::default_zero(),
        )]))
    }

    /// A layer container with a single static layer.
    fn static_layer_container(
        filling: i32,
        entries: Vec<Rc<StaticLayerEntry>>,
        environment_id: i32,
    ) -> Rc<LayerContainer> {
        let layer = Rc::new(StaticLayer::new(
            entries,
            Rc::new(DefaultCoordinateCondition::True),
            environment_id,
        ));
        Rc::new(LayerContainer::new(filling, 0, vec![layer], Vec::new()))
    }

    /// A layer container with the given dynamic layers.
    fn dynamic_layer_container(filling: i32, layers: Vec<Rc<DynamicLayer>>) -> Rc<LayerContainer> {
        Rc::new(LayerContainer::new(filling, 0, Vec::new(), layers))
    }

    /// A layer container without any layer.
    fn layer_container(filling: i32) -> Rc<LayerContainer> {
        Rc::new(LayerContainer::new(filling, 0, Vec::new(), Vec::new()))
    }

    /// A cover entry with a single (deterministic) cover part.
    fn cover_entry(
        log: &Rc<RefCell<Vec<&'static str>>>,
        draws: &Rc<RefCell<Vec<f64>>>,
        density: f64,
        on_water: bool,
    ) -> CoverContainerEntry {
        CoverContainerEntry::new(
            single_part_map(),
            Rc::new(RecordingMapCondition {
                log: Rc::clone(log),
            }),
            Rc::new(RecordingHeightCondition {
                log: Rc::clone(log),
                draws: Rc::clone(draws),
            }),
            Rc::new(RecordingParentCondition {
                log: Rc::clone(log),
            }),
            density,
            on_water,
        )
    }

    /// A weighted map with a single part — a singleton map does **not**
    /// consume a random draw (Java `SingletonWeightedMap.get(Random)`).
    fn single_part_map() -> Rc<dyn IWeightedMap<CoverContainerEntryPart>> {
        let mut builder = WeightedMap::<CoverContainerEntryPart>::builder();
        builder.put(
            CoverContainerEntryPart::new(BlockFluidEntry::new(7, 0, 0), 0),
            1.0,
        );
        Rc::new(builder.build())
    }

    /// An execution whose every column resolves to `biome`.
    fn execution_for(biome: Rc<crate::biome::tile_biome::TileBiome>) -> ChunkGeneratorExecution {
        let generator = generator_for_biome(biome);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        ChunkGeneratorExecution::new(SEED, generator, chunk)
    }

    #[test]
    fn the_column_is_filled_up_to_the_threshold() {
        let biome = tile_biome(
            layer_container(FILLING),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The stub biome has a constant threshold of 0.75 and a height noise
        // of 0.5, so the whole `[0, 63]` band is filled.
        for y in 0..=63 {
            assert_eq!(execution.get_block(0, y, 0), FILLING, "y={y}");
        }
        assert_eq!(execution.get_block(0, 64, 0), 0);
        // The priority of the fill is `FILLING`.
        assert_eq!(
            execution.get_priority_chunk().get(0, 10, 0),
            FILLING_PRIORITY
        );
        // The scratch list is cleared after every column.
        assert!(resource.cover_array.is_empty());
    }

    #[test]
    fn the_cover_is_placed_above_the_first_surface_block() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let draws = Rc::new(RefCell::new(Vec::new()));
        let cover = cover_entry(&log, &draws, 1.0, false);
        let biome = tile_biome(
            layer_container(FILLING),
            Rc::new(CoverContainer::new(vec![cover])),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The surface list holds the first hit of the column (63), so the
        // cover is placed at `63 + 1` = 64.
        assert_eq!(execution.get_block(0, 64, 0), 7);
        assert_eq!(execution.get_priority_chunk().get(0, 64, 0), COVER);
        assert_eq!(execution.get_block(0, 63, 0), FILLING);
    }

    #[test]
    fn the_static_layer_environment_only_branch_sets_the_environment() {
        // A static layer without any block entry but with an environment id.
        let entry = Rc::new(StaticLayerEntry::new(
            Rc::new(NoiseBlockArray::empty()),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantSupplier(0.0)),
            Rc::new(ConstantSupplier(10.0)),
        ));
        let container = static_layer_container(FILLING, vec![entry], 9);
        let biome = tile_biome(
            container,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The environment is set for `max - 1 ..= min` = `9 ..= 0`.
        assert_eq!(execution.get_chunk().get_environment(0, 0, 0), 9);
        assert_eq!(execution.get_chunk().get_environment(0, 9, 0), 9);
        // `y = 10` is outside the `[min, max)` band.
        assert_eq!(execution.get_chunk().get_environment(0, 10, 0), 0);
        // No block was written by the environment-only layer.
        assert_eq!(execution.get_block(0, 5, 0), FILLING);
    }

    #[test]
    fn the_static_layer_environment_only_branch_is_skipped_without_an_environment() {
        // `environmentId == WorldGenConstants.ENVIRONMENT_NOT_SET` → the band
        // is not touched at all.
        let entry = Rc::new(StaticLayerEntry::new(
            Rc::new(NoiseBlockArray::empty()),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantSupplier(0.0)),
            Rc::new(ConstantSupplier(10.0)),
        ));
        let container = static_layer_container(FILLING, vec![entry], ENVIRONMENT_NOT_SET);
        let biome = tile_biome(
            container,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        assert_eq!(execution.get_chunk().get_environment(0, 5, 0), 0);
    }

    #[test]
    fn the_dynamic_layer_writes_above_the_surface() {
        let entry = Rc::new(DynamicLayerEntry::new(
            block_array(7, 2),
            Rc::new(DefaultCoordinateCondition::True),
        ));
        let layer = Rc::new(DynamicLayer::new(
            vec![entry],
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(2.0)),
        ));
        let container = dynamic_layer_container(FILLING, vec![layer]);
        let biome = tile_biome(
            container,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The layer starts at `surfaceY + offset` = 63 + 2 = 65 and writes
        // two blocks (65 and 64).
        assert_eq!(execution.get_block(0, 65, 0), 7);
        assert_eq!(execution.get_block(0, 64, 0), 7);
        assert_eq!(execution.get_block(0, 63, 0), FILLING);
    }

    #[test]
    fn the_dynamic_layer_break_leaves_the_remaining_layers() {
        // The first dynamic layer is pushed far below the surface, so the
        // labelled break leaves the whole layer loop: the second layer never
        // runs.
        let first = Rc::new(DynamicLayer::new(
            vec![Rc::new(DynamicLayerEntry::new(
                block_array(7, 1),
                Rc::new(DefaultCoordinateCondition::True),
            ))],
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(-100.0)),
        ));
        let second = Rc::new(DynamicLayer::new(
            vec![Rc::new(DynamicLayerEntry::new(
                block_array(8, 1),
                Rc::new(DefaultCoordinateCondition::True),
            ))],
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(0.0)),
        ));
        let container = dynamic_layer_container(FILLING, vec![first, second]);
        let biome = tile_biome(
            container,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The first layer breaks out before writing anything and the second
        // layer is never reached.
        assert_eq!(execution.get_block(0, 63, 0), FILLING);
        assert_eq!(execution.get_block(0, 62, 0), FILLING);
    }

    #[test]
    fn the_cover_checks_run_in_the_java_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let draws = Rc::new(RefCell::new(Vec::new()));
        let cover = cover_entry(&log, &draws, 1.0, false);
        let biome = tile_biome(
            layer_container(FILLING),
            Rc::new(CoverContainer::new(vec![cover])),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The parent check runs first (it draws nothing), then the column
        // check and finally the height check.
        let recorded = log.borrow().clone();
        assert_eq!(&recorded[..3], &["parent", "map", "height"]);

        // The height condition sees the **second** draw of the chunk: the
        // first one is the density roll of `isMatchingCoverColumn`.
        let mut reference = FastRandom::new(populator_random_seed(SEED, 0, 0));
        let density_draw = reference.next_double();
        let expected_height_draw = reference.next_double();
        assert!(density_draw < 1.0, "the density roll always passes");
        assert_eq!(draws.borrow()[0], expected_height_draw);
    }

    #[test]
    fn a_cover_below_the_density_is_skipped() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let draws = Rc::new(RefCell::new(Vec::new()));
        // A density of `0.0` never passes — the map condition is short
        // circuited, exactly like the Java `&&`.
        let cover = cover_entry(&log, &draws, 0.0, false);
        let biome = tile_biome(
            layer_container(FILLING),
            Rc::new(CoverContainer::new(vec![cover])),
            Rc::new(WaterContainer::new(Vec::new())),
        );
        let mut execution = execution_for(biome);
        let mut resource = ChunkGeneratorResource::new();

        BlockPopulator::populate(SEED, &mut execution, &mut resource);

        // The parent check still runs (it is the first operand of the `&&`
        // chain), but the map and the height conditions are short circuited
        // by the density roll — exactly like the Java `&&`.
        let recorded = log.borrow().clone();
        assert!(
            !recorded.contains(&"map"),
            "the map condition must not run: {recorded:?}"
        );
        assert!(
            !recorded.contains(&"height"),
            "the height condition must not run: {recorded:?}"
        );
        assert_eq!(execution.get_block(0, 64, 0), 0);
    }
}
