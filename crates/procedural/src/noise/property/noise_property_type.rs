// Ported from: `.hytale-source/.../procedurallib/property/NoisePropertyType.java`
//! Java `NoisePropertyType` — the property kinds known to the noise JSON loaders.

/// Java `NoisePropertyType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoisePropertyType {
    /// Java `DISTORTED`.
    Distorted,
    /// Java `MAX`.
    Max,
    /// Java `MIN`.
    Min,
    /// Java `MULTIPLY`.
    Multiply,
    /// Java `NORMALIZE`.
    Normalize,
    /// Java `FORMULA`.
    Formula,
    /// Java `SCALE`.
    Scale,
    /// Java `SUM`.
    Sum,
    /// Java `INVERT`.
    Invert,
    /// Java `OFFSET`.
    Offset,
    /// Java `ROTATE`.
    Rotate,
    /// Java `GRADIENT`.
    Gradient,
    /// Java `CURVE`.
    Curve,
    /// Java `BLEND`.
    Blend,
}
