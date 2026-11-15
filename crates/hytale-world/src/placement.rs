//! Block-type placement rotation metadata.
//!
//! Java sends `RandomRotation`, `RotationYawPlacementOffset` and `FlipType` to
//! the client as part of `UpdateBlockTypes`. The reference server does not
//! recompute a player's placement rotation: the client sends the final
//! `BlockRotation` in the interaction chain and the server stores it. Rust's
//! interactive placement follows the same rule (`client_rotation`), while
//! server-authored placements (NPC interactions, admin/test paths) have no
//! client rotation and need the metadata applied here.
//!
//! `RandomRotation` is a client-renderer instruction and the reference server
//! never applies it. The Rust port parses every enum value but deliberately
//! does **not** invent a selection algorithm: without a release-client trace the
//! placement rotation stays unchanged (see the T76 blocker). The
//! `RotationYawPlacementOffset` field is ported directly from
//! `BlockType.getRotationYawPlacementOffset`.
//!
//! Ported from: `CoreServer/.../asset/type/blocktype/config/BlockType.java`,
//! `RotationYawPlacementOffset`, `RandomRotation`, `FlipType`.

use hytale_assets::AssetRegistry;
use serde_json::Value;

/// Java `RandomRotation` (protocol wire values 0..=4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RandomRotation {
    /// No random rotation.
    #[default]
    None,
    /// Random yaw, pitch and roll quarter-turns.
    YawPitchRollStep1,
    /// Random yaw quarter-turn.
    YawStep1,
    /// Random yaw quarter-turn plus independent X/Z (pitch/roll) flips.
    YawStep1XZ,
    /// Random yaw quarter-turn; the protocol only has 90-degree steps, so this
    /// is the same candidate set as [`Self::YawStep1`].
    YawStep90,
}

impl RandomRotation {
    /// Decode a `RandomRotation` asset value.
    pub fn from_name(value: Option<&str>) -> Self {
        match value {
            Some("YawPitchRollStep1") => Self::YawPitchRollStep1,
            Some("YawStep1") => Self::YawStep1,
            Some("YawStep1XZ") => Self::YawStep1XZ,
            Some("YawStep90") => Self::YawStep90,
            _ => Self::None,
        }
    }

    /// Decode the `RandomRotation` field of a merged block-type document.
    pub fn from_value(value: Option<&Value>) -> Self {
        Self::from_name(value.and_then(Value::as_str))
    }
}

/// Java `Rotation` used by `RotationYawPlacementOffset`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum YawPlacementOffset {
    /// No additional yaw.
    #[default]
    None,
    /// +90 degrees.
    Ninety,
    /// +180 degrees.
    OneEighty,
    /// +270 degrees.
    TwoSeventy,
}

impl YawPlacementOffset {
    /// Decode the `RotationYawPlacementOffset` field of a merged block type.
    pub fn from_name(value: Option<&str>) -> Self {
        match value {
            Some("Ninety") => Self::Ninety,
            Some("OneEighty") => Self::OneEighty,
            Some("TwoSeventy") => Self::TwoSeventy,
            _ => Self::None,
        }
    }

    /// Decode a merged block-type JSON field.
    pub fn from_value(value: Option<&Value>) -> Self {
        Self::from_name(value.and_then(Value::as_str))
    }

    /// Offset as a 0..=3 quarter-turn count.
    pub const fn steps(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Ninety => 1,
            Self::OneEighty => 2,
            Self::TwoSeventy => 3,
        }
    }
}

/// Java `BlockFlipType`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlipType {
    /// 45-degree symmetry in the default (left-handed) orientation.
    Orthogonal,
    /// 45-degree symmetry in the mirrored (right-handed) orientation.
    OrthogonalInverse,
    /// 90-degree rotational symmetry.
    #[default]
    Symmetric,
}

impl FlipType {
    /// Decode a merged block-type `FlipType`.
    pub fn from_name(value: Option<&str>) -> Self {
        match value {
            Some("Orthogonal") => Self::Orthogonal,
            Some("OrthogonalInverse") => Self::OrthogonalInverse,
            _ => Self::Symmetric,
        }
    }

