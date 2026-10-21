// Ported from: `.hytale-source/.../worldgen/cave/prefab/CavePrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.prefab.CavePrefabContainer` —
//! the prefabs a cave node can spawn.
//!
//! ⚠ **SEAM(phase-4b / phase-5)**: the prefab suppliers are CoreServer /
//! loader types (`worldgen.loader.WorldGenPrefabSupplier` and the prefab
//! buffer it hands out). They are not ported yet, so the container talks to
//! them through the [`CavePrefabSupplier`] seam (see
//! `cave/prefab/cave_prefab_supplier.rs`).

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::condition::{ICoordinateCondition, ICoordinateRndCondition};
use procedural::noise::supplier::IDoubleRange;
use procedural::noise::support::IIntCondition;

use crate::biome::biome::Biome;
use crate::cave::cave_prefab_placement::CavePrefabPlacement;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::prefab::cave_prefab_supplier::{CavePrefabKey, CavePrefabSupplier};
use crate::supplier::IDoubleCoordinateHashSupplier;
use crate::util::condition::BlockMaskCondition;
use crate::weighted_map::IWeightedMap;

/// Java `class CavePrefabContainer`.
pub struct CavePrefabContainer {
    /// Java `protected final CavePrefabEntry[] entries`.
    pub entries: Vec<CavePrefabEntry>,
}

impl CavePrefabContainer {
    /// Java `CavePrefabContainer(CavePrefabEntry[] entries)`.
    pub fn new(entries: Vec<CavePrefabEntry>) -> Self {
        Self { entries }
    }

    /// Java `CavePrefabEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[CavePrefabEntry] {
        &self.entries
    }
}

/// Java `static class CavePrefabContainer.CavePrefabEntry`.
pub struct CavePrefabEntry {
    /// Java `protected final IWeightedMap<WorldGenPrefabSupplier> prefabs`.
    pub prefabs: Rc<dyn IWeightedMap<CavePrefabKey>>,
    /// Java `protected final CavePrefabConfig config`.
    pub config: CavePrefabConfig,
}

