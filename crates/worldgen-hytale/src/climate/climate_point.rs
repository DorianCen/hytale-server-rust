// Ported from: `.hytale-source/.../worldgen/climate/ClimatePoint.java`
//! Java `climate.ClimatePoint`.

/// Java `climate.ClimatePoint` — a point in (temperature, intensity) space
/// with a weighted-voronoi modifier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimatePoint {
    /// Java `public double temperature`.
    pub temperature: f64,
    /// Java `public double intensity`.
    pub intensity: f64,
    /// Java `public double modifier`.
    pub modifier: f64,
}

impl ClimatePoint {
    /// Java `ClimatePoint(double temperature, double intensity)` — `modifier = 1.0`.
    pub fn new(temperature: f64, intensity: f64) -> Self {
        Self::with_modifier(temperature, intensity, 1.0)
    }

    /// Java `ClimatePoint(double temperature, double intensity, double modifier)`.
    pub fn with_modifier(temperature: f64, intensity: f64, modifier: f64) -> Self {
        Self {
            temperature,
            intensity,
            modifier,
        }
    }
}
