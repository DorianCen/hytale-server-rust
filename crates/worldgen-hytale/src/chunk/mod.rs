// Ported from: `.hytale-source/.../worldgen/chunk/`
//! Java `com.hypixel.hytale.server.worldgen.chunk` package — the chunk-core
//! types.
//!
//! | Java class                    | Rust module                                  |
//! |-------------------------------|----------------------------------------------|
//! | `ChunkGenerator`              | [`chunk_generator`]                          |
//! | `ChunkGeneratorExecution`     | [`chunk_generator_execution`]                 |
//! | `ChunkGeneratorResource`      | [`chunk_generator_resource`]                 |
//! | `HeightThresholdInterpolator` | [`height_threshold_interpolator`]            |
//! | `ZoneBiomeResult`             | [`zone_biome_result`]                        |
//! | `MaskProvider`                | [`mask_provider`]                            |
//! | `BlockPopulator`              | [`populator::block_populator`]               |
//! | `CavePopulator`               | [`populator::cave_populator`]                |
//! | `PrefabPopulator`             | [`populator::prefab_populator`]              |
//! | `WaterPopulator`              | [`populator::water_populator`]               |
//!
//! All chunk-core stages, caches, cave shapes and the worker-pool/`WorldGen`
//! integration are ported. The only deliberately deferred pieces are the
//! world-map generator (`getGenerator`, `REMAINING.md` §4) and the CoreServer
//! ECS/entity/block-component writes (`REMAINING.md` §2/§3).
//!
//! `ValidationUtil` — the world validation — is ported (Phase 5c).

use std::sync::LazyLock;

use hytale_math::math_util::fast_ceil_f32;

/// Java `HeightThresholdInterpolator.MAX_RADIUS` — the maximum interpolation
/// radius (the per-chunk table is `(2*MAX_RADIUS + 32)^2` entries).
pub const MAX_RADIUS: i32 = 5;

/// Java `HeightThresholdInterpolator.MAX_RADIUS2` = `MAX_RADIUS * MAX_RADIUS`
/// — the squared interpolation radius used as the default per-biome radius.
pub const MAX_RADIUS2: i32 = MAX_RADIUS * MAX_RADIUS;

/// Java `ChunkGenerator.POOL_SIZE`
/// = `Math.max(2, MathUtil.fastCeil(Runtime.getRuntime()
/// .availableProcessors() * 0.75f))`.
///
/// Deviation: a Rust `const` cannot call
/// `std::thread::available_parallelism()`, so the Java `static final`
/// (computed once per class load) maps to a `LazyLock<i32>` (computed once
/// per process, on first access). The formula itself is unchanged: f32
/// scaling then `fastCeil` (`MathUtil.fastCeil(float)`), clamped to 2.
pub static POOL_SIZE: LazyLock<i32> = LazyLock::new(|| {
    let processors = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as f32;
    fast_ceil_f32(processors * 0.75).max(2)
});

pub mod chunk_generator;
pub mod chunk_generator_execution;
pub mod chunk_generator_resource;
pub mod height_threshold_interpolator;
pub mod mask_provider;
pub mod populator;
pub mod validation_util;
pub mod zone_biome_result;

pub use chunk_generator::{generate_interpolated_threshold, ChunkGenerator};
pub use chunk_generator_execution::ChunkGeneratorExecution;
pub use chunk_generator_resource::ChunkGeneratorResource;
pub use height_threshold_interpolator::HeightThresholdInterpolator;
pub use mask_provider::MaskProvider;
pub use validation_util::ValidationUtil;
pub use zone_biome_result::ZoneBiomeResult;
