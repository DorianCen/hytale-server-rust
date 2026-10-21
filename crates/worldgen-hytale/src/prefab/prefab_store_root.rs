// Ported from: `.hytale-source/.../worldgen/prefab/PrefabStoreRoot.java`
//! Java `prefab.PrefabStoreRoot`.

use std::path::{Path, PathBuf};

/// Java `prefab.PrefabStoreRoot` — where the prefabs of a world live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabStoreRoot {
    /// Java `ASSETS` — the CoreServer asset store.
    Assets,
    /// Java `WORLD_GEN` — the `Prefabs` folder of the world-gen data folder.
    WorldGen,
}

impl PrefabStoreRoot {
    /// Java `PrefabStoreRoot.DEFAULT` — `WORLD_GEN`.
    ///
    /// (Java: "TODO: Make ASSETS the default once world-gen assets have been
    /// migrated & configured accordingly.")
    pub const DEFAULT: PrefabStoreRoot = PrefabStoreRoot::WorldGen;

    /// Java `PrefabStoreRoot.valueOf(String)`.
    pub fn value_of(name: &str) -> Self {
        match Self::try_value_of(name) {
            Some(store) => store,
            None => panic!(
                "No enum constant com.hypixel.hytale.server.worldgen.prefab.PrefabStoreRoot.{name}"
            ),
        }
    }

    /// Java `PrefabStoreRoot.valueOf(String)` — the fallible variant (the
    /// Java `IllegalArgumentException` becomes `None`).
    pub fn try_value_of(name: &str) -> Option<Self> {
        match name {
            "ASSETS" => Some(Self::Assets),
            "WORLD_GEN" => Some(Self::WorldGen),
            _ => None,
        }
    }

    /// Java `PrefabStoreRoot.name()`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Assets => "ASSETS",
            Self::WorldGen => "WORLD_GEN",
        }
    }

    /// Java `WorldGenPrefabLoader.PREFAB_FOLDER`.
    pub const PREFAB_FOLDER: &'static str = "Prefabs";

    /// Java `static Path resolvePrefabStore(PrefabStoreRoot store, Path dataFolder)`.
    ///
    /// Deviation: the Java `ASSETS` branch resolves
    /// `PrefabStore.get().getAssetPrefabsPath()` — the CoreServer asset
    /// store. The Rust port takes the assets root as an argument (the
    /// CoreServer asset pipeline is not part of the world generator — see
    /// `REMAINING.md` § 6).
    pub fn resolve_prefab_store(
        store: PrefabStoreRoot,
        data_folder: &Path,
        assets_root: &Path,
    ) -> PathBuf {
        match store {
            PrefabStoreRoot::Assets => assets_root.join("Server").join("Prefabs"),
            PrefabStoreRoot::WorldGen => data_folder.join(Self::PREFAB_FOLDER),
        }
    }

    /// The default assets root of the Rust port — the extracted base-game
    /// asset pack (`<workspace>/Assets/extracted`).
    ///
    /// Java `AssetUtil.getHytaleAssetsPath()` — the root of the mounted
    /// asset pack.
    pub fn default_assets_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../Assets/extracted")
            .to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_of_parses_the_enum_names() {
        assert_eq!(PrefabStoreRoot::value_of("ASSETS"), PrefabStoreRoot::Assets);
        assert_eq!(
            PrefabStoreRoot::value_of("WORLD_GEN"),
            PrefabStoreRoot::WorldGen
        );
        assert_eq!(PrefabStoreRoot::DEFAULT, PrefabStoreRoot::WorldGen);
    }

    #[test]
    fn unknown_name_panics() {
        let result = std::panic::catch_unwind(|| PrefabStoreRoot::value_of("NOPE"));
        let payload = result.unwrap_err();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_else(|| "<non-string panic>".to_string());
        assert_eq!(
            message,
            "No enum constant com.hypixel.hytale.server.worldgen.prefab.PrefabStoreRoot.NOPE"
        );
    }

    #[test]
    fn world_gen_store_resolves_inside_the_data_folder() {
        let path = PrefabStoreRoot::resolve_prefab_store(
            PrefabStoreRoot::WorldGen,
            Path::new("/tmp/world"),
            Path::new("/tmp/assets"),
        );
        assert_eq!(path, PathBuf::from("/tmp/world/Prefabs"));
    }

    #[test]
    fn the_assets_store_resolves_inside_the_assets_root() {
        let path = PrefabStoreRoot::resolve_prefab_store(
            PrefabStoreRoot::Assets,
            Path::new("/tmp/world"),
            Path::new("/tmp/assets"),
        );
        assert_eq!(path, PathBuf::from("/tmp/assets/Server/Prefabs"));
    }
}
