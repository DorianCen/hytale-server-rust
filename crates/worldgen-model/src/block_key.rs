// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/BlockPattern.java`
// (`BlockPattern.BlockEntry.decode` / `encode`),
// `.hytale-source/.../CoreServer/.../asset/type/blocktype/config/Rotation.java`,
// `.hytale-source/.../CoreServer/.../asset/type/blocktype/config/RotationTuple.java`
// and `.hytale-source/.../CoreServer/.../util/FillerBlockUtil.java`.
//! Block-key decoding — the `"Name|Yaw=...|Pitch=...|Roll=...|Filler=x,y,z"`
//! syntax the world JSONs use to reference a block type plus its rotation and
//! filler offset.
//!
//! Java's `BlockPattern.BlockEntry.decode` is the single entry point the
//! loaders call; the numeric id is then looked up through
//! `BlockType.getAssetMap().getIndex(blockTypeKey)` (case-insensitive). The
//! Rust port splits that into [`BlockEntry::decode`] (pure string parsing) and
//! [`decode_block_key`] (the registry lookup).
//!
//! ## Separators
//!
//! `BlockPattern.parse` normalizes the alternate block separator `;` to `,`
//! **before** splitting a pattern into block arguments. `BlockEntry.decode`
//! itself does *not*: it always uses `,` for the filler triplet and `|` for
//! the metadata separator. The Rust port transcribes `decode` only, so a
//! `;`-separated filler is rejected exactly like Java.
//!
//! ## `@Nullable` rotation metadata
//!
//! `BlockEntry.decode` starts with all three rotations `None`; absent
//! `|Yaw=` / `|Pitch=` / `|Roll=` keys therefore leave that axis at zero
//! degrees, giving rotation index `0`.

use std::fmt;

use crate::registry::Registries;

/// Java `Rotation` — the four 90° rotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rotation {
    /// Java `Rotation.None` = `0` degrees.
    None = 0,
    /// Java `Rotation.Ninety` = `90` degrees.
    Ninety = 1,
    /// Java `Rotation.OneEighty` = `180` degrees.
    OneEighty = 2,
    /// Java `Rotation.TwoSeventy` = `270` degrees.
    TwoSeventy = 3,
}

impl Rotation {
    /// Java `Rotation.VALUES`.
    pub const VALUES: [Rotation; 4] = [
        Rotation::None,
        Rotation::Ninety,
        Rotation::OneEighty,
        Rotation::TwoSeventy,
    ];

    /// Java `Rotation.getDegrees()`.
    #[inline]
    pub const fn get_degrees(self) -> i32 {
        self as i32 * 90
    }

    /// Java `Rotation.ofDegrees(int)`.
    ///
    /// ```java
    /// degrees = Math.floorMod(degrees, 360);
    /// return switch (degrees) { ... default -> throw new IllegalArgumentException(...); };
    /// ```
    #[inline]
    pub fn of_degrees(degrees: i32) -> Rotation {
        let normalized = degrees.rem_euclid(360);
        match normalized {
            0 => Rotation::None,
            90 => Rotation::Ninety,
            180 => Rotation::OneEighty,
            270 => Rotation::TwoSeventy,
            _ => panic!("Rotation degrees value {normalized} cannot be mapped to type Rotation"),
        }
    }
}

/// Java `RotationTuple(int index, Rotation yaw, Rotation pitch, Rotation roll)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RotationTuple {
    /// Java `int index` — `roll * 16 + pitch * 4 + yaw`.
    pub index: i32,
    /// Java `Rotation yaw`.
    pub yaw: Rotation,
    /// Java `Rotation pitch`.
    pub pitch: Rotation,
    /// Java `Rotation roll`.
    pub roll: Rotation,
}

impl RotationTuple {
    /// Java `RotationTuple.NONE`.
    pub const NONE: RotationTuple = RotationTuple {
        index: RotationTuple::NONE_INDEX,
        yaw: Rotation::None,
        pitch: Rotation::None,
        roll: Rotation::None,
    };

    /// Java `RotationTuple.NONE_INDEX` = `0`.
    pub const NONE_INDEX: i32 = 0;

    /// Java `RotationTuple.index(Rotation yaw, Rotation pitch, Rotation roll)`.
    #[inline]
    pub const fn index(yaw: Rotation, pitch: Rotation, roll: Rotation) -> i32 {
        (roll as i32) * 16 + (pitch as i32) * 4 + (yaw as i32)
    }

