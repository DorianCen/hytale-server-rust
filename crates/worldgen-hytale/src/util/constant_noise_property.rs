// Ported from: `.hytale-source/.../worldgen/util/ConstantNoiseProperty.java`
//! Java `com.hypixel.hytale.server.worldgen.util.ConstantNoiseProperty` —
//! holder for the two constant noise properties used as defaults.
//!
//! Java's class is a static-only holder (its constructor throws
//! `UnsupportedOperationException`), so the Rust port exposes the two
//! constants as functions returning freshly allocated instances.

use std::rc::Rc;

use procedural::noise::constant_noise::ConstantNoise;
use procedural::noise::function::NoiseFunction;
use procedural::noise::property::single_noise_property::SingleNoiseProperty;
use procedural::noise::support::NoiseProperty;

/// Java `ConstantNoiseProperty.DEFAULT_ZERO` =
/// `new SingleNoiseProperty(0, new ConstantNoise(0))`.
///
/// Note: `SingleNoiseProperty` remaps the noise function's `[-1, 1]` output to
/// `[0, 1]`, so this property actually evaluates to `0.5`. The Java code has
/// the same behaviour — the port keeps it bit-exact.
pub fn default_zero() -> Rc<dyn NoiseProperty> {
    let noise: Rc<dyn NoiseFunction> = Rc::new(ConstantNoise::new(0.0));
    Rc::new(SingleNoiseProperty::new(0, noise))
}

/// Java `ConstantNoiseProperty.DEFAULT_ONE` =
/// `new SingleNoiseProperty(0, new ConstantNoise(1))`.
///
/// Note: as above, the remap makes this property evaluate to `1.0`.
pub fn default_one() -> Rc<dyn NoiseProperty> {
    let noise: Rc<dyn NoiseFunction> = Rc::new(ConstantNoise::new(1.0));
    Rc::new(SingleNoiseProperty::new(0, noise))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_zero_is_the_remapped_constant_zero() {
        let property = default_zero();
        assert_eq!(property.get(1, 0.0, 0.0), 0.5);
        assert_eq!(property.get_3d(1, 0.0, 0.0, 0.0), 0.5);
    }

    #[test]
    fn default_one_is_the_remapped_constant_one() {
        let property = default_one();
        assert_eq!(property.get(1, 0.0, 0.0), 1.0);
        assert_eq!(property.get_3d(1, 0.0, 0.0, 0.0), 1.0);
    }

    #[test]
    fn instances_are_independent_but_stateless() {
        let a = default_zero();
        let b = default_zero();
        assert_eq!(a.get(3, 1.0, 2.0), b.get(3, 1.0, 2.0));
    }
}
