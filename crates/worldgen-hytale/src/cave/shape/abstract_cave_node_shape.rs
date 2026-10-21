// Ported from: `.hytale-source/.../worldgen/cave/shape/AbstractCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.AbstractCaveNodeShape` —
//! the shared base of the geometric cave node shapes.
//!
//! # Java → Rust
//!
//! The Java class is an abstract base class that implements
//! [`CaveNodeShape::populate_chunk`] once for every geometric shape (the
//! pipe, the cylinder, the ellipsoid and the empty line). Rust has no class
//! inheritance, so the shared implementation is the free function
//! [`populate_chunk`] and every concrete shape delegates to it from its
//! [`CaveNodeShape`] implementation.
//!
//! # The draw order (bit-exact)
//!
//! The Java method walks the chunk in `x` → `z` → `y` order and, per
//! column, first carves the cave body and then places the covers, then
//! invalidates the unsupported ceiling / floor covers. The order of the
//! random draws inside the loop is part of the contract: the *filling block*
//! is drawn **per block** (inside the `y` loop) and the *cover entry* is
//! drawn **per cover** (after the `y` loop).

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use worldgen_model::block_priority;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

use crate::cave::cave::Cave;
use crate::cave::cave_node_type::CaveNodeCoverType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_utils::{
    get_cover_height, get_filling_block, invalidate_cover, is_cover_matching_parent,
};
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::util::bounds::{IChunkBounds, IWorldBounds};

/// Java `@Override public void populateChunk(int seed, @Nonnull
/// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull CaveNode
/// node, @Nonnull Random random)`.
pub fn populate_chunk<S: CaveNodeShape + ?Sized>(
    shape: &S,
    seed: i32,
    execution: &mut ChunkGeneratorExecution,
    cave: &Cave,
    node: &CaveNode,
    random: &mut FastRandom,
) {
    let cave_type = cave.get_cave_type();
    let cave_node_type = node.get_cave_node_type();
    let shape_bounds = shape.get_bounds();
    let surface_limited = cave_type.is_surface_limited();
    let environment = if cave_node_type.has_environment() {
        cave_node_type.get_environment()
    } else {
        cave_type.get_environment()
    };

    let chunk_low_x = chunk_util::min_block(execution.get_x());
    let chunk_low_z = chunk_util::min_block(execution.get_z());
    let chunk_high_x = chunk_util::max_block(execution.get_x());
    let chunk_high_z = chunk_util::max_block(execution.get_z());

    let min_x = chunk_low_x.max(shape_bounds.get_low_bound_x());
    let min_y = shape_bounds.get_low_bound_y();
    let min_z = chunk_low_z.max(shape_bounds.get_low_bound_z());
    let max_x = chunk_high_x.min(shape_bounds.get_high_bound_x());
    let max_y = shape_bounds.get_high_bound_y();
    let max_z = chunk_high_z.min(shape_bounds.get_high_bound_z());

    for x in min_x..=max_x {
        let cx = x - chunk_low_x;
        for z in min_z..=max_z {
            let cz = z - chunk_low_z;

            // Calculate the maximum height to generate the cave at
            let mut height = max_y;
            let mut height_limited = false;
            if surface_limited {
                let chunk_height =
                    chunk_height(execution.get_chunk(), execution.registry(), cx, cz);
                if height >= chunk_height {
                    height = chunk_height;
                    height_limited = true;
                }
            }

            // Lowest block of this node to add covers
            let mut lowest = i32::MAX;
            let mut lowest_possible = i32::MAX;

            // Highest block of this node to remove covers on top of the node
            let mut highest = i32::MIN;
            let mut highest_possible = i32::MIN;

            for y in min_y..=height {
                if !shape.should_replace(seed, x as f64, z as f64, y) {
                    continue;
                }
                if y < lowest_possible {
                    lowest_possible = y;
                }
                if y > highest_possible {
                    highest_possible = y;
                }
                let current = execution.get_block(cx, y, cz);
                let current_fluid = execution.get_fluid(cx, y, cz);
                let is_candidate_block = !surface_limited || current != 0;
                if is_candidate_block {
                    let block_entry = get_filling_block(cave_type, cave_node_type, y, random);
                    if cave_type.get_block_mask().eval(
                        current,
                        current_fluid,
                        block_entry.block_id,
                        block_entry.fluid_id,
                    ) {
                        if execution.set_block_entry_with_environment(
                            cx,
                            y,
                            cz,
                            block_priority::CAVE,
                            &block_entry,
                            environment,
                        ) {
                            if y < lowest {
                                lowest = y;
                            }
                            if y > highest {
                                highest = y;
                            }
                        }

                        if execution.set_fluid_with_environment(
                            cx,
                            y,
                            cz,
                            block_priority::CAVE,
                            block_entry.fluid_id,
                            environment,
                        ) {
                            if y < lowest {
                                lowest = y;
                            }
                            if y > highest {
                                highest = y;
                            }
                        }
                    }
                }
            }

            for cover in cave_node_type.get_covers() {
                let entry = cover.get(random).expect(
                    "AbstractCaveNodeShape.populateChunk: the cover has no entry (Java: NullPointerException)",
                );
                let y = get_cover_height(
                    lowest,
                    lowest_possible,
                    highest,
                    highest_possible,
                    height_limited,
                    cover,
                    entry,
                );

                let cover_entry = entry.get_entry();
                if y >= 0
                    && cover.get_density_condition().eval2(
                        seed.wrapping_add(node.get_seed_offset()),
                        x,
                        z,
                    )
                    && cover.get_height_condition().eval(seed, x, z, y, random)
                    && cover.get_map_condition().eval2(seed, x, z)
                    && is_cover_matching_parent(cx, cz, y, execution, cover)
                {
                    execution.set_block_entry_with_environment(
                        cx,
                        y,
                        cz,
                        block_priority::CAVE_COVER,
                        &cover_entry,
                        environment,
                    );
                    execution.set_fluid_with_environment(
                        cx,
                        y,
                        cz,
                        block_priority::CAVE_COVER,
                        cover_entry.fluid_id,
                        environment,
                    );
                }
            }

            // Remove unsupported ceiling covers where we've intersected the
            // ceiling of another cave from above
            if invalidate_cover(
                cx,
                lowest.wrapping_sub(1),
                cz,
                CaveNodeCoverType::Ceiling,
                execution,
                &execution.registry().blocks,
            ) {
                let block_entry =
                    get_filling_block(cave_type, cave_node_type, lowest.wrapping_sub(1), random);
                execution.override_block_entry(
                    cx,
                    lowest.wrapping_sub(1),
                    cz,
                    block_priority::CAVE,
                    &block_entry,
                );
                let fluid_level = execution
                    .registry()
                    .fluids
                    .get_max_fluid_level(block_entry.fluid_id);
                execution.override_fluid(
                    cx,
                    lowest.wrapping_sub(1),
                    cz,
                    block_priority::CAVE,
                    block_entry.fluid_id,
                    fluid_level,
                );
            }

            // Remove unsupported floor covers where we've breached surface or
            // intersected the floor of another cave from below
            if invalidate_cover(
                cx,
                highest.wrapping_add(1),
                cz,
                CaveNodeCoverType::Floor,
                execution,
                &execution.registry().blocks,
            ) {
                let block_entry =
                    get_filling_block(cave_type, cave_node_type, highest.wrapping_add(1), random);
                execution.override_block_entry(
                    cx,
                    highest.wrapping_add(1),
                    cz,
                    block_priority::CAVE,
                    &block_entry,
                );
                let fluid_level = execution
                    .registry()
                    .fluids
                    .get_max_fluid_level(block_entry.fluid_id);
                execution.override_fluid(
                    cx,
                    highest.wrapping_add(1),
                    cz,
                    block_priority::CAVE,
                    block_entry.fluid_id,
                    fluid_level,
                );
            }
        }
    }
}

