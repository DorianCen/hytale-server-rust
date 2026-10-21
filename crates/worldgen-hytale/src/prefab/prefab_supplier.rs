// Ported from: `.hytale-source/.../worldgen/loader/WorldGenPrefabSupplier.java`
//! Java `com.hypixel.hytale.server.worldgen.loader.WorldGenPrefabSupplier` —
//! the supplier of one prefab *file* of the prefab store.
//!
//! Deviation: the Java supplier holds the loader (and the loader's cache
//! holds the suppliers — a reference cycle the Java GC collects). The Rust
//! port holds the prefab store instead, so that the store's caches do not
//! keep the suppliers alive.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::prefab_paste_util::PrefabSupplier;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_store::PrefabStore;
use crate::util::bounds::ChunkBounds;

/// Java `com.hypixel.hytale.server.worldgen.loader.WorldGenPrefabSupplier`.
pub struct WorldGenPrefabSupplier {
    /// The prefab store the supplier resolves its buffer through.
    store: Rc<PrefabStore>,
    /// Java `private final String prefabKey`.
    prefab_key: String,
    /// Java `private final Path path`.
    path: PathBuf,
    /// Java `private String prefabName` — the lazily resolved name.
    prefab_name: RefCell<Option<String>>,
    /// Java `@Nullable private ChunkBounds bounds` — the lazily computed
    /// bounds.
    bounds: RefCell<Option<ChunkBounds>>,
}

impl Clone for WorldGenPrefabSupplier {
    /// The Java supplier is a value object (its `equals` compares the path).
    fn clone(&self) -> Self {
        Self {
            store: Rc::clone(&self.store),
            prefab_key: self.prefab_key.clone(),
            path: self.path.clone(),
            prefab_name: RefCell::new(self.prefab_name.borrow().clone()),
            bounds: RefCell::new(*self.bounds.borrow()),
        }
    }
}

impl PartialEq for WorldGenPrefabSupplier {
    /// Java `WorldGenPrefabSupplier.equals(Object o)` — the path.
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for WorldGenPrefabSupplier {}

impl std::hash::Hash for WorldGenPrefabSupplier {
    /// Java `WorldGenPrefabSupplier.hashCode()` — the path.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl WorldGenPrefabSupplier {
    /// Java `public WorldGenPrefabSupplier(WorldGenPrefabLoader loader,
    /// String prefabKey, Path path)`.
    pub fn new(store: Rc<PrefabStore>, prefab_key: &str, path: PathBuf) -> Self {
        Self {
            store,
            prefab_key: prefab_key.to_string(),
            path,
            prefab_name: RefCell::new(None),
            bounds: RefCell::new(None),
        }
    }

