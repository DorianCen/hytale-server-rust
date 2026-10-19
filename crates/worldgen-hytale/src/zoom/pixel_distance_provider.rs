// Ported from: `.hytale-source/.../worldgen/zoom/PixelDistanceProvider.java`
//! Java `zoom.PixelDistanceProvider` — 8x8-cell spatial index answering
//! "distance to the nearest pixel of a different color".

use std::collections::HashSet;

use crate::zoom::pixel_provider::{arr_index, PixelProvider};

/// Java `private static final int TABLE_SIZE = 8`.
const TABLE_SIZE: i32 = 8;

/// Java private `IPixelSet` — modeled as an enum.
#[derive(Debug, Clone)]
enum PixelSet {
    /// Java `SinglePixelSet`.
    Single(i32),
    /// Java `MultiplePixelSet`.
    Multiple(HashSet<i32>),
}

impl PixelSet {
    fn contains(&self, pixel: i32) -> bool {
        match self {
            PixelSet::Single(p) => *p == pixel,
            PixelSet::Multiple(set) => set.contains(&pixel),
        }
    }

    fn size(&self) -> usize {
        match self {
            PixelSet::Single(_) => 1,
            PixelSet::Multiple(set) => set.len(),
        }
    }
}

/// Java `zoom.PixelDistanceProvider`.
pub struct PixelDistanceProvider {
    /// Java `protected final PixelProvider image`.
    pub image: PixelProvider,
    /// Java `protected final int width, height`.
    width: i32,
    height: i32,
    /// Java `protected final int cellsX, cellsY`.
    cells_x: i32,
    cells_y: i32,
    /// Java `protected final IPixelSet[] table`.
    table: Vec<PixelSet>,
    /// Java `protected final IntSet pixels` — every color present in the image.
    pixels: HashSet<i32>,
}

impl PixelDistanceProvider {
    /// Java `PixelDistanceProvider(PixelProvider)`.
    pub fn new(image: PixelProvider) -> Self {
        let width = image.width as i32;
        let height = image.height as i32;
        let cells_x = width / TABLE_SIZE;
        let cells_y = height / TABLE_SIZE;

        let mut provider = Self {
            image,
            width,
            height,
            cells_x,
            cells_y,
            table: vec![PixelSet::Single(0); (cells_x * cells_y) as usize],
            pixels: HashSet::new(),
        };
        provider.prepare_segment_table();
        provider
    }

    /// Java `getColors()`.
    pub fn get_colors(&self) -> &HashSet<i32> {
        &self.pixels
    }

    /// Java `distanceSqToDifferentPixel(double ox, double oy, int px, int py)`.
    pub fn distance_sq_to_different_pixel(&self, ox: f64, oy: f64, px: i32, py: i32) -> f64 {
        let px = self.clamp_x(px);
        let py = self.clamp_y(py);
        let color = self.image.get_pixel(px, py);
        let cell_x = px / TABLE_SIZE;
        let cell_y = py / TABLE_SIZE;
        let mut distance = f64::INFINITY;

        // iterate over this and neighboring cells
        let min_x = (cell_x - 1).max(0);
        let max_x = (cell_x + 2).min(self.cells_x);
        let min_y = (cell_y - 1).max(0);
        let max_y = (cell_y + 2).min(self.cells_y);
        for ix in min_x..max_x {
            for iy in min_y..max_y {
                let dist = self.distance_sq_to_diff_in_seq(ox, oy, color, ix, iy);
                if dist < distance {
                    distance = dist;
                }
            }
        }
        distance
    }

    /// Java `protected double distanceSqToDiffInSeq(...)`.
    fn distance_sq_to_diff_in_seq(&self, ox: f64, oy: f64, pixel: i32, cell_x: i32, cell_y: i32) -> f64 {
        let mut dist_sq = f64::INFINITY;
        if self.has_different_pixel(cell_x, cell_y, pixel) {
            let offset_x = cell_x * TABLE_SIZE;
            let offset_y = cell_y * TABLE_SIZE;
            for ix in 0..TABLE_SIZE {
                let px = ix + offset_x;
                for iy in 0..TABLE_SIZE {
                    let py = iy + offset_y;
                    if pixel != self.image.get_pixel(px, py) {
                        let dist = super::pixel_distance_provider::distance_sq_to_pixel(ox, oy, px, py);
                        if dist < dist_sq {
                            dist_sq = dist;
                        }
                    }
                }
            }
        }
        dist_sq
    }