/// Java `GeneratedBlockChunk.getHeight(int x, int z)` — the highest
/// non-transparent block of the column.
///
/// Deviation: the Java method walks the *lazily created* chunk sections and
/// skips the empty ones; the Rust chunk model has one flat grid, so the
/// section skipping is not needed. The Java opacity test
/// (`BlockType.getOpacity() != Opacity.Transparent`) needs the CoreServer
/// block assets, which the ported block registry does not carry — a *known*
/// block is therefore treated as opaque and an unknown one as transparent
/// (the Java `type != null` check).
#[allow(deprecated)] // Java `ChunkUtil.HEIGHT` (deprecated in Java too).
pub fn chunk_height(chunk: &GeneratedChunk, registry: &Registries, cx: i32, cz: i32) -> i32 {
    let mut y = chunk_util::HEIGHT;
    loop {
        y -= 1;
        if y <= 0 {
            break;
        }
        let block_id = chunk.get_block(cx, y, cz);
        if block_id != 0 {
            if let Some(info) = registry.blocks.get(block_id) {
                if !info.opacity.is_transparent() {
                    break;
                }
            }
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    use crate::cave::cave::Cave;
    use crate::cave::cave_node_type::{
        CaveNodeCoverEntry, CaveNodeCoverEntryEntry, CaveNodeCoverType, CaveNodeType,
    };
    use crate::cave::element::cave_node::CaveNode;
    use crate::cave::shape::pipe_cave_node_shape::PipeCaveNodeShape;
    use crate::cave::test_stubs::{
        cave_node_type_with_filling, cave_type_with_height_factor, chunk_generator,
    };
    use crate::loader::util::Vector3d;
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use crate::util::bounds::WorldBounds;
    use crate::weighted_map::WeightedMap;
    use procedural::noise::condition::{
        ConstantBlockFluidCondition, DefaultCoordinateCondition, DefaultCoordinateRndCondition,
    };
    use worldgen_model::registry::{BlockRegistry, BlockTypeInfo, DrawType};

    #[test]
    fn the_chunk_height_of_an_empty_column_is_zero() {
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        assert_eq!(chunk_height(&chunk, &Registries::empty(), 0, 0), 0);
    }

    #[test]
    fn the_chunk_height_finds_the_highest_known_block() {
        let mut chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        chunk.set_block(3, 42, 7, 1, 0, 0);
        let registry = Registries::new(
            BlockRegistry::new(vec![
                Some(BlockTypeInfo::new(DrawType::Empty)),
                Some(BlockTypeInfo::new(DrawType::Cube)),
            ]),
            worldgen_model::registry::FluidRegistry::empty(),
        );
        assert_eq!(chunk_height(&chunk, &registry, 3, 7), 42);
        // Another column is still empty.
        assert_eq!(chunk_height(&chunk, &registry, 0, 0), 0);
    }

    #[test]
    fn the_pipe_carves_the_expected_blocks_into_the_chunk() {
        // A pipe from (0, 40, 0) to (16, 40, 0) with a radius of 3 and a
        // filling block of 42.
        let cave_type = cave_type_with_height_factor("pipe", 1.0);
        let shape = PipeCaveNodeShape::new(
            Rc::clone(&cave_type),
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(16.0, 0.0, 0.0),
            3.0,
            3.0,
            3.0,
        );
        let node_type = cave_node_type_with_filling(0, BlockFluidEntry::new(42, 0, 0));
        let node = CaveNode::new(0, node_type, Rc::new(shape), 0.0, 0.0);
        let cave = Cave::new(Rc::clone(&cave_type));

        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, chunk_generator(), chunk);
        let mut random = FastRandom::new(7);

        node.get_shape()
            .populate_chunk(42, &mut execution, &cave, &node, &mut random);

        // The tube carves the blocks from y = 38 to y = 42.
        for y in 38..=42 {
            assert_eq!(
                execution.get_block(8, y, 0),
                42,
                "the block at y = {y} was not carved"
            );
            assert_eq!(
                execution.get_priority_chunk().get(8, y, 0),
                block_priority::CAVE
            );
        }
        // Outside of the tube nothing was written.
        assert_eq!(execution.get_block(8, 37, 0), 0);
        assert_eq!(execution.get_block(8, 43, 0), 0);
        assert_eq!(
            execution.get_priority_chunk().get(8, 37, 0),
            block_priority::NONE
        );
        // The caps of the pipe reach beyond the segment.
        assert_eq!(execution.get_block(0, 40, 0), 42);
    }

    #[test]
    fn the_floor_cover_is_placed_below_the_carved_column() {
        // The same pipe, but with a floor cover.
        let cave_type = cave_type_with_height_factor("pipe", 1.0);
        let shape = PipeCaveNodeShape::new(
            Rc::clone(&cave_type),
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(16.0, 0.0, 0.0),
            3.0,
            3.0,
            3.0,
        );

        let mut entries = WeightedMap::<CaveNodeCoverEntryEntry>::builder();
        entries.put(
            CaveNodeCoverEntryEntry::new(BlockFluidEntry::new(99, 0, 0), 0),
            1.0,
        );
        let cover = CaveNodeCoverEntry::new(
            Rc::new(entries.build()),
            Rc::new(DefaultCoordinateRndCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantBlockFluidCondition::True),
            CaveNodeCoverType::Floor,
        );

        let mut builder = WeightedMap::<BlockFluidEntry>::builder();
        builder.put(BlockFluidEntry::new(42, 0, 0), 1.0);
        let node_type = Rc::new(CaveNodeType::new(
            "node".to_string(),
            None,
            Rc::new(builder.build()),
            Rc::new(crate::cave::test_stubs::StubShapeGenerator {
                bounds: WorldBounds::new(),
            }),
            Rc::new(DefaultCoordinateCondition::True),
            None,
            vec![cover],
            0,
            worldgen_model::chunk::ENVIRONMENT_NOT_SET,
        ));

        let node = CaveNode::new(0, node_type, Rc::new(shape), 0.0, 0.0);
        let cave = Cave::new(Rc::clone(&cave_type));

        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, chunk_generator(), chunk);
        let mut random = FastRandom::new(7);

        node.get_shape()
            .populate_chunk(42, &mut execution, &cave, &node, &mut random);

        // The tube reaches down to y = 38, so the floor cover sits at y = 37.
        assert_eq!(execution.get_block(8, 37, 0), 99);
        assert_eq!(
            execution.get_priority_chunk().get(8, 37, 0),
            block_priority::CAVE_COVER
        );
        // The cover is only placed below a column that was carved.
        assert_eq!(execution.get_block(8, 100, 0), 0);
    }
}
