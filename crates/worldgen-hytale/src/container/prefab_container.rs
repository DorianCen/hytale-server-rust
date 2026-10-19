// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/PrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.PrefabContainer` — the
//! prefab catalogue of a zone.
//!
//! ## Phase 5 seams
//!
//! Two Java members are *not* ported yet because they belong to the prefab
//! package (Phase 5):
//!
//! * Java `PrefabContainerEntry.getPrefabPatternGenerator()` returns the
//!   `PrefabPatternGenerator` object. Only its `getMaxSize()` is consumed
//!   here, so the entry stores the plain
//!   [`PrefabContainerEntry::pattern_generator_max_size`] instead of the
//!   generator itself.
//! * Java `PrefabContainerEntry.getExtents()` lazily walks the prefab
//!   suppliers and computes the maximum X/Z extent of the contained prefabs.
//!   It needs the real prefab buffers (`PrefabSupplier.get()` /
//!   `getBounds()`), which arrive with the prefab package (Phase 5).

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::util::list_pool::ListPool;
use crate::weighted_map::IWeightedMap;

thread_local! {
    /// Java `public static final ListPool<PrefabContainerEntry> ENTRY_POOL` =
    /// `new ListPool<>(10, new PrefabContainerEntry[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`] — the
    /// pooled entries hold `Rc<dyn …>` trait objects (not `Send`), so the
    /// pool is thread-local rather than a process-wide `static`.
    pub static ENTRY_POOL: &'static ListPool<PrefabContainerEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `com.hypixel.hytale.server.worldgen.loader.WorldGenPrefabSupplier` —
/// the Phase-5 seam for a prefab source.
///
/// Java's class is a concrete loader-backed supplier (it resolves the
/// prefab buffer and its bounds). The Rust port declares the seam as an
/// (currently empty) trait: the container only ever holds the *keys* of the
/// weighted map, the prefab data itself is produced by the prefab package
/// (Phase 5).
pub trait PrefabSupplier {
    // Phase 5 adds the Java members:
    //   IPrefabBuffer get();
    //   IChunkBounds getBounds(IPrefabBuffer buffer);
}

/// The weighted-map key wrapper around a [`PrefabSupplier`].
///
/// Deviation: Rust forbids a trait as a generic type argument (E0782), so
/// the Java type `IWeightedMap<WorldGenPrefabSupplier>` becomes
/// `IWeightedMap<PrefabSupplierKey>` with this concrete wrapper.
///
/// Deviation: Java's `WorldGenPrefabSupplier.equals`/`hashCode` compare the
/// prefab *path*. The Rust wrapper compares the supplier *identity*; the
/// path-based comparison is added in Phase 5, when the seam carries the
/// path.
#[derive(Clone)]
pub struct PrefabSupplierKey {
    supplier: Rc<dyn PrefabSupplier>,
}

impl PrefabSupplierKey {
    /// Wraps a prefab supplier into a weighted-map key.
    pub fn new(supplier: Rc<dyn PrefabSupplier>) -> Self {
        Self { supplier }
    }

    /// The wrapped supplier.
    pub fn get_supplier(&self) -> &Rc<dyn PrefabSupplier> {
        &self.supplier
    }
}

impl PartialEq for PrefabSupplierKey {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.supplier, &other.supplier)
    }
}

impl Eq for PrefabSupplierKey {}

impl Hash for PrefabSupplierKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (Rc::as_ptr(&self.supplier) as *const () as usize).hash(state);
    }
}

impl std::fmt::Debug for PrefabSupplierKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrefabSupplierKey")
    }
}

/// Java `static class PrefabContainer.PrefabContainerEntry`.
pub struct PrefabContainerEntry {
    /// Java `protected final IWeightedMap<WorldGenPrefabSupplier> prefabs`.
    prefabs: Rc<dyn IWeightedMap<PrefabSupplierKey>>,
    /// Java `PrefabPatternGenerator.getMaxSize()` — the maximum size of the
    /// prefabs placed by this entry.
    ///
    /// Deviation: the Java `PrefabPatternGenerator` belongs to the prefab
    /// package (Phase 5); only its `getMaxSize()` is consumed here.
    pub pattern_generator_max_size: i32,
    /// Java `protected final int environmentId`.
    environment_id: i32,
}

