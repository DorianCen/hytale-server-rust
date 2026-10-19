// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/FadeContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.FadeContainer` — the
//! mask/terrain fade gradients attached to a zone.
//!
//! The container stores two independent gradients (the *mask* gradient and
//! the *terrain* gradient). Each gradient is described by a start offset and
//! a width; the factor at a column is derived from the distance to the
//! closest zone border.

use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `class FadeContainer`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FadeContainer {
    /// Java `protected final double maskFadeStart`.
    pub mask_fade_start: f64,
    /// Java `protected final double maskFadeLength`.
    pub mask_fade_length: f64,
    /// Java `protected final double maskFadeSum` — `maskFadeStart + maskFadeLength`.
    pub mask_fade_sum: f64,
    /// Java `protected final double terrainFadeStart`.
    pub terrain_fade_start: f64,
    /// Java `protected final double terrainFadeLength`.
    pub terrain_fade_length: f64,
    /// Java `protected final double terrainFadeSum` — `terrainFadeStart + terrainFadeLength`.
    pub terrain_fade_sum: f64,
    /// Java `protected final double fadeHeightmap`.
    pub fade_heightmap: f64,
}

impl FadeContainer {
    /// Java `public static final double NO_FADE_HEIGHTMAP` =
    /// `Double.NEGATIVE_INFINITY`.
    pub const NO_FADE_HEIGHTMAP: f64 = f64::NEG_INFINITY;

    /// Java `FadeContainer(double maskFadeStart, double maskFadeLength,
    /// double terrainFadeStart, double terrainFadeLength, double
    /// fadeHeightmap)`.
    pub const fn new(
        mask_fade_start: f64,
        mask_fade_length: f64,
        terrain_fade_start: f64,
        terrain_fade_length: f64,
        fade_heightmap: f64,
    ) -> Self {
        Self {
            mask_fade_start,
            mask_fade_length,
            mask_fade_sum: mask_fade_start + mask_fade_length,
            terrain_fade_start,
            terrain_fade_length,
            terrain_fade_sum: terrain_fade_start + terrain_fade_length,
            fade_heightmap,
        }
    }

    /// Java `double getMaskFadeStart()`.
    pub const fn get_mask_fade_start(&self) -> f64 {
        self.mask_fade_start
    }

    /// Java `double getMaskFadeLength()`.
    pub const fn get_mask_fade_length(&self) -> f64 {
        self.mask_fade_length
    }

    /// Java `double getMaskFadeSum()`.
    pub const fn get_mask_fade_sum(&self) -> f64 {
        self.mask_fade_sum
    }

    /// Java `double getHeightFadeStart()` — returns `terrainFadeStart`.
    pub const fn get_height_fade_start(&self) -> f64 {
        self.terrain_fade_start
    }

    /// Java `double getHeightFadeLength()` — returns `terrainFadeLength`.
    pub const fn get_height_fade_length(&self) -> f64 {
        self.terrain_fade_length
    }

    /// Java `double getHeightFadeSum()` — returns `terrainFadeSum`.
    pub const fn get_height_fade_sum(&self) -> f64 {
        self.terrain_fade_sum
    }

    /// Java `double getFadeHeightmap()`.
    pub const fn get_fade_heightmap(&self) -> f64 {
        self.fade_heightmap
    }

    /// Java `double getMaskFactor(ZoneGeneratorResult result)`.
    pub fn get_mask_factor(&self, result: &ZoneGeneratorResult) -> f64 {
        self.get_factor(result, self.mask_fade_start, self.mask_fade_length)
    }

    /// Java `double getTerrainFactor(ZoneGeneratorResult result)`.
    pub fn get_terrain_factor(&self, result: &ZoneGeneratorResult) -> f64 {
        self.get_factor(result, self.terrain_fade_start, self.terrain_fade_length)
    }

    /// Java `double getFactor(ZoneGeneratorResult result, double
    /// distanceFromBorder, double gradientWidth)`.
    ///
    /// ```java
    /// return 1 - limit((result.getBorderDistance() - distanceFromBorder) / gradientWidth);
    /// ```
    pub fn get_factor(
        &self,
        result: &ZoneGeneratorResult,
        distance_from_border: f64,
        gradient_width: f64,
    ) -> f64 {
        1.0 - limit((result.get_border_distance() - distance_from_border) / gradient_width)
    }

