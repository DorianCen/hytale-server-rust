// Ported from: `.hytale-source/.../worldgen/cave/CaveGenerator.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveGenerator` — grows the
//! cave node graph of a zone.
//!
//! The Java class walks the node graph recursively: it starts at the entry
//! node ([`CaveGeneratorImpl::start_cave`]) and then grows the children of
//! every node ([`CaveGeneratorImpl::continue_node`]) while the remaining depth
//! is positive. Every candidate node is filtered by
//!
//! * the per-node-type height condition (at the child origin and at the end
//!   of the generated shape),
//! * the cave type's biome mask, which evaluates to the
//!   [`GENERATE`](crate::cave::cave_biome_mask_flags::GENERATE) /
//!   [`POPULATE`](crate::cave::cave_biome_mask_flags::POPULATE) /
//!   [`CONTINUE`](crate::cave::cave_biome_mask_flags::CONTINUE) bit flags,
//! * the per-child chance and repeat counters, and
//! * the per-node-type child count bound.
//!
//! The generated nodes are added to the [`Cave`], which files them under
//! every chunk they touch and sorts each chunk bucket by node priority when
//! the cave is compiled.
//!
//! # Java → Rust shape
//!
//! Java `class CaveGenerator` maps to **two** Rust items because the zone
//! record holds the generator as a trait object
//! (`Option<Rc<dyn CaveGenerator>>`):
//!
//! * [`CaveGenerator`] — the object-safe trait the zone / chunk generator
//!   talk to (Java's method surface),
//! * [`CaveGeneratorImpl`] — the concrete implementation of the recursive
//!   walk (the Java class proper).
//!
//! # Seams
//!
//! * `CaveNodeType.generateCaveNodeShape` is behind
//!   [`crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator`] —
//!   the concrete shapes (which draw from the random and build their geometry)
//!   are Phase 4c (ported) and Phase 4d (the distorted family).
//! * `CaveGenerator.getRotation` uses the `instanceof PrefabCaveNodeShape`
//!   check; Rust models it with
//!   [`CaveNodeShape::as_prefab_rotation`](crate::cave::shape::cave_node_shape::CaveNodeShape::as_prefab_rotation),
//!   which returns `None` for every shape but the prefab one.
//!
//! # Deviations
//!
//! * **`ChunkGenerator.getResource().getRandom()` is a local [`FastRandom`].**
//!   `generatePrefabs` seeds the shared resource random with
//!   `seed + node.getSeedOffset()` and draws the prefab configuration from
//!   it. The Rust port has no ambient (thread-local) resource, so it creates
//!   an identically seeded local random. Nothing else draws from the resource
//!   random in the cave path, so the draws are identical.
//! * **The Java identity short-circuits of the default conditions are
//!   omitted** (`condition == DefaultCoordinateCondition.DEFAULT_TRUE` /
//!   `mask == CaveBiomeMaskFlags.DEFAULT_ALLOW`). The port evaluates the
//!   condition instead; for the constants this yields the same value, and the
//!   conditions are pure, so the result is identical.
//! * **`generatePrefabs`/`generatePrefab` are structural ports over the
//!   [`CavePrefabSupplier`](crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier)
//!   seam** — the prefab buffers themselves are Phase 5.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use hytale_math::math_util;
use procedural::noise::condition::ICoordinateCondition;

use crate::cave::cave::Cave;
use crate::cave::cave_biome_mask_flags::{can_continue, can_generate, can_populate};
use crate::cave::cave_node_type::{CaveNodeChildEntry, CaveNodeType};
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_element::CaveElement;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::element::cave_prefab::CavePrefab;
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::array_utli::shuffle_array;
use crate::util::bounds::IChunkBounds;
use crate::util::condition::flag::Int2FlagsCondition;

/// Java `interface`-shaped **object-safe** view of `class CaveGenerator`.
///
/// The Java class is concrete; the zone record holds its generator as a
/// nullable reference, which the Rust port models as
/// `Option<Rc<dyn CaveGenerator>>`. [`CaveGeneratorImpl`] is the concrete
/// Java class.
pub trait CaveGenerator {
    /// Java `CaveType[] getCaveTypes()`.
    fn get_cave_types(&self) -> &[Rc<CaveType>];

    /// Java `@Nonnull Cave generate(int seed, ChunkGenerator chunkGenerator,
    /// CaveType caveType, int x, int y, int z)`.
    fn generate(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        cave_type: &CaveType,
        x: i32,
        y: i32,
        z: i32,
    ) -> Cave;
}

/// Java `class CaveGenerator` — the recursive cave node-graph growth.
pub struct CaveGeneratorImpl {
    /// Java `private final CaveType[] caveTypes`.
    cave_types: Vec<Rc<CaveType>>,
}

impl CaveGeneratorImpl {
    /// Java `CaveGenerator(CaveType[] caveTypes)`.
    pub fn new(cave_types: Vec<Rc<CaveType>>) -> Self {
        Self { cave_types }
    }

