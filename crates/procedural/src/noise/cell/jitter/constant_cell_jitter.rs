// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/jitter/ConstantCellJitter.java`
//! Java `ConstantCellJitter`.

use crate::noise::double_array::{Double2, Double3};

/// Java `ConstantCellJitter`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantCellJitter {
    /// Java `protected final double jitterX, jitterY, jitterZ`.
    pub jitter_x: f64,
    pub jitter_y: f64,
    pub jitter_z: f64,
}

impl ConstantCellJitter {
    /// Java `ConstantCellJitter(double jitterX, double jitterY, double jitterZ)`.
    pub fn new(jitter_x: f64, jitter_y: f64, jitter_z: f64) -> Self {
        Self {
            jitter_x,
            jitter_y,
            jitter_z,
        }
    }

    /// Java `getMaxX()`.
    pub const fn get_max_x(&self) -> f64 {
        self.jitter_x
    }

    /// Java `getMaxY()`.
    pub const fn get_max_y(&self) -> f64 {
        self.jitter_y
    }

    /// Java `getMaxZ()`.
    pub const fn get_max_z(&self) -> f64 {
        self.jitter_z
    }

    /// Java `getPointX(int cx, DoubleArray.Double2 vec)`.
    pub fn get_point_x2(&self, cx: i32, vec: Double2) -> f64 {
        cx as f64 + vec.x * self.jitter_x
    }

    /// Java `getPointY(int cy, DoubleArray.Double2 vec)`.
    pub fn get_point_y2(&self, cy: i32, vec: Double2) -> f64 {
        cy as f64 + vec.y * self.jitter_y
    }

    /// Java `getPointX(int cx, DoubleArray.Double3 vec)`.
    pub fn get_point_x3(&self, cx: i32, vec: Double3) -> f64 {
        cx as f64 + vec.x * self.jitter_x
    }

    /// Java `getPointY(int cy, DoubleArray.Double3 vec)`.
    pub fn get_point_y3(&self, cy: i32, vec: Double3) -> f64 {
        cy as f64 + vec.y * self.jitter_y
    }

    /// Java `getPointZ(int cz, DoubleArray.Double3 vec)`.
    pub fn get_point_z3(&self, cz: i32, vec: Double3) -> f64 {
        cz as f64 + vec.z * self.jitter_z
    }
}