    /// Java `@Nonnull public String getName()` — the path of the prefab.
    pub fn get_name(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    /// Java `@Nonnull public String getPrefabKey()`.
    pub fn get_prefab_key(&self) -> &str {
        &self.prefab_key
    }

    /// Java `@Nonnull public String getPrefabName()`.
    pub fn get_prefab_name(&self) -> String {
        if let Some(name) = self.prefab_name.borrow().as_ref() {
            return name.clone();
        }
        let name =
            resolve_relative_json_path(&self.prefab_key, &self.path, self.store.get_root_folder());
        *self.prefab_name.borrow_mut() = Some(name.clone());
        name
    }

    /// Java `public Path getPath()`.
    pub fn get_path(&self) -> &Path {
        &self.path
    }

    /// Java `@Nullable public IPrefabBuffer get()` — the prefab buffer.
    pub fn get(&self) -> Rc<PrefabBuffer> {
        self.store.load(&self.path)
    }

    /// Java `@Nonnull public IChunkBounds getBounds(@Nonnull IPrefabBuffer
    /// buffer)`.
    ///
    /// The bounds grow over the prefab and its children, recursively, up to
    /// the maximum recursion depth.
    pub fn get_bounds(&self) -> ChunkBounds {
        if let Some(bounds) = self.bounds.borrow().as_ref() {
            return *bounds;
        }
        let mut bounds = ChunkBounds::new();
        self.compute_bounds(0, 0, 0, &self.get(), PrefabRotation::Rotation0, &mut bounds);
        *self.bounds.borrow_mut() = Some(bounds);
        bounds
    }

    /// Java `private ChunkBounds getBounds(int depth, int x, int z, @Nonnull
    /// IPrefabBuffer prefab, @Nonnull PrefabRotation rotation, @Nonnull
    /// ChunkBounds bounds)`.
    fn compute_bounds(
        &self,
        depth: i32,
        x: i32,
        z: i32,
        prefab: &PrefabBuffer,
        rotation: PrefabRotation,
        bounds: &mut ChunkBounds,
    ) {
        if depth >= crate::prefab::prefab_paste_util::MAX_RECURSION_DEPTH {
            return;
        }

        let access = prefab.new_access();
        let min_x = x + access.get_min_x(rotation);
        let min_z = z + access.get_min_z(rotation);
        let max_x = x + access.get_max_x(rotation);
        let max_z = z + access.get_max_z(rotation);
        bounds.include(min_x, min_z, max_x, max_z);

        for child in access.get_child_prefabs() {
            let child_x = x + rotation.get_x(child.x, child.z);
            let child_z = z + rotation.get_z(child.x, child.z);

            for supplier in self.store_suppliers(&child.path) {
                let child_prefab = supplier.get();
                let child_rotation = rotation.add(child.rotation);
                supplier.compute_bounds(
                    depth + 1,
                    child_x,
                    child_z,
                    &child_prefab,
                    child_rotation,
                    bounds,
                );
            }
        }
    }

    /// The suppliers of a child prefab path.
    fn store_suppliers(&self, path: &str) -> Vec<WorldGenPrefabSupplier> {
        self.store
            .resolve(path)
            .iter()
            .map(|child_path| {
                WorldGenPrefabSupplier::new(Rc::clone(&self.store), path, child_path.clone())
            })
            .collect()
    }
}

impl PrefabSupplier for WorldGenPrefabSupplier {
    /// Java `@Nullable public IPrefabBuffer get()`.
    fn get(&self) -> Option<Rc<PrefabBuffer>> {
        Some(self.get())
    }

    /// Java `@Nonnull public String getPrefabName()`.
    fn get_prefab_name(&self) -> String {
        self.get_prefab_name()
    }

    /// Java `buffer.supplier.getLoader().get(path)`.
    fn get_child_suppliers(&self, path: &str) -> Vec<Rc<dyn PrefabSupplier>> {
        self.store_suppliers(path)
            .into_iter()
            .map(|supplier| Rc::new(supplier) as Rc<dyn PrefabSupplier>)
            .collect()
    }
}

impl crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier for WorldGenPrefabSupplier {
    /// Java `IPrefabBuffer.getMinX(PrefabRotation rotation)`.
    fn get_min_x(&self, rotation: PrefabRotation) -> i32 {
        self.get().new_access().get_min_x(rotation)
    }

    /// Java `IPrefabBuffer.getMinY()`.
    fn get_min_y(&self) -> i32 {
        self.get().new_access().get_min_y()
    }

    /// Java `IPrefabBuffer.getMinZ(PrefabRotation rotation)`.
    fn get_min_z(&self, rotation: PrefabRotation) -> i32 {
        self.get().new_access().get_min_z(rotation)
    }

    /// Java `IPrefabBuffer.getMaxX(PrefabRotation rotation)`.
    fn get_max_x(&self, rotation: PrefabRotation) -> i32 {
        self.get().new_access().get_max_x(rotation)
    }

    /// Java `IPrefabBuffer.getMaxY()`.
    fn get_max_y(&self) -> i32 {
        self.get().new_access().get_max_y()
    }

    /// Java `IPrefabBuffer.getMaxZ(PrefabRotation rotation)`.
    fn get_max_z(&self, rotation: PrefabRotation) -> i32 {
        self.get().new_access().get_max_z(rotation)
    }

    /// Java `WorldGenPrefabSupplier.get()`.
    fn get_buffer(&self) -> Option<Rc<PrefabBuffer>> {
        Some(self.get())
    }

    /// The supplier the prefab paste uses.
    fn as_paste_supplier(&self) -> Option<Rc<dyn PrefabSupplier>> {
        Some(Rc::new(self.clone_handle()))
    }

