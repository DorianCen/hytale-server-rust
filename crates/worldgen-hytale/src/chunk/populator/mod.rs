// Ported from: `.hytale-source/.../worldgen/chunk/populator/`
//! Java `com.hypixel.hytale.server.worldgen.chunk.populator` package — the
//! stages that write the blocks of a chunk.
//!
//! | Java class        | Rust module                                  | Phase |
//! |-------------------|----------------------------------------------|-------|
//! | `BlockPopulator`  | [`block_populator`]                          | 3 ✅  |
//! | `WaterPopulator`  | [`water_populator`]                          | 3 ✅  |
//! | `CavePopulator`   | [`cave_populator`]                           | 4b ✅ |
//! | `PrefabPopulator` | [`prefab_populator`]                         | 5b ✅ |
//!
//! # Status
//!
//! All four stages are ported and active. The generated chunk grid is bit-exact
//! against the Java pipeline on the golden chunk grid
//! (`crates/worldgen-hytale/tests/chunks_golden.rs`): terrain, layers, covers,
//! water, caves (all shapes) and the regular/unique prefabs.
//!
//! The stage order of the Java `ChunkGeneratorExecution.execute` (tint →
//! environment → blocks → caves → prefabs → water) is visible in
//! [`crate::chunk::ChunkGeneratorExecution::execute`].

pub mod block_populator;
pub mod cave_populator;
pub mod prefab_populator;
pub mod water_populator;

pub use block_populator::BlockPopulator;
pub use cave_populator::CavePopulator;
pub use prefab_populator::PrefabPopulator;
pub use water_populator::WaterPopulator;

#[cfg(test)]
pub(crate) mod test_stubs;
