//! 32³ section palettes.
//!
//! Ported from `CoreServer/.../world/chunk/section/palette/` and
//! `Protocol/.../packets/world/PaletteType.java`.
//!
//! A section holds 32×32×32 = 32,768 block indexes. The palette maps dense
//! internal ids to external asset ids; the storage footprint depends on the
//! number of distinct ids:
//!
//! | Type | Internal id | Distinct ids | Block storage |
//! |---|---|---|---|
//! | `Empty` | — | 1 (air) | none |
//! | `HalfByte` | `u8` nibble | ≤ 16 | 16,384 bytes |
//! | `Byte` | `u8` | ≤ 256 | 32,768 bytes |
//! | `Short` | `u16` | ≤ 65,536 | 65,536 bytes |
//!
//! Two byte layouts exist:
//!
//! - **packet**: little-endian palette table, external ids are raw `i32`;
//! - **storage**: big-endian palette table, external ids go through a
//!   [`KeyCodec`] (byte for blocks, short for fillers, byte for rotations).
//!
//! The palette table is emitted in ascending internal-id order, which is the
//! deterministic order the Rust server owns.

use std::collections::{BTreeMap, BTreeSet};

use hytale_protocol::io::{ProtocolError, Reader, Writer};

/// Number of blocks in one section.
pub const BLOCK_COUNT: usize = 32 * 32 * 32;

/// Number of bytes in a half-byte block array.
pub const HALF_BYTE_BLOCK_BYTES: usize = BLOCK_COUNT / 2;

/// Number of bytes in a byte block array.
pub const BYTE_BLOCK_BYTES: usize = BLOCK_COUNT;

/// Number of bytes in a short block array.
pub const SHORT_BLOCK_BYTES: usize = BLOCK_COUNT * 2;

/// Maximum distinct external ids in a half-byte palette.
pub const HALF_BYTE_MAX_SIZE: usize = 16;

/// Maximum distinct external ids in a byte palette.
pub const BYTE_MAX_SIZE: usize = 256;

/// Maximum distinct external ids in a short palette.
pub const SHORT_MAX_SIZE: usize = 65_536;

/// Wire ordinal of the palette type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PaletteType {
    /// One id (air), no block storage.
    Empty = 0,
    /// Nibble-packed internal ids.
    HalfByte = 1,
    /// Byte internal ids.
    Byte = 2,
    /// Short internal ids.
    Short = 3,
}

impl PaletteType {
    /// Decode the protocol ordinal.
    pub fn from_ordinal(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::HalfByte),
            2 => Some(Self::Byte),
            3 => Some(Self::Short),
            _ => None,
        }
    }

    /// Maximum distinct external ids this palette can address.
    pub const fn max_size(self) -> usize {
        match self {
            Self::Empty => 1,
            Self::HalfByte => HALF_BYTE_MAX_SIZE,
            Self::Byte => BYTE_MAX_SIZE,
            Self::Short => SHORT_MAX_SIZE,
        }
    }

    /// Storage size of one palette-table entry's internal id.
    const fn internal_id_size(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::HalfByte | Self::Byte => 1,
            Self::Short => 2,
        }
    }

    /// Size of the packed block array.
    const fn block_bytes(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::HalfByte => HALF_BYTE_BLOCK_BYTES,
            Self::Byte => BYTE_BLOCK_BYTES,
            Self::Short => SHORT_BLOCK_BYTES,
        }
    }
}

/// Result of a palette `set` operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetResult {
    /// A previously unseen id was added or the previous id disappeared.
    AddedOrRemoved,
    /// An existing id replaced another existing id.
    Changed,
    /// The block already had the requested id.
    Unchanged,
    /// The palette is full and must be [`Palette::promote`]d before retrying.
    RequiresPromote,
}

/// One palette table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Internal id used by the packed block array.
    pub internal: u16,
    /// External asset id.
    pub external: i32,
    /// Number of blocks using this id.
    pub count: usize,
}