    /// Java `RotationTuple.of(Rotation yaw, Rotation pitch, Rotation roll)`.
    #[inline]
    pub const fn of(yaw: Rotation, pitch: Rotation, roll: Rotation) -> RotationTuple {
        RotationTuple {
            index: Self::index(yaw, pitch, roll),
            yaw,
            pitch,
            roll,
        }
    }

    /// Java `RotationTuple.get(int index)` — the lookup table built in the
    /// static initializer.
    #[inline]
    pub fn get(index: i32) -> RotationTuple {
        let yaw = Rotation::VALUES[(index % 4) as usize];
        let pitch = Rotation::VALUES[((index / 4) % 4) as usize];
        let roll = Rotation::VALUES[((index / 16) % 4) as usize];
        RotationTuple {
            index,
            yaw,
            pitch,
            roll,
        }
    }
}

/// Java `FillerBlockUtil` — the packed `(x, y, z)` filler offset.
///
/// Only the packing helpers are ported (the chunk-writing helpers belong to
/// the world storage layer, not the generator). The layout is
/// `x | (z << 5) | (y << 10)`, five signed bits per axis.
pub struct FillerBlockUtil;

impl FillerBlockUtil {
    /// Java `FillerBlockUtil.NO_FILLER` = `0`.
    pub const NO_FILLER: i32 = 0;

    /// Java `BITS_PER_AXIS` = `5`.
    pub const BITS_PER_AXIS: u32 = 5;

    /// Java `MASK` = `(1 << BITS_PER_AXIS) - 1`.
    pub const MASK: i32 = (1 << Self::BITS_PER_AXIS) - 1;

    /// Java `INVERT` = `(~0) << BITS_PER_AXIS` — the sign-extension bits.
    pub const INVERT: i32 = (!0) << Self::BITS_PER_AXIS;

    /// Java `FillerBlockUtil.pack(int x, int y, int z)`.
    #[inline]
    pub const fn pack(x: i32, y: i32, z: i32) -> i32 {
        ((x & Self::MASK) | ((z & Self::MASK) << Self::BITS_PER_AXIS))
            | ((y & Self::MASK) << (Self::BITS_PER_AXIS * 2))
    }

    /// Java `FillerBlockUtil.unpackX(int val)`.
    #[inline]
    pub const fn unpack_x(val: i32) -> i32 {
        let result = val & Self::MASK;
        if result & (1 << (Self::BITS_PER_AXIS - 1)) != 0 {
            result | Self::INVERT
        } else {
            result
        }
    }

    /// Java `FillerBlockUtil.unpackY(int val)`.
    #[inline]
    pub const fn unpack_y(val: i32) -> i32 {
        let result = (val >> (Self::BITS_PER_AXIS * 2)) & Self::MASK;
        if result & (1 << (Self::BITS_PER_AXIS - 1)) != 0 {
            result | Self::INVERT
        } else {
            result
        }
    }

    /// Java `FillerBlockUtil.unpackZ(int val)`.
    #[inline]
    pub const fn unpack_z(val: i32) -> i32 {
        let result = (val >> Self::BITS_PER_AXIS) & Self::MASK;
        if result & (1 << (Self::BITS_PER_AXIS - 1)) != 0 {
            result | Self::INVERT
        } else {
            result
        }
    }
}

/// Java `BlockPattern.BlockEntry(String blockTypeKey, int rotation, int filler)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockEntry {
    /// Java `blockTypeKey()` — the block name (before the first `|`).
    pub block_type_key: String,
    /// Java `rotation()` — the [`RotationTuple::index`] value.
    pub rotation: i32,
    /// Java `filler()` — the [`FillerBlockUtil::pack`] value.
    pub filler: i32,
}

impl BlockEntry {
    /// Java `new BlockEntry(...)`.
    pub fn new(block_type_key: String, rotation: i32, filler: i32) -> Self {
        Self {
            block_type_key,
            rotation,
            filler,
        }
    }

