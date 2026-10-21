//! `worldgen-server` — the CoreServer-facing chunk-provider adapter.
//!
//! [`worldgen_hytale::world_gen::WorldGenService`] is the documented
//! generation boundary, but it exposes the port's own
//! [`WorldGenRequest`](worldgen_hytale::world_gen::WorldGenRequest) (an `i32`
//! priority and a cooperative `Arc<AtomicBool>` token). A server thinks in
//! the Java `IWorldGen.generate(seed, index, x, z, stillNeeded, priority)`
//! terms instead:
//!
//! | Java `IWorldGen.generate`      | Rust adapter                          |
//! |--------------------------------|---------------------------------------|
//! | `long index`                   | [`ChunkRequest::index`]               |
//! | `isChunkOutsideGeneratableArea`| checked by the adapter (service bounds)|
//! | `LongPredicate stillNeeded`    | [`ChunkRequest::still_needed`]        |
//! | `Long2FloatFunction priority`  | [`ChunkRequest::priority`] (f32 → i32)|
//! | shutdown of the generator pool | [`ChunkProvider::shutdown`]           |
//!
//! The adapter is deliberately thin: it owns no chunk data and no state
//! beyond the service handle, so a server can depend on this crate without
//! touching worldgen internals. [`ChunkProvider`] is object-safe, allowing
//! the server to keep a `Box<dyn ChunkProvider>` (or an `Arc`) and swap the
//! implementation in tests.
//!
//! # Registries
//!
//! The port resolves block / fluid / environment *names* through a registry
//! snapshot (`WorldGenOptions::registries_path`, the `registries.json`
//! schema). The committed dump is the canonical reference for the parity
//! tests, but it is **not** a live server's id order: the vanilla asset
//! pipeline decodes in parallel and assigns a few sibling ids in a
//! schedule-dependent order (see `REMAINING.md` §6). A server integration
//! should therefore hand the loader a snapshot of its own `BlockType` /
//! `Fluid` / `Environment` maps at boot and pass that file here.
//!
//! # Cancellation
//!
//! Java evaluates `stillNeeded.test(index)` once at the start of the
//! generation task; the port evaluates the predicate once before submitting
//! the request as well. The *cooperative* cancellation that checks between
//! generation stages is mapped onto [`ChunkRequest::cancel`], so a server
//! that wants mid-generation cancellation can share one token per request
//! (e.g. set when the player leaves the chunk's view distance).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use hytale_math::chunk_util;
use worldgen_hytale::world_gen::{
    WorldGen, WorldGenError, WorldGenOptions, WorldGenRequest, WorldGenResult, WorldGenService,
    WorldGenStats,
};
use worldgen_hytale::world_gen_config::WorldGenConfig;

/// Java `IWorldGen.generate`'s `@Nullable LongPredicate stillNeeded`.
pub type StillNeeded = Arc<dyn Fn(i64) -> bool + Send + Sync>;

/// A chunk-generation request in the server's terms.
#[derive(Clone)]
pub struct ChunkRequest {
    /// The world seed.
    pub seed: i32,
    /// The packed chunk index (`ChunkUtil.indexChunk(x, z)`).
    pub index: i64,
    /// The chunk x coordinate.
    pub chunk_x: i32,
    /// The chunk z coordinate.
    pub chunk_z: i32,
    /// The scheduling priority (higher first). Java receives the float of a
    /// `Long2FloatFunction`; [`ChunkRequest::with_server_priority`] converts
    /// it with [`server_priority_to_i32`].
    pub priority: i32,
    /// Polled once before generation (Java `stillNeeded.test(index)`); `false`
    /// means the chunk is no longer wanted.
    pub still_needed: Option<StillNeeded>,
    /// The cooperative cancellation token of the generation stages.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl ChunkRequest {
    /// A request at the default priority.
    pub fn new(seed: i32, chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            seed,
            index: chunk_util::index_chunk(chunk_x, chunk_z),
            chunk_x,
            chunk_z,
            priority: 0,
            still_needed: None,
            cancel: None,
        }
    }

    /// Sets the server's float priority (Java `Long2FloatFunction priority`).
    pub fn with_server_priority(mut self, priority: f32) -> Self {
        self.priority = server_priority_to_i32(priority);
        self
    }

    /// Installs the `stillNeeded` predicate.
    pub fn with_still_needed(
        mut self,
        still_needed: impl Fn(i64) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.still_needed = Some(Arc::new(still_needed));
        self
    }

    /// Installs the cooperative cancellation token.
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// The equivalent port request.
    fn as_world_gen_request(&self) -> WorldGenRequest {
        WorldGenRequest {
            seed: self.seed,
            chunk_x: self.chunk_x,
            chunk_z: self.chunk_z,
            priority: self.priority,
            cancel: self.cancel.clone(),
        }
    }
}

/// Maps the server's f32 chunk priority onto the port's `i32` queue priority.
///
/// The Java `Long2FloatFunction` ordering is preserved with a fixed-point
/// conversion (1/1024 granularity); values outside the `i32` range saturate
/// (Rust's `f32 as i32` cast is saturating). NaN maps to the default priority.
pub fn server_priority_to_i32(priority: f32) -> i32 {
    if priority.is_nan() {
        return 0;
    }
    (priority * 1024.0).round() as i32
}

