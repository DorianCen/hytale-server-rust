// Ported from: `.hytale-source/.../worldgen/HytaleWorldGenProvider.java` and
// `CoreServer/.../universe/world/worldgen/WorldGenTimingsCollector.java`
//! The integration boundary of the port: a [`WorldGen`] trait, a dedicated
//! worker pool ([`WorldGenService`]) and the per-stage timing aggregation.
//!
//! Java's `ChunkGenerator.generate(...)` submits a task to its fixed
//! `ThreadPoolExecutor` and returns a `CompletableFuture<GeneratedChunk>`;
//! the Rust port keeps the same shape:
//!
//! * [`WorldGen`] is the blocking, object-safe generation boundary returning
//!   a [`WorldGenResult`] (chunk + stage timings).
//! * [`WorldGenService`] owns a fixed set of worker threads. Every worker
//!   loads its **own** [`ChunkGenerator`] (the ported generator graph is
//!   `Rc`-based and therefore single-threaded — the Java shares one generator
//!   across threads through thread-local resources, the Rust port isolates
//!   the generators per worker instead; see the deviation notes below).
//! * Requests are ordered by a priority queue (higher priority first, FIFO
//!   within the same priority, like the Java `PriorityBlockingQueue` with its
//!   submit sequence counter) and carry a cooperative cancellation token.
//! * [`WorldGenService::shutdown`] drains the queued requests and joins the
//!   workers (graceful shutdown).
//!
//! ## Deviations from Java
//!
//! * The Java `ThreadPoolExecutor` uses the thread-local
//!   `ChunkGeneratorResource` per worker; the Rust workers own one
//!   [`ChunkGeneratorResource`] each.
//! * The Java `WorldGenTimingsCollector` warms up the first 100 chunks and
//!   reports averages; the Rust [`WorldGenStats`] does the same (`WARMUP`).
//! * The generator is loaded once per worker (the world load is I/O heavy) —
//!   the Java generator is shared, but all of its caches are keyed by seed and
//!   coordinates, so the generated output is identical.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::chunk_generator_execution::{ChunkGeneratorExecution, StageTimings};
use crate::chunk::chunk_generator_resource::ChunkGeneratorResource;
use crate::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use crate::world_gen_config::WorldGenConfig;

/// Java `WorldGenTimingsCollector.WARMUP` — the chunks generated before the
/// averages are recorded.
pub const WARMUP: u64 = 100;

/// The default worker count: Java `ChunkGenerator.POOL_SIZE` =
/// `max(2, ceil(availableProcessors * 0.75))`.
pub fn default_worker_count() -> usize {
    let cores = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    ((cores * 3).div_ceil(4)).max(2)
}

/// One chunk-generation request.
#[derive(Debug, Clone)]
pub struct WorldGenRequest {
    /// The world seed.
    pub seed: i32,
    /// The chunk x coordinate.
    pub chunk_x: i32,
    /// The chunk z coordinate.
    pub chunk_z: i32,
    /// The scheduling priority (higher first — the Java `ChunkPriority`)
    /// The Java enum is not ported; the CLI/server maps its own priorities.
    pub priority: i32,
    /// The cooperative cancellation token (checked before every stage).
    pub cancel: Option<Arc<AtomicBool>>,
}

impl WorldGenRequest {
    /// A request at the default priority.
    pub fn new(seed: i32, chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            seed,
            chunk_x,
            chunk_z,
            priority: 0,
            cancel: None,
        }
    }

    /// Sets the scheduling priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Installs a cancellation token.
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|token| token.load(AtomicOrdering::Relaxed))
    }
}

/// One generated chunk plus its stage timings.
#[derive(Debug, Clone)]
pub struct WorldGenResult {
    /// The world seed.
    pub seed: i32,
    /// The chunk x coordinate.
    pub chunk_x: i32,
    /// The chunk z coordinate.
    pub chunk_z: i32,
    /// The generated chunk.
    pub chunk: GeneratedChunk,
    /// The stage timings of this chunk.
    pub timings: StageTimings,
}

/// The generation errors (Java throws `WorldGenLoadException` /
/// `CancellationException`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldGenError {
    /// The cancellation token was set before the chunk completed.
    Cancelled,
    /// The service has been shut down.
    Shutdown,
    /// A worker panicked while loading the world or generating a chunk.
    Panic(String),
    /// The world could not be loaded.
    Load(String),
}

