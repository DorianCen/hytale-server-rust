// Ported from: `.hytale-source/.../worldgen/util/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util` package.
//!
//! | Java class / package                | Rust module                    |
//! |-------------------------------------|--------------------------------|
//! | `ArrayUtli`                        | [`array_utli`]                 |
//! | `BlockArray`                       | [`block_array`]                |
//! | `BlockFluidEntry`                  | [`block_fluid_entry`]          |
//! | `NoiseBlockArray`                  | [`noise_block_array`]          |
//! | `ResolvedBlockArray`               | [`resolved_block_array`]       |
//! | `ListPool`                         | [`list_pool`]                  |
//! | `ObjectPool`                       | [`object_pool`]                |
//! | `ConstantNoiseProperty`            | [`constant_noise_property`]    |
//! | `LogUtil`                          | [`log_util`]                   |
//! | `function/*`                       | [`function`]                   |
//! | `bounds/*`                         | [`bounds`]                     |
//! | `cache/*`                          | [`cache`]                      |
//! | `condition/*`                      | [`condition`]                  |
//!
//! ⚠ **Not ported** (see `REMAINING.md`):
//! - `CleanupFutureAction` / `CleanupRunnable` — the Java background scheduler
//!   plumbing; the Rust caches use lazy eviction plus an explicit
//!   [`cache::Cache::cleanup`].
//!
//! (The Java `ChunkThreadPoolExecutor` / `ChunkWorkerThreadFactory` are replaced
//! by the dedicated [`crate::world_gen::WorldGenService`] worker pool.)

pub mod array_utli;
pub mod block_array;
pub mod block_fluid_entry;
pub mod bounds;
pub mod cache;
pub mod condition;
pub mod constant_noise_property;
pub mod function;
pub mod list_pool;
pub mod log_util;
pub mod noise_block_array;
pub mod object_pool;
pub mod resolved_block_array;
pub mod transform;

pub use array_utli::{shuffle_array, shuffle_array_i32};
pub use block_array::BlockArray;
pub use block_fluid_entry::BlockFluidEntry;
pub use bounds::{get_random_offset, ChunkBounds, IChunkBounds, IWorldBounds, WorldBounds};
pub use cache::{Cache, ConcurrentSizedTimeoutCache, SizedTimeoutCache, TimeoutCache};
pub use condition::flag::{
    CompositeInt2Flags, ConstantInt2Flags, FlagCondition, FlagOperator, Int2FlagsCondition,
};
pub use condition::{
    BlockMaskCondition, FilteredBlockFluidCondition, HashSetBlockFluidCondition,
    HashSetIntCondition, IntConditionBuilder, RandomCoordinateCondition,
};
pub use constant_noise_property::{default_one, default_zero};
pub use function::{
    ConstantCoordinateDoubleSupplier, ICoordinateDoubleSupplier, RandomCoordinateDoubleSupplier,
};
pub use list_pool::ListPool;
pub use log_util::LOGGER_NAME;
pub use noise_block_array::NoiseBlockArray;
pub use object_pool::ObjectPool;
pub use resolved_block_array::ResolvedBlockArray;
pub use transform::{Rotation3f, Transform, NO_HEIGHT};