/// Errors from palette operations.
#[derive(Debug, thiserror::Error)]
pub enum PaletteError {
    /// A block index was outside `0..BLOCK_COUNT`.
    #[error("block index {index} is outside 0..{BLOCK_COUNT}")]
    IndexOutOfRange {
        /// Requested index.
        index: usize,
    },
    /// The input did not contain exactly one section of block ids.
    #[error("expected {BLOCK_COUNT} block ids, got {0}")]
    InvalidBlockCount(usize),
    /// Promotion is not possible (Short is the widest palette).
    #[error("cannot promote a {0:?} palette")]
    CannotPromote(PaletteType),
    /// Demotion is only valid when the palette is small enough.
    #[error("cannot demote a {0:?} palette with {1} distinct ids")]
    CannotDemote(PaletteType, usize),
    /// An external id does not fit the key width.
    #[error("external id {0} does not fit in {1} bytes")]
    KeyOverflow(i32, usize),
    /// A palette type byte was invalid.
    #[error("invalid palette type {0}")]
    InvalidType(u8),
    /// The wire reader rejected the input.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

type Result<T> = std::result::Result<T, PaletteError>;

/// External-id width used by the storage format.
pub trait KeyCodec {
    /// Bytes required for `external_id`.
    fn key_size(&self, external_id: i32) -> usize;
    /// Write an external id.
    fn write_key(&self, writer: &mut Writer, external_id: i32) -> Result<()>;
    /// Read an external id.
    fn read_key(&self, reader: &mut Reader<'_>) -> Result<i32>;
}

/// One-byte external ids, used for blocks and rotations.
#[derive(Debug, Clone, Copy, Default)]
pub struct ByteKey;

impl KeyCodec for ByteKey {
    fn key_size(&self, _external_id: i32) -> usize {
        1
    }

    fn write_key(&self, writer: &mut Writer, external_id: i32) -> Result<()> {
        let value =
            u8::try_from(external_id).map_err(|_| PaletteError::KeyOverflow(external_id, 1))?;
        writer.write_u8(value);
        Ok(())
    }

    fn read_key(&self, reader: &mut Reader<'_>) -> Result<i32> {
        Ok(i32::from(reader.read_u8()?))
    }
}

/// Two-byte external ids, used for fillers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShortKey;

impl KeyCodec for ShortKey {
    fn key_size(&self, _external_id: i32) -> usize {
        2
    }

    fn write_key(&self, writer: &mut Writer, external_id: i32) -> Result<()> {
        let value =
            u16::try_from(external_id).map_err(|_| PaletteError::KeyOverflow(external_id, 2))?;
        writer.write_u16(value);
        Ok(())
    }

