// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/jitter/DefaultCellJitter.java`
//! Java `DefaultCellJitter` — no jitter: the cell centre-point is the raw
//! offset applied directly to the cell index.

use crate::noise::double_array::{Double2, Double3};

/// Java `DefaultCellJitter`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DefaultCellJitter;

impl DefaultCellJitter {
    /// Java `DefaultCellJitter.DEFAULT_ONE`.
    pub const DEFAULT_ONE: Self = Self;

    /// Java `getMaxX()`.
    pub const fn get_max_x(&self) -> f64 {
        1.0
    }

    /// Java `getMaxY()`.
    pub const fn get_max_y(&self) -> f64 {
        1.0
    }

    /// Java `getMaxZ()`.
    pub const fn get_max_z(&self) -> f64 {
        1.0
    }

    /// Java `getPointX(int cx, DoubleArray.Double2 vec)`.
    pub fn get_point_x2(&self, cx: i32, vec: Double2) -> f64 {
        cx as f64 + vec.x
    }

    /// Java `getPointY(int cy, DoubleArray.Double2 vec)`.
    pub fn get_point_y2(&self, cy: i32, vec: Double2) -> f64 {
        cy as f64 + vec.y
    }

    /// Java `getPointX(int cx, DoubleArray.Double3 vec)`.
    pub fn get_point_x3(&self, cx: i32, vec: Double3) -> f64 {
        cx as f64 + vec.x
    }

    /// Java `getPointY(int cy, DoubleArray.Double3 vec)`.
    pub fn get_point_y3(&self, cy: i32, vec: Double3) -> f64 {
        cy as f64 + vec.y
    }

    /// Java `getPointZ(int cz, DoubleArray.Double3 vec)`.
    pub fn get_point_z3(&self, cz: i32, vec: Double3) -> f64 {
        cz as f64 + vec.z
    }
}
