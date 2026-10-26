//! Java `SearchDirections` tables and a block-grid A*.
//!
//! The direction tables are byte-for-byte ports of the Java record (same
//! generation order, distances, inverse pairs and crossing-step normals) and
//! are gated by `testdata/golden/server/npc/npc.json`.
//!
//! The planner is the block-grid subset of `AStarBase`/`AStarWithTarget`: it
//! searches the same 6/26-neighbour tables with Euclidean step and heuristic
//! costs, enforces Java's path/open/total node limits, and returns the closest
//! reached node when the exact goal is unreachable. Java's fractional
//! half-block nodes, `MotionController.probeMove` reachability and
//! `ProbeMoveData` remain later T58 work.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use hytale_world::collision::Vec3;

/// One block position in the navigation grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockPos {
    /// Block x.
    pub x: i32,
    /// Block y.
    pub y: i32,
    /// Block z.
    pub z: i32,
}

impl BlockPos {
    /// Construct a block position.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The block centre as a world position.
    pub fn center(self) -> Vec3 {
        Vec3::new(
            f64::from(self.x) + 0.5,
            f64::from(self.y),
            f64::from(self.z) + 0.5,
        )
    }

    /// Add a direction step.
    pub fn offset(self, delta: Vec3) -> Self {
        Self {
            x: self.x + delta.x as i32,
            y: self.y + delta.y as i32,
            z: self.z + delta.z as i32,
        }
    }

    /// Euclidean distance in block units.
    pub fn distance(self, other: Self) -> f64 {
        let dx = f64::from(self.x - other.x);
        let dy = f64::from(self.y - other.y);
        let dz = f64::from(self.z - other.z);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// One neighbour step from the Java direction table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchDirection {
    /// Step vector (each component `-1`, `0` or `1`).
    pub delta: Vec3,
    /// Euclidean step length.
    pub distance: f64,
    /// Index of the inverse step.
    pub inverse: usize,
    /// Index of the crossing-step normal, when the 2D diagonal pruning uses it.
    pub normal: Option<usize>,
}

/// Java `SearchDirections.create`.
pub fn search_directions(
    is_2d: bool,
    can_move_diagonal: bool,
    projected: [bool; 3],
) -> Vec<SearchDirection> {
    let mut deltas = Vec::new();
    for x in -1i32..=1 {
        if projected[0] && x != 0 {
            continue;
        }
        for y in -1i32..=1 {
            if projected[1] && y != 0 {
                continue;
            }
            for z in -1i32..=1 {
                if (projected[2] && z != 0) || (x == 0 && y == 0 && z == 0) {
                    continue;
                }
                if !can_move_diagonal && x.abs() + y.abs() + z.abs() != 1 {
                    continue;
                }
                deltas.push(Vec3::new(f64::from(x), f64::from(y), f64::from(z)));
            }
        }
    }

    let mut directions = Vec::with_capacity(deltas.len());
    for delta in &deltas {
        let inverse = deltas
            .iter()
            .position(|candidate| {
                candidate.x == -delta.x && candidate.y == -delta.y && candidate.z == -delta.z
            })
            .expect("every Java search direction has an inverse");
        directions.push(SearchDirection {
            delta: *delta,
            distance: delta.length_squared().sqrt(),
            inverse,
            normal: None,
        });
    }

    if is_2d && can_move_diagonal {
        for (index, direction) in directions.iter_mut().enumerate() {
            let normal = deltas
                .iter()
                .enumerate()
                .find(|(other_index, other)| {
                    *other_index != index
                        && direction.delta.x * other.x
                            + direction.delta.y * other.y
                            + direction.delta.z * other.z
                            == 0.0
                })
                .map(|(other_index, _)| other_index);
            direction.normal = normal;
        }
    }
    directions
}

/// Java `AStarBase` search limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AStarConfig {
    /// Maximum path length in nodes.
    pub max_path_length: usize,
    /// Maximum open-list size (`0` disables the limit).
    pub open_nodes_limit: usize,
    /// Maximum visited positions (`0` disables the limit).
    pub total_nodes_limit: usize,
    /// Whether diagonal steps are allowed.
    pub can_move_diagonal: bool,
    /// Whether the search treats the projected axes as 2D.
    pub is_2d: bool,
    /// Projected axes (`x`, `y`, `z`); a projected axis carries no step.
    pub projected: [bool; 3],
    /// Reject diagonal steps that cut through a blocked cardinal corner.
    pub avoid_corner_cutting: bool,
}

impl Default for AStarConfig {
    fn default() -> Self {
        Self {
            max_path_length: 200,
            open_nodes_limit: 80,
            total_nodes_limit: 400,
            can_move_diagonal: true,
            is_2d: false,
            projected: [false, false, false],
            avoid_corner_cutting: true,
        }
    }
}

