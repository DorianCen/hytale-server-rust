// Ported from: `.hytale-source/.../worldgen/biome/CustomBiomeGenerator.java`
//! Java `biome.CustomBiomeGenerator` — the noise/threshold test deciding
//! where a `CustomBiome` overrides the parent tile biome.
use std::rc::Rc;

use procedural::noise::support::IDoubleThreshold;
use procedural::noise::support::{IIntCondition, NoiseProperty};

use crate::biome::biome::Biome;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `biome.CustomBiomeGenerator`.
#[derive(Clone)]
pub struct CustomBiomeGenerator {
    /// Java `protected final NoiseProperty noiseProperty`.
    noise_property: Rc<dyn NoiseProperty>,
    /// Java `protected final IDoubleThreshold threshold`.
    threshold: Rc<dyn IDoubleThreshold>,
    /// Java `protected final IIntCondition biomeMask`.
    biome_mask: Rc<dyn IIntCondition>,
    /// Java `protected final int priority`.
    priority: i32,
}

impl CustomBiomeGenerator {
    /// Java `CustomBiomeGenerator(NoiseProperty noiseProperty,
    /// IDoubleThreshold threshold, IIntCondition biomeMask, int priority)`.
    pub fn new(
        noise_property: Rc<dyn NoiseProperty>,
        threshold: Rc<dyn IDoubleThreshold>,
        biome_mask: Rc<dyn IIntCondition>,
        priority: i32,
    ) -> Self {
        Self {
            noise_property,
            threshold,
            biome_mask,
            priority,
        }
    }

    /// Java `boolean shouldGenerateAt(int seed, double x, double z,
    /// ZoneGeneratorResult zoneResult, Biome customBiome)`.
    pub fn should_generate_at(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        zone_result: &ZoneGeneratorResult,
        custom_biome: &dyn Biome,
    ) -> bool {
        let noise = self.noise_property.get(seed, x, z);
        if zone_result.get_border_distance() < custom_biome.get_fade_container().get_mask_fade_sum()
        {
            let factor = custom_biome
                .get_fade_container()
                .get_mask_factor(zone_result);
            self.is_threshold_factor(noise, factor)
        } else {
            self.is_threshold(noise)
        }
    }

    /// Java `boolean isThreshold(double d)`.
    pub fn is_threshold(&self, d: f64) -> bool {
        self.threshold.eval(d)
    }

    /// Java `boolean isThreshold(double d, double factor)`.
    pub fn is_threshold_factor(&self, d: f64, factor: f64) -> bool {
        factor >= 0.00001 && self.threshold.eval_factor(d, factor)
    }

    /// Java `boolean isValidParentBiome(int index)`.
    pub fn is_valid_parent_biome(&self, index: i32) -> bool {
        self.biome_mask.eval(index)
    }

    /// Java `int getPriority()`.
    pub fn get_priority(&self) -> i32 {
        self.priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::constant_noise_property::{default_one, default_zero};

    struct AlwaysTrueInt;
    impl IIntCondition for AlwaysTrueInt {
        fn eval(&self, _value: i32) -> bool {
            true
        }
    }

    struct AboveHalfThreshold;
    impl IDoubleThreshold for AboveHalfThreshold {
        fn eval(&self, d: f64) -> bool {
            d > 0.5
        }
        fn eval_factor(&self, d: f64, _factor: f64) -> bool {
            d > 0.5
        }
    }

    #[test]
    fn is_threshold_routes_to_the_right_overload() {
        // `default_one` remaps ConstantNoise(1) to exactly 1.0.
        let gen = CustomBiomeGenerator::new(
            default_one(),
            Rc::new(AboveHalfThreshold),
            Rc::new(AlwaysTrueInt),
            3,
        );
        assert!(gen.is_threshold(0.7));
        assert!(!gen.is_threshold(0.5));
    }

    #[test]
    fn factor_gating_requires_a_material_factor() {
        let gen = CustomBiomeGenerator::new(
            default_one(),
            Rc::new(AboveHalfThreshold),
            Rc::new(AlwaysTrueInt),
            3,
        );
        // Java: `factor >= 0.00001 && threshold.eval(d, factor)`
        assert!(!gen.is_threshold_factor(0.7, 0.0));
        assert!(gen.is_threshold_factor(0.7, 0.5));
    }

    #[test]
    fn parent_mask_and_priority_are_exposed() {
        // `default_zero` remaps ConstantNoise(0) to exactly 0.5 → not above
        // the 0.5 half-open threshold (Java-guaranteed remap).
        let gen = CustomBiomeGenerator::new(
            default_zero(),
            Rc::new(AboveHalfThreshold),
            Rc::new(AlwaysTrueInt),
            3,
        );
        assert!(gen.is_valid_parent_biome(4));
        assert_eq!(gen.get_priority(), 3);
        assert!(!gen.is_threshold(gen.noise_property.get(0, 0.0, 0.0)));
    }
}