    /// Java `protected boolean hasDifferentPixel(...)`.
    fn has_different_pixel(&self, cell_x: i32, cell_y: i32, pixel: i32) -> bool {
        let pixel_set = &self.table[self.cell_index(cell_x, cell_y) as usize];
        !pixel_set.contains(pixel) || pixel_set.size() > 1
    }

    /// Java `private void prepareSegmentTable()`.
    fn prepare_segment_table(&mut self) {
        for cell_x in 0..self.cells_x {
            for cell_y in 0..self.cells_y {
                let mut colors: HashSet<i32> = HashSet::new();

                let offset_x = cell_x * TABLE_SIZE;
                let offset_y = cell_y * TABLE_SIZE;
                for ix in 0..TABLE_SIZE {
                    for iy in 0..TABLE_SIZE {
                        let x = ix + offset_x;
                        let y = iy + offset_y;
                        if x >= self.width || y >= self.height {
                            continue;
                        }
                        colors.insert(self.image.pixels[arr_index(x, y, self.height as usize)]);
                    }
                }

                self.pixels.extend(&colors);

                let set = if colors.len() == 1 {
                    PixelSet::Single(*colors.iter().next().unwrap())
                } else {
                    PixelSet::Multiple(colors)
                };
                let index = self.cell_index(cell_x, cell_y) as usize;
                self.table[index] = set;
            }
        }
    }

    /// Java `protected int clampX(int x)`.
    fn clamp_x(&self, x: i32) -> i32 {
        if x < 0 {
            return 0;
        }
        if x >= self.width {
            return self.width - 1;
        }
        x
    }

    /// Java `protected int clampY(int y)`.
    fn clamp_y(&self, y: i32) -> i32 {
        if y < 0 {
            return 0;
        }
        if y >= self.height {
            return self.height - 1;
        }
        y
    }

    /// Java `protected int cellIndex(int cellX, int cellY)`.
    fn cell_index(&self, cell_x: i32, cell_y: i32) -> i32 {
        cell_x * self.cells_y + cell_y
    }
}

/// Java private static `distanceSqToPixel` — distance to the *unit square*
/// around the pixel (block distance, not center distance).
fn distance_sq_to_pixel(ox: f64, oy: f64, px: i32, py: i32) -> f64 {
    let dx = (px as f64 - ox).max(ox - px as f64 - 1.0).max(0.0);
    let dy = (py as f64 - oy).max(oy - py as f64 - 1.0).max(0.0);
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_image_has_infinite_distance() {
        // 64x64 solid image: no different pixel anywhere
        let provider = PixelDistanceProvider::new(PixelProvider::from_pixels(
            64,
            64,
            vec![0x112233; 64 * 64],
        ));
        let d = provider.distance_sq_to_different_pixel(10.0, 10.0, 10, 10);
        assert_eq!(d, f64::INFINITY);
    }

    #[test]
    fn finds_nearest_different_pixel() {
        // 64x64 all 0 except one pixel of 1 at (40, 40)
        let mut pixels = vec![0i32; 64 * 64];
        pixels[arr_index(40, 40, 64)] = 1;
        let provider = PixelDistanceProvider::new(PixelProvider::from_pixels(64, 64, pixels));

        // Query right next to the different pixel: block distance 1 → 1.0
        let d = provider.distance_sq_to_different_pixel(39.0, 40.0, 39, 40);
        assert_eq!(d, 1.0);

        // Far away: block distance (40-41-1 → clipped) ... pixel square at
        // (40,40): dx = max(40-50, 50-40-1, 0) = 9 → 81
        let d = provider.distance_sq_to_different_pixel(50.0, 40.0, 50, 40);
        assert_eq!(d, 81.0);
    }

    #[test]
    fn distance_sq_to_pixel_unit_square_semantics() {
        // Pixel unit square is [0..1); (0.5, 0.5) is inside → 0
        assert_eq!(distance_sq_to_pixel(0.5, 0.5, 0, 0), 0.0);
        // (-0.5, -0.5) is outside: dx = max(max(0-(-0.5), -0.5-0-1), 0) = 0.5
        assert_eq!(distance_sq_to_pixel(-0.5, -0.5, 0, 0), 0.5);
        // From (2,0): dx = max(0-2, 2-0-1, 0) = 1
        assert_eq!(distance_sq_to_pixel(2.0, 0.0, 0, 0), 1.0);
    }
}
