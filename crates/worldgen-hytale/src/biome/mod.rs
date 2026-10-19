// Ported from: `.hytale-source/.../worldgen/biome/`
//! Java `com.hypixel.hytale.server.worldgen.biome` package — the biome
//! model: the abstract `Biome`, the two concrete biomes ([`TileBiome`] with
//! a jittered-voronoi tile pattern ([`BiomePatternGenerator`]) and the
//! noise-threshold-driven [`CustomBiome`]), plus [`BiomeInterpolation`].

// (allow module inception: Java class Biome in Java package biome)
#[allow(clippy::module_inception)]
pub mod biome;
pub mod biome_interpolation;
pub mod biome_pattern_generator;
pub mod custom_biome;
pub mod custom_biome_generator;
pub mod tile_biome;

pub use biome::Biome;
pub use biome_interpolation::BiomeInterpolation;
pub use biome_pattern_generator::BiomePatternGenerator;
pub use custom_biome::CustomBiome;
pub use custom_biome_generator::CustomBiomeGenerator;
pub use tile_biome::TileBiome;
