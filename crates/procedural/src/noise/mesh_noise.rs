// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/MeshNoise.java`
//! Java `MeshNoise`.
//!
//! ⚠ NOTE(subagent): `condition.IIntCondition` is `support::IIntCondition`;
//! `org.joml.Vector2i` is `support::Vector2i`; the 3d `get` throws
//! `UnsupportedOperationException` in Java — ported as a panic.

use std::rc::Rc;

use crate::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
use crate::noise::cellular_noise::CELL_2D;
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::fast_floor;
use crate::noise::support::{IIntCondition, Vector2i};

/// Java statics `MeshNoise.ADJACENT_CELLS`.
pub const ADJACENT_CELLS: [Vector2i; 4] = [
    Vector2i::new(-1, 0),
    Vector2i::new(0, -1),
    Vector2i::new(1, 0),
    Vector2i::new(0, 1),
];

/// Java `MeshNoise` — `private final IIntCondition density`.
#[derive(Clone)]
pub struct MeshNoise {
    density: Rc<dyn IIntCondition>,
    thickness: f64,
    thickness_squared: f64,
    jitter_x: f64,
    jitter_y: f64,
}

impl MeshNoise {
    /// Java `MeshNoise(IIntCondition density, double thickness, double jitterX, double jitterY)`.
    pub fn new(
        density: Rc<dyn IIntCondition>,
        thickness: f64,
        jitter_x: f64,
        jitter_y: f64,
    ) -> Self {
        Self {
            density,
            thickness,
            thickness_squared: thickness * thickness,
            jitter_x,
            jitter_y,
        }
    }
}

impl NoiseFunction2d for MeshNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        let thickness = self.thickness;
        let mut lowest = self.thickness_squared;

        // Identify cell and relative position inside of the cell.
        let _x = fast_floor(x);
        let _y = fast_floor(y);
        let rx = x - _x as f64;
        let ry = y - _y as f64;

        // This loop iterates over all 3x3 cells around the _x, _y. This loop
        // iterates over 9 cells instead of having 2 nested for-loops
        // iterating from -1 to 1 we go from 0-8 as it allows me to use the
        // switch-case when entering the loop.
        // We don't need to iterate the 9th cell (3, 3) as it's handled by (2,3) and (3,2)
        for c in 0..8 {
            // This switch-case is a performance improvement and shortcut. It
            // does not provide functionality. The shortcut prevents testing
            // adjacent cells if there is no way of the connection "thickness"
            // to extend into those cells.
            match c {
                0 => {
                    // -1, -1
                    if rx >= thickness || ry >= thickness {
                        continue;
                    }
                }
                1 => {
                    // -1,  0
                    if rx >= thickness {
                        continue;
                    }
                }
                2 => {
                    // -1,  1
                    if rx >= thickness || ry < 1.0 - thickness {
                        continue;
                    }
                }
                3 => {
                    //  0, -1
                    if ry >= thickness {
                        continue;
                    }
                }
                // case 4:  0,  0 center — Java has no case 4 (no shortcut), so
                // the center cell is always processed: it always contains part
                // of the line.
                4 => {}
                5 => {
                    //  0,  1
                    if ry < 1.0 - thickness {
                        continue;
                    }
                }
                6 => {
                    //  1, -1
                    if rx < 1.0 - thickness || ry >= thickness {
                        continue;
                    }
                }
                7 => {
                    //  1,  0
                    if rx < 1.0 - thickness {
                        continue;
                    }
                }
                _ => unreachable!(),
            }

            // disassemble the cell index into cell coordinates
            let cx = c / 3;
            let cy = c % 3;

            let xr = _x + cx - 1;
            let yr = _y + cy - 1;

            // cellHash mirrors behavior from CellNoise to guarantee compatibility
            // with Mesh & Cell noise
            let cell_hash = GridCellDistanceFunction::get_hash(offset_seed, xr, yr);
            if !self.density.eval(cell_hash) {
                continue; // replicate density feature from cell noise
            }

            // relative position of this cell
            let cell = CELL_2D[(cell_hash & 255) as usize];
            let cell_x = cell.x * self.jitter_x;
            let cell_y = cell.y * self.jitter_y;

            // absolute position of cell
            let center_x = xr as f64 + cell_x;
            let center_y = yr as f64 + cell_y;

            // relative offset of query point to cell to later calculate distance
            let q_x = x - center_x;
            let q_y = y - center_y;

            // this iterates over all possible connected cells
            for v in ADJACENT_CELLS {
                // determine the cell's position of this possible location
                // similar to above
                let xi = xr + v.x;
                let yi = yr + v.y;

                let vec_hash = GridCellDistanceFunction::get_hash(offset_seed, xi, yi);
                if !self.density.eval(vec_hash) {
                    continue;
                }

                let vec = CELL_2D[(vec_hash & 255) as usize];
                let vec_x = vec.x * self.jitter_x;
                let vec_y = vec.y * self.jitter_y;

                let vx = v.x as f64 + vec_x - cell_x;
                let vy = v.y as f64 + vec_y - cell_y;

                // determine the projected location of the query point on the
                // connection line
                let mut t = (q_x * vx + q_y * vy) / (vx * vx + vy * vy);
                if t < 0.0 {
                    t = 0.0;
                } else if t > 1.0 {
                    t = 1.0;
                }

                // projected position on connection
                let lx = center_x + vx * t;
                let ly = center_y + vy * t;

                // determine total distance from query point to projected position
                let dx = x - lx;
                let dy = y - ly;
                let distance = dx * dx + dy * dy;
                if distance < lowest {
                    // found a better value
                    lowest = distance;
                }
            }
        }

        // convert the lowest value to the grayscale value
        if lowest != self.thickness_squared {
            let distance = f64::sqrt(lowest);
            let d = distance / thickness;
            d * 2.0 - 1.0
        } else {
            1.0
        }
    }
}

impl NoiseFunction3d for MeshNoise {
    fn get(&self, _seed: i32, _offset_seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
        // Java: throw new UnsupportedOperationException("3d not supported");
        // this should be easy to support. follow the pattern of the 2d version
        // and expand the switch statement to all 27 cases
        // contact slikey if help is needed
        panic!("3d not supported");
    }
}
