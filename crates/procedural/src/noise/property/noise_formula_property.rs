// Ported from: `.hytale-source/.../procedurallib/property/NoiseFormulaProperty.java`
//! Java `NoiseFormulaProperty` (+ the `NoiseFormula` formula enum).

use std::rc::Rc;

use crate::noise::property::NoiseProperty;

/// Java `NoiseFormulaProperty.NoiseFormula.Formula` — `double eval(double
/// noise)`; implemented by the [`NoiseFormula`] enum below.
pub trait Formula {
    /// Java `Formula.eval(double noise)`.
    fn eval(&self, noise: f64) -> f64;
}

impl Formula for NoiseFormula {
    fn eval(&self, noise: f64) -> f64 {
        match self {
            NoiseFormula::Normal => noise,
            NoiseFormula::Inverted => 1.0 - noise,
            NoiseFormula::Squared => noise * noise,
            NoiseFormula::InvertedSquared => 1.0 - noise * noise,
            NoiseFormula::Sqrt => f64::sqrt(noise),
            NoiseFormula::InvertedSqrt => 1.0 - f64::sqrt(noise),
            NoiseFormula::Ridged => (noise - 0.5).abs(),
            NoiseFormula::InvertedRidged => 1.0 - (noise - 0.5).abs(),
            NoiseFormula::RidgedSqrt => f64::sqrt((noise - 0.5).abs()),
            NoiseFormula::InvertedRidgedSqrt => 1.0 - f64::sqrt((noise - 0.5).abs()),
            NoiseFormula::RidgedFix => (noise * 2.0 - 1.0).abs(),
            NoiseFormula::InvertedRidgedFix => 1.0 - (noise * 2.0 - 1.0).abs(),
            NoiseFormula::RidgedSqrtFix => f64::sqrt((noise * 2.0 - 1.0).abs()),
            NoiseFormula::InvertedRidgedSqrtFix => 1.0 - f64::sqrt((noise * 2.0 - 1.0).abs()),
            NoiseFormula::RidgedSquaredFix => {
                let noise = (noise * 2.0 - 1.0).abs();
                noise * noise
            }
            NoiseFormula::InvertedRidgedSquaredFix => {
                let noise = (noise * 2.0 - 1.0).abs();
                1.0 - noise * noise
            }
        }
    }
}

/// Java `NoiseFormulaProperty.NoiseFormula` — the 16 built-in formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoiseFormula {
    /// Java `NORMAL`.
    Normal,
    /// Java `INVERTED`.
    Inverted,
    /// Java `SQUARED`.
    Squared,
    /// Java `INVERTED_SQUARED`.
    InvertedSquared,
    /// Java `SQRT`.
    Sqrt,
    /// Java `INVERTED_SQRT`.
    InvertedSqrt,
    /// Java `RIDGED`.
    Ridged,
    /// Java `INVERTED_RIDGED`.
    InvertedRidged,
    /// Java `RIDGED_SQRT`.
    RidgedSqrt,
    /// Java `INVERTED_RIDGED_SQRT`.
    InvertedRidgedSqrt,
    /// Java `RIDGED_FIX`.
    RidgedFix,
    /// Java `INVERTED_RIDGED_FIX`.
    InvertedRidgedFix,
    /// Java `RIDGED_SQRT_FIX`.
    RidgedSqrtFix,
    /// Java `INVERTED_RIDGED_SQRT_FIX`.
    InvertedRidgedSqrtFix,
    /// Java `RIDGED_SQUARED_FIX`.
    RidgedSquaredFix,
    /// Java `INVERTED_RIDGED_SQUARED_FIX`.
    InvertedRidgedSquaredFix,
}

/// Java `NoiseFormulaProperty`.
pub struct NoiseFormulaProperty {
    /// Java `protected final NoiseProperty property`.
    pub property: Rc<dyn NoiseProperty>,
    /// Java `protected final NoiseFormula.Formula formula`.
    pub formula: Rc<dyn Formula>,
}

impl NoiseFormulaProperty {
    /// Java `NoiseFormulaProperty(NoiseProperty, NoiseFormula.Formula)`.
    pub fn new(property: Rc<dyn NoiseProperty>, formula: Rc<dyn Formula>) -> Self {
        Self { property, formula }
    }

    /// Java overload taking the `NoiseFormula` enum directly.
    pub fn with_enum(property: Rc<dyn NoiseProperty>, formula: NoiseFormula) -> Self {
        Self {
            property,
            formula: Rc::new(formula),
        }
    }
}

impl NoiseProperty for NoiseFormulaProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.formula.eval(self.property.get(seed, x, y))
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.formula.eval(self.property.get_3d(seed, x, y, z))
    }
}
