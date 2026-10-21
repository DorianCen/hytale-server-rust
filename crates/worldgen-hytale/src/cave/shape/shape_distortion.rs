// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/ShapeDistortion.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.ShapeDistortion`
//! — the width / floor / ceiling noise that distorts a cave node shape.
//!
//! The three noises are evaluated at the *distorted* (rotated) `x/z`
//! coordinates of the shape and modulate:
//!
//! | Noise             | Java accessor         | Applied by                                     |
//! |-------------------|-----------------------|------------------------------------------------|
//! | `widthNoise`      | `getWidthFactor`      | the horizontal radius of an extrusion / body    |
//! | `floorNoise`      | `getFloorFactor`      | the vertical radius below the shape centre      |
//! | `ceilingNoise`    | `getCeilingFactor`    | the vertical radius above the shape centre      |
//!
//! # Java → Rust
//!
//! * Java declares the singleton `DEFAULT` (`SingleNoiseProperty(new
//!   ConstantNoise(1))`, i.e. every factor is exactly `1.0`). A process-wide
//!   `static` is impossible here because [`NoiseProperty`] is an `Rc`-based
//!   trait object (not `Sync`), so the default distortion is built by
//!   [`ShapeDistortion::default`]. `of(None, None, None)` returns that value —
//!   the observable behaviour of Java's identity comparison against `DEFAULT`
//!   (the comparison only avoids allocating a value-equal object).
//! * `@Nullable NoiseProperty` → `Option<Rc<dyn NoiseProperty>>`.
//! * File location: Java puts this class in the `distorted` package; the
//!   port keeps it next to `distorted_cave_node_shape` as specified by the
//!   4d slice plan.

use std::rc::Rc;

use procedural::noise::property::SingleNoiseProperty;
use procedural::noise::support::NoiseProperty;
use procedural::noise::ConstantNoise;

/// Java `class ShapeDistortion`.
///
/// Deviation: Java hands the *same* immutable instance to every generated cave
/// node shape. The Rust port derives `Clone`, which clones the three `Rc`
/// handles (the noise objects stay shared), so cloning is observably
/// identical to sharing the Java object.
#[derive(Clone)]
pub struct ShapeDistortion {
    /// Java `private final NoiseProperty widthNoise`.
    width_noise: Rc<dyn NoiseProperty>,
    /// Java `private final NoiseProperty floorNoise`.
    floor_noise: Rc<dyn NoiseProperty>,
    /// Java `private final NoiseProperty ceilingNoise`.
    ceiling_noise: Rc<dyn NoiseProperty>,
}

impl ShapeDistortion {
    /// Java `private static final NoiseProperty DEFAULT_NOISE =
    /// new SingleNoiseProperty(new ConstantNoise(1))`.
    ///
    /// `SingleNoiseProperty.get` remaps the raw noise from `[-1, 1]` to
    /// `[0, 1]` (`limit(value * 0.5 + 0.5)`), so the constant `1` becomes
    /// exactly `1.0`.
    fn default_noise() -> Rc<dyn NoiseProperty> {
        Rc::new(SingleNoiseProperty::new(
            0,
            Rc::new(ConstantNoise::new(1.0)),
        ))
    }

    /// Java `public ShapeDistortion(NoiseProperty widthNoise, NoiseProperty
    /// floorNoise, NoiseProperty ceilingNoise)`.
    pub fn new(
        width_noise: Rc<dyn NoiseProperty>,
        floor_noise: Rc<dyn NoiseProperty>,
        ceiling_noise: Rc<dyn NoiseProperty>,
    ) -> Self {
        Self {
            width_noise,
            floor_noise,
            ceiling_noise,
        }
    }

    /// Java `public static ShapeDistortion of(@Nullable NoiseProperty
    /// widthNoise, @Nullable NoiseProperty floorNoise, @Nullable NoiseProperty
    /// ceilingNoise)` — substitutes the default noise for the missing ones and
    /// returns the all-default distortion without allocating.
    pub fn of(
        width_noise: Option<Rc<dyn NoiseProperty>>,
        floor_noise: Option<Rc<dyn NoiseProperty>>,
        ceiling_noise: Option<Rc<dyn NoiseProperty>>,
    ) -> Self {
        if width_noise.is_none() && floor_noise.is_none() && ceiling_noise.is_none() {
            return Self::default();
        }

        Self::new(
            width_noise.unwrap_or_else(Self::default_noise),
            floor_noise.unwrap_or_else(Self::default_noise),
            ceiling_noise.unwrap_or_else(Self::default_noise),
        )
    }

    /// Java `double getWidthFactor(int seed, double x, double z)`.
    pub fn get_width_factor(&self, seed: i32, x: f64, z: f64) -> f64 {
        self.width_noise.get(seed, x, z)
    }

    /// Java `double getFloorFactor(int seed, double x, double z)`.
    pub fn get_floor_factor(&self, seed: i32, x: f64, z: f64) -> f64 {
        self.floor_noise.get(seed, x, z)
    }

    /// Java `double getCeilingFactor(int seed, double x, double z)`.
    pub fn get_ceiling_factor(&self, seed: i32, x: f64, z: f64) -> f64 {
        self.ceiling_noise.get(seed, x, z)
    }
}

impl Default for ShapeDistortion {
    /// Java `public static final ShapeDistortion DEFAULT`.
    fn default() -> Self {
        let default_noise = Self::default_noise();
        Self {
            width_noise: Rc::clone(&default_noise),
            floor_noise: Rc::clone(&default_noise),
            ceiling_noise: default_noise,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A noise property that answers a fixed value for every sample.
    struct FixedNoise(f64);

    impl NoiseProperty for FixedNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }

        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    #[test]
    fn the_default_distortion_does_not_change_the_shape() {
        let distortion = ShapeDistortion::default();
        assert_eq!(distortion.get_width_factor(1234, 12.5, -7.25), 1.0);
        assert_eq!(distortion.get_floor_factor(1234, 12.5, -7.25), 1.0);
        assert_eq!(distortion.get_ceiling_factor(1234, 12.5, -7.25), 1.0);
    }

    #[test]
    fn of_with_only_null_noises_is_the_default() {
        let distortion = ShapeDistortion::of(None, None, None);
        assert_eq!(distortion.get_width_factor(42, 1.0, 2.0), 1.0);
        assert_eq!(distortion.get_floor_factor(42, 1.0, 2.0), 1.0);
        assert_eq!(distortion.get_ceiling_factor(42, 1.0, 2.0), 1.0);
    }

    #[test]
    fn of_uses_the_default_noise_for_the_missing_channels() {
        let distortion = ShapeDistortion::of(
            Some(Rc::new(FixedNoise(0.25))),
            None,
            Some(Rc::new(FixedNoise(-0.5))),
        );
        assert_eq!(distortion.get_width_factor(0, 0.0, 0.0), 0.25);
        // The default channel is the remapped constant `1` → `1.0`.
        assert_eq!(distortion.get_floor_factor(0, 0.0, 0.0), 1.0);
        assert_eq!(distortion.get_ceiling_factor(0, 0.0, 0.0), -0.5);
    }
}
