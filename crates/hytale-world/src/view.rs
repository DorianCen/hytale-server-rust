//! Chunk view tracking for the world thread.
//!
//! A [`ChunkView`] is the square of chunks around a player/center. Planning a
//! view against the currently loaded set produces the chunks to load (ordered by
//! distance from the center) and the chunks to unload. Replacing the view
//! cancels every still-queued load that is no longer desired.

use std::collections::BTreeSet;

use crate::world::chunk_coordinates_around;

/// A square view of chunks around a center chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkView {
    center_x: i32,
    center_z: i32,
    radius: u32,
}

impl ChunkView {
    /// Create a view centered on a chunk coordinate.
    pub const fn new(center_x: i32, center_z: i32, radius: u32) -> Self {
        Self {
            center_x,
            center_z,
            radius,
        }
    }

    /// Center chunk x.
    pub const fn center_x(&self) -> i32 {
        self.center_x
    }

    /// Center chunk z.
    pub const fn center_z(&self) -> i32 {
        self.center_z
    }

    /// Chunk radius.
    pub const fn radius(&self) -> u32 {
        self.radius
    }

    /// Chunks inside the view, in stable Z-then-X order.
    pub fn desired(&self) -> Vec<(i32, i32)> {
        chunk_coordinates_around(self.center_x, self.center_z, self.radius)
    }

    /// Compare this view with the loaded chunk set.
    ///
    /// `loads` is ordered by distance from the center (then X/Z) so the world
    /// thread fills the nearest ring first. `unloads` is ordered by X/Z.
    pub fn plan(&self, loaded: impl IntoIterator<Item = (i32, i32)>) -> ChunkPlan {
        let desired: BTreeSet<(i32, i32)> = self.desired().into_iter().collect();
        let loaded: BTreeSet<(i32, i32)> = loaded.into_iter().collect();

        let mut loads: Vec<(i32, i32)> = desired.difference(&loaded).copied().collect();
        loads.sort_by_key(|&(x, z)| (self.distance_squared(x, z), x, z));
        let unloads: Vec<(i32, i32)> = loaded.difference(&desired).copied().collect();
        ChunkPlan { loads, unloads }
    }

    /// Squared chunk distance from the center.
    pub fn distance_squared(&self, x: i32, z: i32) -> i64 {
        let dx = i64::from(x) - i64::from(self.center_x);
        let dz = i64::from(z) - i64::from(self.center_z);
        dx * dx + dz * dz
    }
}

/// The work needed to move from the loaded set to a [`ChunkView`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkPlan {
    /// Chunks to load/generate, nearest first.
    pub loads: Vec<(i32, i32)>,
    /// Chunks outside the view, sorted by X/Z.
    pub unloads: Vec<(i32, i32)>,
}

impl ChunkPlan {
    /// True when nothing has to be loaded or unloaded.
    pub fn is_empty(&self) -> bool {
        self.loads.is_empty() && self.unloads.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_zero_is_one_chunk() {
        let view = ChunkView::new(4, -5, 0);
        assert_eq!(view.desired(), vec![(4, -5)]);
        let plan = view.plan([]);
        assert_eq!(plan.loads, vec![(4, -5)]);
        assert!(plan.unloads.is_empty());
    }

    #[test]
    fn loads_are_ordered_by_distance() {
        let view = ChunkView::new(0, 0, 1);
        let plan = view.plan([(0, 0)]);
        // The center is already loaded; the four edge-adjacent chunks (distance
        // 1) come before the corners (distance 2).
        assert_eq!(plan.loads.len(), 8);
        assert_eq!(plan.loads[0], (-1, 0));
        assert_eq!(plan.loads[1], (0, -1));
        assert_eq!(plan.loads[2], (0, 1));
        assert_eq!(plan.loads[3], (1, 0));
        assert_eq!(plan.loads[4], (-1, -1));
        assert!(plan.unloads.is_empty());
    }

    #[test]
    fn chunks_outside_the_view_are_unloaded() {
        let view = ChunkView::new(0, 0, 0);
        let plan = view.plan([(0, 0), (1, 0), (0, 1), (10, 10)]);
        assert!(plan.loads.is_empty());
        assert_eq!(plan.unloads, vec![(0, 1), (1, 0), (10, 10)]);
    }

    #[test]
    fn replacing_the_view_cancels_pending_loads() {
        let first = ChunkView::new(0, 0, 2).plan([]);
        assert_eq!(first.loads.len(), 25);

        // Only chunk (0,0) got loaded before the player moved; the new view is
        // radius 0 around (10, 10). None of the old pending loads survive.
        let second = ChunkView::new(10, 10, 0).plan([(0, 0)]);
        assert_eq!(second.loads, vec![(10, 10)]);
        assert_eq!(second.unloads, vec![(0, 0)]);
    }

    #[test]
    fn negative_centers_are_floor_correct() {
        let view = ChunkView::new(-1, -1, 1);
        let desired: BTreeSet<_> = view.desired().into_iter().collect();
        assert!(desired.contains(&(-2, -2)));
        assert!(desired.contains(&(0, 0)));
        assert!(desired.contains(&(-1, -1)));
    }
}
