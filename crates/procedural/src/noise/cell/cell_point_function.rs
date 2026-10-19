// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/CellPointFunction.java`
//! Java `CellPointFunction` interface.

use crate::noise::double_array::Double2;

/// Java `CellPointFunction`.
pub trait CellPointFunction {
    /// Java `default double scale(double value)`.
    fn scale(&self, value: f64) -> f64 {
        value
    }

    /// Java `default double normalize(double value)`.
    fn normalize(&self, value: f64) -> f64 {
        value
    }

    /// Java `int getHash(int seed, int cellX, int cellY)`.
    fn get_hash(&self, seed: i32, cell_x: i32, cell_y: i32) -> i32;

    /// Java `double getX(double x, double y)`.
    fn get_x(&self, x: f64, y: f64) -> f64;

    /// Java `double getY(double x, double y)`.
    fn get_y(&self, x: f64, y: f64) -> f64;

    /// Java `DoubleArray.Double2 getOffsets(int hash)`.
    fn get_offsets(&self, hash: i32) -> Double2;
}
