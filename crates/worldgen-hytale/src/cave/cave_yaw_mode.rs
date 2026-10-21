// Ported from: `.hytale-source/.../worldgen/cave/CaveYawMode.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveYawMode` — how the yaw
//! angle of a child cave node is derived from its parent.

use crate::prefab::prefab_rotation::PrefabRotation;

/// Java `enum CaveYawMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaveYawMode {
    /// Java `NODE` — returns the parent node's yaw, ignoring its
    /// [`PrefabRotation`].
    Node,
    /// Java `SUM` — returns the sum of the parent node's yaw and its
    /// [`PrefabRotation`] angle.
    Sum,
    /// Java `PREFAB` — returns the parent node's [`PrefabRotation`] angle,
    /// ignoring its yaw.
    Prefab,
}

impl CaveYawMode {
    /// Java `float combine(float parentYaw, PrefabRotation parentRotation)`.
    ///
    /// Combines the parent cave-node's yaw and rotation into a new angle for
    /// a child cave-node to inherit.
    pub fn combine(self, parent_yaw: f32, parent_rotation: Option<PrefabRotation>) -> f32 {
        match self {
            CaveYawMode::Node => parent_yaw,
            CaveYawMode::Sum => match parent_rotation {
                None => parent_yaw,
                Some(rotation) => parent_yaw + rotation.get_yaw(),
            },
            CaveYawMode::Prefab => match parent_rotation {
                None => parent_yaw,
                Some(rotation) => rotation.get_yaw(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_node_mode_ignores_the_rotation() {
        assert_eq!(CaveYawMode::Node.combine(1.5, None), 1.5);
        assert_eq!(
            CaveYawMode::Node.combine(1.5, Some(PrefabRotation::Rotation90)),
            1.5
        );
    }

    #[test]
    fn the_sum_mode_adds_the_rotation_yaw() {
        assert_eq!(CaveYawMode::Sum.combine(1.5, None), 1.5);
        assert_eq!(
            CaveYawMode::Sum.combine(1.5, Some(PrefabRotation::Rotation90)),
            1.5 + PrefabRotation::Rotation90.get_yaw()
        );
    }

    #[test]
    fn the_prefab_mode_returns_the_rotation_yaw() {
        assert_eq!(CaveYawMode::Prefab.combine(1.5, None), 1.5);
        assert_eq!(
            CaveYawMode::Prefab.combine(1.5, Some(PrefabRotation::Rotation180)),
            PrefabRotation::Rotation180.get_yaw()
        );
    }
}
