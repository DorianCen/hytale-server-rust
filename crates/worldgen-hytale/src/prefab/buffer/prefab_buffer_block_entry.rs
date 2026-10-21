// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/impl/PrefabBufferBlockEntry.java`
//! Java `PrefabBufferBlockEntry` — one block of a prefab column.

/// Java `public class PrefabBufferBlockEntry`.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabBufferBlockEntry {
    /// Java `public final int y` — the height of the block inside the column
    /// (relative to the prefab anchor).
    pub y: i32,
    /// Java `public String blockTypeKey` — the name of the block type.
    pub block_type_key: String,
    /// Java `public int blockId` — the numeric id of the block type.
    pub block_id: i32,
    /// Java `public float chance` — the placement chance (`1.0` = always).
    pub chance: f32,
    /// Java `public int fluidId` — the fluid of the cell (`0` = empty).
    pub fluid_id: i32,
    /// Java `public byte fluidLevel` — the fluid level.
    pub fluid_level: i8,
    /// Java `public byte supportValue` — the support value of the block.
    pub support_value: i8,
    /// Java `public int filler` — the packed filler offset.
    pub filler: i32,
    /// Java `public int rotation` — the rotation index of the block.
    pub rotation: i32,
}

impl PrefabBufferBlockEntry {
    /// Java `PrefabBufferBlockEntry(int y, int blockId, String blockTypeKey)`.
    pub fn new(y: i32, block_id: i32, block_type_key: &str) -> Self {
        Self::with_chance(y, block_id, block_type_key, 1.0)
    }

    /// Java `PrefabBufferBlockEntry(int y, int blockId, String blockTypeKey,
    /// float chance)`.
    pub fn with_chance(y: i32, block_id: i32, block_type_key: &str, chance: f32) -> Self {
        Self {
            y,
            block_type_key: block_type_key.to_string(),
            block_id,
            chance,
            fluid_id: 0,
            fluid_level: 0,
            support_value: 0,
            filler: 0,
            rotation: 0,
        }
    }
}