    /// Decode a merged block-type `FlipType` JSON field.
    pub fn from_value(value: Option<&Value>) -> Self {
        Self::from_name(value.and_then(Value::as_str))
    }

    /// Java `BlockFlipType.flipYaw`: fake a mirror on `axis` by rotating yaw.
    ///
    /// `yaw` is a 0..=3 quarter-turn (`Rotation.ordinal()`), and `axis` is the
    /// mirror axis (`X` or `Z`; a `None` mirror returns the input unchanged).
    pub fn flip_yaw(self, yaw: i32, axis: MirrorAxis) -> i32 {
        let yaw = yaw.rem_euclid(4);
        let alignment = if yaw % 2 == 1 {
            MirrorAxis::X
        } else {
            MirrorAxis::Z
        };
        match axis {
            MirrorAxis::None => yaw,
            MirrorAxis::X => match self {
                Self::Orthogonal => {
                    let step = if yaw % 2 == 1 { 1 } else { -1 };
                    (yaw + step).rem_euclid(4)
                }
                Self::OrthogonalInverse => {
                    let step = if yaw % 2 == 1 { -1 } else { 1 };
                    (yaw + step).rem_euclid(4)
                }
                Self::Symmetric => {
                    if alignment == MirrorAxis::X {
                        (yaw + 2).rem_euclid(4)
                    } else {
                        yaw
                    }
                }
            },
            MirrorAxis::Z => match self {
                Self::Orthogonal => {
                    let step = if yaw % 2 == 1 { -1 } else { 1 };
                    (yaw + step).rem_euclid(4)
                }
                Self::OrthogonalInverse => {
                    let step = if yaw % 2 == 1 { 1 } else { -1 };
                    (yaw + step).rem_euclid(4)
                }
                Self::Symmetric => {
                    if alignment == MirrorAxis::Z {
                        (yaw + 2).rem_euclid(4)
                    } else {
                        yaw
                    }
                }
            },
        }
    }
}

/// Mirror axis used by `FlipType` and the custom-template pattern transforms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MirrorAxis {
    /// No mirror.
    #[default]
    None,
    /// Mirror across the X axis (`x -> -x`).
    X,
    /// Mirror across the Z axis (`z -> -z`).
    Z,
}

/// One block type's placement rotation policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlacementRotation {
    /// Java `RandomRotation`.
    pub random: RandomRotation,
    /// Java `RotationYawPlacementOffset`.
    pub yaw_offset: YawPlacementOffset,
    /// Java `FlipType`, used by connected-block mirrors.
    pub flip_type: FlipType,
}

impl PlacementRotation {
    /// Decode the placement fields from a merged block-type document.
    pub fn from_block_type(value: &Value) -> Self {
        Self {
            random: RandomRotation::from_value(value.get("RandomRotation")),
            yaw_offset: YawPlacementOffset::from_value(value.get("RotationYawPlacementOffset")),
            flip_type: FlipType::from_value(value.get("FlipType")),
        }
    }

    /// Apply this policy to a requested Java rotation tuple index.
    ///
    /// The selected `RandomRotation` is parsed metadata only: its client-side
    /// selection is blocked on a release-client trace, so this method leaves
    /// the requested yaw/pitch/roll unchanged and applies only the explicit
    /// `RotationYawPlacementOffset` to yaw.
    pub fn apply(self, requested: i32, _position: [i32; 3]) -> i32 {
        let requested = requested.rem_euclid(64);
        let yaw = (requested % 4 + self.yaw_offset.steps()).rem_euclid(4);
        let pitch = (requested / 4) % 4;
        let roll = (requested / 16) % 4;
        yaw + pitch * 4 + roll * 16
    }
}

