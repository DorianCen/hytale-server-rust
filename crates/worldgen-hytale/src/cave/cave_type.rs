// Ported from: `.hytale-source/.../worldgen/cave/CaveType.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveType` — the description
//! of a cave: where it starts, how it turns, how deep it goes and which
//! biome mask it obeys.

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use procedural::json::seed_string::java_string_hash_code;
use procedural::noise::condition::{ICoordinateCondition, IHeightThresholdInterpreter};
use procedural::noise::point::IPointGenerator;
use procedural::noise::supplier::IDoubleRange;
use procedural::noise::support::NoiseProperty;

use crate::cave::cave_node_type::CaveNodeType;
use crate::supplier::IFloatRange;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::condition::flag::Int2FlagsCondition;
use crate::util::condition::BlockMaskCondition;

/// Java `class CaveType`.
#[derive(Clone)]
pub struct CaveType {
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final CaveNodeType entryNodeType`.
    entry_node_type: Rc<CaveNodeType>,
    /// Java `protected final IFloatRange yaw`.
    yaw: Rc<dyn IFloatRange>,
    /// Java `protected final IFloatRange pitch`.
    pitch: Rc<dyn IFloatRange>,
    /// Java `protected final IDoubleRange depth`.
    depth: Rc<dyn IDoubleRange>,
    /// Java `protected final IHeightThresholdInterpreter heightFactors`.
    height_factors: Rc<dyn IHeightThresholdInterpreter>,
    /// Java `protected final IPointGenerator pointGenerator`.
    point_generator: Rc<dyn IPointGenerator>,
    /// Java `protected final Int2FlagsCondition biomeMask`.
    biome_mask: Rc<dyn Int2FlagsCondition>,
    /// Java `protected final BlockMaskCondition blockMask`.
    block_mask: BlockMaskCondition,
    /// Java `protected final ICoordinateCondition mapCondition`.
    map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final ICoordinateCondition heightCondition`.
    height_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final IDoubleRange fixedEntryHeight` — nullable.
    fixed_entry_height: Option<Rc<dyn IDoubleRange>>,
    /// Java `protected final NoiseProperty fixedEntryHeightNoise` —
    /// nullable.
    fixed_entry_height_noise: Option<Rc<dyn NoiseProperty>>,
    /// Java `protected final FluidLevel fluidLevel`.
    fluid_level: FluidLevel,
    /// Java `protected final int environment`.
    environment: i32,
    /// Java `protected final boolean surfaceLimited`.
    surface_limited: bool,
    /// Java `protected final boolean submerge`.
    submerge: bool,
    /// Java `protected final double maximumSize`.
    maximum_size: f64,
    /// Java `protected final int hashCode` — the value of `_hashCode()`.
    hash_code: i32,
}

impl CaveType {
    /// Java `CaveType(String name, CaveNodeType entryNodeType, IFloatRange
    /// yaw, IFloatRange pitch, IDoubleRange depth, IHeightThresholdInterpreter
    /// heightFactors, IPointGenerator pointGenerator, Int2FlagsCondition
    /// biomeMask, BlockMaskCondition blockMask, ICoordinateCondition
    /// mapCondition, ICoordinateCondition heightCondition, IDoubleRange
    /// fixedEntryHeight, NoiseProperty fixedEntryHeightNoise, FluidLevel
    /// fluidLevel, int environment, boolean surfaceLimited, boolean submerge,
    /// double maximumSize)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        entry_node_type: Rc<CaveNodeType>,
        yaw: Rc<dyn IFloatRange>,
        pitch: Rc<dyn IFloatRange>,
        depth: Rc<dyn IDoubleRange>,
        height_factors: Rc<dyn IHeightThresholdInterpreter>,
        point_generator: Rc<dyn IPointGenerator>,
        biome_mask: Rc<dyn Int2FlagsCondition>,
        block_mask: BlockMaskCondition,
        map_condition: Rc<dyn ICoordinateCondition>,
        height_condition: Rc<dyn ICoordinateCondition>,
        fixed_entry_height: Option<Rc<dyn IDoubleRange>>,
        fixed_entry_height_noise: Option<Rc<dyn NoiseProperty>>,
        fluid_level: FluidLevel,
        environment: i32,
        surface_limited: bool,
        submerge: bool,
        maximum_size: f64,
    ) -> Self {
        let mut cave_type = Self {
            name,
            entry_node_type,
            yaw,
            pitch,
            depth,
            height_factors,
            point_generator,
            biome_mask,
            block_mask,
            map_condition,
            height_condition,
            fixed_entry_height,
            fixed_entry_height_noise,
            fluid_level,
            environment,
            surface_limited,
            submerge,
            maximum_size,
            hash_code: 0,
        };
        cave_type.hash_code = cave_type.compute_hash_code();
        cave_type
    }

