//! Block-grid walkability over a runtime [`World`](hytale_world::world::World).
//!
//! The planner itself is asset-agnostic; callers provide the solid-block rule
//! (the server uses `BlockTypeAsset.material == Solid`). This adapter keeps the
//! grid conversion in one place and is covered by a real `World` scenario.

use hytale_world::world::World;

use crate::navigation::{AStarConfig, AStarPath, AStarPlanner, BlockPos, SearchDirection};

/// A loaded-world view that exposes walkable/flyable block positions.
#[derive(Debug)]
pub struct WorldGrid<'a, F>
where
    F: Fn(i32) -> bool,
{
    world: &'a World,
    is_solid: F,
}

impl<'a, F> WorldGrid<'a, F>
where
    F: Fn(i32) -> bool,
{
    /// Wrap a world with a block-id solidity rule.
    pub fn new(world: &'a World, is_solid: F) -> Self {
        Self { world, is_solid }
    }

    /// Whether the block at `position` is solid.
    pub fn is_solid(&self, position: BlockPos) -> bool {
        (self.is_solid)(self.world.get_block(position.x, position.y, position.z))
    }

    /// Whether every block in the column is loaded.
    pub fn is_loaded(&self, position: BlockPos) -> bool {
        self.world.chunk(position.x >> 5, position.z >> 5).is_some()
    }

    /// A 2D-walk position: feet and head air, floor solid.
    pub fn is_walkable(&self, position: BlockPos) -> bool {
        if !self.is_loaded(position) {
            return false;
        }
        !self.is_solid(position)
            && !self.is_solid(BlockPos::new(position.x, position.y + 1, position.z))
            && self.is_solid(BlockPos::new(position.x, position.y - 1, position.z))
    }

    /// A 3D-fly position: a single air block, no floor requirement.
    pub fn is_flyable(&self, position: BlockPos) -> bool {
        self.is_loaded(position) && !self.is_solid(position)
    }

    /// Plan a walk path over the loaded world.
    pub fn plan_walk_path(&self, start: BlockPos, goal: BlockPos) -> Option<AStarPath> {
        AStarPlanner::new(AStarConfig::walk())
            .find_path(start, goal, |position| self.is_walkable(position))
    }

    /// Plan a 3D fly path over the loaded world.
    pub fn plan_fly_path(&self, start: BlockPos, goal: BlockPos) -> Option<AStarPath> {
        AStarPlanner::new(AStarConfig::fly())
            .find_path(start, goal, |position| self.is_flyable(position))
    }

    /// The Java direction table for a walk search.
    pub fn walk_directions(&self) -> Vec<SearchDirection> {
        crate::navigation::search_directions(true, true, [false, true, false])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hytale_world::chunk::column::ChunkColumn;
    use hytale_world::world::{World, WorldStorage};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hytale-npc-worldgrid-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    fn world_with_wall() -> (World, PathBuf) {
        let dir = temp_dir("wall");
        let mut world = World::new(WorldStorage::new(dir.clone()));
        let mut column = ChunkColumn::new(0, 0);
        // Floor 0..6 x 0..6, and a wall at x=2 from z=0..4.
        for x in 0..=6 {
            for z in 0..=6 {
                column.set_block(x, 0, z, 1, 0, 0).unwrap();
            }
        }
        for z in 0..=4 {
            column.set_block(2, 1, z, 1, 0, 0).unwrap();
        }
        world.insert_chunk(column);
        (world, dir)
    }

    #[test]
    fn a_real_world_plan_goes_around_a_wall() {
        let (world, dir) = world_with_wall();
        let grid = WorldGrid::new(&world, |block| block != 0);
        let start = BlockPos::new(0, 1, 0);
        let goal = BlockPos::new(4, 1, 0);
        assert!(grid.is_walkable(start));
        assert!(!grid.is_walkable(BlockPos::new(2, 1, 0)));
        let path = grid.plan_walk_path(start, goal).expect("around the wall");
        assert_eq!(path.end(), Some(goal));
        assert!(path
            .waypoints
            .iter()
            .all(|position| grid.is_walkable(*position)));
        assert!(
            path.waypoints.iter().any(|position| position.z >= 5),
            "the path must leave the wall row"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_fly_plan_crosses_the_wall() {
        let (world, dir) = world_with_wall();
        let grid = WorldGrid::new(&world, |block| block != 0);
        let path = grid
            .plan_fly_path(BlockPos::new(0, 2, 0), BlockPos::new(4, 2, 0))
            .expect("fly across the wall");
        assert_eq!(path.end(), Some(BlockPos::new(4, 2, 0)));
        std::fs::remove_dir_all(dir).ok();
    }
}