impl std::fmt::Display for WorldGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "the world generation was cancelled"),
            Self::Shutdown => write!(f, "the world generation service has been shut down"),
            Self::Panic(message) => write!(f, "the world generation worker panicked: {message}"),
            Self::Load(message) => write!(f, "the world could not be loaded: {message}"),
        }
    }
}

impl std::error::Error for WorldGenError {}

/// Java `IWorldGen` — the object-safe generation boundary.
pub trait WorldGen {
    /// Generates one chunk (synchronously; the service dispatches to a worker).
    fn generate(&self, request: WorldGenRequest) -> Result<WorldGenResult, WorldGenError>;
}

/// The options of [`WorldGenService`].
#[derive(Debug, Clone)]
pub struct WorldGenOptions {
    /// The fixed worker count (defaults to [`default_worker_count`]).
    pub workers: usize,
    /// The loader seed-string label — Java `HytaleWorldGenProvider` uses the
    /// constant `"ChunkGenerator"`.
    pub seed_label: String,
    /// The extracted asset-pack root override (prefabs, environments, tints).
    pub assets_root: Option<PathBuf>,
    /// The golden registry dump override (`registries.json`).
    pub registries_path: Option<PathBuf>,
    /// An already loaded registry snapshot (the server's own asset ids).
    ///
    /// Takes precedence over [`Self::registries_path`]. Each worker clones the
    /// snapshot because the generator graph is `Rc`-based.
    pub registries: Option<Arc<Registries>>,
}

impl Default for WorldGenOptions {
    fn default() -> Self {
        Self {
            workers: default_worker_count(),
            seed_label: "ChunkGenerator".to_string(),
            assets_root: None,
            registries_path: None,
            registries: None,
        }
    }
}

/// Generates one chunk with an already loaded generator (the direct,
/// single-threaded path used by the CLI and the tests).
pub fn generate_chunk(
    generator: &Rc<ChunkGenerator>,
    registry: Rc<worldgen_model::registry::Registries>,
    resource: &mut ChunkGeneratorResource,
    request: &WorldGenRequest,
) -> Result<WorldGenResult, WorldGenError> {
    if request.is_cancelled() {
        return Err(WorldGenError::Cancelled);
    }
    let chunk = GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(request.chunk_x, request.chunk_z),
        request.chunk_x,
        request.chunk_z,
    );
    let mut execution =
        ChunkGeneratorExecution::with_registry(request.seed, Rc::clone(generator), chunk, registry);
    let completed =
        execution.execute_cancellable(request.seed, resource, request.cancel.as_deref());
    if !completed || request.is_cancelled() {
        return Err(WorldGenError::Cancelled);
    }
    Ok(WorldGenResult {
        seed: request.seed,
        chunk_x: request.chunk_x,
        chunk_z: request.chunk_z,
        timings: execution.get_timings(),
        chunk: execution.into_chunk(),
    })
}

// ---------------------------------------------------------------------------
// priority queue
// ---------------------------------------------------------------------------