    /// Java `CaveType[] getCaveTypes()`.
    pub fn get_cave_types(&self) -> &[Rc<CaveType>] {
        &self.cave_types
    }

    /// Java `@Nonnull Cave generate(int seed, ChunkGenerator chunkGenerator,
    /// CaveType caveType, int x, int y, int z)`.
    ///
    /// Deviation: the Java caller passes the `CaveType` reference itself; the
    /// Rust trait takes it by reference, so the shared handle is looked up in
    /// [`Self::cave_types`] (falling back to a value clone, which is
    /// behaviourally identical because [`CaveType`] has no mutable state).
    pub fn generate(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        cave_type: &CaveType,
        x: i32,
        y: i32,
        z: i32,
    ) -> Cave {
        // Java: `final int seedOffset = (int) HashUtil.rehash(seed, x, y, z);`
        let seed_offset = hash_util::rehash4(seed as i64, x as i64, y as i64, z as i64) as i32;
        // Java: `final Random random = new FastRandom(seedOffset);`
        let mut random = FastRandom::new(seed_offset as i64);

        let cave_type_handle = self.resolve_cave_type(cave_type);
        let mut cave = self.new_cave(cave_type_handle);
        // Java: `final var origin = new Vector3d(x, y, z);`
        let mut origin = Vector3d::new(x as f64, y as f64, z as f64);

        // Java: `origin.y = caveType.getModifiedStartHeight(seed + seedOffset,
        // x, y, z, random);`
        origin.y = cave.get_cave_type().get_modified_start_height(
            seed.wrapping_add(seed_offset),
            x,
            y,
            z,
            &mut random,
        ) as f64;

        self.start_cave(seed, chunk_generator, &mut cave, origin, &mut random);
        cave.compile();

        cave
    }

    /// Finds the shared `Rc<CaveType>` handle of the generator.
    fn resolve_cave_type(&self, cave_type: &CaveType) -> Rc<CaveType> {
        self.cave_types
            .iter()
            .find(|candidate| std::ptr::eq(candidate.as_ref(), cave_type))
            .cloned()
            .unwrap_or_else(|| Rc::new(cave_type.clone()))
    }

    /// Java `@Nonnull protected Cave newCave(CaveType caveType)`.
    pub fn new_cave(&self, cave_type: Rc<CaveType>) -> Cave {
        Cave::new(cave_type)
    }

    /// Java `protected void startCave(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, @Nonnull Cave cave, @Nonnull Vector3d origin, @Nonnull
    /// Random random)`.
    pub fn start_cave(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        cave: &mut Cave,
        origin: Vector3d,
        random: &mut FastRandom,
    ) {
        // Java: `final var biomeMask = cave.getCaveType().getBiomeMask();`
        let biome_mask = Rc::clone(cave.get_cave_type().get_biome_mask());
        let start_biome_mask_result =
            Self::get_biome_mask_result(seed, chunk_generator, biome_mask.as_ref(), origin);
        if !can_generate(start_biome_mask_result) {
            return;
        }

        // Java re-reads `cave.getCaveType()`; take the shared handle once.
        let cave_type = Rc::clone(cave.get_cave_type());
        let depth = cave_type.get_start_depth(random);

        let node_type = Rc::clone(cave_type.get_entry_node());
        let yaw = cave_type.get_start_yaw(random);
        let pitch = cave_type.get_start_pitch(random);
        let seed_offset = random.next_int();

        let shape = node_type.generate_cave_node_shape(
            random,
            cave_type.as_ref(),
            None,
            None,
            origin,
            yaw,
            pitch,
        );

        let end_biome_mask_result = Self::get_biome_mask_result(
            seed,
            chunk_generator,
            biome_mask.as_ref(),
            shape.get_end(),
        );
        if !can_generate(end_biome_mask_result) {
            return;
        }

        let node = Rc::new(CaveNode::new(
            seed.wrapping_add(seed_offset),
            Rc::clone(&node_type),
            Rc::from(shape),
            yaw,
            pitch,
        ));
        if node.get_shape().has_geometry()
            && can_populate(start_biome_mask_result)
            && can_populate(end_biome_mask_result)
        {
            cave.add_node(&node);
        }

        self.continue_node(seed, chunk_generator, cave, &node, depth, random);
    }

