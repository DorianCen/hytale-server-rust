// Ported from: `.hytale-source/.../worldgen/climate/util/DistanceTransform.java`
//! Java `climate.util.DistanceTransform` — Dijkstra-style distance transform
//! from region boundary pixels, normalized by radius.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use hytale_math::math_util;

use super::maps::{DoubleMap, IntMap};

/// Java `DistanceTransform.DX`.
const DX: [i32; 8] = [-1, 1, 0, 0, -1, -1, 1, 1];
/// Java `DistanceTransform.DY`.
const DY: [i32; 8] = [0, 0, -1, 1, -1, 1, -1, 1];
/// Java `DistanceTransform.COST`.
const COST: [f64; 8] = [1.0, 1.0, 1.0, 1.0, std::f64::consts::SQRT_2, std::f64::consts::SQRT_2, std::f64::consts::SQRT_2, std::f64::consts::SQRT_2];

/// Java `DistanceTransform.apply(IntMap source, DoubleMap dest, double radius)`.
///
/// ⚠ NOTE on parity: Java uses a `PriorityQueue` ordered by distance. Ties
/// (equal distances) may pop in a different order than Rust's `BinaryHeap`,
/// but Dijkstra with strict relaxation (`distance < dist[..]`) yields the
/// same final distance map regardless of tie order — the output is
/// bit-identical.
pub fn apply(source: &IntMap, dest: &mut DoubleMap, radius: f64) {
    if radius <= 0.0 {
        panic!("radius must be > 0");
    }

    let width = source.width;
    let height = source.height;
    let size = (width * height) as usize;

    // Group pixel indices by their id/value and collect boundary pixels
    // (where different pixel values neighbor each other).
    let mut regions: HashMap<i32, Vec<usize>> = HashMap::new();
    let mut boundaries: HashMap<i32, Vec<usize>> = HashMap::new();

    for y in 0..height {
        for x in 0..width {
            let index = source.index(x, y);
            let value = source.at_index(index);
            regions.entry(value).or_default().push(index);

            // Only iterate cardinal neighbors for boundary detection.
            for i in 0..4 {
                let nx = x + DX[i];
                let ny = y + DY[i];
                if nx >= 0 && nx < width && ny >= 0 && ny < height {
                    let neighbor_index = source.index(nx, ny);
                    if source.at_index(neighbor_index) != value {
                        boundaries.entry(value).or_default().push(index);
                        break;
                    }
                }
            }
        }
    }

    let mut dist = vec![0.0f64; size];

    // Iterate over each region and run a Dijkstra-like distance transform
    // from the boundary pixels into the region.
    for (id, region) in &regions {
        let boundary = boundaries.get(id).map(Vec::as_slice).unwrap_or(&[]);

        // No in-bounds boundary => fill with max (1.0)
        if boundary.is_empty() {
            for &index in region {
                dest.set_index(index, 1.0);
            }
            continue;
        }

        dist.fill(radius);

        // min-heap ordered by distance (Java PriorityQueue with Node::sort)
        let mut queue = BinaryHeap::new();

        for &index in boundary {
            dist[index] = 0.0;
            queue.push(Reverse((OrderedF64(0.0), index)));
        }

        while let Some(Reverse((distance, index))) = queue.pop() {
            if distance.0 > dist[index] {
                continue;
            }

            let cx = index as i32 % width;
            let cy = index as i32 / width;

            for i in 0..DX.len() {
                let nx = cx + DX[i];
                let ny = cy + DY[i];
                if nx < 0 || nx >= width || ny < 0 || ny >= height {
                    continue;
                }

                let neighbor_index = source.index(nx, ny);
                let neighbor_id = source.at_index(neighbor_index);
                if neighbor_id != *id {
                    continue; // Stay inside same region
                }

                let distance = distance.0 + COST[i];
                if distance < dist[neighbor_index] {
                    dist[neighbor_index] = distance;
                    queue.push(Reverse((OrderedF64(distance), neighbor_index)));
                }
            }
        }

        // Clamp distances to [0.0, radius] and normalize to [0.0, 1.0]
        for &index in region {
            let value = math_util::clamp_f64(dist[index], 0.0, radius);
            dest.set_index(index, value / radius);
        }
    }
}

/// Wrapper giving `f64` a total order for the binary heap (NaNs never occur:
/// distances are sums of `COST` values).
#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedF64(f64);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_region_without_boundary_fills_one() {
        // Entire 4x4 map is region 1 → no boundary → all 1.0
        let source = IntMap::new(4, 4);
        // IntMap fills with -1; set everything to 1
        let mut source = source;
        for i in 0..16 {
            source.set_index(i, 1);
        }
        let mut dest = DoubleMap::new(4, 4);
        apply(&source, &mut dest, 10.0);
        for i in 0..16 {
            assert_eq!(dest.at_index(i), 1.0);
        }
    }

    #[test]
    fn two_regions_distance_from_boundary() {
        // 8x8: left half region 0, right half region 1
        let mut source = IntMap::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                source.set(x, y, if x < 4 { 0 } else { 1 });
            }
        }
        let mut dest = DoubleMap::new(8, 8);
        apply(&source, &mut dest, 4.0);

        // Region-1 pixels at x=4 are on the boundary → 0.0
        assert_eq!(dest.at(4, 0), 0.0);
        // x=5: distance 1 / radius 4 → 0.25
        assert!((dest.at(5, 0) - 0.25).abs() < 1e-12, "{}", dest.at(5, 0));
        // x=7: distance 3 / radius 4 → 0.75
        assert!((dest.at(7, 0) - 0.75).abs() < 1e-12, "{}", dest.at(7, 0));
        // Region-0 boundary at x=3 → 0.0, x=0 → 3/4
        assert_eq!(dest.at(3, 0), 0.0);
        assert!((dest.at(0, 0) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn clamps_at_radius() {
        let mut source = IntMap::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                source.set(x, y, if x < 4 { 0 } else { 1 });
            }
        }
        let mut dest = DoubleMap::new(8, 8);
        apply(&source, &mut dest, 1.5);
        // x=7: distance 3 clamped to 1.5 / 1.5 → 1.0
        assert_eq!(dest.at(7, 0), 1.0);
    }
}
