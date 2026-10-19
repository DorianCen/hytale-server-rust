// Ported from: `.hytale-source/.../worldgen/chunk/populator/`
//! Java `com.hypixel.hytale.server.worldgen.chunk.populator` package — the
//! stages that write the blocks of a chunk.
//!
//! | Java class        | Rust module                                  | Phase |
//! |-------------------|----------------------------------------------|-------|
//! | `BlockPopulator`  | [`block_populator`]                          | 3 ✅  |
//! | `WaterPopulator`  | [`water_populator`]                          | 3 ✅  |
//! | `CavePopulator`   | —                                            | 4 ⏳  |
//! | `PrefabPopulator` | —                                            | 5 ⏳  |
//!
//! # Deferred to the later phases
//!
//! * **`CavePopulator`** (Phase 4) — the cave stage carves into the terrain
//!   that [`block_populator`] produced and writes with the
//!   `CAVE_COVER` / `CAVE` priorities.
//! * **`PrefabPopulator`** (Phase 5) — the prefab stage pastes the prefab
//!   structures with the `CAVE_PREFAB` / `PREFAB_CAVE` / `PREFAB` priorities.
//!
//! Both stages are wired into [`crate::chunk::ChunkGeneratorExecution::execute`]
//! as documented `TODO` markers, so the stage order of the Java
//! `ChunkGeneratorExecution.execute` (tint → environment → blocks → caves →
//! prefabs → water) is already visible in the Rust port.

pub mod block_populator;
pub mod water_populator;

pub use block_populator::BlockPopulator;
pub use water_populator::WaterPopulator;

#[cfg(test)]
pub(crate) mod test_stubs;
