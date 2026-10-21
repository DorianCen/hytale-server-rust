// Ported from: `.hytale-source/.../worldgen/cave/element/*.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.element` package.
//!
//! | Java class     | Rust module        | State      |
//! |----------------|--------------------|------------|
//! | `CaveElement`  | [`cave_element`]    | ✅          |
//! | `CaveNode`     | [`cave_node`]       | ✅          |
//! | `CavePrefab`   | [`cave_prefab`]     | ✅          |
//!
//! The cave nodes carry their shape as a trait object (the concrete shapes are
//! Phase 4c/4d) and their prefabs through the
//! [`crate::cave::prefab::CavePrefabSupplier`] abstraction (implemented by the
//! real prefab store suppliers — Phase 5).

pub mod cave_element;
pub mod cave_node;
pub mod cave_prefab;

pub use cave_element::CaveElement;
pub use cave_node::CaveNode;
pub use cave_prefab::CavePrefab;