    /// Java `BlockPattern.BlockEntry.decode(String key)`.
    ///
    /// The error strings are verbatim Java:
    ///
    /// * `Invalid filler metadata! Missing comma`
    /// * `Invalid filler metadata! Missing second comma`
    /// * `Rotation degrees value {n} cannot be mapped to type Rotation`
    /// * `NumberFormatException: For input string: "{value}"` (from
    ///   `Integer.parseInt`)
    pub fn decode(key: &str) -> BlockEntry {
        let mut filler = FillerBlockUtil::NO_FILLER;
        if let Some(filler_at) = key.find("|Filler=") {
            let start = filler_at + "|Filler=".len();
            let first_comma = key[start..]
                .find(',')
                .map(|offset| start + offset)
                .unwrap_or_else(|| panic!("Invalid filler metadata! Missing comma"));
            let second_comma = key[first_comma + 1..]
                .find(',')
                .map(|offset| first_comma + 1 + offset)
                .unwrap_or_else(|| panic!("Invalid filler metadata! Missing second comma"));

            // Java searches the OR separator from the *start* of the filler
            // value (not from the second comma); keep that quirk.
            let end = key[start..]
                .find('|')
                .map(|offset| start + offset)
                .unwrap_or(key.len());

            let filler_x = parse_int(&key[start..first_comma]);
            let filler_y = parse_int(&key[first_comma + 1..second_comma]);
            let filler_z = parse_int(&key[second_comma + 1..end]);

            filler = FillerBlockUtil::pack(filler_x, filler_y, filler_z);
        }

        let mut rotation_yaw = Rotation::None;
        let mut rotation_pitch = Rotation::None;
        let mut rotation_roll = Rotation::None;

        if let Some(yaw_at) = key.find("|Yaw=") {
            let start = yaw_at + "|Yaw=".len();
            let end = key[start..]
                .find('|')
                .map(|offset| start + offset)
                .unwrap_or(key.len());
            rotation_yaw = Rotation::of_degrees(parse_int(&key[start..end]));
        }
        if let Some(pitch_at) = key.find("|Pitch=") {
            let start = pitch_at + "|Pitch=".len();
            let end = key[start..]
                .find('|')
                .map(|offset| start + offset)
                .unwrap_or(key.len());
            rotation_pitch = Rotation::of_degrees(parse_int(&key[start..end]));
        }
        if let Some(roll_at) = key.find("|Roll=") {
            let start = roll_at + "|Roll=".len();
            let end = key[start..]
                .find('|')
                .map(|offset| start + offset)
                .unwrap_or(key.len());
            rotation_roll = Rotation::of_degrees(parse_int(&key[start..end]));
        }

        // Java: `indexOf(BlockMask.OR_MASK_SEPARATOR)` = the first `|`.
        let end = key.find('|').unwrap_or(key.len());
        let name = &key[..end];

        BlockEntry::new(
            name.to_string(),
            RotationTuple::index(rotation_yaw, rotation_pitch, rotation_roll),
            filler,
        )
    }
}

impl fmt::Display for BlockEntry {
    /// Java record `toString()` —
    /// `BlockEntry[blockTypeKey=..., rotation=..., filler=...]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BlockEntry[blockTypeKey={}, rotation={}, filler={}]",
            self.block_type_key, self.rotation, self.filler
        )
    }
}

/// Java `Integer.parseInt(CharSequence, int begin, int end, int radix)`.
fn parse_int(value: &str) -> i32 {
    value
        .parse::<i32>()
        .unwrap_or_else(|_| panic!("NumberFormatException: For input string: \"{value}\""))
}