    /// Java `protected void continueNode(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, @Nonnull Cave cave, @Nonnull CaveNode parent, int
    /// depth, @Nonnull Random random)`.
    pub fn continue_node(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        cave: &mut Cave,
        parent: &Rc<CaveNode>,
        depth: i32,
        random: &mut FastRandom,
    ) {
        if depth <= 0 {
            // end of this branch reached
            return;
        }

        let biome_mask = Rc::clone(cave.get_cave_type().get_biome_mask());
        let child_entries = Self::get_child_entries_randomized(parent.get_cave_node_type(), random);
        let children_count = Self::get_children_count(parent.get_cave_node_type(), random);
        let mut generated_children = 0i32;
        for child_entry in &child_entries {
            let repeat = Self::get_repeat_counter(child_entry, random);
            for _ in 0..repeat {
                if !Self::should_generate_child(child_entry, random) {
                    continue;
                }
                if generated_children >= children_count {
                    return;
                }

                let parent_rotation = Self::get_rotation(parent);
                let origin = Self::get_child_origin(parent, parent_rotation, child_entry);
                let node_type = child_entry
                    .get_types()
                    .get_random(random)
                    .expect("continueNode: the child type map is empty (Java: NPE)")
                    .clone();
                if !Self::is_matching_height(
                    seed,
                    origin,
                    node_type.get_height_condition().as_ref(),
                ) {
                    continue;
                }

                let yaw = Self::get_child_yaw(parent, parent_rotation, child_entry, random);
                let pitch = (child_entry.get_pitch_modifier())(parent.get_pitch(), random);
                let hash = random.next_int();

                let shape = node_type.generate_cave_node_shape(
                    random,
                    cave.get_cave_type().as_ref(),
                    Some(parent),
                    Some(child_entry),
                    origin,
                    yaw,
                    pitch,
                );
                if !Self::is_matching_height(
                    seed,
                    shape.get_end(),
                    node_type.get_height_condition().as_ref(),
                ) {
                    continue;
                }

                // Test the BiomeMask to see if the node can generate/populate
                // here.
                let biome_mask_result = Self::get_biome_mask_result(
                    seed,
                    chunk_generator,
                    biome_mask.as_ref(),
                    shape.get_end(),
                );
                if !can_generate(biome_mask_result) {
                    if can_continue(biome_mask_result) {
                        // Attempt another direction/shape for the current
                        // CaveNodeChildEntry
                        continue;
                    }
                    // Move onto the next CaveNodeChildEntry
                    break;
                }

                let node = Rc::new(CaveNode::new(
                    hash,
                    Rc::clone(&node_type),
                    Rc::from(shape),
                    yaw,
                    pitch,
                ));
                if node.get_shape().has_geometry() && can_populate(biome_mask_result) {
                    self.generate_prefabs(seed, chunk_generator, parent, &node);
                    cave.add_node(&node);
                }

                let next_depth = Self::get_next_depth(child_entry, depth, random);
                self.continue_node(seed, chunk_generator, cave, &node, next_depth, random);
                generated_children += 1;
            }
        }
    }

    /// Java `protected int getChildrenCount(@Nonnull CaveNodeType type,
    /// Random random)`.
    pub fn get_children_count(node_type: &CaveNodeType, random: &mut FastRandom) -> i32 {
        match node_type.get_children_count_bounds() {
            None => i32::MAX,
            Some(count_array) => math_util::floor(count_array.get_value_random(random)),
        }
    }

    /// Java `protected CaveNodeType.CaveNodeChildEntry[]
    /// getChildEntriesRandomized(@Nonnull CaveNodeType type, @Nonnull Random
    /// random)`.
    ///
    /// The Java `System.arraycopy` is modelled by cloning the entries (the
    /// Java copy is shallow; every field is a reference or a value).
    pub fn get_child_entries_randomized(
        node_type: &CaveNodeType,
        random: &mut FastRandom,
    ) -> Vec<CaveNodeChildEntry> {
        let mut child_entries = node_type.get_children().to_vec();
        // We only have to randomize if we have a limited amount of children to
        // always pick a different set of children.
        if node_type.get_children_count_bounds().is_none() || child_entries.is_empty() {
            child_entries
        } else {
            shuffle_array(&mut child_entries, random);
            child_entries
        }
    }

    /// Java `protected int getRepeatCounter(@Nonnull
    /// CaveNodeType.CaveNodeChildEntry entry, Random random)`.
    pub fn get_repeat_counter(entry: &CaveNodeChildEntry, random: &mut FastRandom) -> i32 {
        math_util::floor(entry.get_repeat().get_value_random(random))
    }

    /// Java `@Nullable protected PrefabRotation getRotation(@Nonnull CaveNode
    /// caveNode)`.
    ///
    /// Java: `shape instanceof PrefabCaveNodeShape` → the shape's rotation.
    /// The Rust [`CaveNodeShape`](crate::cave::shape::cave_node_shape::CaveNodeShape)
    /// seam exposes the same check as
    /// [`as_prefab_rotation`](crate::cave::shape::cave_node_shape::CaveNodeShape::as_prefab_rotation)
    /// (`None` for every shape but the prefab one).
    pub fn get_rotation(cave_node: &CaveNode) -> Option<PrefabRotation> {
        cave_node.get_shape().as_prefab_rotation()
    }

