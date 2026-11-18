//! `worldgen-hytale` — Rust port of the Hytale `WorldGenerator`
//! (`com.hypixel.hytale.server.worldgen.**`).
//!
//! Bit-exactness contract: same seed → identical chunks, verified against the
//! Java reference via `testdata/golden/` (see PORTING.md).

/// Version tag for the generator's *output semantics* (Java: no equivalent —
/// the game ships as one unit; this port needs one for cache invalidation).
///
/// Consumers key caches and exports by this value (the seed explorer's tile
/// keys and PNG metadata hash it into a `GeneratorIdentity`); any bump
/// invalidates every cached chunk, column, tile and golden-derived artifact
/// keyed to the previous value.
///
/// Bump policy — bump when any of the following changes:
/// - generation logic in this crate (noise, biomes, heights, caves, prefabs,
///   placement order, RNG draw sequences);
/// - the world asset/config format or a `testdata`/live asset pack change that
///   alters generation output;
/// - the registry snapshot semantics (block/fluid ids, opacity) when they
///   affect generated output;
/// - block palette semantics consumed downstream.
///
/// Do NOT bump for refactors, performance work or documentation that keep
/// output byte-identical (the golden chunk tests are the guard). Record the
/// reason in the commit message; this is a cache-invalidation action.
pub const GENERATOR_VERSION: u32 = 1;

pub mod biome;
pub mod cache;
pub mod cave;
pub mod chunk;
pub mod climate;
pub mod container;
pub mod loader;
pub mod prefab;
pub mod unique_zone_placement;
pub mod util;
pub mod weighted_map;
pub mod world_gen;
pub mod world_gen_config;
pub mod world_gen_provider;
pub mod zone;
pub mod zoom;

#[cfg(test)]
mod tests {
    /// Locks the tag's presence, type and value. A bump is deliberate: change
    /// this assertion in the same commit and record the reason there.
    #[test]
    fn generator_version_is_tagged() {
        let version: u32 = crate::GENERATOR_VERSION;
        assert_eq!(version, 1, "GENERATOR_VERSION changed; see the bump policy");
    }
}