    fn read_key(&self, reader: &mut Reader<'_>) -> Result<i32> {
        Ok(i32::from(reader.read_u16()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Blocks {
    Empty,
    Half(Vec<u8>),
    Byte(Vec<u8>),
    Short(Vec<u16>),
}

/// One section palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    kind: PaletteType,
    internal_to_external: Vec<Option<i32>>,
    external_to_internal: BTreeMap<i32, u16>,
    counts: Vec<u16>,
    blocks: Blocks,
}

impl Palette {
    /// Create an all-air palette.
    pub fn empty() -> Self {
        Self::with_kind(PaletteType::Empty)
    }

    /// Create an all-air palette of a specific type.
    pub fn with_kind(kind: PaletteType) -> Self {
        let mut palette = Self {
            kind,
            internal_to_external: vec![None; kind.max_size()],
            external_to_internal: BTreeMap::new(),
            counts: vec![0; kind.max_size()],
            blocks: match kind {
                PaletteType::Empty => Blocks::Empty,
                PaletteType::HalfByte => Blocks::Half(vec![0; HALF_BYTE_BLOCK_BYTES]),
                PaletteType::Byte => Blocks::Byte(vec![0; BYTE_BLOCK_BYTES]),
                PaletteType::Short => Blocks::Short(vec![0; BLOCK_COUNT]),
            },
        };
        palette.internal_to_external[0] = Some(0);
        palette.external_to_internal.insert(0, 0);
        palette.counts[0] = BLOCK_COUNT as u16;
        palette
    }

    /// Build a palette from a dense array of external ids.
    ///
    /// The palette type follows Java's `AbstractSectionPalette.from`
    /// thresholds. Internal ids are assigned in ascending external-id order so
    /// the resulting table is deterministic.
    pub fn from_ids(ids: &[i32]) -> Result<Self> {
        if ids.len() != BLOCK_COUNT {
            return Err(PaletteError::InvalidBlockCount(ids.len()));
        }
        let distinct: BTreeSet<i32> = ids.iter().copied().collect();
        let kind = if distinct.len() == 1 && distinct.contains(&0) {
            PaletteType::Empty
        } else if distinct.len() < PaletteType::HalfByte.max_size() {
            PaletteType::HalfByte
        } else if distinct.len() < PaletteType::Byte.max_size() {
            PaletteType::Byte
        } else if distinct.len() < PaletteType::Short.max_size() {
            PaletteType::Short
        } else {
            return Err(PaletteError::CannotPromote(PaletteType::Short));
        };

        let mut palette = Self::with_kind(if kind == PaletteType::Empty {
            PaletteType::Empty
        } else {
            kind
        });
        if kind == PaletteType::Empty {
            return Ok(palette);
        }

        // Air is internal id 0 if present; all other ids follow in sorted order.
        palette.external_to_internal.clear();
        palette.internal_to_external.fill(None);
        palette.counts.fill(0);
        let mut ordered: Vec<i32> = distinct.into_iter().collect();
        ordered.sort_unstable();
        if let Some(position) = ordered.iter().position(|id| *id == 0) {
            ordered.remove(position);
            ordered.insert(0, 0);
        }
        for (internal, external) in ordered.iter().enumerate() {
            let internal = internal as u16;
            palette.internal_to_external[internal as usize] = Some(*external);
            palette.external_to_internal.insert(*external, internal);
        }
        for (position, id) in ids.iter().enumerate() {
            let internal = *palette
                .external_to_internal
                .get(id)
                .expect("id was collected") as usize;
            palette.counts[internal] += 1;
            palette.set_raw_internal(position, internal);
        }
        Ok(palette)
    }

    /// Palette type.
    pub const fn kind(&self) -> PaletteType {
        self.kind
    }

    /// Wire ordinal.
    pub const fn ordinal(&self) -> u8 {
        self.kind as u8
    }

    /// Number of live external ids.
    pub fn count(&self) -> usize {
        self.external_to_internal.len()
    }

    /// Number of blocks using `external_id`.
    pub fn count_of(&self, external_id: i32) -> usize {
        self.external_to_internal
            .get(&external_id)
            .map(|internal| usize::from(self.counts[*internal as usize]))
            .unwrap_or(0)
    }

    /// True when the palette contains `external_id`.
    pub fn contains(&self, external_id: i32) -> bool {
        self.external_to_internal.contains_key(&external_id)
    }

    /// True when every block has `external_id`.
    pub fn is_solid(&self, external_id: i32) -> bool {
        self.count() == 1 && self.contains(external_id)
    }

    /// Read the external id at `index`.
    pub fn get(&self, index: usize) -> i32 {
        assert!(index < BLOCK_COUNT, "block index out of range: {index}");
        if self.kind == PaletteType::Empty {
            return 0;
        }
        let internal = self.get_raw_internal(index);
        self.internal_to_external[internal as usize].unwrap_or(0)
    }

    /// Set the external id at `index`.
    pub fn set(&mut self, index: usize, external_id: i32) -> SetResult {
        assert!(index < BLOCK_COUNT, "block index out of range: {index}");
        if self.kind == PaletteType::Empty {
            return if external_id == 0 {
                SetResult::Unchanged
            } else {
                SetResult::RequiresPromote
            };
        }

        let old_internal = self.get_raw_internal(index);
        if let Some(new_internal) = self.external_to_internal.get(&external_id).copied() {
            if new_internal == old_internal {
                return SetResult::Unchanged;
            }
            let removed = self.decrement(old_internal);
            self.increment(new_internal);
            self.set_raw_internal(index, new_internal as usize);
            return if removed {
                SetResult::AddedOrRemoved
            } else {
                SetResult::Changed
            };
        }

        // A new external id needs an internal slot. A unique old id can be
        // reused, otherwise take the lowest free internal id.
        let candidate = if self.counts[old_internal as usize] == 1 {
            Some(old_internal)
        } else {
            self.lowest_free_internal()
        };
        let Some(candidate) = candidate else {
            return SetResult::RequiresPromote;
        };
        if usize::from(candidate) >= self.kind.max_size() {
            return SetResult::RequiresPromote;
        }

        self.decrement(old_internal);
        self.internal_to_external[candidate as usize] = Some(external_id);
        self.external_to_internal.insert(external_id, candidate);
        self.counts[candidate as usize] = 1;
        self.set_raw_internal(index, candidate as usize);
        SetResult::AddedOrRemoved
    }

    /// Grow to the next palette type, preserving ids and block data.
    pub fn promote(&mut self) -> Result<()> {
        let next = match self.kind {
            PaletteType::Empty => PaletteType::HalfByte,
            PaletteType::HalfByte => PaletteType::Byte,
            PaletteType::Byte => PaletteType::Short,
            PaletteType::Short => return Err(PaletteError::CannotPromote(self.kind)),
        };
        let old_kind = self.kind;
        let old_blocks = std::mem::replace(
            &mut self.blocks,
            match next {
                PaletteType::Empty => Blocks::Empty,
                PaletteType::HalfByte => Blocks::Half(vec![0; HALF_BYTE_BLOCK_BYTES]),
                PaletteType::Byte => Blocks::Byte(vec![0; BYTE_BLOCK_BYTES]),
                PaletteType::Short => Blocks::Short(vec![0; BLOCK_COUNT]),
            },
        );
        // Widen the packed data before changing the kind.
        let widened = match (old_kind, next, old_blocks) {
            (PaletteType::Empty, _, Blocks::Empty) => Blocks::Half(vec![0; HALF_BYTE_BLOCK_BYTES]),
            (PaletteType::HalfByte, _, Blocks::Half(bytes)) => Blocks::Byte(widen_half(&bytes)),
            (PaletteType::Byte, _, Blocks::Byte(bytes)) => {
                Blocks::Short(bytes.into_iter().map(u16::from).collect())
            }
            (_, _, blocks) => blocks,
        };
        self.kind = next;
        self.blocks = widened;
        self.internal_to_external.resize(next.max_size(), None);
        self.counts.resize(next.max_size(), 0);
        Ok(())
    }

    /// True when the palette can be demoted to the next smaller type.
    pub fn should_demote(&self) -> bool {
        match self.kind {
            PaletteType::Empty => false,
            PaletteType::HalfByte => self.is_solid(0),
            PaletteType::Byte => self.count() <= 14,
            PaletteType::Short => self.count() <= 251,
        }
    }

    /// Shrink to the next smaller palette type.
    pub fn demote(&mut self) -> Result<()> {
        if !self.should_demote() {
            return Err(PaletteError::CannotDemote(self.kind, self.count()));
        }
        let next = match self.kind {
            PaletteType::Empty => return Err(PaletteError::CannotDemote(self.kind, self.count())),
            PaletteType::HalfByte => PaletteType::Empty,
            PaletteType::Byte => PaletteType::HalfByte,
            PaletteType::Short => PaletteType::Byte,
        };
        if next == PaletteType::Empty {
            *self = Self::empty();
            return Ok(());
        }

        // Compact internal ids into the smaller address space, then rebuild the
        // packed block array through that mapping.
        let entries = self.entries();
        let mut mapping = BTreeMap::new();
        let mut internal_to_external = vec![None; next.max_size()];
        let mut external_to_internal = BTreeMap::new();
        let mut counts = vec![0u16; next.max_size()];
        for (new_internal, entry) in entries.iter().enumerate() {
            mapping.insert(entry.internal, new_internal as u16);
            internal_to_external[new_internal] = Some(entry.external);
            external_to_internal.insert(entry.external, new_internal as u16);
            counts[new_internal] = entry.count as u16;
        }

        let old_blocks = std::mem::replace(&mut self.blocks, Blocks::Empty);
        let mut new_blocks = match next {
            PaletteType::Empty => Blocks::Empty,
            PaletteType::HalfByte => Blocks::Half(vec![0; HALF_BYTE_BLOCK_BYTES]),
            PaletteType::Byte => Blocks::Byte(vec![0; BYTE_BLOCK_BYTES]),
            PaletteType::Short => Blocks::Short(vec![0; BLOCK_COUNT]),
        };
        for index in 0..BLOCK_COUNT {
            let old_internal = match &old_blocks {
                Blocks::Empty => 0,
                Blocks::Half(bytes) => get_nibble(bytes, index),
                Blocks::Byte(bytes) => u16::from(bytes[index]),
                Blocks::Short(values) => values[index],
            };
            let new_internal = mapping.get(&old_internal).copied().unwrap_or(0);
            match &mut new_blocks {
                Blocks::Empty => {}
                Blocks::Half(bytes) => set_nibble(bytes, index, new_internal as u8),
                Blocks::Byte(bytes) => bytes[index] = new_internal as u8,
                Blocks::Short(values) => values[index] = new_internal,
            }
        }

        self.kind = next;
        self.blocks = new_blocks;
        self.internal_to_external = internal_to_external;
        self.external_to_internal = external_to_internal;
        self.counts = counts;
        Ok(())
    }

    /// Palette entries in ascending internal-id order (the serialized order).
    pub fn entries(&self) -> Vec<PaletteEntry> {
        let mut entries = Vec::with_capacity(self.count());
        for (internal, external) in self.internal_to_external.iter().enumerate() {
            if let Some(external) = external {
                entries.push(PaletteEntry {
                    internal: internal as u16,
                    external: *external,
                    count: usize::from(self.counts[internal]),
                });
            }
        }
        entries
    }

    /// Serialized size in the network packet layout.
    pub fn serialized_packet_size(&self) -> usize {
        if self.kind == PaletteType::Empty {
            return 0;
        }
        2 + self.count() * (self.kind.internal_id_size() + 4 + 2) + self.kind.block_bytes()
    }

    /// Serialize in the network packet layout.
    pub fn serialize_for_packet(&self, writer: &mut Writer) {
        if self.kind == PaletteType::Empty {
            return;
        }
        writer.write_u16(self.count() as u16);
        for entry in self.entries() {
            match self.kind {
                PaletteType::HalfByte | PaletteType::Byte => {
                    writer.write_u8(entry.internal as u8);
                }
                PaletteType::Short => writer.write_u16(entry.internal),
                PaletteType::Empty => unreachable!(),
            }
            writer.write_i32(entry.external);
            writer.write_u16(entry.count as u16);
        }
        self.write_blocks(writer, false);
    }

    /// Serialized size in the storage layout.
    pub fn serialized_size(&self, key: &dyn KeyCodec) -> usize {
        if self.kind == PaletteType::Empty {
            return 0;
        }
        let table: usize = self
            .entries()
            .iter()
            .map(|entry| self.kind.internal_id_size() + key.key_size(entry.external) + 2)
            .sum();
        2 + table + self.kind.block_bytes()
    }

    /// Serialize in the storage layout.
    pub fn serialize(&self, key: &dyn KeyCodec, writer: &mut Writer) -> Result<()> {
        if self.kind == PaletteType::Empty {
            return Ok(());
        }
        writer.write_u16_be(self.count() as u16);
        for entry in self.entries() {
            match self.kind {
                PaletteType::HalfByte | PaletteType::Byte => {
                    writer.write_u8(entry.internal as u8);
                }
                PaletteType::Short => writer.write_u16_be(entry.internal),
                PaletteType::Empty => unreachable!(),
            }
            key.write_key(writer, entry.external)?;
            writer.write_u16_be(entry.count as u16);
        }
        self.write_blocks(writer, true);
        Ok(())
    }

    /// Deserialize the storage layout for a known palette type.
    pub fn deserialize(
        kind: PaletteType,
        key: &dyn KeyCodec,
        reader: &mut Reader<'_>,
    ) -> Result<Self> {
        if kind == PaletteType::Empty {
            return Ok(Self::empty());
        }
        let mut palette = Self::with_kind(kind);
        palette.external_to_internal.clear();
        palette.internal_to_external.fill(None);
        palette.counts.fill(0);

        let table_len = reader.read_u16_be()? as usize;
        if table_len > kind.max_size() {
            return Err(PaletteError::Protocol(ProtocolError::LengthTooLarge {
                field: "palette table",
                length: table_len,
                max: kind.max_size(),
            }));
        }

        let mut remap: BTreeMap<u16, u16> = BTreeMap::new();
        for _ in 0..table_len {
            let internal = match kind {
                PaletteType::HalfByte | PaletteType::Byte => u16::from(reader.read_u8()?),
                PaletteType::Short => reader.read_u16_be()?,
                PaletteType::Empty => unreachable!(),
            };
            if usize::from(internal) >= kind.max_size() {
                return Err(PaletteError::Protocol(ProtocolError::LengthTooLarge {
                    field: "palette internal id",
                    length: usize::from(internal),
                    max: kind.max_size(),
                }));
            }
            let external = key.read_key(reader)?;
            let count = reader.read_u16_be()?;

            if let Some(existing) = palette.external_to_internal.get(&external).copied() {
                remap.insert(internal, existing);
                palette.counts[existing as usize] =
                    palette.counts[existing as usize].wrapping_add(count);
                continue;
            }
            palette.external_to_internal.insert(external, internal);
            palette.internal_to_external[internal as usize] = Some(external);
            palette.counts[internal as usize] = count;
        }

        palette.blocks = match kind {
            PaletteType::Empty => Blocks::Empty,
            PaletteType::HalfByte => {
                let bytes = reader.read_slice(HALF_BYTE_BLOCK_BYTES)?.to_vec();
                Blocks::Half(if remap.is_empty() {
                    bytes
                } else {
                    remap_half(&bytes, &remap)
                })
            }
            PaletteType::Byte => {
                let bytes = reader.read_slice(BYTE_BLOCK_BYTES)?.to_vec();
                Blocks::Byte(if remap.is_empty() {
                    bytes
                } else {
                    bytes
                        .into_iter()
                        .map(|internal| {
                            remap
                                .get(&u16::from(internal))
                                .map(|mapped| *mapped as u8)
                                .unwrap_or(internal)
                        })
                        .collect()
                })
            }
            PaletteType::Short => {
                let mut values = Vec::with_capacity(BLOCK_COUNT);
                for _ in 0..BLOCK_COUNT {
                    let internal = reader.read_u16_be()?;
                    values.push(remap.get(&internal).copied().unwrap_or(internal));
                }
                Blocks::Short(values)
            }
        };
        Ok(palette)
    }

    /// Deserialize the network packet layout for a known palette type.
    pub fn deserialize_for_packet(kind: PaletteType, reader: &mut Reader<'_>) -> Result<Self> {
        Self::deserialize_impl(
            kind,
            reader,
            |reader| {
                let internal = match kind {
                    PaletteType::HalfByte | PaletteType::Byte => u16::from(reader.read_u8()?),
                    PaletteType::Short => reader.read_u16()?,
                    PaletteType::Empty => 0,
                };
                Ok((internal, reader.read_i32()?, reader.read_u16()?))
            },
            false,
        )
    }

    fn deserialize_impl(
        kind: PaletteType,
        reader: &mut Reader<'_>,
        mut read_entry: impl FnMut(&mut Reader<'_>) -> Result<(u16, i32, u16)>,
        big_endian_blocks: bool,
    ) -> Result<Self> {
        if kind == PaletteType::Empty {
            return Ok(Self::empty());
        }
        let mut palette = Self::with_kind(kind);
        palette.external_to_internal.clear();
        palette.internal_to_external.fill(None);
        palette.counts.fill(0);

        let table_len = if big_endian_blocks {
            reader.read_u16_be()? as usize
        } else {
            reader.read_u16()? as usize
        };
        if table_len > kind.max_size() {
            return Err(PaletteError::Protocol(ProtocolError::LengthTooLarge {
                field: "palette table",
                length: table_len,
                max: kind.max_size(),
            }));
        }

        let mut remap: BTreeMap<u16, u16> = BTreeMap::new();
        for _ in 0..table_len {
            let (internal, external, count) = read_entry(reader)?;
            if usize::from(internal) >= kind.max_size() {
                return Err(PaletteError::Protocol(ProtocolError::LengthTooLarge {
                    field: "palette internal id",
                    length: usize::from(internal),
                    max: kind.max_size(),
                }));
            }
            if let Some(existing) = palette.external_to_internal.get(&external).copied() {
                remap.insert(internal, existing);
                palette.counts[existing as usize] =
                    palette.counts[existing as usize].wrapping_add(count);
                continue;
            }
            palette.external_to_internal.insert(external, internal);
            palette.internal_to_external[internal as usize] = Some(external);
            palette.counts[internal as usize] = count;
        }

        palette.blocks = match kind {
            PaletteType::Empty => Blocks::Empty,
            PaletteType::HalfByte => {
                let bytes = reader.read_slice(HALF_BYTE_BLOCK_BYTES)?.to_vec();
                Blocks::Half(if remap.is_empty() {
                    bytes
                } else {
                    remap_half(&bytes, &remap)
                })
            }
            PaletteType::Byte => {
                let bytes = reader.read_slice(BYTE_BLOCK_BYTES)?.to_vec();
                Blocks::Byte(if remap.is_empty() {
                    bytes
                } else {
                    bytes
                        .into_iter()
                        .map(|internal| {
                            remap
                                .get(&u16::from(internal))
                                .map(|mapped| *mapped as u8)
                                .unwrap_or(internal)
                        })
                        .collect()
                })
            }
            PaletteType::Short => {
                let mut values = Vec::with_capacity(BLOCK_COUNT);
                for _ in 0..BLOCK_COUNT {
                    let internal = if big_endian_blocks {
                        reader.read_u16_be()?
                    } else {
                        reader.read_u16()?
                    };
                    values.push(remap.get(&internal).copied().unwrap_or(internal));
                }
                Blocks::Short(values)
            }
        };
        Ok(palette)
    }

    fn lowest_free_internal(&self) -> Option<u16> {
        self.internal_to_external
            .iter()
            .position(Option::is_none)
            .map(|index| index as u16)
    }

    fn decrement(&mut self, internal: u16) -> bool {
        let index = internal as usize;
        self.counts[index] -= 1;
        if self.counts[index] != 0 {
            return false;
        }
        if let Some(external) = self.internal_to_external[index].take() {
            self.external_to_internal.remove(&external);
        }
        true
    }

    fn increment(&mut self, internal: u16) {
        self.counts[internal as usize] += 1;
    }

    fn get_raw_internal(&self, index: usize) -> u16 {
        match &self.blocks {
            Blocks::Empty => 0,
            Blocks::Half(bytes) => get_nibble(bytes, index),
            Blocks::Byte(bytes) => u16::from(bytes[index]),
            Blocks::Short(values) => values[index],
        }
    }

    fn set_raw_internal(&mut self, index: usize, internal: usize) {
        match &mut self.blocks {
            Blocks::Empty => {}
            Blocks::Half(bytes) => set_nibble(bytes, index, internal as u8),
            Blocks::Byte(bytes) => bytes[index] = internal as u8,
            Blocks::Short(values) => values[index] = internal as u16,
        }
    }

    fn write_blocks(&self, writer: &mut Writer, big_endian: bool) {
        match &self.blocks {
            Blocks::Empty => {}
            Blocks::Half(bytes) | Blocks::Byte(bytes) => writer.write_slice(bytes),
            Blocks::Short(values) => {
                for value in values {
                    if big_endian {
                        writer.write_slice(&value.to_be_bytes());
                    } else {
                        writer.write_u16(*value);
                    }
                }
            }
        }
    }
}

fn get_nibble(bytes: &[u8], index: usize) -> u16 {
    let byte = bytes[index >> 1];
    let nibble = if index & 1 == 0 {
        byte >> 4
    } else {
        byte & 0x0F
    };
    u16::from(nibble)
}

fn set_nibble(bytes: &mut [u8], index: usize, value: u8) {
    let slot = &mut bytes[index >> 1];
    if index & 1 == 0 {
        *slot = (*slot & 0x0F) | ((value & 0x0F) << 4);
    } else {
        *slot = (*slot & 0xF0) | (value & 0x0F);
    }
}

fn widen_half(bytes: &[u8]) -> Vec<u8> {
    let mut widened = Vec::with_capacity(BLOCK_COUNT);
    for index in 0..BLOCK_COUNT {
        widened.push(get_nibble(bytes, index) as u8);
    }
    widened
}

fn remap_half(bytes: &[u8], remap: &BTreeMap<u16, u16>) -> Vec<u8> {
    let mut mapped = bytes.to_vec();
    for index in 0..BLOCK_COUNT {
        let internal = get_nibble(bytes, index);
        if let Some(replacement) = remap.get(&internal) {
            set_nibble(&mut mapped, index, *replacement as u8);
        }
    }
    mapped
}
