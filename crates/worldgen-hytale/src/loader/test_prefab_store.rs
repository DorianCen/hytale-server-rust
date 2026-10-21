//! A test helper: a prefab store with a couple of fake prefabs.
//!
//! The unit tests of the cave and the container loaders need a prefab
//! loader (the Java loaders resolve the prefab names through the store).
//! The helper builds a store over a temporary folder with the prefabs
//! `A` and `B`.

use std::path::PathBuf;
use std::rc::Rc;

use worldgen_model::registry::Registries;

use crate::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use crate::prefab::prefab_store_root::PrefabStoreRoot;

/// The prefab loader of the test store.
pub fn test_prefab_loader() -> Rc<WorldGenPrefabLoader> {
    test_prefab_loader_with(&["A", "B"])
}

/// A prefab loader over a temporary folder that holds the given prefabs.
pub fn test_prefab_loader_with(names: &[&str]) -> Rc<WorldGenPrefabLoader> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let root = std::env::temp_dir().join("worldgen-prefab-store-test/Prefabs");
    std::fs::create_dir_all(&root).expect("the test prefab folder");
    for name in names {
        let path = root.join(format!("{name}.prefab.json"));
        // The file is written through a temporary file and renamed, so a
        // parallel test never reads a half-written prefab.
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!("{name}.prefab.json.{unique}.tmp"));
        std::fs::write(
            &temporary,
            r#"{"version": 8, "blockIdVersion": 11, "anchorX": 0, "anchorY": 0, "anchorZ": 0, "blocks": []}"#,
        )
        .expect("the test prefab file");
        std::fs::rename(&temporary, &path).expect("the test prefab file rename");
    }
    let data_folder = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    Rc::new(WorldGenPrefabLoader::new(
        PrefabStoreRoot::WorldGen,
        &data_folder,
        &data_folder,
        Rc::new(Registries::empty()),
    ))
}
