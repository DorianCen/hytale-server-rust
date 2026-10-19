// Ported from: `.hytale-source/.../worldgen/chunk/` (scaffolding slice)
//! Java `com.hypixel.hytale.server.worldgen.chunk` package — scaffolding.
//!
//! ⚠ Partial: only the shared constants of `HeightThresholdInterpolator`
//! live here for now; the full `HeightThresholdInterpolator` port (with
//! `CoreDataCacheEntry`, `InterpolatedBiomeCountList`, `ZoneBiomeResult` and
//! the `ChunkGenerator` skeleton it drives) is Subtask 3 (chunk core) per
//! `NEXT_STEPS.md`. `BiomeInterpolation` only consumes these constants.

/// Java `HeightThresholdInterpolator.MAX_RADIUS` — the maximum interpolation
/// radius (the per-chunk table is `(2*MAX_RADIUS + 32)^2` entries).
pub const MAX_RADIUS: i32 = 5;

/// Java `HeightThresholdInterpolator.MAX_RADIUS2` = `MAX_RADIUS * MAX_RADIUS`
/// — the squared interpolation radius used as the default per-biome radius.
pub const MAX_RADIUS2: i32 = MAX_RADIUS * MAX_RADIUS;
