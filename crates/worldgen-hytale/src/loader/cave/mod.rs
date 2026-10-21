// Ported from: `.hytale-source/.../worldgen/loader/cave/**`
//! Java `loader.cave` package — the loaders of the cave configuration of a
//! zone.
//!
//! | Java class                          | Rust module                                        |
//! |-------------------------------------|----------------------------------------------------|
//! | `CaveGeneratorJsonLoader`           | [`cave_generator_json_loader`]                     |
//! | `CaveTypesJsonLoader`               | [`cave_types_json_loader`]                         |
//! | `CaveTypeJsonLoader`                | [`cave_type_json_loader`]                          |
//! | `CaveNodeTypeJsonLoader`            | [`cave_node_type_json_loader`]                     |
//! | `CaveNodeTypeStorage`               | [`cave_node_type_storage`]                         |
//! | `CaveNodeChildEntryJsonLoader`      | [`cave_node_child_entry_json_loader`]              |
//! | `CaveNodeCoverEntryJsonLoader`      | [`cave_node_cover_entry_json_loader`]              |
//! | `CaveBiomeMaskJsonLoader`           | [`cave_biome_mask_json_loader`]                    |
//! | `CavePrefabConfigJsonLoader`        | [`cave_prefab_config_json_loader`]                 |
//! | `CavePrefabContainerJsonLoader`     | [`cave_prefab_container_json_loader`]              |
//! | `CavePrefabEntryJsonLoader`         | [`cave_prefab_entry_json_loader`]                  |
//! | `FluidLevelJsonLoader`              | [`fluid_level_json_loader`]                        |
//! | `shape/*`                           | [`shape`]                                          |
//!
//! # Seams
//!
//! * The **modifier / event plugin system** is deferred (`REMAINING.md` §1):
//!   the `ModifyEvents.CaveTypes` and `ModifyEvents.CaveCovers` dispatches
//!   are skipped (the Java loaders document the same hook).
//! * The **prefab package** is ported (Phase 5a): the prefab *names* of a
//!   cave prefab container and of a prefab cave node are resolved through
//!   the prefab store into the suppliers of the matching prefab files.
//!   A loader that is used outside of the world-generator loader chain
//!   (without a prefab loader) panics with a documented message.

pub mod cave_biome_mask_json_loader;
pub mod cave_generator_json_loader;
pub mod cave_node_child_entry_json_loader;
pub mod cave_node_cover_entry_json_loader;
pub mod cave_node_type_json_loader;
pub mod cave_node_type_storage;
pub mod cave_prefab_config_json_loader;
pub mod cave_prefab_container_json_loader;
pub mod cave_prefab_entry_json_loader;
pub mod cave_type_json_loader;
pub mod cave_types_json_loader;
pub mod fluid_level_json_loader;
pub mod shape;
