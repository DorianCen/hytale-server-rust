// Ported from: `.hytale-source/.../worldgen/prefab/**` and
// `.hytale-source/.../CoreServer/.../prefab/**`
//! Java `prefab` packages — the prefab types of the world generator and of
//! the CoreServer.
//!
//! | Java class                | Rust module                          |
//! |---------------------------|--------------------------------------|
//! | `PrefabCategory`          | [`prefab_category`]                  |
//! | `PrefabStoreRoot`         | [`prefab_store_root`]                |
//! | `PrefabRotation`          | [`prefab_rotation`]                  |
//! | `PrefabWeights`           | [`prefab_weights`]                   |
//! | `PrefabBaseCheck`         | [`prefab_base_check`]                |
//! | `PrefabPatternGenerator`  | [`prefab_pattern_generator`]         |
//! | `PrefabPasteUtil`         | [`prefab_paste_util`]                |
//! | `PrefabStore`             | [`prefab_store`]                     |
//! | `PrefabBuffer`            | [`buffer::prefab_buffer`]            |
//! | `BlockMask` / `BlockFilter` | [`mask`]                           |
//! | `UniquePrefabConfiguration` | [`unique::unique_prefab_configuration`] |
//! | `UniquePrefabGenerator`   | [`unique::unique_prefab_generator`]  |

pub mod buffer;
pub mod mask;
pub mod prefab_base_check;
pub mod prefab_category;
pub mod prefab_json;
pub mod prefab_paste_util;
pub mod prefab_pattern_generator;
pub mod prefab_rotation;
pub mod prefab_store;
pub mod prefab_store_root;
pub mod prefab_supplier;
pub mod prefab_weights;
pub mod unique;

pub use prefab_base_check::PrefabBaseCheck;
pub use prefab_category::PrefabCategory;
pub use prefab_json::{parse_prefab_json, PrefabJsonStats};
pub use prefab_paste_util::{PrefabPasteBuffer, PrefabSupplier};
pub use prefab_pattern_generator::PrefabPatternGenerator;
pub use prefab_rotation::PrefabRotation;
pub use prefab_store::PrefabStore;
pub use prefab_store_root::PrefabStoreRoot;
pub use prefab_supplier::WorldGenPrefabSupplier;
pub use prefab_weights::PrefabWeights;
pub use unique::unique_prefab_configuration::UniquePrefabConfiguration;
pub use unique::unique_prefab_generator::UniquePrefabGenerator;
