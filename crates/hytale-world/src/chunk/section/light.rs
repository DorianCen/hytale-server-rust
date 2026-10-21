//! Section light storage.
//!
//! Ported from `CoreServer/.../world/chunk/section/ChunkLightData.java`.
//!
//! Java stores 32,768 packed RGB+sky values in an octree: a node is one mask
//! byte followed by eight children, each either a leaf `u16` or a pointer to a
//! child node. Storage uses big-endian leaves and prefixes the octree with the
//! change id and byte length; the packet form uses little-endian leaves. This
//! port keeps the same on-disk structure and compacts uniform nodes bottom-up.

use hytale_protocol::io::{Reader, Writer};

use super::{Result, SectionError};

/// Number of packed light values in one section.
pub const LIGHT_BLOCK_COUNT: usize = 32 * 32 * 32;

const TREE_CHILDREN: usize = 8;
/// Deepest node that may contain branches; its children are leaves.
const TREE_DEPTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Leaf(u16),
    Branch(Box<[Node; TREE_CHILDREN]>),
}

/// Optional packed light values for one section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LightData {
    values: Option<Vec<u16>>,
    change_id: i16,
}

impl LightData {
    /// Java `ChunkLightData.EMPTY`.
    pub fn empty(change_id: i16) -> Self {
        Self {
            values: None,
            change_id,
        }
    }

    /// A light buffer from a dense array.
    pub fn from_values(values: Vec<u16>, change_id: i16) -> Self {
        Self {
            values: Some(values),
            change_id,
        }
    }

    /// True when there is no light buffer (`hasLight == false`).
    pub fn is_empty(&self) -> bool {
        self.values.is_none()
    }

    /// Java `getChangeId()`.
    pub const fn change_id(&self) -> i16 {
        self.change_id
    }

    /// Set the change id.
    pub fn set_change_id(&mut self, change_id: i16) {
        self.change_id = change_id;
    }

    /// Read one packed value; absent light is zero.
    pub fn get(&self, index: usize) -> u16 {
        self.values
            .as_ref()
            .and_then(|values| values.get(index))
            .copied()
            .unwrap_or(0)
    }

    /// Write one packed value, materialising a zeroed buffer on first write.
    pub fn set(&mut self, index: usize, value: u16) {
        let values = self
            .values
            .get_or_insert_with(|| vec![0; LIGHT_BLOCK_COUNT]);
        values[index] = value;
    }

    /// Borrow the dense values, when present.
    pub fn values(&self) -> Option<&[u16]> {
        self.values.as_deref()
    }

    /// Storage byte size, including the change id and length prefix.
    pub fn serialized_size(&self) -> usize {
        2 + 1
            + self
                .values
                .as_ref()
                .map_or(0, |_| 4 + self.octree_bytes(true).len())
    }

    /// Packet byte size.
    pub fn serialized_packet_size(&self) -> usize {
        1 + self
            .values
            .as_ref()
            .map_or(0, |_| self.octree_bytes(false).len())
    }

    /// Java `serialize`: change id (BE), has-light flag, length (BE), octree.
    pub fn serialize(&self, writer: &mut Writer) -> Result<()> {
        writer.write_i16_be(self.change_id);
        match &self.values {
            None => writer.write_bool(false),
            Some(_) => {
                writer.write_bool(true);
                let octree = self.octree_bytes(true);
                writer.write_u32(
                    u32::try_from(octree.len()).map_err(|_| {
                        SectionError::InvalidData("light octree is larger than u32")
                    })?,
                );
                writer.write_slice(&octree);
            }
        }
        Ok(())
    }

    /// Java `ChunkLightData.deserialize`.
    pub fn deserialize(reader: &mut Reader<'_>) -> Result<Self> {
        let change_id = reader.read_i16_be()?;
        if !reader.read_bool()? {
            return Ok(Self::empty(change_id));
        }
        let length = reader.read_u32()? as usize;
        let bytes = reader.read_slice(length)?;
        let values = decode_octree(bytes, true)?;
        Ok(Self {
            values: Some(values),
            change_id,
        })
    }

