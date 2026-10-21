// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/**`
//! Java `com.hypixel.hytale.server.core.prefab.selection.buffer` package —
//! the in-memory form of a prefab.
//!
//! | Java class                | Rust module                                     |
//! |---------------------------|-------------------------------------------------|
//! | `PrefabBuffer`            | [`prefab_buffer`]                               |
//! | `IPrefabBuffer`           | [`prefab_buffer::PrefabBufferAccessor`]         |
//! | `PrefabBufferBlockEntry`  | [`prefab_buffer_block_entry`]                   |
//! | `PrefabBufferColumn`      | [`prefab_buffer_column`]                        |
//! | `PrefabBufferCall`        | [`prefab_buffer_call`]                          |
//! | `Int2ObjectOpenHashMap`   | [`int_object_open_hash_map`] (the fastutil map) |

pub mod int_object_open_hash_map;
pub mod prefab_buffer;
pub mod prefab_buffer_block_entry;
pub mod prefab_buffer_call;
pub mod prefab_buffer_column;

pub use int_object_open_hash_map::IntObjectOpenHashMap;
pub use prefab_buffer::{
    ChildPrefab, PrefabBlockVisit, PrefabBuffer, PrefabBufferAccessor, PrefabBufferBuilder,
    PrefabChildVisit, DEFAULT_CHANCE,
};
pub use prefab_buffer_block_entry::PrefabBufferBlockEntry;
pub use prefab_buffer_call::{PrefabBufferCall, PrefabBufferCallState};
pub use prefab_buffer_column::PrefabBufferColumn;