    /// Java `WorldGenPrefabSupplier.getBounds(@Nonnull IPrefabBuffer
    /// buffer)`.
    fn get_bounds(&self) -> crate::util::bounds::ChunkBounds {
        self.get_bounds()
    }
}

impl WorldGenPrefabSupplier {
    /// A handle to this supplier that the paste can use.
    ///
    /// The supplier is not `Clone` (it holds the lazily computed caches), so
    /// the paste gets a handle that shares the store.
    pub fn clone_handle(&self) -> WorldGenPrefabSupplierHandle {
        WorldGenPrefabSupplierHandle {
            store: Rc::clone(&self.store),
            prefab_key: self.prefab_key.clone(),
            path: self.path.clone(),
        }
    }
}

/// A cheap handle to a prefab — the paste supplier of a prefab file.
pub struct WorldGenPrefabSupplierHandle {
    /// The prefab store.
    store: Rc<PrefabStore>,
    /// The prefab key.
    prefab_key: String,
    /// The prefab path.
    path: PathBuf,
}

impl PrefabSupplier for WorldGenPrefabSupplierHandle {
    fn get(&self) -> Option<Rc<PrefabBuffer>> {
        Some(self.store.load(&self.path))
    }

    fn get_prefab_name(&self) -> String {
        crate::prefab::prefab_supplier::resolve_relative_json_path(
            &self.prefab_key,
            &self.path,
            self.store.get_root_folder(),
        )
    }

    fn get_child_suppliers(&self, path: &str) -> Vec<Rc<dyn PrefabSupplier>> {
        self.store
            .resolve(path)
            .iter()
            .map(|child_path| {
                Rc::new(WorldGenPrefabSupplierHandle {
                    store: Rc::clone(&self.store),
                    prefab_key: path.to_string(),
                    path: child_path.clone(),
                }) as Rc<dyn PrefabSupplier>
            })
            .collect()
    }
}

/// Java `PrefabLoader.resolveRelativeJsonPath(@Nonnull String prefabName,
/// @Nonnull Path prefabPath, @Nonnull Path rootPrefabDir)`.
pub fn resolve_relative_json_path(
    prefab_name: &str,
    prefab_path: &Path,
    root_prefab_dir: &Path,
) -> String {
    // Java: "Check if the prefabName points to a single file already".
    if !prefab_name.ends_with(".*") {
        return prefab_name.to_string();
    }

    let filepath = prefab_path
        .strip_prefix(root_prefab_dir)
        .unwrap_or(prefab_path)
        .to_string_lossy()
        .to_string();

    // Java: "If the prefabName is \".*\" exactly there's nothing to
    // relativize against, otherwise the first char of the child is at the
    // '*' position."
    let start = if prefab_name == ".*" {
        0
    } else {
        prefab_name.len() - 1
    };
    let length = filepath_length_no_extension(&filepath);
    if length < start {
        panic!("Prefab key '{prefab_name}' is longer than its filepath '{filepath}'");
    }

    filepath[start..length].replace(std::path::MAIN_SEPARATOR, ".")
}

/// Java `private static int getFilepathLengthNoExtension(@Nonnull String
/// filepath)`.
fn filepath_length_no_extension(filepath: &str) -> usize {
    let extension_size = if filepath.ends_with(".prefab.json.lpf") {
        ".prefab.json.lpf".len()
    } else if filepath.ends_with(".prefab.json") {
        ".prefab.json".len()
    } else if filepath.ends_with(".lpf") {
        ".lpf".len()
    } else {
        0
    };
    filepath.len() - extension_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_file_name_is_the_key() {
        assert_eq!(
            resolve_relative_json_path(
                "Cave.Formations.Rock",
                Path::new("/root/Cave/Formations/Rock.prefab.json"),
                Path::new("/root")
            ),
            "Cave.Formations.Rock"
        );
    }

    #[test]
    fn a_wildcard_name_becomes_the_relative_path() {
        // The key "Cave.Formations.*" has 16 characters before the '*', so
        // the relative path is cut at the same offset.
        assert_eq!(
            resolve_relative_json_path(
                "Cave.Formations.*",
                Path::new("/root/Cave/Formations/Rock_Sandstone.prefab.json"),
                Path::new("/root")
            ),
            "Rock_Sandstone"
        );
    }
}