impl CavePrefabEntry {
    /// Java `CavePrefabEntry(IWeightedMap<WorldGenPrefabSupplier> prefabs,
    /// CavePrefabConfig config)`.
    pub fn new(prefabs: Rc<dyn IWeightedMap<CavePrefabKey>>, config: CavePrefabConfig) -> Self {
        Self { prefabs, config }
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> getPrefabs()`.
    pub fn get_prefabs(&self) -> &Rc<dyn IWeightedMap<CavePrefabKey>> {
        &self.prefabs
    }

    /// Java `@Nullable WorldGenPrefabSupplier getPrefab(double random)`.
    pub fn get_prefab(&self, random: f64) -> Option<&Rc<dyn CavePrefabSupplier>> {
        self.prefabs.get(random).map(|key| &key.supplier)
    }

    /// Java `CavePrefabConfig getConfig()`.
    pub fn get_config(&self) -> &CavePrefabConfig {
        &self.config
    }
}

/// Java `static class CavePrefabContainer.CavePrefabEntry.CavePrefabConfig`.
pub struct CavePrefabConfig {
    /// Java `protected final PrefabRotation[] rotations`.
    pub rotations: Vec<crate::prefab::prefab_rotation::PrefabRotation>,
    /// Java `protected final CavePrefabPlacement placement`.
    pub placement: CavePrefabPlacement,
    /// Java `protected final IIntCondition biomeMask`.
    pub biome_mask: Rc<dyn IIntCondition>,
    /// Java `protected final BlockMaskCondition blockMask`.
    pub block_mask: BlockMaskCondition,
    /// Java `protected final IDoubleRange iterations`.
    pub iterations: Rc<dyn IDoubleRange>,
    /// Java `protected final IDoubleCoordinateHashSupplier
    /// displacementSupplier`.
    pub displacement_supplier: Rc<dyn IDoubleCoordinateHashSupplier>,
    /// Java `protected final ICoordinateCondition maskCondition`.
    pub mask_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final ICoordinateRndCondition heightCondition`.
    pub height_condition: Rc<dyn ICoordinateRndCondition>,
}

impl CavePrefabConfig {
    /// Java `CavePrefabConfig(PrefabRotation[] rotations, CavePrefabPlacement
    /// placement, IIntCondition biomeMask, BlockMaskCondition blockMask,
    /// IDoubleRange iterations, IDoubleCoordinateHashSupplier
    /// displacementSupplier, ICoordinateCondition maskCondition,
    /// ICoordinateRndCondition heightCondition)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rotations: Vec<crate::prefab::prefab_rotation::PrefabRotation>,
        placement: CavePrefabPlacement,
        biome_mask: Rc<dyn IIntCondition>,
        block_mask: BlockMaskCondition,
        iterations: Rc<dyn IDoubleRange>,
        displacement_supplier: Rc<dyn IDoubleCoordinateHashSupplier>,
        mask_condition: Rc<dyn ICoordinateCondition>,
        height_condition: Rc<dyn ICoordinateRndCondition>,
    ) -> Self {
        Self {
            rotations,
            placement,
            biome_mask,
            block_mask,
            iterations,
            displacement_supplier,
            mask_condition,
            height_condition,
        }
    }

    /// Java `PrefabRotation getRotation(Random random)` —
    /// `rotations[random.nextInt(rotations.length)]`.
    ///
    /// Panics when the rotation list is empty — the Java code throws an
    /// `ArrayIndexOutOfBoundsException` in the same case.
    pub fn get_rotation(
        &self,
        random: &mut FastRandom,
    ) -> crate::prefab::prefab_rotation::PrefabRotation {
        assert!(
            !self.rotations.is_empty(),
            "CavePrefabConfig.getRotation: the rotation list is empty (Java: ArrayIndexOutOfBoundsException)"
        );
        self.rotations[random.next_int_bound(self.rotations.len() as i32) as usize]
    }

    /// Java `IIntCondition getBiomeMask()`.
    pub fn get_biome_mask(&self) -> &Rc<dyn IIntCondition> {
        &self.biome_mask
    }

    /// Java `BlockMaskCondition getBlockMask()`.
    pub fn get_block_mask(&self) -> &BlockMaskCondition {
        &self.block_mask
    }

    /// Java `int getIterations(double random)` —
    /// `MathUtil.floor(iterations.getValue(random))`.
    pub fn get_iterations(&self, random: f64) -> i32 {
        hytale_math::math_util::floor(self.iterations.get_value(random))
    }

    /// Java `double getDisplacement(int seed, int x, int z, CaveNode
    /// caveNode)` — `displacementSupplier.get(seed, x, z, seed +
    /// caveNode.getSeedOffset())`.
    pub fn get_displacement(&self, seed: i32, x: i32, z: i32, cave_node: &CaveNode) -> f64 {
        self.displacement_supplier.get(
            seed,
            x,
            z,
            (seed as i64).wrapping_add(cave_node.get_seed_offset() as i64),
        )
    }

    /// Java `int getHeight(int seed, int x, int z, CaveNode caveNode)`.
    pub fn get_height(&self, seed: i32, x: i32, z: i32, cave_node: &CaveNode) -> i32 {
        // Java passes the int coordinates to the `double` parameters of the
        // placement function.
        let y = self
            .placement
            .get_function()
            .generate(seed, x as f64, z as f64, cave_node);
        if y == CavePrefabPlacement::NO_HEIGHT {
            CavePrefabPlacement::NO_HEIGHT
        } else {
            // Java: `(int) (y + getDisplacement(seed, x, z, caveNode))`.
            (y as f64 + self.get_displacement(seed, x, z, cave_node)) as i32
        }
    }

    /// Java `boolean isMatchingNoiseDensity(int seed, int x, int z)` —
    /// `maskCondition.eval(seed, x, z)` (the three-argument overload, so the
    /// z coordinate is passed as the `y` argument).
    pub fn is_matching_noise_density(&self, seed: i32, x: i32, z: i32) -> bool {
        self.mask_condition.eval2(seed, x, z)
    }

    /// Java `boolean isMatchingHeight(int seed, int x, int y, int z, Random
    /// random)`.
    pub fn is_matching_height(
        &self,
        seed: i32,
        x: i32,
        y: i32,
        z: i32,
        random: &mut FastRandom,
    ) -> bool {
        self.height_condition.eval(seed, x, z, y, random)
    }

    /// Java `boolean isMatchingBiome(Biome biome)` —
    /// `biomeMask.eval(biome.getId())`.
    pub fn is_matching_biome(&self, biome: &dyn Biome) -> bool {
        self.biome_mask.eval(biome.get_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::cave_prefab_placement::CavePrefabPlacement;
    use crate::cave::test_stubs::{cave_node_type, StubPrefabSupplier, StubShape};
    use crate::prefab::prefab_rotation::PrefabRotation;
    use crate::supplier::ConstantDoubleCoordinateHashSupplier;
    use crate::util::bounds::WorldBounds;
    use crate::weighted_map::WeightedMap;
    use procedural::noise::condition::{DefaultCoordinateCondition, DefaultCoordinateRndCondition};
    use procedural::noise::supplier::DoubleRange;
    use procedural::noise::support::AlwaysTrueIntCondition;

    /// A config with the given iterations and placement.
    fn config(iterations: f64, placement: CavePrefabPlacement) -> CavePrefabConfig {
        CavePrefabConfig::new(
            vec![PrefabRotation::Rotation0, PrefabRotation::Rotation90],
            placement,
            Rc::new(AlwaysTrueIntCondition),
            BlockMaskCondition::new(),
            Rc::new(DoubleRange::constant(iterations)),
            Rc::new(ConstantDoubleCoordinateHashSupplier::ZERO),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateRndCondition::True),
        )
    }

    /// A cave node with the given floor and ceiling.
    fn node(floor: f64, ceiling: f64) -> Rc<CaveNode> {
        Rc::new(CaveNode::new(
            5,
            cave_node_type(0),
            Rc::new(StubShape::new(
                WorldBounds::with_bounds(0, 0, 0, 0, 10, 0),
                floor,
                ceiling,
            )),
            0.0,
            0.0,
        ))
    }

    #[test]
    fn the_iterations_are_floored() {
        let config = config(3.75, CavePrefabPlacement::Floor);
        assert_eq!(config.get_iterations(0.0), 3);
    }

    #[test]
    fn the_rotation_is_picked_at_random() {
        let config = config(1.0, CavePrefabPlacement::Floor);
        let mut random = hytale_math::fast_random::FastRandom::new(5);
        let rotation = config.get_rotation(&mut random);
        assert!(matches!(
            rotation,
            PrefabRotation::Rotation0 | PrefabRotation::Rotation90
        ));
    }

    #[test]
    fn the_height_comes_from_the_placement() {
        let config = config(1.0, CavePrefabPlacement::Ceiling);
        let node = node(1.5, 9.5);
        assert_eq!(config.get_height(0, 0, 0, &node), 9);
    }

    #[test]
    fn the_floor_placement_uses_the_floor_of_the_node() {
        let config = config(1.0, CavePrefabPlacement::Floor);
        let node = node(1.5, 9.5);
        assert_eq!(config.get_height(0, 0, 0, &node), 1);
    }

    #[test]
    fn the_default_placement_uses_the_middle_of_the_node() {
        let config = config(1.0, CavePrefabPlacement::Default);
        let node = node(1.5, 9.5);
        // `fractionY(0.5)` of the bounds `0..10` is `5`.
        assert_eq!(config.get_height(0, 0, 0, &node), 5);
    }

    #[test]
    fn the_conditions_are_evaluated() {
        let config = config(1.0, CavePrefabPlacement::Floor);
        assert!(config.is_matching_noise_density(0, 1, 2));
        let mut random = hytale_math::fast_random::FastRandom::new(5);
        assert!(config.is_matching_height(0, 1, 2, 3, &mut random));
    }

    #[test]
    fn the_prefab_of_an_entry_is_picked_from_the_weighted_map() {
        let mut builder = WeightedMap::<CavePrefabKey>::builder();
        builder.put(
            CavePrefabKey::new(Rc::new(StubPrefabSupplier {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1,
                max_y: 1,
                max_z: 1,
            })),
            1.0,
        );
        let prefabs: Rc<dyn IWeightedMap<CavePrefabKey>> = Rc::new(builder.build());

        let entry = CavePrefabEntry::new(prefabs, config(1.0, CavePrefabPlacement::Floor));
        assert!(entry.get_prefab(0.5).is_some());

        let container = CavePrefabContainer::new(vec![entry]);
        assert_eq!(container.get_entries().len(), 1);
    }
}
