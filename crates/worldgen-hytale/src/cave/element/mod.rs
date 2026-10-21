// Ported from: `.hytale-source/.../worldgen/cave/element/*.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.element` package.
//!
//! | Java class     | Rust module        | State      |
//! |----------------|--------------------|------------|
//! | `CaveElement`  | [`cave_element`]    | —          |
//! | `CaveNode`     | [`cave_node`]       | SEAM(4c)   |
//! | `CavePrefab`   | [`cave_prefab`]     | SEAM(5)    |
//!
//! The cave nodes carry their shape as a trait object (the concrete shapes
//! arrive with Phase 4c) and their prefabs through the
//! [`crate::cave::prefab::CavePrefabSupplier`] seam (Phase 5).

pub mod cave_element;
pub mod cave_node;
pub mod cave_prefab;

pub use cave_element::CaveElement;
pub use cave_node::CaveNode;
pub use cave_prefab::CavePrefab;