/// Decodes a block key and resolves its block name through `registries`.
///
/// Java `BlockType.getAssetMap().getIndex(key.blockTypeKey())` returns
/// `AssetMapWithIndexes.NOT_FOUND` for an unknown name; the loaders then
/// throw `"Unknown key! " + key`. The Rust port cannot dynamically register
/// unknown assets (`getBlockIdOrUnknown`), so this helper returns `None` and
/// the caller raises the (verbatim) panic.
pub fn decode_block_key(registries: &Registries, key: &str) -> Option<(i32, i32, i32)> {
    let entry = BlockEntry::decode(key);
    let block = registries.blocks.index_of(&entry.block_type_key)?;
    Some((block, entry.rotation, entry.filler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_decodes_to_rotation_none_and_no_filler() {
        let entry = BlockEntry::decode("Rock_Stone");
        assert_eq!(entry.block_type_key, "Rock_Stone");
        assert_eq!(entry.rotation, RotationTuple::NONE_INDEX);
        assert_eq!(entry.filler, FillerBlockUtil::NO_FILLER);
        assert_eq!(
            entry.to_string(),
            "BlockEntry[blockTypeKey=Rock_Stone, rotation=0, filler=0]"
        );
    }

    #[test]
    fn rotation_axes_pack_into_the_rotation_index() {
        // Java `RotationTuple.index = roll * 16 + pitch * 4 + yaw`.
        assert_eq!(BlockEntry::decode("A|Yaw=90").rotation, 1);
        assert_eq!(BlockEntry::decode("A|Yaw=180").rotation, 2);
        assert_eq!(BlockEntry::decode("A|Yaw=270").rotation, 3);
        assert_eq!(BlockEntry::decode("A|Pitch=90").rotation, 4);
        assert_eq!(BlockEntry::decode("A|Roll=90").rotation, 16);
        assert_eq!(
            BlockEntry::decode("A|Yaw=90|Pitch=180|Roll=270").rotation,
            1 + 8 + 48
        );
        assert_eq!(BlockEntry::decode("A|Yaw=360").rotation, 0);
        assert_eq!(BlockEntry::decode("A|Yaw=-90").rotation, 3);
    }

    #[test]
    fn the_metadata_suffix_is_not_part_of_the_name() {
        let entry = BlockEntry::decode("Rock_Stone|Yaw=90|Filler=1,2,3");
        assert_eq!(entry.block_type_key, "Rock_Stone");
        assert_eq!(entry.rotation, 1);
        assert_eq!(entry.filler, FillerBlockUtil::pack(1, 2, 3));
    }

    #[test]
    fn filler_coordinates_round_trip_through_the_packing() {
        for x in -16..=15 {
            for y in -16..=15 {
                for z in -16..=15 {
                    let packed = FillerBlockUtil::pack(x, y, z);
                    assert_eq!(FillerBlockUtil::unpack_x(packed), x);
                    assert_eq!(FillerBlockUtil::unpack_y(packed), y);
                    assert_eq!(FillerBlockUtil::unpack_z(packed), z);
                }
            }
        }
        assert_eq!(FillerBlockUtil::NO_FILLER, FillerBlockUtil::pack(0, 0, 0));
    }

    #[test]
    fn filler_requires_both_commas() {
        let result = std::panic::catch_unwind(|| BlockEntry::decode("A|Filler=1"));
        let message = panic_message(result);
        assert_eq!(message, "Invalid filler metadata! Missing comma");

        let result = std::panic::catch_unwind(|| BlockEntry::decode("A|Filler=1,2"));
        let message = panic_message(result);
        assert_eq!(message, "Invalid filler metadata! Missing second comma");
    }

    #[test]
    fn invalid_rotation_degrees_panics_verbatim() {
        let result = std::panic::catch_unwind(|| BlockEntry::decode("A|Yaw=45"));
        let message = panic_message(result);
        assert_eq!(
            message,
            "Rotation degrees value 45 cannot be mapped to type Rotation"
        );
    }

    #[test]
    fn the_semicolon_separator_is_not_normalized() {
        // Java `BlockEntry.decode` does not call `BlockPattern.parse`'s
        // `;` → `,` normalization, so the name keeps the `;`.
        let entry = BlockEntry::decode("A;Yaw=90");
        assert_eq!(entry.block_type_key, "A;Yaw=90");
        assert_eq!(entry.rotation, 0);
    }

    #[test]
    fn the_registry_lookup_is_case_insensitive() {
        use crate::registry::{
            BlockRegistry, BlockTypeInfo, DrawType, EnvironmentRegistry, FluidRegistry, Registries,
        };

        let mut assets = vec![None; 8];
        assets[7] = Some(BlockTypeInfo::new(DrawType::Cube));
        let mut names = vec![None; 8];
        names[7] = Some("Rock_Stone".to_string());
        let registries = Registries::with_environments(
            BlockRegistry::with_names(assets, names),
            FluidRegistry::empty(),
            EnvironmentRegistry::empty(),
        );

        assert_eq!(decode_block_key(&registries, "rock_stone"), Some((7, 0, 0)));
        assert_eq!(
            decode_block_key(&registries, "ROCK_STONE|Yaw=90"),
            Some((7, 1, 0))
        );
        assert_eq!(decode_block_key(&registries, "missing"), None);
    }

    fn panic_message<T>(result: std::thread::Result<T>) -> String {
        match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string()),
        }
    }
}
