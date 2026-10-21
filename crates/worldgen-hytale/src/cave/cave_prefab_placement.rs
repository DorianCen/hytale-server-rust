// Ported from: `.hytale-source/.../worldgen/cave/CavePrefabPlacement.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CavePrefabPlacement` — where
//! inside a cave node a prefab is placed.

use crate::cave::element::cave_element::CaveElement;
use crate::cave::element::cave_node::CaveNode;
use crate::util::bounds::IWorldBounds;

/// Java `enum CavePrefabPlacement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CavePrefabPlacement {
    /// Java `CEILING` — the prefab sits on the ceiling of the cave node.
    Ceiling,
    /// Java `FLOOR` — the prefab sits on the floor of the cave node.
    Floor,
    /// Java `DEFAULT` — the prefab sits in the middle of the cave node.
    Default,
}

impl CavePrefabPlacement {
    /// Java `CavePrefabPlacement.NO_HEIGHT` — the "no placement" marker.
    pub const NO_HEIGHT: i32 = -1;

    /// Java `PrefabPlacementFunction getFunction()`.
    pub fn get_function(self) -> PrefabPlacementFunction {
        match self {
            CavePrefabPlacement::Ceiling => PrefabPlacementFunction::Ceiling,
            CavePrefabPlacement::Floor => PrefabPlacementFunction::Floor,
            CavePrefabPlacement::Default => PrefabPlacementFunction::Default,
        }
    }
}

/// Java `interface CavePrefabPlacement.PrefabPlacementFunction`.
///
/// The Java interface is a functional interface with three implementations
/// (one per enum constant); the Rust port models the three functions as an
/// enum so that the dispatch stays a plain match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrefabPlacementFunction {
    /// Java `CEILING` — `caveNode.getCeilingPosition(seed, x, z)`.
    Ceiling,
    /// Java `FLOOR` — `caveNode.getFloorPosition(seed, x, z)`.
    Floor,
    /// Java `DEFAULT` — `(int) caveNode.getBounds().fractionY(0.5)`.
    Default,
}

impl PrefabPlacementFunction {
    /// Java `int generate(int seed, double x, double z, CaveNode caveNode)`.
    pub fn generate(self, seed: i32, x: f64, z: f64, cave_node: &CaveNode) -> i32 {
        match self {
            PrefabPlacementFunction::Ceiling => cave_node.get_ceiling_position(seed, x, z),
            PrefabPlacementFunction::Floor => cave_node.get_floor_position(seed, x, z),
            PrefabPlacementFunction::Default => {
                // Java: `(int) caveNode.getBounds().fractionY(0.5)`.
                cave_node.get_bounds().fraction_y(0.5) as i32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_no_height_marker_is_minus_one() {
        assert_eq!(CavePrefabPlacement::NO_HEIGHT, -1);
    }

    #[test]
    fn the_placement_functions_map_to_the_enum_constants() {
        assert_eq!(
            CavePrefabPlacement::Ceiling.get_function(),
            PrefabPlacementFunction::Ceiling
        );
        assert_eq!(
            CavePrefabPlacement::Floor.get_function(),
            PrefabPlacementFunction::Floor
        );
        assert_eq!(
            CavePrefabPlacement::Default.get_function(),
            PrefabPlacementFunction::Default
        );
    }
}
