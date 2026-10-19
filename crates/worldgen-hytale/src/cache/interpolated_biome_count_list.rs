// Ported from: `.hytale-source/.../worldgen/cache/InterpolatedBiomeCountList.java`
//! Java `cache.InterpolatedBiomeCountList` — the per-chunk list of biomes
//! surrounding a column, with their running height-noise averages.
//!
//! Java mutates the list (and the [`BiomeCountResult`] records) in place
//! while callers hold the shared reference. The Rust port keeps the same
//! shared-mutation semantics through `RwLock` interior mutability (see
//! `cache/mod.md` for why not `RefCell`): the whole struct is `!Send`
//! because of the `Rc<dyn Biome>` payloads.
use std::rc::Rc;
use std::sync::RwLock;

use crate::biome::biome::Biome;
use crate::chunk::zone_biome_result::ZoneBiomeResult;

/// Java `cache.InterpolatedBiomeCountList`.
pub struct InterpolatedBiomeCountList {
    /// Java `Int2ObjectMap<BiomeCountResult> results`.
    results: RwLock<std::collections::HashMap<i32, BiomeCountResult>>,
    /// Java `IntList biomeIds` — insertion-ordered biome ids.
    biome_ids: RwLock<Vec<i32>>,
    /// Java `Biome center` — nullable until `setCenter` is called.
    center: RwLock<Option<Rc<dyn Biome>>>,
}

impl InterpolatedBiomeCountList {
    /// Java `InterpolatedBiomeCountList()` (empty maps/lists, no center).
    pub fn new() -> Self {
        Self {
            results: RwLock::new(std::collections::HashMap::new()),
            biome_ids: RwLock::new(Vec::new()),
            center: RwLock::new(None),
        }
    }

    /// Java `BiomeCountResult get(Biome biome)` — by biome id.
    ///
    /// Returns a snapshot copy (the Java record is mutated in place by
    /// `add`; callers re-`get` after adding).
    pub fn get(&self, biome: &dyn Biome) -> Option<BiomeCountResult> {
        self.get_by_id(biome.get_id())
    }

