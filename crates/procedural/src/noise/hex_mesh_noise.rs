// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/HexMeshNoise.java`
//! Java `HexMeshNoise`.
//!
//! ⚠ NOTE(subagent): `condition.IIntCondition` is `support::IIntCondition`.
//! The Java 3d `get` throws `UnsupportedOperationException("3d not supported")`
//! — ported as a panic.

use std::rc::Rc;

use crate::noise::cell::hex_cell_distance_function::{
    HexCellDistanceFunction, HEX_CELL_2D, SCALE,
};
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::math_util;
use crate::noise::support::IIntCondition;

/// Java `HexMeshNoise`.
#[derive(Clone)]
pub struct HexMeshNoise {
    /// Java `protected final IIntCondition density`.
    pub density: Rc<dyn IIntCondition>,
    /// Java `protected final double thickness`.
    pub thickness: f64,
    /// Java `protected final double thicknessSquared`.
    pub thickness_squared: f64,
    /// Java `protected final CellJitter jitter`.
    pub jitter: crate::noise::cell::jitter::CellJitter,
    /// Java `protected final boolean linesX`.
    pub lines_x: bool,
    /// Java `protected final boolean linesY`.
    pub lines_y: bool,
    /// Java `protected final boolean linesZ`.
    pub lines_z: bool,
}

#[allow(clippy::too_many_arguments)]
impl HexMeshNoise {
    /// Java `HexMeshNoise(IIntCondition density, double thickness, CellJitter
    /// jitter, boolean linesX, boolean linesY, boolean linesZ)`.
    pub fn new(
        density: Rc<dyn IIntCondition>,
        thickness: f64,
        jitter: crate::noise::cell::jitter::CellJitter,
        lines_x: bool,
        lines_y: bool,
        lines_z: bool,
    ) -> Self {
        // Note: Hex noise uses a modified domain scale. We need to apply this
        // to the thickness input value so that the resulting noise produces
        // lines with equivalent width to the square-based version of mesh noise.
        // Java: double domainLocalThickness = DISTANCE_FUNCTION.scale(thickness);
        let domain_local_thickness = thickness * SCALE;

        Self {
            density,
            thickness: domain_local_thickness,
            thickness_squared: domain_local_thickness * domain_local_thickness,
            jitter,
            lines_x,
            lines_y,
            lines_z,
        }
    }

    /// Java `protected double checkConnections(int offsetSeed, double x, double y,
    /// int cx, int cy, double nearest)`.
    fn check_connections(
        &self,
        offset_seed: i32,
        x: f64,
        y: f64,
        cx: i32,
        cy: i32,
        nearest: f64,
    ) -> f64 {
        let mut nearest = nearest;
        let hash = HexCellDistanceFunction::get_hash(offset_seed, cx, cy);
        if !self.density.eval(hash) {
            return nearest;
        }

        let vec = HEX_CELL_2D[(hash & 255) as usize];
        let px = self.jitter.get_point_x2(cx, vec);
        let py = self.jitter.get_point_y2(cy, vec);

        let ax = HexCellDistanceFunction::to_hex_x(px, py);
        let ay = HexCellDistanceFunction::to_hex_y(px, py);

        let adx = x - ax;
        let ady = y - ay;

        if self.lines_x {
            nearest = nearest.min(self.dist_2_cell(
                offset_seed, x, y, adx, ady, ax, ay, cx - 1, cy,
            ));
            nearest = nearest.min(self.dist_2_cell(
                offset_seed, x, y, adx, ady, ax, ay, cx + 1, cy,
            ));
        }

        if self.lines_y {
            nearest = nearest.min(self.dist_2_cell(
                offset_seed, x, y, adx, ady, ax, ay, cx, cy - 1,
            ));
            nearest = nearest.min(self.dist_2_cell(
                offset_seed, x, y, adx, ady, ax, ay, cx, cy + 1,
            ));
        }

        nearest
    }

