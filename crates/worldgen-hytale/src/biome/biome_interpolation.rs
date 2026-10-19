// Ported from: `.hytale-source/.../worldgen/biome/BiomeInterpolation.java`
//! Java `biome.BiomeInterpolation` — per-biome squared interpolation radii.
//!
//! Java stores an `Int2IntMap` with `defaultReturnValue(radius * radius)`
//! (or the empty-map singleton returning `MAX_RADIUS2`). The Rust port keeps
//! that two-state structure: an empty map (Java `Int2IntMaps.EmptyMap`
//! subclass overriding `defaultReturnValue()` and `get()` to `MAX_RADIUS2`)
//! or a populated map with the squared default.
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::chunk::{MAX_RADIUS, MAX_RADIUS2};

/// Java `BiomeInterpolation` (holding the inner `EmptyInt2IntMap` /
/// populated-map split).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiomeInterpolation {
    /// Java `protected final int radius`.
    radius: i32,
    /// Java `protected final Int2IntMap biomeRadii2` — `None` = the Java
    /// empty-map singleton (lookup always yields `MAX_RADIUS2`).
    biome_radii2: Option<HashMap<i32, i32>>,
}

/// Java `BiomeInterpolation.DEFAULT`
/// = `new BiomeInterpolation(HeightThresholdInterpolator.MAX_RADIUS, EMPTY_MAP)`.
pub static DEFAULT: LazyLock<BiomeInterpolation> = LazyLock::new(|| BiomeInterpolation {
    radius: MAX_RADIUS,
    biome_radii2: None,
});

impl BiomeInterpolation {
    /// Java `BiomeInterpolation.create(int radius, Int2IntMap biomeRadii2)`.
    ///
    /// Transcribed exactly: the shared default instance is reused when the
    /// radius matches and the map is empty; empty maps keep the
    /// `MAX_RADIUS2`-defaulting semantics; populated maps default their
    /// missing entries to `radius * radius`.
    pub fn create(radius: i32, biome_radii2: HashMap<i32, i32>) -> Self {
        if radius == DEFAULT.radius && biome_radii2.is_empty() {
            return (*DEFAULT).clone();
        }

        // Note: Enforce that maps return the correct default value
        if biome_radii2.is_empty() {
            BiomeInterpolation {
                radius,
                biome_radii2: None,
            }
        } else {
            BiomeInterpolation {
                radius,
                biome_radii2: Some(biome_radii2),
            }
        }
    }

    /// Java `int getRadius()`.
    pub fn get_radius(&self) -> i32 {
        self.radius
    }

    /// Java `int getBiomeRadius2(int biome)` — the squared interpolation
    /// radius for a given biome id.
    ///
    /// Missing entries follow the Java `Int2IntMap.defaultReturnValue`:
    /// populated maps fall back to `radius * radius`, the empty map falls
    /// back to `MAX_RADIUS2` (Java `EmptyInt2IntMap.get`).
    pub fn get_biome_radius2(&self, biome: i32) -> i32 {
        match &self.biome_radii2 {
            None => MAX_RADIUS2,
            Some(map) => map
                .get(&biome)
                .copied()
                .unwrap_or(self.radius * self.radius),
        }
    }

    /// Java `defaultReturnValue()` of the underlying map — populated maps:
    /// `radius * radius`; the empty map: `MAX_RADIUS2`.
    pub fn get_default_return(&self) -> i32 {
        match &self.biome_radii2 {
            None => MAX_RADIUS2,
            Some(_) => self.radius * self.radius,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_the_height_threshold_constants() {
        assert_eq!(DEFAULT.get_radius(), 5);
        assert_eq!(DEFAULT.get_biome_radius2(123), 25);
        assert!(DEFAULT.biome_radii2.is_none());
    }

    #[test]
    fn create_reuses_default_for_default_radius_and_empty_map() {
        let created = BiomeInterpolation::create(MAX_RADIUS, HashMap::new());
        assert_eq!(created, *DEFAULT);
    }

    #[test]
    fn populated_maps_default_to_radius_squared() {
        let mut radii = HashMap::new();
        radii.insert(7, 4); // 2² — custom radius for biome 7
        let interp = BiomeInterpolation::create(3, radii);
        assert_eq!(interp.get_biome_radius2(7), 4);
        assert_eq!(interp.get_biome_radius2(9), 9); // 3² default
        assert_eq!(interp.get_default_return(), 9);
    }

    #[test]
    fn non_default_radius_with_empty_map_still_defaults_to_max_radius_squared() {
        let interp = BiomeInterpolation::create(8, HashMap::new());
        assert_eq!(interp.get_biome_radius2(1), 25); // MAX_RADIUS²
        assert_eq!(interp.get_radius(), 8);
    }
}