    /// Java `String getName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Java `CaveNodeType getEntryNode()`.
    pub fn get_entry_node(&self) -> &Rc<CaveNodeType> {
        &self.entry_node_type
    }

    /// Java `int getModifiedStartHeight(int seed, int x, int y, int z, Random
    /// random)`.
    pub fn get_modified_start_height(
        &self,
        seed: i32,
        x: i32,
        y: i32,
        z: i32,
        random: &mut FastRandom,
    ) -> i32 {
        let Some(fixed_entry_height) = &self.fixed_entry_height else {
            return y;
        };
        let val = match &self.fixed_entry_height_noise {
            // Java: `fixedEntryHeight.getValue(fixedEntryHeightNoise.get(seed,
            // x, z))`.
            Some(noise) => fixed_entry_height.get_value(noise.get(seed, x as f64, z as f64)),
            // Java: `fixedEntryHeight.getValue(random)`.
            None => fixed_entry_height.get_value_random(random),
        };
        math_util::floor(val)
    }

    /// Java `float getStartPitch(Random random)`.
    pub fn get_start_pitch(&self, random: &mut FastRandom) -> f32 {
        self.pitch.get_value_random(random)
    }

    /// Java `float getStartYaw(Random random)`.
    pub fn get_start_yaw(&self, random: &mut FastRandom) -> f32 {
        self.yaw.get_value_random(random)
    }

    /// Java `int getStartDepth(Random random)` —
    /// `MathUtil.floor(depth.getValue(random))`.
    pub fn get_start_depth(&self, random: &mut FastRandom) -> i32 {
        math_util::floor(self.depth.get_value_random(random))
    }

    /// Java `float getHeightRadiusFactor(int seed, double x, double z, int y)`.
    pub fn get_height_radius_factor(&self, seed: i32, x: f64, z: f64, y: i32) -> f32 {
        self.height_factors.get_threshold(seed, x, z, y)
    }