impl AStarConfig {
    /// A 2D walk over the x/z plane.
    pub fn walk() -> Self {
        Self {
            is_2d: true,
            projected: [false, true, false],
            ..Self::default()
        }
    }

    /// A 3D fly search.
    pub fn fly() -> Self {
        Self {
            is_2d: false,
            projected: [false, false, false],
            ..Self::default()
        }
    }
}

/// A completed block-grid path.
#[derive(Debug, Clone, PartialEq)]
pub struct AStarPath {
    /// Start to end positions, inclusive.
    pub waypoints: Vec<BlockPos>,
    /// Travel cost.
    pub cost: f64,
    /// Expanded nodes.
    pub iterations: usize,
    /// Whether the goal itself was reached (false for a closest-node path).
    pub complete: bool,
}

impl AStarPath {
    /// Last reached position.
    pub fn end(&self) -> Option<BlockPos> {
        self.waypoints.last().copied()
    }

    /// Block centres for [`PathFollower::set_path`](crate::path::PathFollower::set_path).
    pub fn waypoint_positions(&self) -> Vec<Vec3> {
        self.waypoints
            .iter()
            .map(|position| position.center())
            .collect()
    }
}

/// The block-grid A* planner.
#[derive(Debug, Clone)]
pub struct AStarPlanner {
    config: AStarConfig,
    directions: Vec<SearchDirection>,
}

impl AStarPlanner {
    /// Build a planner from a configuration.
    pub fn new(config: AStarConfig) -> Self {
        let directions =
            search_directions(config.is_2d, config.can_move_diagonal, config.projected);
        Self { config, directions }
    }

    /// The direction table this planner expands.
    pub fn directions(&self) -> &[SearchDirection] {
        &self.directions
    }

    /// Search for an exact path. `None` when the goal is unreachable within
    /// the configured limits.
    pub fn find_path<F>(&self, start: BlockPos, goal: BlockPos, walkable: F) -> Option<AStarPath>
    where
        F: Fn(BlockPos) -> bool,
    {
        self.search(start, goal, &walkable, false)
    }

    /// Search and return the closest reached node when the goal is
    /// unreachable (Java `AStarWithTarget.findClosestPath`).
    pub fn find_path_closest<F>(
        &self,
        start: BlockPos,
        goal: BlockPos,
        walkable: F,
    ) -> Option<AStarPath>
    where
        F: Fn(BlockPos) -> bool,
    {
        self.search(start, goal, &walkable, true)
    }

    fn search<F>(
        &self,
        start: BlockPos,
        goal: BlockPos,
        walkable: &F,
        allow_closest: bool,
    ) -> Option<AStarPath>
    where
        F: Fn(BlockPos) -> bool,
    {
        if start == goal {
            return Some(AStarPath {
                waypoints: vec![start],
                cost: 0.0,
                iterations: 0,
                complete: true,
            });
        }

        let mut open = BinaryHeap::new();
        let mut costs: HashMap<BlockPos, f64> = HashMap::new();
        let mut came_from: HashMap<BlockPos, BlockPos> = HashMap::new();
        let mut lengths: HashMap<BlockPos, usize> = HashMap::new();
        let mut closed: HashMap<BlockPos, f64> = HashMap::new();
        let mut best_closest: Option<(BlockPos, f64)> = None;

        costs.insert(start, 0.0);
        lengths.insert(start, 1);
        open.push(QueueNode {
            position: start,
            estimate: start.distance(goal),
            cost: 0.0,
        });

        let mut iterations = 0usize;
        while let Some(current) = open.pop() {
            if closed
                .get(&current.position)
                .is_some_and(|known| *known <= current.cost)
            {
                continue;
            }
            let current_cost = costs
                .get(&current.position)
                .copied()
                .unwrap_or(current.cost);
            closed.insert(current.position, current_cost);
            iterations += 1;

            if current.position == goal {
                return Some(self.build_path(
                    start,
                    goal,
                    current_cost,
                    iterations,
                    true,
                    &came_from,
                ));
            }
            let heuristic = current.position.distance(goal);
            if best_closest.is_none_or(|(_, best)| heuristic < best) {
                best_closest = Some((current.position, heuristic));
            }

            let length = lengths.get(&current.position).copied().unwrap_or(1);
            if length < self.config.max_path_length {
                for (direction_index, direction) in self.directions.iter().enumerate() {
                    let _ = direction_index;
                    let next = current.position.offset(direction.delta);
                    if closed.contains_key(&next) {
                        continue;
                    }
                    if !walkable(next) {
                        continue;
                    }
                    if self.config.avoid_corner_cutting
                        && direction.delta.x != 0.0
                        && direction.delta.z != 0.0
                    {
                        let x_step = BlockPos::new(
                            current.position.x + direction.delta.x as i32,
                            current.position.y,
                            current.position.z,
                        );
                        let z_step = BlockPos::new(
                            current.position.x,
                            current.position.y,
                            current.position.z + direction.delta.z as i32,
                        );
                        if !walkable(x_step) || !walkable(z_step) {
                            continue;
                        }
                    }

                    let next_cost = current_cost + direction.distance;
                    if costs
                        .get(&next)
                        .is_some_and(|known| *known <= next_cost + f64::EPSILON)
                    {
                        continue;
                    }
                    costs.insert(next, next_cost);
                    lengths.insert(next, length + 1);
                    came_from.insert(next, current.position);
                    open.push(QueueNode {
                        position: next,
                        estimate: next.distance(goal),
                        cost: next_cost,
                    });
                }
            }

            if self.config.open_nodes_limit > 0 && open.len() >= self.config.open_nodes_limit {
                break;
            }
            if self.config.total_nodes_limit > 0 && closed.len() >= self.config.total_nodes_limit {
                break;
            }
        }

        if allow_closest {
            let (best, _) = best_closest?;
            let cost = costs.get(&best).copied().unwrap_or(0.0);
            return Some(self.build_path(start, best, cost, iterations, false, &came_from));
        }
        None
    }

