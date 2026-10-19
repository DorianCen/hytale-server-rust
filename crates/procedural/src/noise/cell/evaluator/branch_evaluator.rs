// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/BranchEvaluator.java`
//! Java `BranchEvaluator`.
//!
//! ⚠ NOTE(subagent): `org.joml.Vector2i` is `support::Vector2i`; the 3d/second
//! 2d overloads are no-ops exactly like the (empty) Java bodies.

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::cell_point_function::CellPointFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::jitter::CellJitter;
use crate::noise::math_util;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use crate::noise::support::Vector2i;

/// Java `BranchEvaluator.Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchDirection {
    Outward,
    Inward,
    Random,
}

/// Mask applied to the cell hash to determine whether the cell connects in the
/// x or y axes (Java `CARDINAL_MASK`).
const CARDINAL_MASK: i32 = 1;
/// Value that (masked against [`CARDINAL_MASK`]) produces a connection in the
/// x-axis (Java `CARDINAL_MASK_RESULT_X`).
const CARDINAL_MASK_RESULT_X: i32 = 0;
/// Value that (masked against [`CARDINAL_MASK`]) produces a connection in the
/// y-axis (Java `CARDINAL_MASK_RESULT_Y`).
const CARDINAL_MASK_RESULT_Y: i32 = 1;
/// Mask applied to the parent region's hash to produce an index used to look
/// up a direction vector from [`RANDOM_DIRECTIONS`] (Java `RANDOM_DIRECTION_MASK`).
const RANDOM_DIRECTION_MASK: i32 = 3;

/// Java `RANDOM_DIRECTIONS`.
const RANDOM_DIRECTIONS: [Vector2i; 4] = [
    Vector2i::new(1, 1),
    Vector2i::new(1, -1),
    Vector2i::new(-1, 1),
    Vector2i::new(-1, -1),
];

/// Java `BranchEvaluator`.
#[derive(Clone)]
pub struct BranchEvaluator {
    /// Java `@Nonnull protected final CellPointFunction pointFunction`.
    pub point_function: Rc<dyn CellPointFunction>,
    /// Java `protected final Direction direction`.
    pub direction: BranchDirection,
    /// Java `protected final CellJitter jitter`.
    pub jitter: CellJitter,
    /// Java `protected final double branch2parentScale`.
    pub branch2parent_scale: f64,
    /// Java `protected final double invLineNormalization`.
    pub inv_line_normalization: f64,
}

impl BranchEvaluator {
    /// Java `BranchEvaluator(CellDistanceFunction parentFunction,
    /// CellPointFunction linePointFunction, Direction direction, CellJitter
    /// jitter, double branchScale)`.
    pub fn new(
        parent_function: Rc<dyn CellDistanceFunction>,
        line_point_function: Rc<dyn CellPointFunction>,
        direction: BranchDirection,
        jitter: CellJitter,
        branch_scale: f64,
    ) -> Self {
        // Convert branch coords back to normal coordinate scale
        let inverse_scalar = 1.0 / line_point_function.scale(branch_scale);
        // Convert normal coords to parent's coordinate scale
        let branch2parent_scale = parent_function.scale(inverse_scalar);
        let inv_line_normalization = 1.0 / line_point_function.normalize(1.0);

        Self {
            point_function: line_point_function,
            direction,
            jitter,
            branch2parent_scale,
            inv_line_normalization,
        }
    }

    /// Java `protected static int getConnectionX(Direction, int regionHash,
    /// double regionCoord, int cellHash, double cellCoord)`.
    #[inline]
    fn get_connection_x(
        direction: BranchDirection,
        region_hash: i32,
        region_coord: f64,
        cell_hash: i32,
        cell_coord: f64,
    ) -> i32 {
        if (cell_hash & CARDINAL_MASK) != CARDINAL_MASK_RESULT_X {
            return 0;
        }

        match direction {
            BranchDirection::Inward => {
                if cell_coord > region_coord {
                    -1
                } else {
                    1
                }
            }
            BranchDirection::Outward => {
                if cell_coord < region_coord {
                    -1
                } else {
                    1
                }
            }
            BranchDirection::Random => RANDOM_DIRECTIONS[(region_hash & RANDOM_DIRECTION_MASK) as usize].x,
        }
    }