    /// Java `boolean shouldFade()`.
    pub fn should_fade(&self) -> bool {
        self.fade_heightmap != Self::NO_FADE_HEIGHTMAP
    }
}

/// Java `private static double limit(double d)`.
#[inline]
fn limit(d: f64) -> f64 {
    if d < 0.0 {
        return 0.0;
    }
    if d > 1.0 {
        return 1.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(border_distance: f64) -> ZoneGeneratorResult {
        ZoneGeneratorResult::new_with(None, border_distance)
    }

    #[test]
    fn fade_sums_are_start_plus_length() {
        let container = FadeContainer::new(2.0, 3.0, 5.0, 7.0, 0.0);
        assert_eq!(container.get_mask_fade_start(), 2.0);
        assert_eq!(container.get_mask_fade_length(), 3.0);
        assert_eq!(container.get_mask_fade_sum(), 5.0);
        assert_eq!(container.get_height_fade_start(), 5.0);
        assert_eq!(container.get_height_fade_length(), 7.0);
        assert_eq!(container.get_height_fade_sum(), 12.0);
    }

    #[test]
    fn factor_is_one_inside_the_border_and_zero_outside() {
        // mask fade: start 0, width 10 → the factor ramps from 1 (at the
        // border) down to 0 (10 units away).
        let container = FadeContainer::new(0.0, 10.0, 0.0, 10.0, 0.0);
        // Exactly on the border: (0 - 0) / 10 = 0 → 1 - 0 = 1.
        assert_eq!(container.get_mask_factor(&result(0.0)), 1.0);
        // Halfway through the gradient: (5 - 0) / 10 = 0.5 → 1 - 0.5 = 0.5.
        assert_eq!(container.get_mask_factor(&result(5.0)), 0.5);
        // At the end of the gradient: (10 - 0) / 10 = 1 → 1 - 1 = 0.
        assert_eq!(container.get_mask_factor(&result(10.0)), 0.0);
        // Beyond the gradient the ratio is clamped to 1 → the factor stays 0.
        assert_eq!(container.get_mask_factor(&result(20.0)), 0.0);
    }

    #[test]
    fn factor_is_zero_before_the_fade_start() {
        // The fade starts 4 units away from the border and is 2 units wide.
        let container = FadeContainer::new(4.0, 2.0, 4.0, 2.0, 0.0);
        // (0 - 4) / 2 = -2 → limit → 0 → factor 1.
        assert_eq!(container.get_mask_factor(&result(0.0)), 1.0);
        // (5 - 4) / 2 = 0.5 → factor 0.5.
        assert_eq!(container.get_mask_factor(&result(5.0)), 0.5);
        // (6 - 4) / 2 = 1 → factor 0.
        assert_eq!(container.get_mask_factor(&result(6.0)), 0.0);
        // (7 - 4) / 2 = 1.5 → limit → 1 → factor 0.
        assert_eq!(container.get_mask_factor(&result(7.0)), 0.0);
    }

    #[test]
    fn mask_and_terrain_gradients_are_independent() {
        let container = FadeContainer::new(0.0, 10.0, 0.0, 4.0, 0.0);
        let column = result(2.0);
        assert_eq!(container.get_mask_factor(&column), 0.8);
        assert_eq!(container.get_terrain_factor(&column), 0.5);
    }

    #[test]
    fn should_fade_is_false_for_the_no_fade_heightmap() {
        let container = FadeContainer::new(0.0, 1.0, 0.0, 1.0, FadeContainer::NO_FADE_HEIGHTMAP);
        assert!(!container.should_fade());
        assert!(container.get_fade_heightmap().is_infinite());
        assert!(container.get_fade_heightmap() < 0.0);

        let fading = FadeContainer::new(0.0, 1.0, 0.0, 1.0, 64.0);
        assert!(fading.should_fade());
    }
}
