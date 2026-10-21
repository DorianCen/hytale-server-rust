// Ported from: `.hytale-source/.../worldgen/loader/WorldGenPrefabSupplier.java`
//! Java `worldgen.loader.WorldGenPrefabSupplier` — the prefab a cave node
//! can spawn.
//!
//! The trait is the cave-side view of the supplier: the extents of the
//! prefab (for a given rotation), the prefab buffer itself and the paste
//! supplier the [`PrefabPasteUtil`](crate::prefab::prefab_paste_util) needs.

use std::rc::Rc;

use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::prefab_paste_util::PrefabSupplier;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::bounds::ChunkBounds;

/// Java `worldgen.loader.WorldGenPrefabSupplier` (the prefab buffer it hands
/// out).
pub trait CavePrefabSupplier {
    /// Java `IPrefabBuffer.getMinX(PrefabRotation rotation)`.
    fn get_min_x(&self, rotation: PrefabRotation) -> i32;

    /// Java `IPrefabBuffer.getMinY()`.
    fn get_min_y(&self) -> i32;

    /// Java `IPrefabBuffer.getMinZ(PrefabRotation rotation)`.
    fn get_min_z(&self, rotation: PrefabRotation) -> i32;

    /// Java `IPrefabBuffer.getMaxX(PrefabRotation rotation)`.
    fn get_max_x(&self, rotation: PrefabRotation) -> i32;

    /// Java `IPrefabBuffer.getMaxY()`.
    fn get_max_y(&self) -> i32;

    /// Java `IPrefabBuffer.getMaxZ(PrefabRotation rotation)`.
    fn get_max_z(&self, rotation: PrefabRotation) -> i32;

    /// Java `WorldGenPrefabSupplier.get()` — the prefab buffer.
    fn get_buffer(&self) -> Option<Rc<PrefabBuffer>>;

    /// The supplier the prefab paste uses.
    fn as_paste_supplier(&self) -> Option<Rc<dyn PrefabSupplier>>;

    /// Java `WorldGenPrefabSupplier.getBounds(@Nonnull IPrefabBuffer
    /// buffer)` — the bounds of the prefab *and* of its child prefabs.
    fn get_bounds(&self) -> ChunkBounds;
}

/// The key of the cave prefab weighted map — a shared handle to a prefab
/// supplier.
///
/// Deviation: the Java `WorldGenPrefabSupplier` compares by path (its
/// `equals` / `hashCode`); the port compares by *identity* (the address of
/// the shared handle).
#[derive(Clone)]
pub struct CavePrefabKey {
    /// The prefab supplier.
    pub supplier: std::rc::Rc<dyn CavePrefabSupplier>,
}

impl CavePrefabKey {
    /// A key for the given supplier.
    pub fn new(supplier: std::rc::Rc<dyn CavePrefabSupplier>) -> Self {
        Self { supplier }
    }
}

impl PartialEq for CavePrefabKey {
    /// The identity of the supplier.
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.supplier, &other.supplier)
    }
}

impl Eq for CavePrefabKey {}

impl std::hash::Hash for CavePrefabKey {
    /// The identity of the supplier.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let address = std::rc::Rc::as_ptr(&self.supplier) as *const () as usize;
        state.write_usize(address);
    }
}
