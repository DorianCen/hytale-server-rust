// Ported from: `.hytale-source/.../worldgen/loader/WorldGenPrefabLoader.java`
//! Java `com.hypixel.hytale.server.worldgen.loader.WorldGenPrefabLoader` —
//! the prefab loader of the world generator: it resolves a prefab *name*
//! into the prefab *files* it matches and hands out the suppliers of those
//! files.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use worldgen_model::registry::Registries;

use crate::prefab::prefab_store::PrefabStore;
use crate::prefab::prefab_store_root::PrefabStoreRoot;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;

/// Java `WorldGenPrefabLoader.PREFAB_FOLDER` = `"Prefabs"`.
pub const PREFAB_FOLDER: &str = "Prefabs";

/// Java `com.hypixel.hytale.server.worldgen.loader.WorldGenPrefabLoader`.
pub struct WorldGenPrefabLoader {
    /// Java `@Nonnull private final Path root`.
    root: PathBuf,
    /// Java `@Nonnull private final PrefabStoreRoot store`.
    store: PrefabStoreRoot,
    /// The prefab store the loader resolves the names through.
    prefab_store: Rc<PrefabStore>,
}

impl WorldGenPrefabLoader {
    /// Java `public WorldGenPrefabLoader(@Nonnull PrefabStoreRoot store,
    /// @Nonnull WorldGenConfig config)`.
    ///
    /// The Java loader searches every asset pack; the Rust port searches the
    /// single root the store was configured with.
    pub fn new(
        store: PrefabStoreRoot,
        data_folder: &Path,
        assets_root: &Path,
        registries: Rc<Registries>,
    ) -> Self {
        let root = PrefabStoreRoot::resolve_prefab_store(store, data_folder, assets_root);
        let prefab_store = Rc::new(PrefabStore::new(root.clone(), registries));
        Self {
            root,
            store,
            prefab_store,
        }
    }

    /// Java `@Nonnull public PrefabStoreRoot getStore()`.
    pub fn get_store(&self) -> PrefabStoreRoot {
        self.store
    }

    /// Java `public Path getRootFolder()`.
    pub fn get_root_folder(&self) -> &Path {
        &self.root
    }

    /// The prefab store of the loader.
    pub fn get_prefab_store(&self) -> &Rc<PrefabStore> {
        &self.prefab_store
    }

    /// Java `@Nullable public WorldGenPrefabSupplier[] get(@Nonnull final
    /// String prefabName)`.
    ///
    /// Java throws `new Error("Failed to find prefab: " + key)` when the
    /// name matches no prefab file.
    pub fn get(&self, prefab_name: &str) -> Vec<WorldGenPrefabSupplier> {
        let paths = self.prefab_store.resolve(prefab_name);
        if paths.is_empty() {
            panic!("Failed to find prefab: {prefab_name}");
        }
        paths
            .iter()
            .map(|path| {
                WorldGenPrefabSupplier::new(
                    Rc::clone(&self.prefab_store),
                    prefab_name,
                    path.clone(),
                )
            })
            .collect()
    }
}
