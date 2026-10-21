// Ported from: `.hytale-source/.../worldgen/chunk/populator/CavePopulator.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.populator.CavePopulator` —
//! the cave stage of [`crate::chunk::ChunkGeneratorExecution::execute`].
//!
//! The stage walks every zone of the world; for every cave type of a zone it
//! asks the cave type's entry point generator for the entry points that can
//! reach this chunk and then carves the cached [`Cave`] of those points into
//! the chunk.
//!
//! # Stage semantics (bit-exact)
//!
//! 1. The entry points are collected in a box expanded by the cave type's
//!    `maximumSize` (a `double`; the Java int coordinates widen to it).
//! 2. An entry point is only used when the column resolves to **the same
//!    zone** and the cave type's entry threshold matches, and when the
//!    column height matches the cave type's height condition
//!    (`eval(seed + 0xF5ACDEB5, x, height, z)`).
//! 3. The cave is taken from [`ChunkGenerator::get_cave`]; a cave that has no
//!    node in this chunk is skipped.
//! 4. The cave nodes are populated in **bucket order** (the order
//!    [`Cave::compile`] produced: node-type priority, stable within a
//!    priority) with the [`CaveBlockPriorityModifier`] installed, so a
//!    regular cave carves into prefab caves and cave covers place inside
//!    caves. Afterwards the regular (identity) priority modifier is restored.
//! 5. Per node, the shared `random` is reseeded with
//!    `seed + node.getSeedOffset()` and the node shape's
//!    [`populate_chunk`](crate::cave::shape::cave_node_shape::CaveNodeShape::populate_chunk)
//!    is called.
//! 6. Finally the node prefabs that intersect this chunk are pasted.
//!
//! # Deviations
//!
//! * **The concrete shapes are ported** (Phase 4c/4d) — every node shape
//!   carves its geometry and places its covers; the cave / cave-cover voxels
//!   are bit-exact in `tests/chunks_golden.rs`.
//! * **The cave-prefab paste is ported** (Phase 5a) — `PrefabPasteUtil` and
//!   the reusable prefab buffer are in the prefab package, so a cave node
//!   that carries a prefab pastes it into the chunk (with the
//!   `CAVE_PREFAB` priority, and the `FLAG_SUBMERGE` flag when the cave
//!   type submerges).
//! * **The Java `getBenchmark()` timing hook is not ported** — per-stage
//!   timings live in `StageTimings` / `WorldGenStats` instead.
//! * **`new Random()` is `FastRandom::new(0)`** — every node is reseeded
//!   before the shape is populated, so the initial state is unobservable.
use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use hytale_math::math_util;

use worldgen_model::block_priority::NoneModifier;

use crate::cave::cave::Cave;
use crate::cave::cave_block_priority_modifier::CaveBlockPriorityModifier;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_element::CaveElement;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::element::cave_prefab::CavePrefab;
use crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::prefab::prefab_paste_util::PrefabPasteBuffer;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::bounds::IChunkBounds;
use crate::util::condition::BlockMaskCondition;
use crate::zone::zone::Zone;

/// Java `public class CavePopulator`.
pub struct CavePopulator;

impl CavePopulator {
    /// Java `public static void populate(int seed, @Nonnull
    /// ChunkGeneratorExecution execution)`.
    pub fn populate(seed: i32, execution: &mut ChunkGeneratorExecution) {
        // Java iterates the live zone list of the chunk generator. The list is
        // cloned up front because the closure below mutates the execution.
        let zones: Vec<Rc<Zone>> = execution
            .get_chunk_generator()
            .get_zone_pattern_provider()
            .get_zones()
            .to_vec();
        for zone in zones {
            let Some(cave_generator) = &zone.cave_generator else {
                continue;
            };
            let cave_types: Vec<Rc<CaveType>> = cave_generator.get_cave_types().to_vec();
            for cave_type in cave_types {
                let cave_point_generator = Rc::clone(cave_type.get_entry_point_generator());
                let maximum_size = cave_type.get_maximum_size();
                let min_x = chunk_util::min_block(execution.get_x()) as f64 - maximum_size;
                let min_z = chunk_util::min_block(execution.get_z()) as f64 - maximum_size;
                let max_x = chunk_util::max_block(execution.get_x()) as f64 + maximum_size;
                let max_z = chunk_util::max_block(execution.get_z()) as f64 + maximum_size;

                cave_point_generator.collect(
                    seed,
                    min_x,
                    min_z,
                    max_x,
                    max_z,
                    &mut |x: f64, z: f64| {
                        Self::run(seed, x, z, execution, &zone, &cave_type);
                    },
                );
            }
        }
    }