    /// Java `protected static int getConnectionY(Direction, ...)`.
    #[inline]
    fn get_connection_y(
        direction: BranchDirection,
        region_hash: i32,
        region_coord: f64,
        cell_hash: i32,
        cell_coord: f64,
    ) -> i32 {
        if (cell_hash & CARDINAL_MASK) != CARDINAL_MASK_RESULT_Y {
            return 0;
        }

        match direction {
            BranchDirection::Inward => {
                if cell_coord > region_coord {
                    -1
                } else {
                    1
                }
            }
            BranchDirection::Outward => {
                if cell_coord < region_coord {
                    -1
                } else {
                    1
                }
            }
            BranchDirection::Random => RANDOM_DIRECTIONS[(region_hash & RANDOM_DIRECTION_MASK) as usize].y,
        }
    }

    /// Java `protected static boolean checkBounds(double x, double y, double ax,
    /// double ay, double bx, double by, double thickness)`.
    #[inline]
    fn check_bounds(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64, thickness: f64) -> bool {
        let min_x = ax.min(bx) - thickness;
        let min_y = ay.min(by) - thickness;
        let max_x = ax.max(bx) + thickness;
        let max_y = ay.max(by) + thickness;
        x > min_x && x < max_x && y > min_y && y < max_y
    }
}

#[allow(clippy::too_many_arguments)]
impl PointEvaluator for BranchEvaluator {
    /// Java `getJitter()`.
    fn get_jitter(&self) -> CellJitter {
        self.jitter
    }

    fn eval_point_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        hash_a: i32,
        cax: i32,
        cay: i32,
        ax: f64,
        ay: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        // Get the neighbour direction
        let dx = Self::get_connection_x(
            self.direction,
            buffer.ix2,
            buffer.x2,
            hash_a,
            ax * self.branch2parent_scale,
        );
        let dy = Self::get_connection_y(
            self.direction,
            buffer.ix2,
            buffer.y2,
            hash_a,
            ay * self.branch2parent_scale,
        );

        // Calculate neighbour cell b's voronoi point
        let cbx = cax + dx;
        let cby = cay + dy;
        let hash_b = self.point_function.get_hash(seed, cbx, cby);
        let offsets_b = self.point_function.get_offsets(hash_b);

        let raw_bx = self.jitter.get_point_x2(cbx, offsets_b);
        let raw_by = self.jitter.get_point_y2(cby, offsets_b);
        let bx = self.point_function.get_x(raw_bx, raw_by);
        let by = self.point_function.get_y(raw_bx, raw_by);

        // Check if x,y is within range of the line ab
        if !Self::check_bounds(x, y, ax, ay, bx, by, buffer.distance2) {
            return;
        }

        // Calculate squared distance from x,y to line ab
        let mut dist2 = math_util::distance_to_line_sq(x, y, ax, ay, bx, by);

        // Apply the inverse normalization so that when the CellDistFunction calling
        // us normalizes buffer.distance the true distance is maintained.
        dist2 *= self.inv_line_normalization;

        buffer.register(hash_a, cax, cay, dist2, ax, ay);
    }

    fn eval_point2_2d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _cell_hash: i32,
        _xi: i32,
        _yi: i32,
        _vec_x: f64,
        _vec_y: f64,
        _buffer: &mut ResultBuffer2d,
    ) {
        // Java leaves this body empty
    }

    fn eval_point_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_hash: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _cell_point_x: f64,
        _cell_point_y: f64,
        _cell_point_z: f64,
        _buffer: &mut ResultBuffer3d,
    ) {
        // Java leaves this body empty
    }

    fn eval_point2_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_hash: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _cell_point_x: f64,
        _cell_point_y: f64,
        _cell_point_z: f64,
        _buffer: &mut ResultBuffer3d,
    ) {
        // Java leaves this body empty
    }
}