    /// Java `protected Vector3d getChildOrigin(@Nonnull CaveNode parentNode,
    /// @Nullable PrefabRotation parentRotation, @Nonnull
    /// CaveNodeType.CaveNodeChildEntry childEntry)`.
    pub fn get_child_origin(
        parent_node: &CaveNode,
        parent_rotation: Option<PrefabRotation>,
        child_entry: &CaveNodeChildEntry,
    ) -> Vector3d {
        let mut vector = parent_node.get_end();

        let anchor = child_entry.get_anchor();
        if anchor == Vector3d::ZERO {
            return vector;
        }

        // Assign to `vector` to save having to clone the anchor.
        vector = anchor;

        if let Some(rotation) = parent_rotation {
            if rotation != PrefabRotation::Rotation0 {
                vector.x -= 0.5;
                vector.y -= 0.5;
                vector.z -= 0.5;
                rotation.rotate(&mut vector);
                vector.x += 0.5;
                vector.y += 0.5;
                vector.z += 0.5;
            }
        }

        let (tx, ty, tz) = (vector.x, vector.y, vector.z);
        parent_node.get_shape().get_anchor(vector, tx, ty, tz)
    }

    /// Java `protected float getChildYaw(@Nonnull CaveNode parentNode,
    /// @Nullable PrefabRotation parentRotation, @Nonnull
    /// CaveNodeType.CaveNodeChildEntry childEntry, Random random)`.
    pub fn get_child_yaw(
        parent_node: &CaveNode,
        parent_rotation: Option<PrefabRotation>,
        child_entry: &CaveNodeChildEntry,
        random: &mut FastRandom,
    ) -> f32 {
        let yaw = child_entry
            .get_yaw_mode()
            .combine(parent_node.get_yaw(), parent_rotation);

        (child_entry.get_yaw_modifier())(yaw, random)
    }

    /// Java `protected boolean shouldGenerateChild(@Nonnull
    /// CaveNodeType.CaveNodeChildEntry entry, @Nonnull Random random)`.
    pub fn should_generate_child(entry: &CaveNodeChildEntry, random: &mut FastRandom) -> bool {
        random.next_double() < entry.get_chance()
    }

    /// Java `protected boolean isMatchingHeight(int seed, @Nonnull Vector3d
    /// vec, @Nonnull ICoordinateCondition condition)`.
    ///
    /// Deviation: the Java method short-circuits the two default conditions by
    /// identity; the Rust port evaluates the condition (same value, and the
    /// conditions are pure).
    pub fn is_matching_height(
        seed: i32,
        vec: Vector3d,
        condition: &dyn ICoordinateCondition,
    ) -> bool {
        let x = math_util::floor(vec.x);
        let y = math_util::floor(vec.y);
        let z = math_util::floor(vec.z);
        condition.eval3(seed, x, y, z)
    }

    /// Java `protected int getNextDepth(@Nonnull
    /// CaveNodeType.CaveNodeChildEntry entry, int depth, Random random)`.
    pub fn get_next_depth(entry: &CaveNodeChildEntry, depth: i32, random: &mut FastRandom) -> i32 {
        let next_depth = depth.wrapping_sub(1);
        if let Some(children_limit) = entry.get_children_limit() {
            let limit = math_util::floor(children_limit.get_value_random(random));
            if limit < next_depth {
                return limit;
            }
        }
        next_depth
    }

    /// Java `protected void generatePrefabs(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, CaveNode parent, @Nonnull CaveNode node)`.
    pub fn generate_prefabs(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        parent: &CaveNode,
        node: &CaveNode,
    ) {
        // Java: `final Random random = ChunkGenerator.getResource().getRandom();
        // random.setSeed(seed + node.getSeedOffset());` — the shared resource
        // random is replaced by an identically seeded local one (see the
        // module docs).
        let mut random = FastRandom::new(seed.wrapping_add(node.get_seed_offset()) as i64);

        let Some(container) = node.get_cave_node_type().get_prefab_container() else {
            return;
        };
        for entry in container.get_entries() {
            self.generate_prefab(
                seed,
                chunk_generator,
                Some(parent),
                node,
                entry,
                &mut random,
            );
        }
    }

