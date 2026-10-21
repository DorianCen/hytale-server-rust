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
//! # Deferred to the later phases
//!
//! * **The cave shapes** (Phase 4c/4d) — [`cave_populator`] runs the real
//!   cave stage (the cave graph is grown and compiled, the nodes are visited
//!   in bucket order and the [`CaveBlockPriorityModifier`] is installed), but
//!   the shape `populateChunk` implementations are still the 4a seam, so no
//!   blocks are carved yet.
//! * **The unique prefabs** (Phase 5c) — [`prefab_populator`] places the
//!   *biome* prefabs (the regular prefab categories) and carries the
//!   placement path of the unique prefabs, but the unique prefab *entries*
//!   are not generated yet (the `Zone.uniquePrefabContainer` is not loaded),
//!   so no unique prefab is placed and the exclusion radius of the regular
//!   prefabs never triggers.
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
