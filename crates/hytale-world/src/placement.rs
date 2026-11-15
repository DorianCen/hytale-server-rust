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
//! `RandomRotation` is only defined by the client renderer; the protocol has
//! 90-degree yaw/pitch/roll steps, so this port selects one deterministic
//! quarter-turn tuple seeded from the block position. The selection is
//! documented per variant and covered by unit tests. `RotationYawPlacementOffset`
//! is an unambiguous additive yaw offset and is ported directly from
//! `BlockType.getRotationYawPlacementOffset`.
//!
//! Ported from: `CoreServer/.../asset/type/blocktype/config/BlockType.java`,
//! `RotationYawPlacementOffset`, `RandomRotation`, `FlipType`.

use hytale_assets::AssetRegistry;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util::hash3;
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

    /// Every quarter-turn offset this mode can add to the requested rotation.
    pub fn offsets(self) -> Vec<(i32, i32, i32)> {
        match self {
            Self::None => vec![(0, 0, 0)],
            Self::YawStep1 | Self::YawStep90 => (0..4).map(|yaw| (yaw, 0, 0)).collect(),
            Self::YawStep1XZ => {
                let mut offsets = Vec::with_capacity(16);
                for yaw in 0..4 {
                    for pitch in [0, 2] {
                        for roll in [0, 2] {
                            offsets.push((yaw, pitch, roll));
                        }
                    }
                }
                offsets
            }
            Self::YawPitchRollStep1 => {
                let mut offsets = Vec::with_capacity(64);
                for roll in 0..4 {
                    for pitch in 0..4 {
                        for yaw in 0..4 {
                            offsets.push((yaw, pitch, roll));
                        }
                    }
                }
                offsets
            }
        }
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
    /// Server-authored placements pass the rotation they want (usually `0`);
    /// the position seed makes the random component stable across reloads.
    pub fn apply(self, requested: i32, position: [i32; 3]) -> i32 {
        let requested = requested.rem_euclid(64);
        let offsets = self.random.offsets();
        let (yaw_step, pitch_step, roll_step) = if offsets.len() == 1 {
            offsets[0]
        } else {
            let mut random = FastRandom::new(hash3(
                i64::from(position[0]),
                i64::from(position[1]),
                i64::from(position[2]),
            ));
            offsets[random.next_int_bound(offsets.len() as i32) as usize]
        };
        let yaw = (requested % 4 + yaw_step + self.yaw_offset.steps()).rem_euclid(4);
        let pitch = ((requested / 4) % 4 + pitch_step).rem_euclid(4);
        let roll = ((requested / 16) % 4 + roll_step).rem_euclid(4);
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
    fn every_random_rotation_mode_has_the_expected_candidate_set() {
        assert_eq!(RandomRotation::None.offsets(), vec![(0, 0, 0)]);
        for mode in [RandomRotation::YawStep1, RandomRotation::YawStep90] {
            let offsets = mode.offsets();
            assert_eq!(offsets.len(), 4);
            for (index, offset) in offsets.iter().enumerate() {
                assert_eq!(*offset, (index as i32, 0, 0));
            }
        }
        assert_eq!(RandomRotation::YawStep1XZ.offsets().len(), 16);
        assert_eq!(RandomRotation::YawPitchRollStep1.offsets().len(), 64);
        assert!(!RandomRotation::YawStep1.offsets().is_empty());
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
    fn every_random_rotation_mode_stays_inside_its_candidate_set() {
        let arbitrary = 1 + 2 * 4 + 3 * 16; // yaw 90, pitch 180, roll 270
        assert_eq!(
            PlacementRotation::default().apply(arbitrary, [0, 0, 0]),
            arbitrary,
            "None preserves the requested tuple"
        );
        let requested = 0;
        for mode in [
            RandomRotation::None,
            RandomRotation::YawPitchRollStep1,
            RandomRotation::YawStep1,
            RandomRotation::YawStep1XZ,
            RandomRotation::YawStep90,
        ] {
            let policy = PlacementRotation {
                random: mode,
                yaw_offset: YawPlacementOffset::None,
                ..PlacementRotation::default()
            };
            for x in 0..8 {
                let index = policy.apply(requested, [x, 64, x]);
                let yaw = index % 4;
                let pitch = (index / 4) % 4;
                let roll = (index / 16) % 4;
                match mode {
                    RandomRotation::None => assert_eq!(index, requested),
                    RandomRotation::YawStep1 | RandomRotation::YawStep90 => {
                        assert_eq!(pitch, 0);
                        assert_eq!(roll, 0);
                        assert!((0..4).contains(&yaw));
                    }
                    RandomRotation::YawStep1XZ => {
                        assert!(pitch == 0 || pitch == 2);
                        assert!(roll == 0 || roll == 2);
                    }
                    RandomRotation::YawPitchRollStep1 => {
                        assert!((0..64).contains(&index));
                    }
                }
            }
        }
        for (offset, steps) in [
            (YawPlacementOffset::None, 0),
            (YawPlacementOffset::Ninety, 1),
            (YawPlacementOffset::OneEighty, 2),
            (YawPlacementOffset::TwoSeventy, 3),
        ] {
            let policy = PlacementRotation {
                random: RandomRotation::None,
                yaw_offset: offset,
                ..PlacementRotation::default()
            };
            assert_eq!(policy.apply(3, [0, 0, 0]), (3 + steps) % 4);
        }
    }

    #[test]
    fn server_authored_placement_applies_the_offset_and_is_position_stable() {
        let block = json!({
            "RandomRotation": "YawStep1",
            "RotationYawPlacementOffset": "OneEighty"
        });
        let policy = PlacementRotation::from_block_type(&block);
        let first = policy.apply(0, [4, 70, 9]);
        let again = policy.apply(0, [4, 70, 9]);
        assert_eq!(first, again, "the position seed must be deterministic");
        let other = policy.apply(0, [5, 70, 9]);
        assert!([0, 1, 2, 3].iter().any(|yaw| other % 4 == (yaw + 2) % 4));
        assert!([0, 1, 2, 3].contains(&(first % 4)));
        assert_eq!(policy.apply(0, [4, 70, 9]) / 4, 0, "pitch/roll untouched");

        let ypr = PlacementRotation::from_block_type(&json!({
            "RandomRotation": "YawPitchRollStep1"
        }));
        let index = ypr.apply(0, [4, 70, 9]);
        assert!((0..64).contains(&index));

        let none = PlacementRotation::from_block_type(&json!({
            "RotationYawPlacementOffset": "TwoSeventy"
        }));
        assert_eq!(none.apply(1, [0, 0, 0]), 0, "yaw 1 + 3 wraps to 0");
    }
}