    /// Java `protected void generatePrefab(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, @Nullable CaveNode parent, @Nonnull CaveNode caveNode,
    /// @Nonnull CavePrefabContainer.CavePrefabEntry entry, @Nonnull Random
    /// random)`.
    ///
    /// Deviation: Java hands the possibly-`null` `entry.getPrefab(…)` to the
    /// `CavePrefab` constructor; the Rust prefab carries a non-null supplier,
    /// so a missing supplier skips the placement (the prefab paste is Phase 5
    /// anyway).
    pub fn generate_prefab(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        parent: Option<&CaveNode>,
        cave_node: &CaveNode,
        entry: &crate::cave::prefab::cave_prefab_container::CavePrefabEntry,
        random: &mut FastRandom,
    ) {
        assert!(
            cave_node.get_shape().has_geometry(),
            "Cannot generate cave-prefab inside an invalid shape"
        );

        let config = entry.get_config();
        let iterations = config.get_iterations(random.next_double());
        for _ in 0..iterations {
            let bounds = cave_node.get_bounds();
            let x = bounds.random_x(random);
            let z = bounds.random_z(random);
            if !Self::is_matching_biome(
                seed,
                chunk_generator,
                config.get_biome_mask().as_ref(),
                x,
                z,
            ) {
                continue;
            }

            if !config.is_matching_noise_density(seed, x, z) {
                continue;
            }
            let y = config.get_height(seed, x, z, cave_node);
            if y == crate::cave::cave_prefab_placement::CavePrefabPlacement::NO_HEIGHT
                || !config.is_matching_height(seed, x, y, z, random)
            {
                continue;
            }
            if let Some(parent) = parent {
                if parent
                    .get_shape()
                    .should_replace(seed, x as f64, z as f64, y)
                {
                    continue; // dont spawn prefab in overlap
                }
            }

            let Some(prefab) = entry.get_prefab(random.next_double()) else {
                continue;
            };
            let rotation = config.get_rotation(random);

            let entity = CavePrefab::new(
                Rc::clone(prefab),
                rotation,
                Rc::clone(config.get_biome_mask()),
                config.get_block_mask().clone(),
                x,
                y,
                z,
            );
            cave_node.add_prefab(Rc::new(entity));
        }
    }

    /// Java `protected boolean isMatchingBiome(int seed, @Nonnull
    /// ChunkGenerator chunkGenerator, @Nonnull IIntCondition condition, int
    /// x, int z)`.
    ///
    /// Deviation: see [`Self::is_matching_height`] — the default-condition
    /// identity shortcut is replaced by a plain evaluation.
    pub fn is_matching_biome(
        seed: i32,
        chunk_generator: &ChunkGenerator,
        condition: &dyn procedural::noise::condition::IIntCondition,
        x: i32,
        z: i32,
    ) -> bool {
        let biome_result = chunk_generator.get_zone_biome_result_at(seed, x, z);
        condition.eval(
            biome_result
                .get_biome()
                .expect("isMatchingBiome: the column has no biome (Java: NPE)")
                .get_id(),
        )
    }

    /// Java `protected int getBiomeMaskResult(int seed, @Nonnull
    /// ChunkGenerator chunkGenerator, @Nonnull Int2FlagsCondition mask,
    /// @Nonnull Vector3d vec)`.
    ///
    /// Deviation: see [`Self::is_matching_height`] — the default-mask
    /// identity shortcut is replaced by a plain evaluation.
    pub fn get_biome_mask_result(
        seed: i32,
        chunk_generator: &ChunkGenerator,
        mask: &dyn Int2FlagsCondition,
        vec: Vector3d,
    ) -> i32 {
        let x = math_util::floor(vec.x);
        let z = math_util::floor(vec.z);
        let biome_result = chunk_generator.get_zone_biome_result_at(seed, x, z);
        mask.eval(
            biome_result
                .get_biome()
                .expect("getBiomeMaskResult: the column has no biome (Java: NPE)")
                .get_id(),
        )
    }
}

impl CaveGenerator for CaveGeneratorImpl {
    /// Java `CaveType[] getCaveTypes()`.
    fn get_cave_types(&self) -> &[Rc<CaveType>] {
        &self.cave_types
    }

    /// Java `@Nonnull Cave generate(int seed, ChunkGenerator chunkGenerator,
    /// CaveType caveType, int x, int y, int z)`.
    fn generate(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        cave_type: &CaveType,
        x: i32,
        y: i32,
        z: i32,
    ) -> Cave {
        CaveGeneratorImpl::generate(self, seed, chunk_generator, cave_type, x, y, z)
    }
}

#[cfg(test)]
mod tests {
    //! Deterministic sanity tests for the graph growth. There are no Java
    //! `CaveGenerator` tests in the source tree, so the tests pin the
    //! recursion/flag semantics on a fixed seed instead (node counts, chunk
    //! bucketing, repeatability).

    use std::cell::Cell;
    use std::rc::Rc;

    use hytale_math::fast_random::FastRandom;
    use procedural::noise::condition::DefaultCoordinateCondition;
    use procedural::noise::supplier::DoubleRange;

    use super::*;
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::cave::cave_biome_mask_flags::defaults;
    use crate::cave::cave_node_type::CaveNodeType;
    use crate::cave::cave_type::CaveType;
    use crate::cave::shape::cave_node_shape::CaveNodeShape;
    use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
    use crate::cave::test_stubs::FixedPointGenerator;
    use crate::chunk::chunk_generator::test_stubs::ConstantNoise;
    use crate::chunk::chunk_generator::test_stubs::StubInterpreter;
    use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
    use crate::chunk::chunk_generator_resource::ChunkGeneratorResource;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use crate::util::bounds::WorldBounds;
    use crate::util::condition::flag::{ConstantInt2Flags, Int2FlagsCondition};
    use crate::util::condition::BlockMaskCondition;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zone::zone_pattern_provider::ZonePatternProvider;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::supplier::FloatRange;
    use worldgen_model::chunk::GeneratedChunk;