    /// Java `BiomeCountResult get(int index)`.
    pub fn get_by_id(&self, index: i32) -> Option<BiomeCountResult> {
        self.results
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&index)
            .cloned()
    }

    /// Java `void setCenter(ZoneBiomeResult result)` — records the center
    /// biome with count 1 (Java NPEs when the result has no biome).
    pub fn set_center(&self, result: &ZoneBiomeResult) {
        let biome = match result.get_biome() {
            Some(biome) => Rc::clone(biome),
            None => panic!("setCenter: result has no biome (Java: NPE)"),
        };
        let biome_id = biome.get_id();
        *self.center.write().unwrap_or_else(|p| p.into_inner()) = Some(Rc::clone(&biome));
        self.biome_ids
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .push(biome_id);
        self.results
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                biome_id,
                BiomeCountResult {
                    biome,
                    height_threshold_context: result.get_height_threshold_context(),
                    height_noise: result.get_heightmap_noise(),
                    count: 1,
                },
            );
    }

    /// Java `void add(ZoneBiomeResult result, int distance2)` — folds a
    /// neighbouring column's biome into the running average.
    ///
    /// Skipped when the center biome's interpolation radius for that biome
    /// is smaller than `distance2` (Java: `center.getInterpolation()
    /// .getBiomeRadius2(biomeId) < distance2 → return`). Java NPEs when the
    /// result has no biome.
    pub fn add(&self, result: &ZoneBiomeResult, distance2: i32) {
        let biome = match result.get_biome() {
            Some(biome) => Rc::clone(biome),
            None => panic!("add: result has no biome (Java: NPE)"),
        };
        let biome_id = biome.get_id();

        // If the center biome has a specific interpolation radius for the
        // neighbour biome then skip interpolation with it if it lies
        // outside of that range.
        {
            let center = self.center.read().unwrap_or_else(|p| p.into_inner());
            let center = match center.as_ref() {
                Some(center) => center,
                None => panic!("add: center biome is not set (Java: NPE)"),
            };
            if center.get_interpolation().get_biome_radius2(biome_id) < distance2 {
                return;
            }
        }

        let mut results = self.results.write().unwrap_or_else(|p| p.into_inner());
        match results.get_mut(&biome_id) {
            None => {
                self.biome_ids
                    .write()
                    .unwrap_or_else(|p| p.into_inner())
                    .push(biome_id);
                results.insert(
                    biome_id,
                    BiomeCountResult {
                        biome,
                        height_threshold_context: result.get_height_threshold_context(),
                        height_noise: result.get_heightmap_noise(),
                        count: 1,
                    },
                );
            }
            Some(r) => {
                // Ported from metrics `AverageCollector.add(double val,
                // double v, int n)` = `val - val / n + v / n`, where `n` is
                // the count BEFORE this addition.
                r.height_noise =
                    average_collector_add(r.height_noise, result.get_heightmap_noise(), r.count);
                r.count += 1;
            }
        }
    }

    /// Java `IntList getBiomeIds()`.
    ///
    /// Deviation: returns a snapshot copy (the list is shared and mutated
    /// in place; cloning the small id list keeps the borrow checker away
    /// from the `RwLock` guard).
    pub fn get_biome_ids(&self) -> Vec<i32> {
        self.biome_ids
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The center biome (Java has no getter; exposed for tests/callers).
    pub fn get_center(&self) -> Option<Rc<dyn Biome>> {
        self.center
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

impl Default for InterpolatedBiomeCountList {
    fn default() -> Self {
        Self::new()
    }
}

/// Ported from metrics `AverageCollector.add(double val, double v, int n)`:
/// the running-mean update `val - val / n + v / n`.
fn average_collector_add(val: f64, v: f64, n: i32) -> f64 {
    val - val / n as f64 + v / n as f64
}

/// Java `InterpolatedBiomeCountList.BiomeCountResult`.
#[derive(Clone)]
pub struct BiomeCountResult {
    /// Java `Biome biome`.
    pub biome: Rc<dyn Biome>,
    /// Java `double heightThresholdContext`.
    pub height_threshold_context: f64,
    /// Java `double heightNoise` — the running average, mutated by `add`.
    pub height_noise: f64,
    /// Java `int count` — starts at 1, incremented by `add`.
    pub count: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::biome::biome_interpolation::BiomeInterpolation;

    /// A minimal `Biome` for the list logic: only `getId` and
    /// `getInterpolation` are exercised.
    struct StubBiome {
        id: i32,
        radius: i32,
    }

    impl Biome for StubBiome {
        fn get_id(&self) -> i32 {
            self.id
        }

        fn get_name(&self) -> &str {
            "stub"
        }

        fn get_interpolation(&self) -> Rc<BiomeInterpolation> {
            Rc::new(BiomeInterpolation::create(
                self.radius,
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

        fn get_heightmap_noise(&self) -> Rc<dyn procedural::noise::support::NoiseProperty> {
            unimplemented!("not exercised by these tests")
        }

        fn get_map_color(&self) -> i32 {
            unimplemented!("not exercised by these tests")
        }
    }

    fn biome(id: i32, radius: i32) -> Rc<dyn Biome> {
        Rc::new(StubBiome { id, radius })
    }

    fn result(
        biome: Rc<dyn Biome>,
        height_threshold_context: f64,
        heightmap_noise: f64,
    ) -> ZoneBiomeResult {
        ZoneBiomeResult::new_with(None, Some(biome), height_threshold_context, heightmap_noise)
    }

    #[test]
    fn set_center_creates_a_count_one_result() {
        let list = InterpolatedBiomeCountList::new();
        let center = biome(1, 5);
        list.set_center(&result(center.clone(), 2.0, 1.0));

        assert_eq!(list.get_biome_ids(), vec![1]);
        let r = list.get_by_id(1).expect("the center biome is recorded");
        assert_eq!(r.height_threshold_context, 2.0);
        assert_eq!(r.height_noise, 1.0);
        assert_eq!(r.count, 1);
        assert!(Rc::ptr_eq(&r.biome, &center));
    }

    #[test]
    fn add_folds_into_the_running_average() {
        // Hand-traced Java `AverageCollector` case: first value 1.0 (n=1),
        // then add 0.5 at n=1 → `1.0 - 1.0/1 + 0.5/1 = 0.5`, count 2.
        let list = InterpolatedBiomeCountList::new();
        let center = biome(1, 5);
        list.set_center(&result(center, 0.0, 1.0));
        list.add(&result(biome(1, 5), 0.0, 0.5), 0);

        let r = list.get_by_id(1).expect("the biome is recorded");
        assert_eq!(r.height_noise, 0.5, "Java running mean: val - val/n + v/n");
        assert_eq!(r.count, 2);
        assert_eq!(list.get_biome_ids(), vec![1], "no duplicate id is added");
    }

    #[test]
    fn add_new_biome_appends_the_id() {
        let list = InterpolatedBiomeCountList::new();
        let center = biome(1, 5);
        list.set_center(&result(center, 0.0, 1.0));
        list.add(&result(biome(2, 5), 3.0, 4.0), 0);

        assert_eq!(list.get_biome_ids(), vec![1, 2]);
        let r = list.get_by_id(2).expect("the biome is recorded");
        assert_eq!(r.height_noise, 4.0);
        assert_eq!(r.count, 1);
    }

    #[test]
    fn add_outside_the_interpolation_radius_is_skipped() {
        // Center biome with radius 5 (radius² = 25): a neighbour at
        // distance2 = 30 is outside → skipped entirely.
        let list = InterpolatedBiomeCountList::new();
        list.set_center(&result(biome(1, 5), 0.0, 1.0));
        list.add(&result(biome(2, 5), 3.0, 4.0), 30);

        assert_eq!(list.get_biome_ids(), vec![1], "the neighbour was skipped");
        assert!(list.get_by_id(2).is_none());
    }

    #[test]
    fn add_at_the_radius_boundary_is_kept() {
        // `getBiomeRadius2(biomeId) < distance2` skips — equality is kept.
        let list = InterpolatedBiomeCountList::new();
        list.set_center(&result(biome(1, 5), 0.0, 1.0));
        list.add(&result(biome(2, 5), 3.0, 4.0), 25);

        assert_eq!(list.get_biome_ids(), vec![1, 2]);
    }

    #[test]
    fn get_unknown_biome_is_none() {
        let list = InterpolatedBiomeCountList::new();
        assert!(list.get_by_id(99).is_none());
    }
}