    /// Java `ICoordinateCondition getHeightCondition()`.
    pub fn get_height_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.height_condition
    }

    /// Java `IPointGenerator getEntryPointGenerator()`.
    pub fn get_entry_point_generator(&self) -> &Rc<dyn IPointGenerator> {
        &self.point_generator
    }

    /// The seed offset of the entry point generator — the seed the cave
    /// entry points are derived from.
    ///
    /// Deviation: Java does not expose the seed offset of the point
    /// generator (the `IPointGenerator` interface has no such getter); the
    /// Rust port exposes it so that the *loader* seed chain of the cave
    /// types can be compared against the Java golden dump
    /// (`testdata/golden/loader.json`, the `caves` section).
    pub fn get_entry_point_seed(&self) -> i32 {
        self.point_generator.get_seed_offset()
    }

    /// Java `Int2FlagsCondition getBiomeMask()`.
    pub fn get_biome_mask(&self) -> &Rc<dyn Int2FlagsCondition> {
        &self.biome_mask
    }

    /// Java `BlockMaskCondition getBlockMask()`.
    pub fn get_block_mask(&self) -> &BlockMaskCondition {
        &self.block_mask
    }

    /// Java `FluidLevel getFluidLevel()`.
    pub fn get_fluid_level(&self) -> &FluidLevel {
        &self.fluid_level
    }

    /// Java `int getEnvironment()`.
    pub fn get_environment(&self) -> i32 {
        self.environment
    }

    /// Java `boolean isSurfaceLimited()`.
    pub fn is_surface_limited(&self) -> bool {
        self.surface_limited
    }

    /// Java `boolean isSubmerge()`.
    pub fn is_submerge(&self) -> bool {
        self.submerge
    }

    /// Java `boolean isEntryThreshold(int seed, int x, int z)` —
    /// `mapCondition.eval(seed, x, z)` (the three-argument overload: the z
    /// coordinate is passed as the `y` argument).
    pub fn is_entry_threshold(&self, seed: i32, x: i32, z: i32) -> bool {
        self.map_condition.eval2(seed, x, z)
    }

    /// Java `boolean isHeightThreshold(int seed, int x, int y, int z)`.
    pub fn is_height_threshold(&self, seed: i32, x: i32, y: i32, z: i32) -> bool {
        self.height_condition.eval3(seed, x, y, z)
    }

    /// Java `double getMaximumSize()`.
    pub fn get_maximum_size(&self) -> f64 {
        self.maximum_size
    }

    /// Java `int hashCode()` — the value computed by `_hashCode()`.
    pub fn hash_code(&self) -> i32 {
        self.hash_code
    }

    /// Java `private int _hashCode()`.
    ///
    /// The Java formula mixes the *value* hash of the name with the
    /// *identity* hashes of the referenced objects:
    ///
    /// ```java
    /// result = name != null ? name.hashCode() : 0;
    /// result = 31 * result + (entryNodeType != null ? entryNodeType.hashCode() : 0);
    /// … (yaw, pitch, depth, heightFactors, pointGenerator, blockMask,
    /// …  mapCondition, fixedEntryHeight, fluidLevel)
    /// result = 31 * result + (surfaceLimited ? 1 : 0);
    /// result = 31 * result + (submerge ? 1 : 0);
    /// ```
    ///
    /// Deviation: the referenced objects have no value hash in Rust (they are
    /// trait objects), so their *identity* (the address of the reference
    /// counted value) is used — the Rust analogue of Java's
    /// `Object.hashCode()`. The hash only selects the bucket of the cache
    /// entry; the equality decides the hit.
    fn compute_hash_code(&self) -> i32 {
        let mut result = java_string_hash_code(&self.name);
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.entry_node_type));
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.yaw));
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.pitch));
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.depth));
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.height_factors));
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.point_generator));
        result = result
            .wrapping_mul(31)
            .wrapping_add(self.block_mask.hash_value());
        result = result
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&self.map_condition));
        result = result
            .wrapping_mul(31)
            .wrapping_add(option_identity_hash(&self.fixed_entry_height));
        result = result
            .wrapping_mul(31)
            .wrapping_add(self.fluid_level.hash_value());
        result = result
            .wrapping_mul(31)
            .wrapping_add(if self.surface_limited { 1 } else { 0 });
        result = result
            .wrapping_mul(31)
            .wrapping_add(if self.submerge { 1 } else { 0 });
        result
    }
}

/// Java `static class CaveType.FluidLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FluidLevel {
    /// Java `private final BlockFluidEntry blockEntry`.
    pub block_entry: BlockFluidEntry,
    /// Java `private final int height`.
    pub height: i32,
}

impl FluidLevel {
    /// Java `FluidLevel.EMPTY` = `new FluidLevel(new
    /// BlockFluidEntry(BlockType.EMPTY_ID, RotationTuple.NONE_INDEX,
    /// Fluid.EMPTY_ID), -1)`.
    pub const EMPTY: FluidLevel = FluidLevel {
        block_entry: BlockFluidEntry::EMPTY,
        height: -1,
    };

    /// Java `FluidLevel(BlockFluidEntry blockEntry, int height)`.
    pub const fn new(block_entry: BlockFluidEntry, height: i32) -> Self {
        Self {
            block_entry,
            height,
        }
    }

    /// Java `BlockFluidEntry getBlockEntry()`.
    pub fn get_block_entry(&self) -> BlockFluidEntry {
        self.block_entry
    }

    /// Java `int getHeight()`.
    pub fn get_height(&self) -> i32 {
        self.height
    }

    /// The value hash used by [`CaveType::compute_hash_code`].
    fn hash_value(&self) -> i32 {
        let mut result = self.block_entry.hash_value();
        result = result.wrapping_mul(31).wrapping_add(self.height);
        result
    }
}

