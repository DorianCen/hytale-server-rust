// Ported from: `.hytale-source/.../worldgen/cache/CoreDataCacheEntry.java`
//! Java `cache.CoreDataCacheEntry` — the mutable per-chunk core-data record
//! cached by `ChunkGeneratorCache`: the zone/biome result, the interpolated
//! biome count list, the height and the height noise.
//!
//! Java mutates entries in place from the cache (`public volatile`
//! fields). The Rust port keeps the in-place mutation semantics through
//! interior mutability (`RwLock`/atomics — see `cache/mod.rs` for why not
//! `Cell`); the struct is `!Send` because of the `Rc<dyn Biome>` payloads.
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::RwLock;

use crate::cache::interpolated_biome_count_list::InterpolatedBiomeCountList;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use std::rc::Rc;

/// Java `cache.CoreDataCacheEntry` (implements `Function<CoreDataCacheEntry,
/// CoreDataCacheEntry>` — the `apply` method below).
pub struct CoreDataCacheEntry {
    /// Java `public final ZoneBiomeResult zoneBiomeResult`.
    ///
    /// Java mutates the result's fields through the entry while the entry
    /// is shared, so the record sits behind a lock.
    pub zone_biome_result: RwLock<ZoneBiomeResult>,
    /// Java `@Nullable public volatile InterpolatedBiomeCountList
    /// biomeCountList`.
    pub biome_count_list: RwLock<Option<Rc<InterpolatedBiomeCountList>>>,
    /// Java `public volatile int height` (`NO_HEIGHT` until computed).
    pub height: AtomicI32,
    /// Java `public volatile double heightNoise` (`NO_HEIGHT_NOISE` until
    /// computed).
    pub height_noise: RwLock<f64>,
}

impl CoreDataCacheEntry {
    /// Java `CoreDataCacheEntry.NO_HEIGHT` — the "height not computed"
    /// marker.
    pub const NO_HEIGHT: i32 = -1;

    /// Java `CoreDataCacheEntry.NO_HEIGHT_NOISE` — the "height noise not
    /// computed" marker.
    pub const NO_HEIGHT_NOISE: f64 = f64::NEG_INFINITY;

    /// Java `CoreDataCacheEntry()` — a default zone biome result, no
    /// biome count list, `NO_HEIGHT`, `NO_HEIGHT_NOISE`.
    pub fn new() -> Self {
        Self::new_with(ZoneBiomeResult::new())
    }

    /// Java `CoreDataCacheEntry(ZoneBiomeResult zoneBiomeResult)`.
    pub fn new_with(zone_biome_result: ZoneBiomeResult) -> Self {
        Self {
            zone_biome_result: RwLock::new(zone_biome_result),
            biome_count_list: RwLock::new(None),
            height: AtomicI32::new(Self::NO_HEIGHT),
            height_noise: RwLock::new(Self::NO_HEIGHT_NOISE),
        }
    }

    /// Java `CoreDataCacheEntry apply(CoreDataCacheEntry)` — copies all
    /// fields of the argument into this instance (the Java cache-reuse /
    /// recycle pattern) and returns `this`.
    pub fn apply(&mut self, other: &CoreDataCacheEntry) {
        {
            let src = other
                .zone_biome_result
                .read()
                .unwrap_or_else(|p| p.into_inner());
            *self
                .zone_biome_result
                .write()
                .unwrap_or_else(|p| p.into_inner()) = src.clone();
        }
        *self
            .biome_count_list
            .write()
            .unwrap_or_else(|p| p.into_inner()) = other
            .biome_count_list
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        self.height
            .store(other.height.load(Ordering::SeqCst), Ordering::SeqCst);
        *self.height_noise.write().unwrap_or_else(|p| p.into_inner()) =
            *other.height_noise.read().unwrap_or_else(|p| p.into_inner());
    }
}