    /// Java `private static void run(int seed, double dx, double dz, @Nonnull
    /// ChunkGeneratorExecution execution, Zone zone, @Nonnull CaveType
    /// caveType)`.
    fn run(
        seed: i32,
        dx: f64,
        dz: f64,
        execution: &mut ChunkGeneratorExecution,
        zone: &Rc<Zone>,
        cave_type: &Rc<CaveType>,
    ) {
        let chunk_generator = Rc::clone(execution.get_chunk_generator());
        let x = math_util::floor(dx);
        let z = math_util::floor(dz);
        let result = chunk_generator.get_zone_biome_result_at(seed, x, z);

        // Java: `result.getZoneResult().getZone() == zone` — the zone handles
        // all come from the same pattern provider, so the Java record equality
        // is the identity comparison here.
        let same_zone = result
            .get_zone_result()
            .and_then(|zone_result| zone_result.get_zone())
            .is_some_and(|result_zone| Rc::ptr_eq(result_zone, zone));

        if same_zone
            && cave_type.is_entry_threshold(seed, x, z)
            && Self::is_matching_height_threshold(seed, x, z, &chunk_generator, cave_type)
        {
            let cave = chunk_generator
                .get_cave(cave_type.as_ref(), seed, x, z)
                .expect("CavePopulator.run: the zone has no cave generator (Java: NPE)");
            Self::populate_cave(seed, execution, &cave);
        }
    }

    /// Java `private static void populate(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave)`.
    fn populate_cave(seed: i32, execution: &mut ChunkGeneratorExecution, cave: &Cave) {
        let chunk_index = execution.get_index();
        // no nodes in this chunk's index is not contained in cave
        if !cave.contains(chunk_index) {
            return;
        }

        let chunk_x = execution.get_x();
        let chunk_z = execution.get_z();

        // Java: `final Random random = new Random();` — see the module docs.
        let mut random = FastRandom::new(0);
        let environment = cave.get_cave_type().get_environment();

        // Handles cave-node specific block priority logic
        execution.set_priority_modifier(Rc::new(CaveBlockPriorityModifier::INSTANCE));

        // Iterate over all nodes to check for population
        for node in cave
            .get_cave_nodes(chunk_index)
            .expect("CavePopulator.populate: the chunk has no cave nodes (Java: NPE)")
        {
            Self::populate_cave_node(seed, execution, cave, node, &mut random);
        }

        // Restore regular priority logic
        execution.set_priority_modifier(Rc::new(NoneModifier::NONE));

        for node in cave
            .get_cave_nodes(chunk_index)
            .expect("CavePopulator.populate: the chunk has no cave nodes (Java: NPE)")
        {
            for prefab in node.get_cave_prefabs() {
                if prefab.get_bounds().intersects_chunk(chunk_x, chunk_z) {
                    Self::populate_prefab(seed, environment, execution, cave, node, &prefab);
                }
            }
        }
    }

