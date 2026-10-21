// Ported from: `.hytale-source/.../worldgen/cave/prefab/CavePrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.prefab` package.
//!
//! | Java class          | Rust module                    | State        |
//! |---------------------|--------------------------------|--------------|
//! | `CavePrefabContainer` | [`cave_prefab_container`]    | ✅           |
//!
//! The prefab supplier abstraction ([`cave_prefab_supplier`]) is implemented
//! by the real `WorldGenPrefabSupplier` of the Phase-5 prefab store
//! (`crate::prefab::prefab_supplier`).

pub mod cave_prefab_container;
pub mod cave_prefab_supplier;

pub use cave_prefab_container::{CavePrefabConfig, CavePrefabContainer, CavePrefabEntry};
pub use cave_prefab_supplier::CavePrefabSupplier;
