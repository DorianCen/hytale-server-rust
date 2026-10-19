// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/DistanceCalculationMode.java`
//! Java `DistanceCalculationMode` enum — the four built-in
//! `PointDistanceFunction` implementations.

use crate::noise::cell::point_distance_function::PointDistanceFunction;

/// Java `DistanceCalculationMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistanceCalculationMode {
    /// Java `EUCLIDEAN` — `distance = dx * dx + dy * dy + dz * dz`.
    Euclidean,
    /// Java `MANHATTAN` — `distance = |dx| + |dy| + |dz|`.
    Manhattan,
    /// Java `NATURAL` — `distance = |dx| + |dy| + |dz| + dx * dx + dy * dy + dz * dz`.
    Natural,
    /// Java `MAX` — `distance = max(|dx|, |dy|, |dz|)`.
    Max,
}

impl PointDistanceFunction for DistanceCalculationMode {
    fn distance_2d(&self, delta_x: f64, delta_y: f64) -> f64 {
        match self {
            DistanceCalculationMode::Euclidean => delta_x * delta_x + delta_y * delta_y,
            DistanceCalculationMode::Manhattan => delta_x.abs() + delta_y.abs(),
            DistanceCalculationMode::Natural => {
                delta_x.abs() + delta_y.abs() + delta_x * delta_x + delta_y * delta_y
            }
            DistanceCalculationMode::Max => delta_x.abs().max(delta_y.abs()),
        }
    }

    fn distance_3d(&self, delta_x: f64, delta_y: f64, delta_z: f64) -> f64 {
        match self {
            DistanceCalculationMode::Euclidean => {
                delta_x * delta_x + delta_y * delta_y + delta_z * delta_z
            }
            DistanceCalculationMode::Manhattan => delta_x.abs() + delta_y.abs() + delta_z.abs(),
            DistanceCalculationMode::Natural => {
                delta_x.abs()
                    + delta_y.abs()
                    + delta_z.abs()
                    + delta_x * delta_x
                    + delta_y * delta_y
                    + delta_z * delta_z
            }
            DistanceCalculationMode::Max => delta_x.abs().max(delta_y.abs().max(delta_z.abs())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_formulas() {
        let (dx, dy, dz) = (3.0, -4.0, 12.0);
        assert_eq!(
            DistanceCalculationMode::Euclidean.distance_3d(dx, dy, dz),
            9.0 + 16.0 + 144.0
        );
        assert_eq!(
            DistanceCalculationMode::Manhattan.distance_3d(dx, dy, dz),
            19.0
        );
        assert_eq!(
            DistanceCalculationMode::Natural.distance_3d(dx, dy, dz),
            19.0 + 169.0
        );
        assert_eq!(DistanceCalculationMode::Max.distance_3d(dx, dy, dz), 12.0);
    }
}
