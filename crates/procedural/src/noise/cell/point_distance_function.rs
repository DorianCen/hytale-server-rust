// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/PointDistanceFunction.java`
//! Java `PointDistanceFunction` interface.
//!
//! Java's `distance2D(double, double)` / `distance2D(int seed, int cellX, int
//! cellY, double cellCentreX, double cellCentreY, double deltaX, double deltaY)`
//! overloads become `distance_2d` / `distance_2d_at_cell` (+ `_3d` variants).

/// Java `PointDistanceFunction`.
pub trait PointDistanceFunction {
    /// Java `double distance2D(double deltaX, double deltaY)`.
    fn distance_2d(&self, delta_x: f64, delta_y: f64) -> f64;

    /// Java `double distance3D(double deltaX, double deltaY, double deltaZ)`.
    fn distance_3d(&self, delta_x: f64, delta_y: f64, delta_z: f64) -> f64;

    /// Java default `distance2D(int seed, int cellX, int cellY, double cellCentreX,
    /// double cellCentreY, double deltaX, double deltaY)`.
    #[allow(clippy::too_many_arguments)]
    fn distance_2d_at_cell(
        &self,
        _seed: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_centre_x: f64,
        _cell_centre_y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> f64 {
        self.distance_2d(delta_x, delta_y)
    }

    /// Java default `distance3D(int seed, int cellX, int cellY, int cellZ, ..., deltas)`.
    #[allow(clippy::too_many_arguments)]
    fn distance_3d_at_cell(
        &self,
        _seed: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _cell_centre_x: f64,
        _cell_centre_y: f64,
        _cell_centre_z: f64,
        delta_x: f64,
        delta_y: f64,
        delta_z: f64,
    ) -> f64 {
        self.distance_3d(delta_x, delta_y, delta_z)
    }
}
