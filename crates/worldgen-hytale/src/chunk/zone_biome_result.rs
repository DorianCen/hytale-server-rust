// Ported from: `.hytale-source/.../worldgen/chunk/ZoneBiomeResult.java`
//! Java `chunk.ZoneBiomeResult` — the per-column result of the zone+biome
//! lookup: the zone generator result, the biome, the height threshold
//! context and the heightmap noise. Java mutates instances freely through
//! the setters (scratch-reuse pattern), so the Rust port keeps public
//! fields plus the 1:1 getters/setters.
use std::rc::Rc;

use crate::biome::biome::Biome;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `chunk.ZoneBiomeResult`.
// (no Debug: `Rc<dyn Biome>` is not Debug, matching the Java field type)
#[derive(Clone, Default)]
pub struct ZoneBiomeResult {
    /// Java `public ZoneGeneratorResult zoneResult` — nullable.
    pub zone_result: Option<Rc<ZoneGeneratorResult>>,
    /// Java `public Biome biome` — nullable.
    pub biome: Option<Rc<dyn Biome>>,
    /// Java `public double heightThresholdContext`.
    pub height_threshold_context: f64,
    /// Java `public double heightmapNoise`.
    pub heightmap_noise: f64,
}

impl ZoneBiomeResult {
    /// Java `ZoneBiomeResult()` — all fields default (`null`, `null`,
    /// `0.0`, `0.0`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Java `ZoneBiomeResult(ZoneGeneratorResult, Biome, double, double)`.
    pub fn new_with(
        zone_result: Option<Rc<ZoneGeneratorResult>>,
        biome: Option<Rc<dyn Biome>>,
        height_threshold_context: f64,
        heightmap_noise: f64,
    ) -> Self {
        Self {
            zone_result,
            biome,
            height_threshold_context,
            heightmap_noise,
        }
    }

    /// Java `ZoneGeneratorResult getZoneResult()`.
    pub fn get_zone_result(&self) -> Option<&Rc<ZoneGeneratorResult>> {
        self.zone_result.as_ref()
    }

    /// Java `void setZoneResult(ZoneGeneratorResult)`.
    pub fn set_zone_result(&mut self, zone_result: Option<Rc<ZoneGeneratorResult>>) {
        self.zone_result = zone_result;
    }

    /// Java `Biome getBiome()`.
    pub fn get_biome(&self) -> Option<&Rc<dyn Biome>> {
        self.biome.as_ref()
    }

    /// Java `void setBiome(Biome)`.
    pub fn set_biome(&mut self, biome: Option<Rc<dyn Biome>>) {
        self.biome = biome;
    }

    /// Java `double getHeightThresholdContext()`.
    pub fn get_height_threshold_context(&self) -> f64 {
        self.height_threshold_context
    }

    /// Java `void setHeightThresholdContext(double)`.
    pub fn set_height_threshold_context(&mut self, height_threshold_context: f64) {
        self.height_threshold_context = height_threshold_context;
    }

    /// Java `double getHeightmapNoise()`.
    pub fn get_heightmap_noise(&self) -> f64 {
        self.heightmap_noise
    }

    /// Java `void setHeightmapNoise(double)`.
    pub fn set_heightmap_noise(&mut self, heightmap_noise: f64) {
        self.heightmap_noise = heightmap_noise;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_ctor_leaves_everything_unset() {
        let result = ZoneBiomeResult::new();
        assert!(result.get_zone_result().is_none());
        assert!(result.get_biome().is_none());
        assert_eq!(result.get_height_threshold_context(), 0.0);
        assert_eq!(result.get_heightmap_noise(), 0.0);
    }

    #[test]
    fn the_four_arg_ctor_round_trips_through_the_getters() {
        let zone_result = Rc::new(ZoneGeneratorResult::new_with(None, 3.5));
        let result = ZoneBiomeResult::new_with(Some(zone_result.clone()), None, 12.25, -7.5);
        assert!(Rc::ptr_eq(result.get_zone_result().unwrap(), &zone_result));
        assert!(result.get_biome().is_none());
        assert_eq!(result.get_height_threshold_context(), 12.25);
        assert_eq!(result.get_heightmap_noise(), -7.5);
    }

    #[test]
    fn the_setters_replace_the_fields() {
        let mut result = ZoneBiomeResult::new();
        let zone_result = Rc::new(ZoneGeneratorResult::new());
        result.set_zone_result(Some(zone_result));
        result.set_biome(None);
        result.set_height_threshold_context(1.5);
        result.set_heightmap_noise(2.5);
        assert!(result.get_zone_result().is_some());
        assert!(result.get_biome().is_none());
        assert_eq!(result.get_height_threshold_context(), 1.5);
        assert_eq!(result.get_heightmap_noise(), 2.5);
    }
}