/// Resolve and apply the placement rotation policy for one block type.
pub fn block_placement_rotation(
    assets: &AssetRegistry,
    block_id: i32,
    requested: i32,
    position: [i32; 3],
) -> i32 {
    assets
        .block_types
        .get_by_id(block_id)
        .map(|record| PlacementRotation::from_block_type(&record.value.json))
        .unwrap_or_default()
        .apply(requested, position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn random_rotation_values_parse_every_wire_name() {
        for (name, expected) in [
            ("None", RandomRotation::None),
            ("YawPitchRollStep1", RandomRotation::YawPitchRollStep1),
            ("YawStep1", RandomRotation::YawStep1),
            ("YawStep1XZ", RandomRotation::YawStep1XZ),
            ("YawStep90", RandomRotation::YawStep90),
        ] {
            assert_eq!(RandomRotation::from_name(Some(name)), expected);
            assert_eq!(RandomRotation::from_value(Some(&json!(name))), expected);
        }
        assert_eq!(
            RandomRotation::from_name(Some("bogus")),
            RandomRotation::None
        );
        assert_eq!(RandomRotation::from_name(None), RandomRotation::None);
    }

    #[test]
    fn yaw_placement_offset_values_parse_and_map_to_quarter_turns() {
        for (name, steps) in [
            ("None", 0),
            ("Ninety", 1),
            ("OneEighty", 2),
            ("TwoSeventy", 3),
        ] {
            assert_eq!(YawPlacementOffset::from_name(Some(name)).steps(), steps);
        }
        assert_eq!(YawPlacementOffset::from_name(None).steps(), 0);
    }

    #[test]
    fn flip_yaw_matches_java_for_each_flip_type_and_axis() {
        // Values derived from BlockFlipType.flipComponent for the two mirror
        // axes reachable from CustomConnectedBlockPattern.
        let cases = [
            (FlipType::Orthogonal, MirrorAxis::X, [3, 2, 1, 0]),
            (FlipType::Orthogonal, MirrorAxis::Z, [1, 0, 3, 2]),
            (FlipType::OrthogonalInverse, MirrorAxis::X, [1, 0, 3, 2]),
            (FlipType::OrthogonalInverse, MirrorAxis::Z, [3, 2, 1, 0]),
            (FlipType::Symmetric, MirrorAxis::X, [0, 3, 2, 1]),
            (FlipType::Symmetric, MirrorAxis::Z, [2, 1, 0, 3]),
        ];
        for (flip, axis, expected) in cases {
            for (yaw, expected) in expected.into_iter().enumerate() {
                assert_eq!(
                    flip.flip_yaw(yaw as i32, axis),
                    expected,
                    "{flip:?}/{axis:?} at yaw {yaw}"
                );
            }
        }
        assert_eq!(FlipType::Orthogonal.flip_yaw(3, MirrorAxis::None), 3);
    }

    #[test]
    fn server_authored_placement_applies_only_the_explicit_yaw_offset() {
        let block = json!({
            "RandomRotation": "YawStep1",
            "RotationYawPlacementOffset": "OneEighty"
        });
        let policy = PlacementRotation::from_block_type(&block);
        assert_eq!(policy.random, RandomRotation::YawStep1);
        assert_eq!(
            policy.apply(0, [4, 70, 9]),
            2,
            "the explicit offset applies"
        );
        assert_eq!(
            policy.apply(0, [4, 70, 9]),
            policy.apply(0, [5, 70, 9]),
            "RandomRotation is not guessed from the position"
        );

        let none = PlacementRotation::from_block_type(&json!({
            "RotationYawPlacementOffset": "TwoSeventy"
        }));
        assert_eq!(none.apply(1, [0, 0, 0]), 0, "yaw 1 + 3 wraps to 0");

        let arbitrary = 1 + 2 * 4 + 3 * 16;
        assert_eq!(
            PlacementRotation::from_block_type(&json!({
                "RandomRotation": "YawPitchRollStep1"
            }))
            .apply(arbitrary, [0, 0, 0]),
            arbitrary,
            "the blocked random mode leaves the requested tuple unchanged"
        );
    }
}