impl Default for FluidLevel {
    /// Java `FluidLevel.EMPTY` — the empty block/fluid entry at height `-1`.
    fn default() -> Self {
        Self::EMPTY
    }
}

/// The Java `Object.hashCode()` analogue: the address of the reference
/// counted value, folded into a non-negative 31 bit integer.
fn identity_hash<T: ?Sized>(value: &Rc<T>) -> i32 {
    let address = Rc::as_ptr(value) as *const () as usize;
    ((address >> 4) as u32 & 0x7FFF_FFFF) as i32
}

/// The identity hash of an optional reference (0 for `None`, like the Java
/// `!= null ? hashCode() : 0`).
fn option_identity_hash<T: ?Sized>(value: &Option<Rc<T>>) -> i32 {
    match value {
        Some(value) => identity_hash(value),
        None => 0,
    }
}

impl Hash for CaveType {
    /// The Java `hashCode()` of the cave type.
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash_code);
    }
}

impl PartialEq for CaveType {
    /// Java has no `equals` override, so the Java equality is the *identity*
    /// of the cave type.
    ///
    /// Deviation: the Rust port compares the members by identity (the same
    /// `Rc` handles) and the name by value — two cave types that share all
    /// of their members are considered equal.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && Rc::ptr_eq(&self.entry_node_type, &other.entry_node_type)
            && Rc::ptr_eq(&self.yaw, &other.yaw)
            && Rc::ptr_eq(&self.pitch, &other.pitch)
            && Rc::ptr_eq(&self.depth, &other.depth)
            && Rc::ptr_eq(&self.height_factors, &other.height_factors)
            && Rc::ptr_eq(&self.point_generator, &other.point_generator)
            && Rc::ptr_eq(&self.biome_mask, &other.biome_mask)
            && self.block_mask == other.block_mask
            && Rc::ptr_eq(&self.map_condition, &other.map_condition)
            && Rc::ptr_eq(&self.height_condition, &other.height_condition)
            && option_ptr_eq(&self.fixed_entry_height, &other.fixed_entry_height)
            && option_ptr_eq(
                &self.fixed_entry_height_noise,
                &other.fixed_entry_height_noise,
            )
            && self.fluid_level == other.fluid_level
            && self.environment == other.environment
            && self.surface_limited == other.surface_limited
            && self.submerge == other.submerge
            && self.maximum_size == other.maximum_size
    }
}

impl Eq for CaveType {}