struct Job {
    priority: i32,
    sequence: u64,
    request: WorldGenRequest,
    reply: Sender<Result<WorldGenResult, WorldGenError>>,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for Job {}

impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Job {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then the *earlier* submission first: the
        // BinaryHeap is a max-heap, so the sequence is reversed.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

struct JobQueue {
    heap: Mutex<BinaryHeap<Job>>,
    available: Condvar,
    stopping: AtomicBool,
}

impl JobQueue {
    fn new() -> Self {
        Self {
            heap: Mutex::new(BinaryHeap::new()),
            available: Condvar::new(),
            stopping: AtomicBool::new(false),
        }
    }

    fn push(&self, job: Job) {
        let mut heap = self
            .heap
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        heap.push(job);
        drop(heap);
        self.available.notify_one();
    }

    /// Blocks until a job is available; returns `None` once the queue is
    /// empty **and** the service is stopping (graceful shutdown).
    fn pop(&self) -> Option<Job> {
        let mut heap = self
            .heap
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if let Some(job) = heap.pop() {
                return Some(job);
            }
            if self.stopping.load(AtomicOrdering::Acquire) {
                return None;
            }
            heap = self
                .available
                .wait(heap)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Stops the workers once the queue is empty.
    fn stop(&self) {
        self.stopping.store(true, AtomicOrdering::Release);
        self.available.notify_all();
    }

    #[cfg(test)]
    fn is_stopping(&self) -> bool {
        self.stopping.load(AtomicOrdering::Acquire)
    }

    fn len(&self) -> usize {
        self.heap
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

// ---------------------------------------------------------------------------
// timing aggregation
// ---------------------------------------------------------------------------

/// The aggregated timings (Java `WorldGenTimingsCollector` — averages in
/// seconds over the chunks after the first [`WARMUP`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WorldGenStats {
    /// The number of generated chunks.
    pub chunks: u64,
    /// The average total time per chunk (seconds).
    pub chunk_seconds: f64,
    /// The average tint-mapping time (seconds).
    pub tint_seconds: f64,
    /// The average environment-mapping time (seconds).
    pub environment_seconds: f64,
    /// The average terrain/layer/cover time (seconds).
    pub blocks_seconds: f64,
    /// The average cave time (seconds).
    pub caves_seconds: f64,
    /// The average prefab time (seconds).
    pub prefabs_seconds: f64,
    /// The average water time (seconds).
    pub water_seconds: f64,
    /// The number of requests currently queued.
    pub queue_length: usize,
    /// The number of workers currently generating.
    pub generating: usize,
}

#[derive(Default)]
struct Stats {
    chunks: AtomicU64,
    total_nanos: AtomicU64,
    tint_nanos: AtomicU64,
    environment_nanos: AtomicU64,
    blocks_nanos: AtomicU64,
    caves_nanos: AtomicU64,
    prefabs_nanos: AtomicU64,
    water_nanos: AtomicU64,
    generating: AtomicUsize,
}

impl Stats {
    fn record(&self, timings: StageTimings) {
        // Java `report*` only accumulates after the WARMUP-th chunk.
        let index = self.chunks.fetch_add(1, AtomicOrdering::Relaxed);
        if index < WARMUP {
            return;
        }
        self.total_nanos
            .fetch_add(timings.total_nanos, AtomicOrdering::Relaxed);
        self.tint_nanos
            .fetch_add(timings.tint_nanos, AtomicOrdering::Relaxed);
        self.environment_nanos
            .fetch_add(timings.environment_nanos, AtomicOrdering::Relaxed);
        self.blocks_nanos
            .fetch_add(timings.blocks_nanos, AtomicOrdering::Relaxed);
        self.caves_nanos
            .fetch_add(timings.caves_nanos, AtomicOrdering::Relaxed);
        self.prefabs_nanos
            .fetch_add(timings.prefabs_nanos, AtomicOrdering::Relaxed);
        self.water_nanos
            .fetch_add(timings.water_nanos, AtomicOrdering::Relaxed);
    }

    fn snapshot(&self, queue_length: usize) -> WorldGenStats {
        let chunks = self.chunks.load(AtomicOrdering::Relaxed);
        // Java reports the warm-up chunks as `-Infinity` (no measured
        // average yet); the Rust port reports `0` until the warm-up is over.
        let measured = chunks.saturating_sub(WARMUP) as f64;
        let seconds = |value: &AtomicU64| {
            if measured == 0.0 {
                0.0
            } else {
                value.load(AtomicOrdering::Relaxed) as f64 * 1.0e-9 / measured
            }
        };
        WorldGenStats {
            chunks,
            chunk_seconds: seconds(&self.total_nanos),
            tint_seconds: seconds(&self.tint_nanos),
            environment_seconds: seconds(&self.environment_nanos),
            blocks_seconds: seconds(&self.blocks_nanos),
            caves_seconds: seconds(&self.caves_nanos),
            prefabs_seconds: seconds(&self.prefabs_nanos),
            water_seconds: seconds(&self.water_nanos),
            queue_length,
            generating: self.generating.load(AtomicOrdering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// service
// ---------------------------------------------------------------------------

/// Java `ChunkGenerator`'s worker pool + `IWorldGen` boundary.
pub struct WorldGenService {
    queue: Arc<JobQueue>,
    sequence: AtomicU64,
    stats: Arc<Stats>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    workers: usize,
    /// The lowest chunk coordinate the loaded world can generate (Java
    /// `ChunkGenerator.minChunkCoord`).
    min_chunk_coord: i32,
    /// The highest chunk coordinate the loaded world can generate.
    max_chunk_coord: i32,
    shutting_down: AtomicBool,
    /// Serializes `submit` against `shutdown` (a request pushed after the
    /// workers drained the queue would otherwise hang forever).
    submit_lock: Mutex<()>,
}

impl WorldGenService {
    /// Loads the world and starts the worker pool.
    pub fn new(config: WorldGenConfig, options: WorldGenOptions) -> Result<Self, WorldGenError> {
        assert!(options.workers > 0, "the worker count must be positive");
        let queue = Arc::new(JobQueue::new());
        let stats = Arc::new(Stats::default());
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(i32, i32), WorldGenError>>();
        let mut handles = Vec::with_capacity(options.workers);

        for _ in 0..options.workers {
            let queue = Arc::clone(&queue);
            let stats = Arc::clone(&stats);
            let config = config.clone();
            let options = options.clone();
            let ready_tx = ready_tx.clone();
            let handle = thread::Builder::new()
                .name("worldgen-worker".to_string())
                .spawn(move || worker_main(queue, stats, config, options, ready_tx))
                .map_err(|error| WorldGenError::Load(error.to_string()))?;
            handles.push(handle);
        }
        drop(ready_tx);

        // Wait for every worker to load its generator (propagate the first
        // load failure and stop the pool). Every worker loads the same world,
        // so the first reported generatable-area bounds are used.
        let mut first_error: Option<WorldGenError> = None;
        let mut bounds: Option<(i32, i32)> = None;
        for _ in 0..options.workers {
            match ready_rx.recv() {
                Ok(Ok(loaded)) => {
                    bounds.get_or_insert(loaded);
                }
                Ok(Err(error)) => {
                    first_error.get_or_insert(error);
                }
                Err(_) => {
                    first_error.get_or_insert_with(|| {
                        WorldGenError::Load("a worldgen worker stopped during load".to_string())
                    });
                }
            };
        }
        if let Some(error) = first_error {
            queue.stop();
            for handle in handles {
                let _ = handle.join();
            }
            return Err(error);
        }
        let (min_chunk_coord, max_chunk_coord) = bounds.unwrap_or((
            hytale_math::chunk_util::MIN_CHUNK_COORD,
            hytale_math::chunk_util::MAX_CHUNK_COORD,
        ));

        Ok(Self {
            queue,
            sequence: AtomicU64::new(0),
            stats,
            handles: Mutex::new(handles),
            workers: options.workers,
            min_chunk_coord,
            max_chunk_coord,
            shutting_down: AtomicBool::new(false),
            submit_lock: Mutex::new(()),
        })
    }

    /// The configured worker count.
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Java `ChunkGenerator.isChunkOutsideGeneratableArea(int x, int z)` for
    /// the loaded world (the same bounds every worker loaded).
    pub fn is_chunk_outside_generatable_area(&self, x: i32, z: i32) -> bool {
        x < self.min_chunk_coord
            || x > self.max_chunk_coord
            || z < self.min_chunk_coord
            || z > self.max_chunk_coord
    }

    /// The generatable chunk-coordinate range `(min, max)`.
    pub fn generatable_chunk_bounds(&self) -> (i32, i32) {
        (self.min_chunk_coord, self.max_chunk_coord)
    }

    /// The aggregated timings.
    pub fn stats(&self) -> WorldGenStats {
        self.stats.snapshot(self.queue.len())
    }

    /// Graceful shutdown: workers finish the queued requests, then exit.
    ///
    /// Idempotent; pending callers receive [`WorldGenError::Shutdown`].
    pub fn shutdown(&self) {
        {
            let _guard = self
                .submit_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if self.shutting_down.swap(true, AtomicOrdering::AcqRel) {
                return;
            }
            self.queue.stop();
        }
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for handle in handles.drain(..) {
            let _ = handle.join();
        }
    }

    /// Generates one chunk on a worker (blocking).
    pub fn submit(&self, request: WorldGenRequest) -> Result<WorldGenResult, WorldGenError> {
        let _guard = self
            .submit_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shutting_down.load(AtomicOrdering::Acquire) {
            return Err(WorldGenError::Shutdown);
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        let sequence = self.sequence.fetch_add(1, AtomicOrdering::Relaxed);
        self.queue.push(Job {
            priority: request.priority,
            sequence,
            request,
            reply: reply_tx,
        });
        drop(_guard);
        reply_rx.recv().unwrap_or(Err(WorldGenError::Shutdown))
    }
}

impl WorldGen for WorldGenService {
    fn generate(&self, request: WorldGenRequest) -> Result<WorldGenResult, WorldGenError> {
        self.submit(request)
    }
}

impl Drop for WorldGenService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_main(
    queue: Arc<JobQueue>,
    stats: Arc<Stats>,
    config: WorldGenConfig,
    options: WorldGenOptions,
    ready: Sender<Result<(i32, i32), WorldGenError>>,
) {
    let load = || -> Result<(Rc<ChunkGenerator>, Rc<Registries>), WorldGenError> {
        let mut loader = ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new(&options.seed_label),
            config,
        );
        if let Some(assets_root) = &options.assets_root {
            loader = loader.with_assets_root(assets_root);
        }
        if let Some(registries) = &options.registries {
            loader = loader.with_registries(Rc::new(Registries::clone(registries)));
        } else if let Some(registries_path) = &options.registries_path {
            loader = loader.with_registries_path(registries_path);
        }
        let generator = loader.load();
        // The loader keeps the registry snapshot on the generator (the Java
        // generator reads the static asset maps); reuse that handle instead of
        // loading the dump a second time per worker. Generators built without
        // the loader (unit tests) have no snapshot, so fall back to a load.
        let registry = match generator.get_registries() {
            Some(registry) => Rc::clone(registry),
            None => match &options.registries_path {
                Some(path) => Rc::new(
                    Registries::load_from(path)
                        .map_err(|error| WorldGenError::Load(error.to_string()))?,
                ),
                None => Rc::new(Registries::load()),
            },
        };
        Ok((generator, registry))
    };
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(load));
    let (generator, registry) = match loaded {
        Ok(Ok(loaded)) => loaded,
        Ok(Err(error)) => {
            let _ = ready.send(Err(error));
            return;
        }
        Err(payload) => {
            let message = crate::loader::panic_message(payload);
            let _ = ready.send(Err(WorldGenError::Load(message)));
            return;
        }
    };
    if ready
        .send(Ok((
            generator.get_min_chunk_coord(),
            generator.get_max_chunk_coord(),
        )))
        .is_err()
    {
        return;
    }

    let mut resource = ChunkGeneratorResource::new();
    while let Some(job) = queue.pop() {
        stats.generating.fetch_add(1, AtomicOrdering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            generate_chunk(
                &generator,
                Rc::clone(&registry),
                &mut resource,
                &job.request,
            )
        }))
        .unwrap_or_else(|payload| Err(WorldGenError::Panic(crate::loader::panic_message(payload))));
        stats.generating.fetch_sub(1, AtomicOrdering::Relaxed);
        if let Ok(result) = &result {
            stats.record(result.timings);
        }
        // A disconnected caller simply drops the result.
        let _ = job.reply.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(priority: i32, sequence: u64) -> Job {
        let (reply, _rx) = mpsc::channel();
        Job {
            priority,
            sequence,
            request: WorldGenRequest::new(0, 0, 0),
            reply,
        }
    }

    #[test]
    fn the_queue_orders_by_priority_then_submission() {
        let queue = JobQueue::new();
        queue.push(job(0, 0));
        queue.push(job(10, 1));
        queue.push(job(0, 2));
        queue.push(job(5, 3));

        let order: Vec<(i32, u64)> = (0..4)
            .map(|_| {
                let job = queue.pop().expect("a queued job");
                (job.priority, job.sequence)
            })
            .collect();
        assert_eq!(order, vec![(10, 1), (5, 3), (0, 0), (0, 2)]);
    }

    #[test]
    fn stopping_drains_the_queue_before_the_workers_exit() {
        let queue = JobQueue::new();
        queue.push(job(0, 0));
        queue.stop();
        assert!(queue.pop().is_some());
        assert!(queue.pop().is_none());
        assert!(queue.is_stopping());
    }

    #[test]
    fn the_default_worker_count_matches_the_java_pool_size() {
        assert!(default_worker_count() >= 2);
    }

    #[test]
    fn a_cancelled_request_is_not_generated() {
        let cancel = Arc::new(AtomicBool::new(true));
        let request = WorldGenRequest::new(0, 0, 0).with_cancel(cancel);
        assert!(request.is_cancelled());
    }

    #[test]
    fn the_timings_snapshot_reports_zero_before_the_warmup() {
        let stats = Stats::default();
        let snapshot = stats.snapshot(3);
        assert_eq!(snapshot.chunks, 0);
        assert_eq!(snapshot.queue_length, 3);
        assert_eq!(snapshot.chunk_seconds, 0.0);
    }
}
