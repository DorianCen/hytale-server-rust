// Ported from: `.hytale-source/.../worldgen/cache/` (Java package, partial)
//! Java `com.hypixel.hytale.server.worldgen.cache` package — the chunk-core
//! caches.
//!
//! | Java class                    | Rust module                       |
//! |-------------------------------|-----------------------------------|
//! | `CoreDataCacheEntry`          | [`core_data_cache_entry`]         |
//! | `InterpolatedBiomeCountList`  | [`interpolated_biome_count_list`] |
//! | `CoordinateCache`             | [`coordinate_cache`]              |
//! | `ExtendedCoordinateCache`     | [`extended_coordinate_cache`]     |
//! | `ChunkGeneratorCache`         | [`chunk_generator_cache`]         |
//!
//! # Skipped in this slice (documented per `NEXT_STEPS.md`)
//!
//! * `CaveGeneratorCache` — Phase 4 (cave package).
//! * `UniquePrefabCache` — Phase 5 (prefab package).
//!
//! # The `Send`/`Sync` deviation (important)
//!
//! The house caches in `util/cache.rs` are backed by `moka`, whose values
//! must be `Send + Sync`. The core-data cache values
//! ([`CoreDataCacheEntry`], [`InterpolatedBiomeCountList`]) contain
//! `Rc<dyn Biome>` / `Rc<Zone>` — and every trait object in the biome
//! hierarchy is `!Send`/`!Sync` (the Java port has no `Send`/`Sync`
//! supertraits on `Biome`/`ICoordinateRandomizer`). They therefore cannot
//! be stored in `moka`, and the caches that hold them are built in-house
//! and documented as `!Send`:
//!
//! * Interior mutability uses `RwLock`/atomics (not `Cell`/`RefCell`) so
//!   the entries are at least `Send`-shaped field-wise; the `Rc<dyn …>`
//!   payloads still make the structs `!Send`.
//! * [`crate::zone::zone_pattern_generator_cache::ZonePatternGeneratorCache`]
//!   and [`chunk_generator_cache::ChunkGeneratorCache`] use in-house
//!   `RwLock<HashMap<…>>` caches instead of `moka`.
//!
//! Phase 6 (worker pool) must pin these caches to their owning thread (or
//! wrap them in an external lock). When the biome trait objects gain
//! `Send + Sync` supertraits, the in-house maps can move to `moka` with
//! the same semantic contract (sized + time-to-idle, compute-once).
//!
//! # Other deviations from the Java package
//!
//! * Java's `ObjectPool<CoordinateKey>` thread-local key reuse and the
//!   `destroyer`/`computeKey` callbacks of the sized caches are already
//!   omitted in the house port (`util/cache.rs`); the coordinate caches
//!   construct a fresh key per `get`. The scratch-buffer reuse arrives
//!   with `GenContext` in the ChunkGenerator slice.
//! * `ChunkGeneratorCache.get` returns `Rc<CoreDataCacheEntry>` sharing
//!   the live entry (Java returns the live object); the mutable fields
//!   keep Java's volatile/visible-mutation semantics through interior
//!   mutability.
//! * `ChunkGeneratorCache.get_zone_biome_result` returns a snapshot clone
//!   — the shared entry cannot hand out a `&mut` scratch record.

pub mod chunk_generator_cache;
pub mod coordinate_cache;
pub mod core_data_cache_entry;
pub mod extended_coordinate_cache;
pub mod interpolated_biome_count_list;

pub use chunk_generator_cache::ChunkGeneratorCache;
pub use coordinate_cache::{CoordinateCache, CoordinateKey};
pub use core_data_cache_entry::CoreDataCacheEntry;
pub use extended_coordinate_cache::{ExtendedCoordinateCache, ExtendedCoordinateKey};
pub use interpolated_biome_count_list::{BiomeCountResult, InterpolatedBiomeCountList};
