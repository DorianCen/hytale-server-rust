// Ported from: `.hytale-source/.../worldgen/cave/element/CavePrefab.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.element.CavePrefab` — a
//! prefab that was spawned inside a cave node.

use std::rc::Rc;

use hytale_math::math_util;
use procedural::noise::support::IIntCondition;

use crate::cave::element::cave_element::CaveElement;
use crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::bounds::WorldBounds;
use crate::util::condition::BlockMaskCondition;

/// Java `class CavePrefab implements CaveElement`.
pub struct CavePrefab {
    /// Java `@Nonnull private final WorldGenPrefabSupplier prefabSupplier`.
    pub prefab_supplier: Rc<dyn CavePrefabSupplier>,
    /// Java `@Nonnull private final PrefabRotation rotation`.
    pub rotation: PrefabRotation,
    /// Java `private final IIntCondition biomeMask`.
    pub biome_mask: Rc<dyn IIntCondition>,
    /// Java `private final BlockMaskCondition blockMask`.
    pub block_mask: BlockMaskCondition,
    /// Java `@Nonnull private final IWorldBounds bounds`.
    pub bounds: WorldBounds,
    /// Java `private final int x`.
    pub x: i32,
    /// Java `private final int y`.
    pub y: i32,
    /// Java `private final int z`.
    pub z: i32,
}

impl CavePrefab {
    /// Java `CavePrefab(WorldGenPrefabSupplier prefabSupplier, PrefabRotation
    /// rotation, IIntCondition biomeMask, BlockMaskCondition blockMask, int x,
    /// int y, int z)`.
    ///
    /// The Java constructor resolves the prefab buffer and derives the world
    /// bounds from it; the Rust port asks the
    /// [`CavePrefabSupplier`] seam for the same extents.
    pub fn new(
        prefab_supplier: Rc<dyn CavePrefabSupplier>,
        rotation: PrefabRotation,
        biome_mask: Rc<dyn IIntCondition>,
        block_mask: BlockMaskCondition,
        x: i32,
        y: i32,
        z: i32,
    ) -> Self {
        let bounds = WorldBounds::with_bounds(
            math_util::floor(x as f64 + prefab_supplier.get_min_x(rotation) as f64),
            math_util::floor(y as f64 + prefab_supplier.get_min_y() as f64),
            math_util::floor(z as f64 + prefab_supplier.get_min_z(rotation) as f64),
            math_util::ceil(x as f64 + prefab_supplier.get_max_x(rotation) as f64),
            math_util::ceil(y as f64 + prefab_supplier.get_max_y() as f64),
            math_util::ceil(z as f64 + prefab_supplier.get_max_z(rotation) as f64),
        );
        Self {
            prefab_supplier,
            rotation,
            biome_mask,
            block_mask,
            bounds,
            x,
            y,
            z,
        }
    }

    /// Java `WorldGenPrefabSupplier getPrefab()`.
    pub fn get_prefab(&self) -> &Rc<dyn CavePrefabSupplier> {
        &self.prefab_supplier
    }

    /// Java `PrefabRotation getRotation()`.
    pub fn get_rotation(&self) -> PrefabRotation {
        self.rotation
    }

    /// Java `IIntCondition getBiomeMask()`.
    pub fn get_biome_mask(&self) -> &Rc<dyn IIntCondition> {
        &self.biome_mask
    }

    /// Java `BlockMaskCondition getConfiguration()`.
    pub fn get_configuration(&self) -> &BlockMaskCondition {
        &self.block_mask
    }

    /// Java `int getX()`.
    pub fn get_x(&self) -> i32 {
        self.x
    }

    /// Java `int getY()`.
    pub fn get_y(&self) -> i32 {
        self.y
    }

    /// Java `int getZ()`.
    pub fn get_z(&self) -> i32 {
        self.z
    }
}

impl CaveElement for CavePrefab {
    /// Java `IWorldBounds getBounds()`.
    fn get_bounds(&self) -> WorldBounds {
        self.bounds
    }
}