    /// Java `serializeForPacket`: has-light flag, octree with LE leaves.
    pub fn serialize_for_packet(&self, writer: &mut Writer) -> Result<()> {
        match &self.values {
            None => writer.write_bool(false),
            Some(_) => {
                writer.write_bool(true);
                writer.write_slice(&self.octree_bytes(false));
            }
        }
        Ok(())
    }

    /// Decode the packet form.
    pub fn deserialize_for_packet(reader: &mut Reader<'_>) -> Result<Self> {
        if !reader.read_bool()? {
            return Ok(Self::empty(0));
        }
        let mut values = vec![0u16; LIGHT_BLOCK_COUNT];
        decode_node(reader, 0, 0, &mut values, false)?;
        Ok(Self {
            values: Some(values),
            change_id: 0,
        })
    }

    fn octree_bytes(&self, big_endian: bool) -> Vec<u8> {
        let Some(values) = &self.values else {
            return Vec::new();
        };
        let root = build_node(values, 0, LIGHT_BLOCK_COUNT);
        let mut writer = Writer::new();
        write_node(&root, &mut writer, big_endian);
        writer.into_inner()
    }
}

fn build_node(values: &[u16], start: usize, span: usize) -> Node {
    if span == 1 {
        return Node::Leaf(values[start]);
    }
    let child_span = span / TREE_CHILDREN;
    let children: [Node; TREE_CHILDREN] =
        std::array::from_fn(|index| build_node(values, start + index * child_span, child_span));
    if let Some(Node::Leaf(first)) = children.first() {
        if children
            .iter()
            .all(|child| matches!(child, Node::Leaf(value) if value == first))
        {
            return Node::Leaf(*first);
        }
    }
    Node::Branch(Box::new(children))
}

fn write_node(node: &Node, writer: &mut Writer, big_endian: bool) {
    match node {
        Node::Leaf(value) => write_leaf(writer, *value, big_endian),
        Node::Branch(children) => {
            let mut mask = 0u8;
            for (index, child) in children.iter().enumerate() {
                if matches!(child, Node::Branch(_)) {
                    mask |= 1 << index;
                }
            }
            writer.write_u8(mask);
            for child in children.iter() {
                write_node(child, writer, big_endian);
            }
        }
    }
}

fn write_leaf(writer: &mut Writer, value: u16, big_endian: bool) {
    if big_endian {
        writer.write_u16_be(value);
    } else {
        writer.write_u16(value);
    }
}

fn decode_octree(bytes: &[u8], big_endian: bool) -> Result<Vec<u16>> {
    let mut values = vec![0u16; LIGHT_BLOCK_COUNT];
    let mut reader = Reader::new(bytes);
    decode_node(&mut reader, 0, 0, &mut values, big_endian)?;
    if !reader.is_empty() {
        return Err(SectionError::InvalidData("trailing light octree bytes"));
    }
    Ok(values)
}

fn decode_node(
    reader: &mut Reader<'_>,
    depth: usize,
    start: usize,
    values: &mut [u16],
    big_endian: bool,
) -> Result<()> {
    if depth > TREE_DEPTH {
        return Err(SectionError::InvalidData("light octree is too deep"));
    }
    let mask = reader.read_u8()?;
    let span = LIGHT_BLOCK_COUNT >> (depth * 3);
    let child_span = span / TREE_CHILDREN;
    for index in 0..TREE_CHILDREN {
        let child_start = start + index * child_span;
        if mask & (1 << index) != 0 {
            decode_node(reader, depth + 1, child_start, values, big_endian)?;
        } else {
            let value = if big_endian {
                reader.read_u16_be()?
            } else {
                reader.read_u16()?
            };
            values[child_start..child_start + child_span].fill(value);
        }
    }
    Ok(())
}
