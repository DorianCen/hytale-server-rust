//! `worldgen-model` — the chunk data model used by the Hytale world generator.
//!
//! This crate holds the *data* side of chunk generation: the block-priority
//! grid that arbitrates competing block writes and the flat chunk buffer the
//! populators write into.
//!
//! Ported classes (Java file → Rust module):
//! - `WorldGenerator/.../worldgen/chunk/BlockPriorityChunk.java` →
//!   [`block_priority`] — the byte grid of block priorities (+ flag bits).
//! - `WorldGenerator/.../worldgen/chunk/BlockPriorityModifier.java` →
//!   [`block_priority`] — the pluggable priority modifier.
//! - `CoreServer/.../world/worldgen/GeneratedBlockChunk.java` → [`chunk`] —
//!   the chunk the populators write into.
//! - `CoreServer/.../world/worldgen/GeneratedChunkSection.java` → [`chunk`] —
//!   folded into the flat arrays of [`chunk::GeneratedChunk`].
//! - `CoreServer/.../world/worldgen/GeneratedEnvironment.java` → [`chunk`] —
//!   flattened into a per-column base value plus per-block overrides.
//! - `CoreServer/.../world/chunk/section/FluidSection.java` → [`chunk`] —
//!   flattened into a fluid id array plus a fluid level array.
//! - `Protocol/.../DrawType.java` → [`registry`] — the block draw-type enum.
//! - `CoreServer/.../asset/type/blocktype/config/BlockType.java` →
//!   [`registry`] — reduced to the draw type the generator observes.
//! - `CoreServer/.../asset/type/fluid/Fluid.java` → [`registry`] — reduced to
//!   the max fluid level the generator observes.
//! - `CoreServer/.../asset/type/fluid/FluidTicker.java` → [`registry`] — the
//!   `isSolid` predicate.
//!
//! ## Why this crate does *not* port CoreServer's chunk classes verbatim
//!
//! The locked project decision (see `plan.md`) is that Rust owns its chunk
//! data model: the CoreServer `GeneratedChunk` / `GeneratedBlockChunk` /
//! `GeneratedEntityChunk` / `Holder<ChunkStore>` section machinery, the
//! ECS-bound `BlockChunk` conversion and the entity chunk are **not** ported.
//! [`chunk::GeneratedChunk`] keeps the observable semantics the populators
//! rely on (block id, rotation index, filler offset, tint, environment,
//! fluid id + fluid level) but stores them in flat, allocation-light `Vec`s
//! instead of 10 lazily allocated, palette-backed chunk sections.
//!
//! ## Determinism
//!
//! The model is a pure data container: it contains no randomness and no
//! time/thread dependent state, so the same sequence of writes always yields
//! the same bytes (see `PORTING.md`).

#![deny(clippy::all)]

pub mod block_priority;
pub mod chunk;
pub mod registry;

pub use block_priority::{BlockPriorityChunk, BlockPriorityModifier, NoneModifier};
pub use chunk::GeneratedChunk;
pub use registry::{
    BlockRegistry, BlockTypeInfo, DrawType, EnvironmentRegistry, FluidRegistry, Registries,
    RegistryLoadError,
};