    /// A shape whose bounds start at the origin and whose end is `origin +
    /// (32, 0, 0)` — every node advances one chunk.
    struct GrowingShape {
        bounds: WorldBounds,
        start: Vector3d,
        end: Vector3d,
        /// When set, `populate_chunk` writes one `CAVE` block (the Phase-4c
        /// carving stand-in used by the stage test).
        carve: bool,
    }

    impl CaveNodeShape for GrowingShape {
        fn get_start(&self) -> Vector3d {
            self.start
        }

        fn get_end(&self) -> Vector3d {
            self.end
        }

        fn get_anchor(&self, vector: Vector3d, _tx: f64, _ty: f64, _tz: f64) -> Vector3d {
            vector
        }

        fn get_bounds(&self) -> &WorldBounds {
            &self.bounds
        }

        fn should_replace(&self, _seed: i32, _x: f64, _z: f64, _y: i32) -> bool {
            false
        }

        fn get_floor_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
            self.bounds.min_y as f64
        }

        fn get_ceiling_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
            self.bounds.max_y as f64
        }

        fn populate_chunk(
            &self,
            _seed: i32,
            execution: &mut crate::chunk::chunk_generator_execution::ChunkGeneratorExecution,
            _cave: &Cave,
            _node: &CaveNode,
            _random: &mut FastRandom,
        ) {
            if self.carve {
                // The Phase-4c stand-in: write a single cave block at the
                // bottom-centre of the node.
                execution.set_block(
                    self.bounds.min_x,
                    self.bounds.min_y - 1,
                    self.bounds.min_z,
                    worldgen_model::block_priority::CAVE,
                    42,
                );
            }
        }
    }

    /// A shape generator that makes every node a one-chunk segment advancing by
    /// 32 blocks along x. It draws once from the random, like the real shape
    /// generators do, so the draw order is exercised.
    struct GrowingShapeGenerator {
        carve: bool,
    }

    impl CaveNodeShapeGenerator for GrowingShapeGenerator {
        fn generate_cave_node_shape(
            &self,
            random: &mut FastRandom,
            _cave_type: &CaveType,
            _parent_node: Option<&CaveNode>,
            _child_entry: Option<&CaveNodeChildEntry>,
            origin: Vector3d,
            _yaw: f32,
            _pitch: f32,
        ) -> Box<dyn CaveNodeShape> {
            let jitter = random.next_double();
            let x = math_util::floor(origin.x + jitter);
            let y = math_util::floor(origin.y);
            let z = math_util::floor(origin.z);
            Box::new(GrowingShape {
                bounds: WorldBounds::with_bounds(x, y, z, x + 15, y + 15, z + 15),
                start: origin,
                end: Vector3d::new(origin.x + 32.0, origin.y, origin.z),
                carve: self.carve,
            })
        }
    }

    /// A node type with a fixed priority, count/repeat/chance and children.
    fn node_type(
        name: &str,
        children_count: Option<f64>,
        children: Vec<CaveNodeChildEntry>,
        priority: i32,
    ) -> Rc<CaveNodeType> {
        node_type_with_shape(
            name,
            children_count,
            children,
            priority,
            Rc::new(GrowingShapeGenerator { carve: false }),
        )
    }

    /// A node type that uses the given shape generator.
    fn node_type_with_shape(
        name: &str,
        children_count: Option<f64>,
        children: Vec<CaveNodeChildEntry>,
        priority: i32,
        shape: Rc<dyn CaveNodeShapeGenerator>,
    ) -> Rc<CaveNodeType> {
        let mut builder = WeightedMap::<BlockFluidEntry>::builder();
        builder.put(BlockFluidEntry::EMPTY, 1.0);
        let fillings: Rc<dyn IWeightedMap<BlockFluidEntry>> = Rc::new(builder.build());
        let node_type = CaveNodeType::new(
            name.to_string(),
            None,
            fillings,
            shape,
            Rc::new(DefaultCoordinateCondition::True),
            children_count.map(|value| {
                Rc::new(DoubleRange::constant(value))
                    as Rc<dyn procedural::noise::supplier::IDoubleRange>
            }),
            Vec::new(),
            priority,
            worldgen_model::chunk::ENVIRONMENT_NOT_SET,
        );
        node_type.set_children(children);
        Rc::new(node_type)
    }

    /// A child entry pointing at `types` with a constant repeat and chance.
    fn child_entry(types: Rc<CaveNodeType>, repeat: f64, chance: f64) -> CaveNodeChildEntry {
        let mut builder = WeightedMap::<Rc<CaveNodeType>>::builder();
        builder.put(types, 1.0);
        let types: Rc<dyn IWeightedMap<Rc<CaveNodeType>>> = Rc::new(builder.build());
        CaveNodeChildEntry::new(
            types,
            Vector3d::ZERO,
            Vector3d::ZERO,
            vec![PrefabRotation::Rotation0],
            None,
            Rc::new(DoubleRange::constant(repeat)),
            Rc::new(|current: f32, _random: &mut FastRandom| current),
            Rc::new(|current: f32, _random: &mut FastRandom| current),
            chance,
            crate::cave::cave_yaw_mode::CaveYawMode::Node,
        )
    }

    /// A linear chain: every node spawns exactly one child until the depth is
    /// exhausted.
    fn linear_chain() -> Rc<CaveNodeType> {
        // The self reference is created with `Rc::new_cyclic`-like two-step:
        // build the child entry lazily through a `WeightedNode` wrapper.
        // Instead, build a chain of `depth + 1` distinct node types so that the
        // test needs no cyclic handles.
        let leaf = node_type("leaf", None, Vec::new(), 0);
        let mut next = leaf;
        for level in 0..8 {
            next = node_type(
                &format!("level-{level}"),
                None,
                vec![child_entry(Rc::clone(&next), 1.0, 1.0)],
                level,
            );
        }
        next
    }

    /// A cave type around the entry node with the given masks and depth.
    fn test_cave_type(
        entry: Rc<CaveNodeType>,
        biome_mask: Rc<dyn Int2FlagsCondition>,
        depth: f64,
    ) -> Rc<CaveType> {
        Rc::new(CaveType::new(
            "cave".to_string(),
            entry,
            Rc::new(FloatRange::constant(0.0)),
            Rc::new(FloatRange::constant(0.0)),
            Rc::new(DoubleRange::constant(depth)),
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            Rc::new(FixedPointGenerator),
            biome_mask,
            BlockMaskCondition::new(),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            None,
            None,
            crate::cave::cave_type::FluidLevel::EMPTY,
            0,
            false,
            false,
            64.0,
        ))
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

    fn tile_biome() -> Rc<TileBiome> {
        Rc::new(TileBiome::new(
            7,
            "biome-7".to_string(),
            Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint_container(0x112233),
            environment_container(5),
            Rc::new(WaterContainer::new(Vec::new())),
            fade_container(),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ))
    }

    /// A chunk generator whose single zone carries `generator`.
    fn chunk_generator_with(generator: Rc<dyn CaveGenerator>) -> Rc<ChunkGenerator> {
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(tile_biome(), 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
        let zone = Rc::new(Zone {
            id: 3,
            name: "zone-3".to_string(),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            cave_generator: Some(generator),
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

    #[test]
    fn a_linear_chain_grows_one_node_per_depth_level() {
        let cave_type = test_cave_type(
            linear_chain(),
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            3.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        // root + one child per depth level
        assert_eq!(cave.get_node_count(), 4);
        assert!(Rc::ptr_eq(cave.get_cave_type(), &cave_type));
    }

    #[test]
    fn a_zero_depth_cave_has_only_the_entry_node() {
        let cave_type = test_cave_type(
            linear_chain(),
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            0.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        assert_eq!(cave.get_node_count(), 1);
    }

    #[test]
    fn the_nodes_are_bucketed_into_the_chunks_they_touch() {
        let cave_type = test_cave_type(
            linear_chain(),
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            3.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        // Every node advances by 32 blocks (the `GrowingShapeGenerator`
        // jitter is < 1, so node n starts in chunk n).
        for chunk in 0..4 {
            let index = hytale_math::chunk_util::index_chunk(chunk, 0);
            let nodes = cave
                .get_cave_nodes(index)
                .unwrap_or_else(|| panic!("chunk {chunk} has no cave nodes"));
            assert_eq!(nodes.len(), 1, "one node per chunk bucket");
        }
        assert!(!cave.contains(hytale_math::chunk_util::index_chunk(4, 0)));
    }

    #[test]
    fn the_growth_is_deterministic_for_a_fixed_seed() {
        let make = || {
            let cave_type = test_cave_type(
                linear_chain(),
                Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
                5.0,
            );
            let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
            let chunk_generator =
                chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));
            let cave = generator.generate(7, &chunk_generator, &cave_type, 0, 40, 0);
            let mut buckets = Vec::new();
            for chunk in 0..8 {
                buckets.push(
                    cave.get_cave_nodes(hytale_math::chunk_util::index_chunk(chunk, 0))
                        .map(|nodes| nodes.len())
                        .unwrap_or(0),
                );
            }
            (cave.get_node_count(), buckets)
        };

        assert_eq!(make(), make());
    }

    #[test]
    fn a_denied_biome_mask_grows_no_nodes() {
        let cave_type = test_cave_type(
            linear_chain(),
            Rc::new(ConstantInt2Flags::new(defaults::DISALLOW_ALL)),
            3.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        assert_eq!(cave.get_node_count(), 0);
    }

    #[test]
    fn nodes_without_the_populate_flag_are_not_added_but_still_grow() {
        // GENERATE | CONTINUE: the graph grows, but no node geometry is added.
        let cave_type = test_cave_type(
            linear_chain(),
            Rc::new(ConstantInt2Flags::new(
                defaults::ALLOW_ALL & !crate::cave::cave_biome_mask_flags::POPULATE,
            )),
            3.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        assert_eq!(cave.get_node_count(), 0);
    }

    #[test]
    fn a_zero_chance_child_is_skipped() {
        let leaf = node_type("leaf", None, Vec::new(), 0);
        let entry = node_type(
            "root",
            None,
            vec![child_entry(Rc::clone(&leaf), 1.0, 0.0)],
            0,
        );
        let cave_type = test_cave_type(
            entry,
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            4.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        assert_eq!(cave.get_node_count(), 1, "only the entry node");
    }

    #[test]
    fn the_child_count_bound_stops_the_sibling_growth() {
        // `childrenCount = 0`: the first accepted child check terminates the
        // branch before any child is generated.
        let leaf = node_type("leaf", None, Vec::new(), 0);
        let entry = node_type(
            "root",
            Some(0.0),
            vec![
                child_entry(Rc::clone(&leaf), 1.0, 1.0),
                child_entry(Rc::clone(&leaf), 1.0, 1.0),
            ],
            0,
        );
        let cave_type = test_cave_type(
            entry,
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            4.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        assert_eq!(cave.get_node_count(), 1, "only the entry node");
    }

    #[test]
    fn the_repeat_counter_repeats_the_child() {
        let leaf = node_type("leaf", None, Vec::new(), 0);
        let entry = node_type(
            "root",
            None,
            vec![child_entry(Rc::clone(&leaf), 3.0, 1.0)],
            0,
        );
        let cave_type = test_cave_type(
            entry,
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            1.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        // root + 3 leaves (the depth 1 bounds the leaves' own growth)
        assert_eq!(cave.get_node_count(), 4);
    }

    #[test]
    fn a_continue_flag_retries_the_same_child_entry() {
        // The first end-point evaluation denies generation but allows another
        // attempt; the second allows it. The mask results therefore have to be
        // consumed in call order: start origin, start end, child end (deny +
        // continue), child end (allow).
        struct ScriptedMask {
            calls: Cell<usize>,
        }
        impl Int2FlagsCondition for ScriptedMask {
            fn eval(&self, _input: i32) -> i32 {
                let call = self.calls.get();
                self.calls.set(call + 1);
                match call {
                    0 => defaults::ALLOW_ALL,
                    1 => defaults::ALLOW_ALL,
                    2 => crate::cave::cave_biome_mask_flags::CONTINUE,
                    _ => defaults::ALLOW_ALL,
                }
            }
        }

        let leaf = node_type("leaf", None, Vec::new(), 0);
        let entry = node_type(
            "root",
            None,
            vec![child_entry(Rc::clone(&leaf), 2.0, 1.0)],
            0,
        );
        let cave_type = test_cave_type(
            entry,
            Rc::new(ScriptedMask {
                calls: Cell::new(0),
            }),
            1.0,
        );
        let generator = CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)]);
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));

        let cave = generator.generate(42, &chunk_generator, &cave_type, 0, 40, 0);
        // root + the second (allowed) repeat of the child entry
        assert_eq!(cave.get_node_count(), 2);
    }

    #[test]
    fn the_execution_runs_the_cave_stage_and_carves_a_cave_block() {
        // A single carving node: the stage test only needs the populator to
        // find the cave and run the node's `populate_chunk`.
        let entry = node_type_with_shape(
            "carver",
            None,
            Vec::new(),
            0,
            Rc::new(GrowingShapeGenerator { carve: true }),
        );
        let cave_type = test_cave_type(
            entry,
            Rc::new(ConstantInt2Flags::new(defaults::ALLOW_ALL)),
            0.0,
        );
        let chunk_generator =
            chunk_generator_with(Rc::new(CaveGeneratorImpl::new(vec![Rc::clone(&cave_type)])));
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, chunk_generator, chunk);
        let mut resource = ChunkGeneratorResource::new();

        execution.execute(42, &mut resource);

        // The entry origin is the generated terrain height (no fixed entry
        // height): the stub interpreter fills the column up to y = 63, so the
        // stand-in shape writes one block at `min_y - 1` = 62.
        assert_eq!(execution.get_block(0, 62, 0), 42);
        assert_eq!(
            execution.get_priority_chunk().get(0, 62, 0),
            worldgen_model::block_priority::CAVE
        );
        // The terrain around it is still the (empty) biome layer.
        assert_eq!(execution.get_block(0, 61, 0), 0);
        assert_eq!(
            execution.get_priority_chunk().get(0, 61, 0),
            worldgen_model::block_priority::FILLING
        );
    }
}
