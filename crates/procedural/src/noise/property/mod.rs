// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/property/*.java`
//! Java `procedurallib.property` package — the `NoiseProperty` combinator DSL.
//!
//! The `NoiseProperty` interface itself lives in [`crate::noise::support`]
//! (it is needed by lower-level modules). This package provides the concrete
//! combinators. JSON/Codec deserialization is out of scope (later phase);
//! where a Java Codec defined a default, it is documented at the constructor.

pub mod blend_noise_property;
pub mod curve_noise_property;
pub mod distorted_noise_property;
pub mod fractal_noise_property;
pub mod gradient_noise_property;
pub mod invert_noise_property;
pub mod max_noise_property;
pub mod min_noise_property;
pub mod multiply_noise_property;
pub mod noise_formula_property;
pub mod noise_property_type;
pub mod normalize_noise_property;
pub mod offset_noise_property;
pub mod rotate_noise_property;
pub mod scale_noise_property;
pub mod single_noise_property;
pub mod sum_noise_property;

pub use blend_noise_property::BlendNoiseProperty;
pub use curve_noise_property::{CurveNoiseProperty, PowerCurve};
pub use distorted_noise_property::DistortedNoiseProperty;
pub use fractal_noise_property::{FractalMode, FractalNoiseProperty};
pub use gradient_noise_property::{GradientMode, GradientNoiseProperty};
pub use invert_noise_property::InvertNoiseProperty;
pub use max_noise_property::MaxNoiseProperty;
pub use min_noise_property::MinNoiseProperty;
pub use multiply_noise_property::MultiplyNoiseProperty;
pub use noise_formula_property::{NoiseFormula, NoiseFormulaProperty};
pub use noise_property_type::NoisePropertyType;
pub use normalize_noise_property::NormalizeNoiseProperty;
pub use offset_noise_property::OffsetNoiseProperty;
pub use rotate_noise_property::RotateNoiseProperty;
pub use scale_noise_property::ScaleNoiseProperty;
pub use single_noise_property::SingleNoiseProperty;
pub use sum_noise_property::{SumNoiseProperty, SumNoisePropertyEntry};
