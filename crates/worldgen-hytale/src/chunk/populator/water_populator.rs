// Ported from: `.hytale-source/.../worldgen/chunk/populator/WaterPopulator.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.populator.WaterPopulator` —
//! the water stage: it floods the water bands of a zone and skips the blocks
//! that cannot hold a fluid.
//!
//! # Deviations
//!
//! * **The block registry lives on the execution.** Java reads the block
//!   asset from the static `BlockType.getAssetMap()`; the Rust port reads it
//!   from the execution's [`worldgen_model::registry::Registries`], which is
//!   empty until the loader subtask fills it (see `REMAINING.md` § 6).
//! * **The fluid level is looked up in the fluid registry** — Java's
//!   `setFluid(x, y, z, type, fluid)` overload defaults it to
//!   `Fluid.getAssetMap().getAssetOrDefault(fluid, Fluid.EMPTY)
//!   .getMaxFluidLevel()`.
use std::rc::Rc;

use hytale_math::chunk_util;

use worldgen_model::block_priority::{FLAG_SUBMERGE, MASK, NO_CHANGE, WATER};
use worldgen_model::registry::is_solid;

use crate::biome::biome::Biome;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;

/// Java `BlockType.EMPTY_ID` = `0` — air.
const BLOCK_TYPE_EMPTY_ID: i32 = 0;

/// Java `com.hypixel.hytale.server.worldgen.chunk.populator.WaterPopulator`.
pub struct WaterPopulator;

impl WaterPopulator {
    /// Java `public static void populate(int seed, @Nonnull final
    /// ChunkGeneratorExecution execution)`.
    pub fn populate(seed: i32, execution: &mut ChunkGeneratorExecution) {
        for cx in 0..chunk_util::SIZE {
            for cz in 0..chunk_util::SIZE {
                submerge_column(seed, cx, cz, execution);
            }
        }
    }
}

/// Java `private static void submergeColumn(int seed, int cx, int cz, @Nonnull
/// ChunkGeneratorExecution execution)`.
fn submerge_column(seed: i32, cx: i32, cz: i32, execution: &mut ChunkGeneratorExecution) {
    let biome: Rc<dyn Biome> = execution
        .zone_biome_result(cx, cz)
        .get_biome()
        .cloned()
        .expect("submergeColumn: the column has no biome (Java: NullPointerException)");
    let x = execution.global_x(cx);
    let z = execution.global_z(cz);

    let water_container = biome.get_water_container();
    for water_entry in water_container.get_entries() {
        if !water_entry.should_populate(seed, x, z) {
            continue;
        }

        let water_min = water_entry.get_min(seed, x, z);
        let water_max = water_entry.get_max(seed, x, z);
        if water_min > water_max {
            continue;
        }

        let block_id = water_entry.get_block();
        let fluid_id = water_entry.get_fluid();

        let mut y = water_min;
        while y <= water_max {
            submerge_block(cx, y, cz, block_id, fluid_id, execution);
            y = y.wrapping_add(1);
        }
    }
}

