// Ported from: `.hytale-source/.../worldgen/cave/CaveBlockPriorityModifier.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveBlockPriorityModifier` —
//! the modifier that lets regular caves carve into prefab caves.

use worldgen_model::block_priority::{BlockPriorityModifier, CAVE, CAVE_COVER, PREFAB_CAVE};

/// Java `class CaveBlockPriorityModifier implements BlockPriorityModifier`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaveBlockPriorityModifier;

impl CaveBlockPriorityModifier {
    /// Java `public static final BlockPriorityModifier INSTANCE = new
    /// CaveBlockPriorityModifier()`.
    pub const INSTANCE: Self = Self;
}

impl BlockPriorityModifier for CaveBlockPriorityModifier {
    /// Java `byte modifyCurrent(byte current, byte target)`.
    fn modify_current(&self, current: i8, target: i8) -> i8 {
        // Allow regular caves to carve into prefab caves
        if current == PREFAB_CAVE && target == CAVE {
            return CAVE;
        }

        // Allow cave covers to place inside regular caves
        if current == CAVE && target == CAVE_COVER {
            return CAVE_COVER;
        }

        current
    }

    /// Java `byte modifyTarget(byte current, byte target)`.
    fn modify_target(&self, current: i8, target: i8) -> i8 {
        // Keep the higher prefab-cave priority when a regular cave intersects
        // it
        if current == PREFAB_CAVE && target == CAVE {
            return PREFAB_CAVE;
        }

        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regular_cave_carves_into_a_prefab_cave() {
        let modifier = CaveBlockPriorityModifier::INSTANCE;
        assert_eq!(modifier.modify_current(PREFAB_CAVE, CAVE), CAVE);
        assert_eq!(modifier.modify_target(PREFAB_CAVE, CAVE), PREFAB_CAVE);
    }

    #[test]
    fn a_cave_cover_places_inside_a_regular_cave() {
        let modifier = CaveBlockPriorityModifier::INSTANCE;
        assert_eq!(modifier.modify_current(CAVE, CAVE_COVER), CAVE_COVER);
        // The target stays the cover: only the current priority is modified.
        assert_eq!(modifier.modify_target(CAVE, CAVE_COVER), CAVE_COVER);
    }

    #[test]
    fn every_other_combination_is_left_alone() {
        let modifier = CaveBlockPriorityModifier::INSTANCE;
        assert_eq!(modifier.modify_current(CAVE, CAVE), CAVE);
        assert_eq!(
            modifier.modify_current(PREFAB_CAVE, CAVE_COVER),
            PREFAB_CAVE
        );
        assert_eq!(modifier.modify_current(CAVE_COVER, CAVE), CAVE_COVER);
        assert_eq!(modifier.modify_target(CAVE, CAVE), CAVE);
        assert_eq!(modifier.modify_target(CAVE_COVER, CAVE), CAVE);
    }
}
