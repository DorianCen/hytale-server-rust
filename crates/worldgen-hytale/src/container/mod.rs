// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/*.java`
//! Java `com.hypixel.hytale.server.worldgen.container` package — the
//! per-column data tables of a zone (fade gradients, tint/environment
//! mappings, water levels, cover rules, block layers and the prefab
//! catalogue).
//!
//! | Java class            | Rust module               |
//! |-----------------------|---------------------------|
//! | `FadeContainer`       | [`fade_container`]        |
//! | `TintContainer`       | [`tint_container`]        |
//! | `EnvironmentContainer`| [`environment_container`] |
//! | `WaterContainer`      | [`water_container`]       |
//! | `CoverContainer`      | [`cover_container`]       |
//! | `LayerContainer`      | [`layer_container`]       |
//! | `PrefabContainer`     | [`prefab_container`]      |
//!
//! ## Not ported
//!
//! * Java `UniquePrefabContainer` — only a minimal placeholder is provided
//!   ([`unique_prefab_container`]); the generator / prefab machinery arrives
//!   with the prefab package (Phase 5, see `plan.md`).
//!
//! ## The `IWeightedMap.get` overload collapse
//!
//! Java's `IWeightedMap` offers several `get` overloads. The world generator
//! uses the five-argument form
//! `get(int seed, int x, int z, SeedCoordinateFunction<K> supplier, K k)`,
//! which the Java implementation collapses to
//! `get(supplier.apply(seed, x, z, k))`. The Rust port therefore samples the
//! noise property first and then calls the plain
//! [`crate::weighted_map::IWeightedMap::get`] with the resulting `f64`.

pub mod cover_container;
pub mod environment_container;
pub mod fade_container;
pub mod layer_container;
pub mod prefab_container;
pub mod tint_container;
pub mod unique_prefab_container;
pub mod water_container;

pub use cover_container::{CoverContainer, CoverContainerEntry, CoverContainerEntryPart};
pub use environment_container::{
    DefaultEnvironmentContainerEntry, EnvironmentContainer, EnvironmentContainerEntry,
};
pub use fade_container::FadeContainer;
pub use layer_container::{
    DynamicLayer, DynamicLayerEntry, Layer, LayerContainer, LayerEntry, StaticLayer,
    StaticLayerEntry,
};
pub use prefab_container::{PrefabContainer, PrefabContainerEntry};
pub use tint_container::{DefaultTintContainerEntry, TintContainer, TintContainerEntry};
pub use unique_prefab_container::{UniquePrefabContainer, UniquePrefabGenerator};
pub use water_container::{Entry as WaterContainerEntry, WaterContainer};