    /// Java `protected double checkDiagonalConnections(int offsetSeed, double x,
    /// double y, int cx, int cy, double nearest)`.
    fn check_diagonal_connections(
        &self,
        offset_seed: i32,
        x: f64,
        y: f64,
        cx: i32,
        cy: i32,
        nearest: f64,
    ) -> f64 {
        let mut nearest = nearest;
        let hash = HexCellDistanceFunction::get_hash(offset_seed, cx, cy);
        if !self.density.eval(hash) {
            return nearest;
        }

        let vec = HEX_CELL_2D[(hash & 255) as usize];
        let px = self.jitter.get_point_x2(cx, vec);
        let py = self.jitter.get_point_y2(cy, vec);

        let ax = HexCellDistanceFunction::to_hex_x(px, py);
        let ay = HexCellDistanceFunction::to_hex_y(px, py);

        let adx = x - ax;
        let ady = y - ay;

        nearest = nearest.min(self.dist_2_cell(
            offset_seed, x, y, adx, ady, ax, ay, cx - 1, cy + 1,
        ));
        nearest = nearest.min(self.dist_2_cell(
            offset_seed, x, y, adx, ady, ax, ay, cx + 1, cy - 1,
        ));

        nearest
    }

    /// Java `protected double dist2Cell(int offsetSeed, double x, double y,
    /// double adx, double ady, double ax, double ay, int cx, int cy)`.
    fn dist_2_cell(
        &self,
        offset_seed: i32,
        x: f64,
        y: f64,
        adx: f64,
        ady: f64,
        ax: f64,
        ay: f64,
        cx: i32,
        cy: i32,
    ) -> f64 {
        let hash = HexCellDistanceFunction::get_hash(offset_seed, cx, cy);
        if !self.density.eval(hash) {
            return f64::MAX;
        }

        let vec = HEX_CELL_2D[(hash & 255) as usize];
        let px = self.jitter.get_point_x2(cx, vec);
        let py = self.jitter.get_point_y2(cy, vec);

        let bx = HexCellDistanceFunction::to_hex_x(px, py);
        let by = HexCellDistanceFunction::to_hex_y(px, py);

        // Java: MathUtil.distanceToLineSq(x, y, ax, ay, bx, by, adx, ady, bx - ax, by - ay)
        math_util::distance_to_line_sq_deltas(
            x,
            y,
            ax,
            ay,
            bx,
            by,
            adx,
            ady,
            bx - ax,
            by - ay,
        )
    }
}

impl NoiseFunction2d for HexMeshNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        // Java: x = DISTANCE_FUNCTION.scale(x) — DISTANCE_FUNCTION is a
        // HexCellDistanceFunction, whose scale is `value * SCALE`.
        let x = x * SCALE;
        let y = y * SCALE;

        let cx = HexCellDistanceFunction::to_grid_x(x, y);
        let cy = HexCellDistanceFunction::to_grid_y(x, y);

        let mut nearest = self.thickness_squared;
        nearest = self.check_connections(offset_seed, x, y, cx - 1, cy - 1, nearest);
        nearest = self.check_connections(offset_seed, x, y, cx - 1, cy + 0, nearest);
        nearest = self.check_connections(offset_seed, x, y, cx + 1, cy + 0, nearest);
        nearest = self.check_connections(offset_seed, x, y, cx + 0, cy - 1, nearest);
        nearest = self.check_connections(offset_seed, x, y, cx + 0, cy + 1, nearest);

        if self.lines_z {
            nearest = self.check_diagonal_connections(offset_seed, x, y, cx + 0, cy + 0, nearest);
            nearest = self.check_diagonal_connections(offset_seed, x, y, cx + 0, cy - 1, nearest);
            nearest = self.check_diagonal_connections(offset_seed, x, y, cx + 0, cy + 1, nearest);
            nearest = self.check_diagonal_connections(offset_seed, x, y, cx - 1, cy + 0, nearest);
            nearest = self.check_diagonal_connections(offset_seed, x, y, cx - 1, cy - 1, nearest);
        }

        if nearest < self.thickness_squared {
            let distance = f64::sqrt(nearest);
            let d = distance / self.thickness;
            d * 2.0 - 1.0
        } else {
            1.0
        }
    }
}

impl NoiseFunction3d for HexMeshNoise {
    fn get(&self, _seed: i32, _offset_seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
        // Java: throw new UnsupportedOperationException("3d not supported");
        panic!("3d not supported");
    }
}