/// Java `private static void submergeBlock(int cx, int y, int cz, int
/// blockId, int fluidId, @Nonnull ChunkGeneratorExecution execution)`.
fn submerge_block(
    cx: i32,
    y: i32,
    cz: i32,
    block_id: i32,
    fluid_id: i32,
    execution: &mut ChunkGeneratorExecution,
) {
    let raw_priority = execution.get_priority_chunk().get_raw(cx, y, cz);
    let priority = raw_priority & MASK;

    // Ignore if a higher priority and the submerge flag is not set.
    if priority >= WATER && (raw_priority & FLAG_SUBMERGE) == 0 {
        return;
    }

    let current_block_id = if block_id != BLOCK_TYPE_EMPTY_ID && priority < WATER {
        // Only override the block if the current priority is lower than water.
        execution.set_block(cx, y, cz, NO_CHANGE, block_id);
        block_id
    } else {
        execution.get_block(cx, y, cz)
    };

    // Check if the block needs to be submerged.
    // Java: `BlockType.getAssetMap().getAsset(currentBlockId)` — an unknown
    // block id yields `null` and the water is skipped.
    let block_type = execution.registry().blocks.get(current_block_id).cloned();
    let Some(block_type) = block_type else {
        return;
    };
    if is_solid(&block_type) {
        return;
    }

    execution.set_fluid_defaulted(cx, y, cz, NO_CHANGE, fluid_id);
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use procedural::noise::condition::DefaultCoordinateCondition;
    use procedural::noise::supplier::IDoubleCoordinateSupplier;

    use worldgen_model::block_priority::FLAG_SUBMERGE;
    use worldgen_model::chunk::GeneratedChunk;
    use worldgen_model::registry::{
        BlockRegistry, BlockTypeInfo, DrawType, FluidRegistry, Registries,
    };

    use crate::chunk::chunk_generator_resource::ChunkGeneratorResource;
    use crate::chunk::populator::test_stubs::{generator_for_biome, tile_biome};
    use crate::container::cover_container::CoverContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::water_container::{Entry as WaterEntry, WaterContainer};

    use super::*;

    /// The filling block id used by the test biomes.
    const FILLING: i32 = 42;

    /// The fluid id used by the test water entries.
    const WATER_FLUID: i32 = 3;

    /// The seed the populator tests use.
    const SEED: i32 = 42;

    /// A constant double supplier (the min/max suppliers of the water
    /// entries).
    struct ConstantSupplier(f64);

    impl IDoubleCoordinateSupplier for ConstantSupplier {
        fn get2(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get3(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    /// A water entry that covers `[min, max]` with `block` / `fluid`.
    fn water_entry(block: i32, fluid: i32, min: f64, max: f64) -> WaterEntry {
        WaterEntry::new(
            block,
            fluid,
            Rc::new(ConstantSupplier(min)),
            Rc::new(ConstantSupplier(max)),
            Rc::new(DefaultCoordinateCondition::True),
        )
    }

    /// A registry whose block `id` is solid / not solid and whose fluid
    /// `WATER_FLUID` has the max level `level`.
    fn registry(solid: bool, level: i8) -> Rc<Registries> {
        let mut blocks = vec![None; 64];
        blocks[FILLING as usize] = Some(BlockTypeInfo::new(if solid {
            DrawType::Cube
        } else {
            DrawType::Model
        }));
        let mut fluids = vec![8i8; 64];
        fluids[WATER_FLUID as usize] = level;
        Rc::new(Registries::new(
            BlockRegistry::new(blocks),
            FluidRegistry::new(fluids),
        ))
    }

    /// An execution whose every column resolves to a biome with a single
    /// water entry.
    fn execution_for(
        water_entry: WaterEntry,
        registries: Rc<Registries>,
    ) -> ChunkGeneratorExecution {
        let biome = tile_biome(
            Rc::new(LayerContainer::new(FILLING, 0, Vec::new(), Vec::new())),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(WaterContainer::new(vec![water_entry])),
        );
        let generator = generator_for_biome(biome);
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        ChunkGeneratorExecution::with_registry(SEED, generator, chunk, registries)
    }

    #[test]
    fn the_water_floods_a_column_whose_block_is_not_solid() {
        let mut execution =
            execution_for(water_entry(0, WATER_FLUID, 5.0, 7.0), registry(false, 5));

        // The terrain filling covers the column first.
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );

        WaterPopulator::populate(SEED, &mut execution);

        for y in 5..=7 {
            assert_eq!(execution.get_fluid(0, y, 0), WATER_FLUID, "y={y}");
            assert_eq!(execution.get_chunk().get_fluid_level(0, y, 0), 5, "y={y}");
        }
        // Outside the band there is no water.
        assert_eq!(execution.get_fluid(0, 4, 0), 0);
        assert_eq!(execution.get_fluid(0, 8, 0), 0);
    }

    #[test]
    fn a_solid_block_is_not_submerged() {
        let mut execution = execution_for(water_entry(0, WATER_FLUID, 5.0, 7.0), registry(true, 5));
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );

        WaterPopulator::populate(SEED, &mut execution);

        // `FluidTicker.isSolid` → the water is skipped.
        for y in 5..=7 {
            assert_eq!(execution.get_fluid(0, y, 0), 0, "y={y}");
        }
    }

    #[test]
    fn an_unknown_block_is_not_submerged() {
        // The empty registry misses every lookup — the Java `getAsset` returns
        // `null` and the water is skipped.
        let mut execution = execution_for(
            water_entry(0, WATER_FLUID, 5.0, 7.0),
            Rc::new(Registries::empty()),
        );
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );

        WaterPopulator::populate(SEED, &mut execution);

        assert_eq!(execution.get_fluid(0, 5, 0), 0);
    }

    #[test]
    fn a_higher_priority_skips_the_water() {
        let mut execution =
            execution_for(water_entry(0, WATER_FLUID, 5.0, 7.0), registry(false, 5));
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );
        // Raise the priority of the band to `WATER` — the submerge flag is
        // not set, so the water is skipped.
        for y in 5..=7 {
            assert!(execution.set_block(0, y, 0, WATER, FILLING));
        }

        WaterPopulator::populate(SEED, &mut execution);

        for y in 5..=7 {
            assert_eq!(execution.get_fluid(0, y, 0), 0, "y={y}");
        }
    }

    #[test]
    fn the_submerge_flag_overrides_the_priority() {
        let mut execution =
            execution_for(water_entry(0, WATER_FLUID, 5.0, 7.0), registry(false, 5));
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );
        // `WATER | FLAG_SUBMERGE` → the water is applied even though the
        // priority is not lower than `WATER`.
        for y in 5..=7 {
            assert!(execution.set_block(0, y, 0, WATER | FLAG_SUBMERGE, FILLING));
        }

        WaterPopulator::populate(SEED, &mut execution);

        for y in 5..=7 {
            assert_eq!(execution.get_fluid(0, y, 0), WATER_FLUID, "y={y}");
        }
    }

    #[test]
    fn the_water_entry_block_overrides_the_terrain() {
        // A water entry with a block id replaces the terrain block and the
        // *new* block is the one that is checked for solidity.
        let mut execution = execution_for(
            water_entry(FILLING, WATER_FLUID, 5.0, 5.0),
            registry(false, 8),
        );
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );

        WaterPopulator::populate(SEED, &mut execution);

        // The block was written with the `NO_CHANGE` priority.
        assert_eq!(execution.get_block(0, 5, 0), FILLING);
        assert_eq!(execution.get_fluid(0, 5, 0), WATER_FLUID);
        // The default max fluid level of the registry is used.
        assert_eq!(execution.get_chunk().get_fluid_level(0, 5, 0), 8);
    }

    #[test]
    fn an_inverted_water_band_is_skipped() {
        let mut execution =
            execution_for(water_entry(0, WATER_FLUID, 30.0, 20.0), registry(false, 5));
        crate::chunk::populator::block_populator::BlockPopulator::populate(
            SEED,
            &mut execution,
            &mut ChunkGeneratorResource::new(),
        );

        WaterPopulator::populate(SEED, &mut execution);

        for y in 20..=30 {
            assert_eq!(execution.get_fluid(0, y, 0), 0, "y={y}");
        }
    }
}
