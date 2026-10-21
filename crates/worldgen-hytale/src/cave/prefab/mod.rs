// Ported from: `.hytale-source/.../worldgen/cave/prefab/CavePrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.prefab` package.
//!
//! | Java class          | Rust module                    | State        |
//! |---------------------|--------------------------------|--------------|
//! | `CavePrefabContainer` | [`cave_prefab_container`]    | SEAM(4b/5)   |
//!
//! The prefab supplier seam ([`cave_prefab_supplier`]) stands in for the
//! loader's `WorldGenPrefabSupplier` until the prefab package (Phase 5) is
//! ported.

pub mod cave_prefab_container;
pub mod cave_prefab_supplier;

pub use cave_prefab_container::{CavePrefabConfig, CavePrefabContainer, CavePrefabEntry};
pub use cave_prefab_supplier::CavePrefabSupplier;