impl PrefabContainerEntry {
    /// Java `PrefabContainerEntry(IWeightedMap<WorldGenPrefabSupplier> prefabs,
    /// PrefabPatternGenerator prefabPatternGenerator, int environmentId)`.
    pub fn new(
        prefabs: Rc<dyn IWeightedMap<PrefabSupplierKey>>,
        pattern_generator_max_size: i32,
        environment_id: i32,
    ) -> Self {
        Self {
            prefabs,
            pattern_generator_max_size,
            environment_id,
        }
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> getPrefabs()`.
    pub fn get_prefabs(&self) -> &Rc<dyn IWeightedMap<PrefabSupplierKey>> {
        &self.prefabs
    }

    /// Java `int getEnvironmentId()`.
    pub const fn get_environment_id(&self) -> i32 {
        self.environment_id
    }

    /// Java `PrefabPatternGenerator getPrefabPatternGenerator()`.
    ///
    /// Deviation: the Java `PrefabPatternGenerator` belongs to the prefab
    /// package (Phase 5). Only `getMaxSize()` is consumed by this
    /// container, so the Rust port exposes the plain size instead.
    pub const fn get_pattern_generator_max_size(&self) -> i32 {
        self.pattern_generator_max_size
    }
}

/// Java `class PrefabContainer`.
pub struct PrefabContainer {
    /// Java `private final PrefabContainerEntry[] entries`.
    entries: Vec<PrefabContainerEntry>,
    /// Java `private final int maxSize`.
    max_size: i32,
}

impl PrefabContainer {
    /// Java `PrefabContainer(PrefabContainerEntry[] entries)`.
    pub fn new(entries: Vec<PrefabContainerEntry>) -> Self {
        let max_size = Self::compute_max_size(&entries);
        Self { entries, max_size }
    }

    /// Java `PrefabContainerEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[PrefabContainerEntry] {
        &self.entries
    }

    /// Java `int getMaxSize()`.
    pub const fn get_max_size(&self) -> i32 {
        self.max_size
    }

    /// Java `private static int getMaxSize(PrefabContainerEntry[] entries)`.
    fn compute_max_size(entries: &[PrefabContainerEntry]) -> i32 {
        let mut max = 0;
        for entry in entries {
            max = max.max(entry.get_pattern_generator_max_size());
        }
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weighted_map::WeightedMap;

    /// Test stub: a prefab supplier without any behaviour (Phase 5 fills it).
    struct StubSupplier;

    impl PrefabSupplier for StubSupplier {}

    fn prefab_map(weights: &[f64]) -> Rc<dyn IWeightedMap<PrefabSupplierKey>> {
        let mut builder = WeightedMap::<PrefabSupplierKey>::builder();
        for weight in weights {
            builder.put(PrefabSupplierKey::new(Rc::new(StubSupplier)), *weight);
        }
        Rc::new(builder.build())
    }

    fn entry(max_size: i32, environment_id: i32) -> PrefabContainerEntry {
        PrefabContainerEntry::new(prefab_map(&[1.0]), max_size, environment_id)
    }

    #[test]
    fn the_max_size_folds_over_the_entries() {
        let container = PrefabContainer::new(vec![entry(3, 0), entry(11, 1), entry(7, 2)]);
        assert_eq!(container.get_max_size(), 11);
    }

    #[test]
    fn an_empty_container_has_a_max_size_of_zero() {
        let container = PrefabContainer::new(Vec::new());
        assert_eq!(container.get_max_size(), 0);
        assert!(container.get_entries().is_empty());
    }

    #[test]
    fn negative_sizes_do_not_lower_the_fold() {
        // Java: `max = Math.max(max, entry.getMaxSize())` starting at 0.
        let container = PrefabContainer::new(vec![entry(-5, 0)]);
        assert_eq!(container.get_max_size(), 0);
    }

    #[test]
    fn the_entry_exposes_its_environment_and_prefabs() {
        let entry = entry(4, 9);
        assert_eq!(entry.get_environment_id(), 9);
        assert_eq!(entry.get_pattern_generator_max_size(), 4);
        assert_eq!(entry.get_prefabs().size(), 1);
    }

    #[test]
    fn the_prefab_keys_are_compared_by_identity() {
        let supplier: Rc<dyn PrefabSupplier> = Rc::new(StubSupplier);
        let first = PrefabSupplierKey::new(supplier.clone());
        let second = PrefabSupplierKey::new(supplier);
        let other = PrefabSupplierKey::new(Rc::new(StubSupplier));
        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(entry(1, 0));
        assert_eq!(resource.len(), 1);
    }
}