/// The error of [`ChunkProvider::generate`].
#[derive(Debug)]
pub enum ChunkProviderError {
    /// The chunk lies outside the loaded world's generatable area (Java
    /// `IWorldGen.NO_CHUNK`).
    OutsideGeneratableArea,
    /// The request's `stillNeeded` predicate returned `false` (the Java
    /// `generate` completes with a `null` chunk).
    NotNeeded,
    /// The generation service has been shut down.
    Shutdown,
    /// A worker panicked, the world could not be loaded, or the request was
    /// cancelled through its token.
    Generation(WorldGenError),
}

impl std::fmt::Display for ChunkProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideGeneratableArea => {
                write!(f, "the chunk lies outside the generatable area")
            }
            Self::NotNeeded => write!(f, "the chunk is no longer needed"),
            Self::Shutdown => write!(f, "the chunk provider has been shut down"),
            Self::Generation(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ChunkProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Generation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WorldGenError> for ChunkProviderError {
    fn from(error: WorldGenError) -> Self {
        match error {
            WorldGenError::Shutdown => Self::Shutdown,
            other => Self::Generation(other),
        }
    }
}

/// The server-facing chunk-provider boundary.
pub trait ChunkProvider {
    /// Generates one chunk (blocking; call it off the server's tick thread).
    fn generate(&self, request: ChunkRequest) -> Result<WorldGenResult, ChunkProviderError>;

    /// Drains the queued requests, joins the workers and rejects new work.
    ///
    /// Idempotent; matches Java `IWorldGen.shutdown()`.
    fn shutdown(&self);
}

/// The [`ChunkProvider`] backed by the port's [`WorldGenService`] worker pool.
pub struct WorldGenChunkProvider {
    service: WorldGenService,
}

impl WorldGenChunkProvider {
    /// Loads the world and starts the worker pool (see [`WorldGenService::new`]).
    pub fn new(config: WorldGenConfig, options: WorldGenOptions) -> Result<Self, WorldGenError> {
        Ok(Self {
            service: WorldGenService::new(config, options)?,
        })
    }

    /// Wraps an already constructed service.
    pub fn from_service(service: WorldGenService) -> Self {
        Self { service }
    }

    /// The aggregated stage timings / counters (Java `WorldGenTimingsCollector`).
    pub fn stats(&self) -> WorldGenStats {
        self.service.stats()
    }

    /// The configured worker count.
    pub fn workers(&self) -> usize {
        self.service.workers()
    }

    /// The generatable chunk-coordinate range `(min, max)` of the loaded world.
    pub fn generatable_chunk_bounds(&self) -> (i32, i32) {
        self.service.generatable_chunk_bounds()
    }
}

impl ChunkProvider for WorldGenChunkProvider {
    fn generate(&self, request: ChunkRequest) -> Result<WorldGenResult, ChunkProviderError> {
        if self
            .service
            .is_chunk_outside_generatable_area(request.chunk_x, request.chunk_z)
        {
            return Err(ChunkProviderError::OutsideGeneratableArea);
        }
        if let Some(still_needed) = &request.still_needed {
            if !still_needed(request.index) {
                return Err(ChunkProviderError::NotNeeded);
            }
        }
        self.service
            .generate(request.as_world_gen_request())
            .map_err(ChunkProviderError::from)
    }

    fn shutdown(&self) {
        self.service.shutdown();
    }
}

impl Drop for WorldGenChunkProvider {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_matches_the_port_packing() {
        for (x, z) in [(0, 0), (7, 3), (-12, -25)] {
            assert_eq!(
                ChunkRequest::new(0, x, z).index,
                chunk_util::index_chunk(x, z)
            );
        }
    }

    #[test]
    fn the_server_priority_keeps_the_float_order() {
        let low = server_priority_to_i32(-100.5);
        let default = server_priority_to_i32(0.0);
        let high = server_priority_to_i32(42.25);
        assert!(low < default);
        assert!(default < high);
        // 1/1024 granularity: two distinct server priorities stay distinct.
        assert!(server_priority_to_i32(1.0) < server_priority_to_i32(1.001));
        assert_eq!(server_priority_to_i32(f32::NAN), 0);
        assert_eq!(server_priority_to_i32(f32::INFINITY), i32::MAX);
        assert_eq!(server_priority_to_i32(f32::NEG_INFINITY), i32::MIN);
    }

    #[test]
    fn a_request_maps_onto_the_port_request() {
        let cancel = Arc::new(AtomicBool::new(false));
        let request = ChunkRequest::new(42, 4, -5)
            .with_server_priority(2.5)
            .with_cancel(Arc::clone(&cancel));
        let mapped = request.as_world_gen_request();
        assert_eq!(mapped.seed, 42);
        assert_eq!((mapped.chunk_x, mapped.chunk_z), (4, -5));
        assert_eq!(mapped.priority, 2560);
        assert!(Arc::ptr_eq(mapped.cancel.as_ref().unwrap(), &cancel));
    }
}