    fn build_path(
        &self,
        start: BlockPos,
        end: BlockPos,
        cost: f64,
        iterations: usize,
        complete: bool,
        came_from: &HashMap<BlockPos, BlockPos>,
    ) -> AStarPath {
        let mut waypoints = vec![end];
        let mut current = end;
        let mut guard = 0usize;
        while current != start {
            let Some(previous) = came_from.get(&current).copied() else {
                break;
            };
            waypoints.push(previous);
            current = previous;
            guard += 1;
            if guard > self.config.max_path_length.max(1) * 2 {
                waypoints.truncate(1);
                break;
            }
        }
        waypoints.reverse();
        AStarPath {
            waypoints,
            cost,
            iterations,
            complete,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct QueueNode {
    position: BlockPos,
    estimate: f64,
    cost: f64,
}

impl QueueNode {
    fn total(self) -> f64 {
        self.cost + self.estimate
    }
}

impl PartialEq for QueueNode {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position && self.total() == other.total()
    }
}

impl Eq for QueueNode {}

impl PartialOrd for QueueNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` is a max-heap; invert so the lowest total pops first.
        other
            .total()
            .total_cmp(&self.total())
            .then_with(|| other.position.cmp(&self.position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn grid(blocked: &[(i32, i32)]) -> HashSet<(i32, i32)> {
        blocked.iter().copied().collect()
    }

    #[test]
    fn diagonal_steps_cost_more_than_axis_steps() {
        let directions = search_directions(true, true, [false, true, false]);
        assert_eq!(directions.len(), 8);
        for direction in &directions {
            let axis = direction.delta.x == 0.0 || direction.delta.z == 0.0;
            if axis {
                assert!((direction.distance - 1.0).abs() < 1e-12);
            } else {
                assert!((direction.distance - 2.0f64.sqrt()).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn the_walk_planner_routes_around_a_wall() {
        let blocked = grid(&[(2, 0), (2, 1), (2, 2), (2, 3), (2, 4), (2, 5)]);
        let planner = AStarPlanner::new(AStarConfig::walk());
        let path = planner
            .find_path(BlockPos::new(0, 0, 0), BlockPos::new(4, 0, 0), |position| {
                !blocked.contains(&(position.x, position.z))
            })
            .expect("a path around the wall");
        assert!(path.complete);
        assert_eq!(path.waypoints.first(), Some(&BlockPos::new(0, 0, 0)));
        assert_eq!(path.waypoints.last(), Some(&BlockPos::new(4, 0, 0)));
        assert!(path
            .waypoints
            .iter()
            .all(|position| { !blocked.contains(&(position.x, position.z)) }));
        assert!(
            path.cost > 4.0,
            "the detour must cost more than the straight line"
        );
    }

    #[test]
    fn an_unreachable_goal_returns_the_closest_node() {
        let planner = AStarPlanner::new(AStarConfig::walk());
        let path = planner
            .find_path_closest(
                BlockPos::new(0, 0, 0),
                BlockPos::new(10, 0, 0),
                |position| position.x < 3,
            )
            .expect("closest path");
        assert!(!path.complete);
        assert_eq!(path.end(), Some(BlockPos::new(2, 0, 0)));
        assert!(planner
            .find_path(
                BlockPos::new(0, 0, 0),
                BlockPos::new(10, 0, 0),
                |position| { position.x < 3 }
            )
            .is_none());
    }

    #[test]
    fn the_fly_planner_uses_all_three_axes() {
        let planner = AStarPlanner::new(AStarConfig::fly());
        let path = planner
            .find_path(BlockPos::new(0, 0, 0), BlockPos::new(2, 3, -1), |_| true)
            .expect("3D path");
        assert!(path.complete);
        assert_eq!(path.waypoints.last(), Some(&BlockPos::new(2, 3, -1)));
    }
}