impl Default for CoreDataCacheEntry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::biome::Biome;
    use crate::biome::biome_interpolation::BiomeInterpolation;

    struct StubBiome(i32);

    impl Biome for StubBiome {
        fn get_id(&self) -> i32 {
            self.0
        }

        fn get_name(&self) -> &str {
            "stub"
        }

        fn get_interpolation(&self) -> Rc<BiomeInterpolation> {
            Rc::new(BiomeInterpolation::create(
                5,
                std::collections::HashMap::new(),
            ))
        }

        fn get_heightmap_interpreter(
            &self,
        ) -> Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> {
            unimplemented!("not exercised by these tests")
        }

        fn get_cover_container(&self) -> Rc<crate::container::cover_container::CoverContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_layer_container(&self) -> Rc<crate::container::layer_container::LayerContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_prefab_container(
            &self,
        ) -> Option<Rc<crate::container::prefab_container::PrefabContainer>> {
            unimplemented!("not exercised by these tests")
        }

        fn get_tint_container(&self) -> Rc<crate::container::tint_container::TintContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_environment_container(
            &self,
        ) -> Rc<crate::container::environment_container::EnvironmentContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_water_container(&self) -> Rc<crate::container::water_container::WaterContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_fade_container(&self) -> Rc<crate::container::fade_container::FadeContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_heightmap_noise(&self) -> Rc<dyn procedural::noise::property::NoiseProperty> {
            unimplemented!("not exercised by these tests")
        }

        fn get_map_color(&self) -> i32 {
            unimplemented!("not exercised by these tests")
        }
    }

    fn entry_with_biome(id: i32) -> CoreDataCacheEntry {
        CoreDataCacheEntry::new_with(ZoneBiomeResult::new_with(
            None,
            Some(Rc::new(StubBiome(id))),
            1.5,
            2.5,
        ))
    }

    #[test]
    fn the_default_ctor_marks_the_missing_values() {
        let entry = CoreDataCacheEntry::new();
        assert_eq!(
            entry.height.load(Ordering::SeqCst),
            CoreDataCacheEntry::NO_HEIGHT
        );
        assert_eq!(
            *entry.height_noise.read().unwrap(),
            CoreDataCacheEntry::NO_HEIGHT_NOISE
        );
        assert!(entry.biome_count_list.read().unwrap().is_none());
        assert!(entry
            .zone_biome_result
            .read()
            .unwrap()
            .get_biome()
            .is_none());
    }

    #[test]
    fn new_with_wraps_the_zone_biome_result() {
        let entry = entry_with_biome(9);
        let result = entry.zone_biome_result.read().unwrap();
        assert_eq!(result.get_biome().map(|b| b.get_id()), Some(9));
        assert_eq!(result.get_height_threshold_context(), 1.5);
        assert_eq!(result.get_heightmap_noise(), 2.5);
    }

    #[test]
    fn apply_copies_every_field_into_this_instance() {
        let mut target = CoreDataCacheEntry::new();
        let source = entry_with_biome(3);
        *source.biome_count_list.write().unwrap() =
            Some(Rc::new(InterpolatedBiomeCountList::new()));
        source.height.store(42, Ordering::SeqCst);
        *source.height_noise.write().unwrap() = 7.5;

        target.apply(&source);

        // zoneBiomeResult fields are copied ...
        let result = target.zone_biome_result.read().unwrap();
        assert_eq!(result.get_biome().map(|b| b.get_id()), Some(3));
        assert_eq!(result.get_height_threshold_context(), 1.5);
        assert_eq!(result.get_heightmap_noise(), 2.5);
        // ... as are the list, the height and the height noise.
        assert!(target.biome_count_list.read().unwrap().is_some());
        assert_eq!(target.height.load(Ordering::SeqCst), 42);
        assert_eq!(*target.height_noise.read().unwrap(), 7.5);
    }

    #[test]
    fn fields_keep_their_values_after_apply() {
        let mut target = entry_with_biome(1);
        let source = entry_with_biome(2);
        target.apply(&source);
        assert_eq!(
            target
                .zone_biome_result
                .read()
                .unwrap()
                .get_biome()
                .map(|b| b.get_id()),
            Some(2)
        );
    }
}
