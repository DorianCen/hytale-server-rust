// Ported from: `.hytale-source/.../worldgen/loader/context/FileLoadingContext.java`
//! Java `loader.context.FileLoadingContext` — the root context (world-gen
//! data folder) holding the zone and prefab-category registries.
//!
//! The registries are `RefCell`s because the object graph is cyclic (Java has
//! the same cycles: a `ZoneFileContext` holds its parent loading context while
//! the loading context's registry holds the zone); mutation happens only
//! during the load, before any reader sees the context.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use super::file_context::{FileContext, ParentContext, Registry};
use super::zone_file_context::ZoneFileContext;
use crate::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use crate::prefab::prefab_category::PrefabCategory;

/// Java `FileLoadingContext`.
pub struct FileLoadingContext {
    /// Java `extends FileContext<FileContext.RootContext>`.
    pub base: FileContext,
    /// Java `private final FileContext.Registry<ZoneFileContext> zones`.
    zones: RefCell<Registry<ZoneFileContext>>,
    /// Java `private final FileContext.Registry<PrefabCategory> prefabCategories`.
    prefab_categories: RefCell<Registry<PrefabCategory>>,
    /// Java `private int zoneIdCounter`.
    zone_id_counter: Cell<i32>,
    /// Java `private int biomeIdCounter`.
    biome_id_counter: Cell<i32>,
    /// Java `SeedStringResource.loader` — the prefab loader of the world.
    ///
    /// Deviation: the Java loader lives in the `SeedStringResource` (which
    /// the Java `SeedString` carries); the Rust `SeedString` does not carry
    /// the resource, so the prefab loader lives in the root loading context
    /// (which every loader of the chain can reach).
    prefab_loader: RefCell<Option<Rc<WorldGenPrefabLoader>>>,
}

impl FileLoadingContext {
    /// Java `FileLoadingContext(String name, Path filepath)`.
    pub fn new(name: &str, filepath: PathBuf) -> Rc<Self> {
        Rc::new(Self {
            base: FileContext::new(-1, name.to_string(), filepath, ParentContext::Root),
            zones: RefCell::new(Registry::new("Zone")),
            prefab_categories: RefCell::new(Registry::new("Category")),
            zone_id_counter: Cell::new(-1),
            biome_id_counter: Cell::new(-1),
            prefab_loader: RefCell::new(None),
        })
    }

    /// Java `FileContext.Registry<ZoneFileContext> getZones()`.
    pub fn get_zones(&self) -> &RefCell<Registry<ZoneFileContext>> {
        &self.zones
    }

    /// The prefab loader of the world (Java `SeedStringResource.getLoader()`).
    pub fn get_prefab_loader(&self) -> Option<Rc<WorldGenPrefabLoader>> {
        self.prefab_loader.borrow().clone()
    }

    /// Installs the prefab loader of the world.
    pub fn set_prefab_loader(&self, loader: Rc<WorldGenPrefabLoader>) {
        *self.prefab_loader.borrow_mut() = Some(loader);
    }

    /// Java `Registry<PrefabCategory> getPrefabCategories()`.
    pub fn get_prefab_categories(&self) -> &RefCell<Registry<PrefabCategory>> {
        &self.prefab_categories
    }

    /// Java `protected ZoneFileContext createZone(String name, Path path)`.
    pub fn create_zone(self: &Rc<Self>, name: &str, path: PathBuf) -> Rc<ZoneFileContext> {
        let id = self.next_zone_id();
        self.create_zone_with_id(id, name, path)
    }

    /// Java `protected ZoneFileContext createZone(int id, String name, Path path)`.
    pub fn create_zone_with_id(
        self: &Rc<Self>,
        id: i32,
        name: &str,
        path: PathBuf,
    ) -> Rc<ZoneFileContext> {
        let id = self.update_zone_id(id);
        ZoneFileContext::new(id, name, path, self.clone())
    }

    /// Java `protected int nextZoneId()`.
    pub fn next_zone_id(&self) -> i32 {
        self.zone_id_counter.get() + 1
    }

    /// Java `protected int nextBiomeId()`.
    pub fn next_biome_id(&self) -> i32 {
        self.biome_id_counter.get() + 1
    }

    /// Java `protected int updateZoneId(int id)`.
    pub fn update_zone_id(&self, id: i32) -> i32 {
        Self::validate_id(id, self.zone_id_counter.get(), "Zone");
        self.zone_id_counter.set(id);
        id
    }

    /// Java `protected int updateBiomeId(int id)`.
    pub fn update_biome_id(&self, id: i32) -> i32 {
        Self::validate_id(id, self.biome_id_counter.get(), "Biome");
        self.biome_id_counter.set(id);
        id
    }

    // new ids must be higher than the counter
    /// Java `protected static void validateId(int id, int currentId, String type)`.
    fn validate_id(id: i32, current_id: i32, id_type: &str) {
        if id < 0 || id <= current_id {
            panic!(
                "Invalid ID '{id}' registered for type {id_type}. Current ID counter: {current_id}"
            );
        }
    }
}
