// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/jitter/CellJitter.java`
//! Java `CellJitter` interface — ported as a closed `Copy` enum because the
//! codebase contains exactly two implementations (Default / Constant) plus the
//! `of(x, y, z)` factory. Structs keep their 1:1 per-class files.

use crate::noise::cell::jitter::{ConstantCellJitter, DefaultCellJitter};
use crate::noise::double_array::{Double2, Double3};

/// Java `CellJitter` — dispatch enum over its two implementations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellJitter {
    /// Java `DefaultCellJitter`.
    Default(DefaultCellJitter),
    /// Java `ConstantCellJitter`.
    Constant(ConstantCellJitter),
}

impl CellJitter {
    /// Java `DefaultCellJitter.DEFAULT_ONE`.
    pub const DEFAULT_ONE: CellJitter = CellJitter::Default(DefaultCellJitter);

    /// Java static `CellJitter.of(double x, double y, double z)`.
    pub fn of(x: f64, y: f64, z: f64) -> CellJitter {
        if x == 1.0 && y == 1.0 && z == 1.0 {
            CellJitter::Default(DefaultCellJitter)
        } else {
            CellJitter::Constant(ConstantCellJitter::new(x, y, z))
        }
    }

    /// Java `CellJitter.getMaxX()`.
    pub fn get_max_x(&self) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_max_x(),
            CellJitter::Constant(j) => j.get_max_x(),
        }
    }

    /// Java `CellJitter.getMaxY()`.
    pub fn get_max_y(&self) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_max_y(),
            CellJitter::Constant(j) => j.get_max_y(),
        }
    }

    /// Java `CellJitter.getMaxZ()`.
    pub fn get_max_z(&self) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_max_z(),
            CellJitter::Constant(j) => j.get_max_z(),
        }
    }

    /// Java `CellJitter.getPointX(int cx, DoubleArray.Double2 vec)`.
    pub fn get_point_x2(&self, cx: i32, vec: Double2) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_point_x2(cx, vec),
            CellJitter::Constant(j) => j.get_point_x2(cx, vec),
        }
    }

    /// Java `CellJitter.getPointY(int cy, DoubleArray.Double2 vec)`.
    pub fn get_point_y2(&self, cy: i32, vec: Double2) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_point_y2(cy, vec),
            CellJitter::Constant(j) => j.get_point_y2(cy, vec),
        }
    }

    /// Java `CellJitter.getPointX(int cx, DoubleArray.Double3 vec)`.
    pub fn get_point_x3(&self, cx: i32, vec: Double3) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_point_x3(cx, vec),
            CellJitter::Constant(j) => j.get_point_x3(cx, vec),
        }
    }

    /// Java `CellJitter.getPointY(int cy, DoubleArray.Double3 vec)`.
    pub fn get_point_y3(&self, cy: i32, vec: Double3) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_point_y3(cy, vec),
            CellJitter::Constant(j) => j.get_point_y3(cy, vec),
        }
    }

    /// Java `CellJitter.getPointZ(int cz, DoubleArray.Double3 vec)`.
    pub fn get_point_z3(&self, cz: i32, vec: Double3) -> f64 {
        match self {
            CellJitter::Default(j) => j.get_point_z3(cz, vec),
            CellJitter::Constant(j) => j.get_point_z3(cz, vec),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_factory_matches_java() {
        assert_eq!(CellJitter::of(1.0, 1.0, 1.0), CellJitter::DEFAULT_ONE);
        assert!(matches!(
            CellJitter::of(0.5, 1.0, 1.0),
            CellJitter::Constant(_)
        ));
    }

    #[test]
    fn default_jitter_points() {
        let vec = Double2 { x: 0.25, y: -0.75 };
        assert_eq!(CellJitter::DEFAULT_ONE.get_point_x2(4, vec), 4.25);
        assert_eq!(CellJitter::DEFAULT_ONE.get_point_y2(4, vec), 3.25);
        assert_eq!(CellJitter::DEFAULT_ONE.get_max_x(), 1.0);
    }

    #[test]
    fn constant_jitter_points() {
        let vec = Double2 { x: 0.25, y: -0.75 };
        let j = CellJitter::of(0.5, 0.25, 2.0);
        assert_eq!(j.get_point_x2(4, vec), 4.0 + 0.125);
        assert_eq!(j.get_point_y2(4, vec), 4.0 - 0.1875);
        assert_eq!(j.get_max_y(), 0.25);
    }
}