/// The identity comparison of two optional references.
fn option_ptr_eq<T: ?Sized>(left: &Option<Rc<T>>, right: &Option<Rc<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::{cave_node_type, FixedPointGenerator};
    use crate::chunk::chunk_generator::test_stubs::StubInterpreter;
    use crate::supplier::FloatRange;
    use crate::util::condition::flag::ConstantInt2Flags;
    use procedural::noise::condition::DefaultCoordinateCondition;
    use procedural::noise::supplier::DoubleRange;

    /// A cave type with the given ranges and masks.
    fn cave_type(
        depth: Rc<dyn IDoubleRange>,
        fixed_entry_height: Option<Rc<dyn IDoubleRange>>,
        fixed_entry_height_noise: Option<Rc<dyn NoiseProperty>>,
        map_condition: Rc<dyn ICoordinateCondition>,
        height_condition: Rc<dyn ICoordinateCondition>,
    ) -> CaveType {
        CaveType::new(
            "cave".to_string(),
            cave_node_type(0),
            Rc::new(FloatRange::constant(1.5)),
            Rc::new(FloatRange::constant(-2.5)),
            depth,
            Rc::new(StubInterpreter::new(0.5, 0.0, 0, 63)),
            Rc::new(FixedPointGenerator),
            Rc::new(ConstantInt2Flags::new(7)),
            BlockMaskCondition::new(),
            map_condition,
            height_condition,
            fixed_entry_height,
            fixed_entry_height_noise,
            FluidLevel::EMPTY,
            3,
            true,
            false,
            128.0,
        )
    }

    /// A noise property that always evaluates to a constant.
    struct ConstantNoise(f64);

    impl NoiseProperty for ConstantNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    /// The default cave type of the tests.
    fn default_cave_type() -> CaveType {
        cave_type(
            Rc::new(DoubleRange::constant(1.0)),
            None,
            None,
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
        )
    }

    #[test]
    fn the_start_height_is_the_y_without_a_fixed_height() {
        let cave_type = default_cave_type();
        let mut random = FastRandom::new(1);
        assert_eq!(
            cave_type.get_modified_start_height(0, 0, 42, 0, &mut random),
            42
        );
    }

    #[test]
    fn the_fixed_start_height_comes_from_the_range() {
        let cave_type = cave_type(
            Rc::new(DoubleRange::constant(1.0)),
            Some(Rc::new(DoubleRange::constant(12.75))),
            None,
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
        );
        let mut random = FastRandom::new(1);
        assert_eq!(
            cave_type.get_modified_start_height(0, 0, 42, 0, &mut random),
            12
        );
    }

    #[test]
    fn the_fixed_start_height_uses_the_noise_when_there_is_one() {
        let cave_type = cave_type(
            Rc::new(DoubleRange::constant(1.0)),
            // The range maps the noise value: `0 + 60 * 0.5 = 30`.
            Some(Rc::new(DoubleRange::normal(0.0, 60.0))),
            Some(Rc::new(ConstantNoise(0.5))),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
        );
        let mut random = FastRandom::new(1);
        assert_eq!(
            cave_type.get_modified_start_height(0, 0, 42, 0, &mut random),
            30
        );
    }

    #[test]
    fn the_start_depth_is_the_floored_range_value() {
        let cave_type = cave_type(
            Rc::new(DoubleRange::constant(3.75)),
            None,
            None,
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
        );
        let mut random = FastRandom::new(1);
        assert_eq!(cave_type.get_start_depth(&mut random), 3);
    }

    #[test]
    fn the_start_yaw_and_pitch_come_from_the_float_ranges() {
        let cave_type = default_cave_type();
        let mut random = FastRandom::new(1);
        assert_eq!(cave_type.get_start_yaw(&mut random), 1.5);
        assert_eq!(cave_type.get_start_pitch(&mut random), -2.5);
    }

    #[test]
    fn the_threshold_conditions_are_evaluated() {
        let cave_type = cave_type(
            Rc::new(DoubleRange::constant(1.0)),
            None,
            None,
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::False),
        );
        assert!(cave_type.is_entry_threshold(0, 1, 2));
        assert!(!cave_type.is_height_threshold(0, 1, 2, 3));
    }

    #[test]
    fn the_accessors_return_the_stored_values() {
        let cave_type = default_cave_type();
        assert_eq!(cave_type.get_name(), "cave");
        assert_eq!(cave_type.get_environment(), 3);
        assert!(cave_type.is_surface_limited());
        assert!(!cave_type.is_submerge());
        assert_eq!(cave_type.get_maximum_size(), 128.0);
        assert_eq!(cave_type.get_fluid_level().get_height(), -1);
        assert_eq!(
            cave_type.get_fluid_level().get_block_entry(),
            BlockFluidEntry::EMPTY
        );
    }

    #[test]
    fn the_hash_code_follows_the_java_formula() {
        let cave_type = default_cave_type();
        // The name contributes its Java string hash, every reference its
        // identity hash and the two booleans 1 / 0.
        let expected = java_string_hash_code("cave")
            .wrapping_mul(31)
            .wrapping_add(identity_hash(cave_type.get_entry_node()))
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.yaw))
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.pitch))
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.depth))
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.height_factors))
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.point_generator))
            .wrapping_mul(31)
            .wrapping_add(cave_type.block_mask.hash_value())
            .wrapping_mul(31)
            .wrapping_add(identity_hash(&cave_type.map_condition))
            .wrapping_mul(31)
            .wrapping_add(0)
            .wrapping_mul(31)
            .wrapping_add(cave_type.fluid_level.hash_value())
            .wrapping_mul(31)
            .wrapping_add(1)
            .wrapping_mul(31)
            .wrapping_add(0);
        assert_eq!(cave_type.hash_code(), expected);
    }

    #[test]
    fn the_equality_is_the_identity_of_the_members() {
        let first = default_cave_type();
        let second = default_cave_type();
        // The members are *different* objects, so the identity based
        // equality does not hold.
        assert!(first != second);
        assert!(first == first.clone());
    }
}