    /// Java `private static void populateCaveNode(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull
    /// CaveNode caveNode, @Nonnull Random random)`.
    fn populate_cave_node(
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &Cave,
        cave_node: &CaveNode,
        random: &mut FastRandom,
    ) {
        random.set_seed(seed.wrapping_add(cave_node.get_seed_offset()) as i64);
        // Check if this node is inside this chunk
        cave_node
            .get_shape()
            .populate_chunk(seed, execution, cave, cave_node, random);

        // Java: the benchmark block (`getBenchmark().registerCaveNode`) is
        // Phase-6 timing collection and is not ported.
    }

    /// Java `private static void populatePrefab(int seed, int environment,
    /// @Nonnull ChunkGeneratorExecution execution, @Nonnull Cave cave,
    /// @Nonnull CaveNode node, @Nonnull CavePrefab prefab)`.
    fn populate_prefab(
        seed: i32,
        environment: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &Cave,
        _node: &CaveNode,
        prefab: &CavePrefab,
    ) {
        // Java: `generatePrefabAt(seed, prefab.getX(), prefab.getZ(),
        // prefab.getY(), environment, execution, cave, node,
        // prefab.getConfiguration(), prefab.getPrefab(), prefab.getRotation())`
        // → `PrefabPasteUtil.generate(buffer, rotation, supplier, x, y, z, cx,
        // cz)`.
        generate_prefab_at(
            seed,
            prefab.get_x(),
            prefab.get_z(),
            prefab.get_y(),
            environment,
            execution,
            cave,
            prefab.get_configuration(),
            prefab.get_prefab(),
            prefab.get_rotation(),
        );
    }

    /// Java `private static boolean isMatchingHeightThreshold(int seed, int x,
    /// int z, @Nonnull ChunkGenerator chunkGenerator, @Nonnull CaveType
    /// caveType)`.
    fn is_matching_height_threshold(
        seed: i32,
        x: i32,
        z: i32,
        chunk_generator: &Rc<crate::chunk::chunk_generator::ChunkGenerator>,
        cave_type: &CaveType,
    ) -> bool {
        let height_condition = cave_type.get_height_condition();
        let height = chunk_generator.get_height(seed, x, z);
        // Java: `seed + 0xF5ACDEB5` — the literal is an `int`, so the addition
        // wraps.
        height_condition.eval3(seed.wrapping_add(0xF5ACDEB5u32 as i32), x, height, z)
    }
}

/// Java `private static void generatePrefabAt(int seed, int x, int z, int y,
/// int environment, @Nonnull ChunkGeneratorExecution execution, @Nonnull Cave
/// cave, @Nonnull CaveNode node, BlockMaskCondition configuration, @Nonnull
/// WorldGenPrefabSupplier supplier, PrefabRotation rotation)`.
#[allow(clippy::too_many_arguments)]
fn generate_prefab_at(
    seed: i32,
    x: i32,
    z: i32,
    y: i32,
    environment: i32,
    execution: &mut ChunkGeneratorExecution,
    cave: &Cave,
    configuration: &BlockMaskCondition,
    supplier: &Rc<dyn CavePrefabSupplier>,
    rotation: PrefabRotation,
) {
    let cx = x - chunk_util::min_block(execution.get_x());
    let cz = z - chunk_util::min_block(execution.get_z());
    let external_seed = hash_util::rehash2(x as i64, z as i64).wrapping_mul(0xfa10_cd31);

    let submerge = cave.get_cave_type().is_submerge();

    let Some(paste_supplier) = supplier.as_paste_supplier() else {
        return;
    };

    let buffer = PrefabPasteBuffer::new();
    buffer.set_seed(seed, external_seed);
    *buffer.execution.borrow_mut() = Some(execution);
    *buffer.block_mask.borrow_mut() = Some(configuration.clone());
    buffer.environment_id.set(environment);
    buffer.priority.set(if submerge {
        worldgen_model::block_priority::CAVE_PREFAB | worldgen_model::block_priority::FLAG_SUBMERGE
    } else {
        worldgen_model::block_priority::CAVE_PREFAB
    });

    crate::prefab::prefab_paste_util::generate(&buffer, rotation, paste_supplier, x, y, z, cx, cz);
}
