// Ported from: `.hytale-source/.../worldgen/prefab/unique/*.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.unique` package — the
//! *unique* prefabs (the spawn points, the landmarks, the temples, …).
//!
//! | Java class                    | Rust module                                    |
//! |-------------------------------|------------------------------------------------|
//! | `UniquePrefabConfiguration`   | [`unique_prefab_configuration`]                |
//! | `UniquePrefabGenerator`       | [`unique_prefab_generator`]                    |

pub mod unique_prefab_configuration;
pub mod unique_prefab_generator;

pub use unique_prefab_configuration::UniquePrefabConfiguration;
pub use unique_prefab_generator::UniquePrefabGenerator;
