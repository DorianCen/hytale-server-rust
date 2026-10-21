// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/BsonPrefabBufferDeserializer.java`
//! The `.prefab.json` reader — the on-disk format of a prefab.
//!
//! # Format
//!
//! ```json
//! {
//!   "version": 8,
//!   "blockIdVersion": 11,
//!   "anchorX": 1, "anchorY": 20, "anchorZ": 1,
//!   "blocks": [{"x": -1, "y": 8, "z": 0, "name": "Rock_Sandstone",
//!               "rotation": 1, "filler": 0, "support": 15}],
//!   "fluids": [{"x": 0, "y": 3, "z": 0, "name": "Water_Source", "level": 8}],
//!   "entities": [ ... ]
//! }
//! ```
//!
//! The Java class reads the file as a BSON document; the Rust port reads it
//! with `serde_json` (the JSON is the source of truth — the binary `.lpf`
//! cache is not ported, see `REMAINING.md` § 5).
//!
//! # Deviations
//!
//! * **The block id migrations are not ported** — the Java chains the
//!   `BlockMigration` assets of the `blockIdVersion`; the Rust port has no
//!   migration assets, so the block names are resolved as they are.
//! * **The block components (`"components"`) are not ported** — they are
//!   the ECS state of the block (`REMAINING.md` § 2). The number of the
//!   skipped components is reported by [`PrefabJsonStats`].
//! * **The entities are not ported** (`REMAINING.md` § 2) — the number of
//!   the skipped entities is reported by [`PrefabJsonStats`].

use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::buffer::prefab_buffer_block_entry::PrefabBufferBlockEntry;

/// Java `SelectionPrefabSerializer.VERSION` — the current prefab format.
pub const PREFAB_FORMAT_VERSION: i32 = 8;

/// Java `BsonPrefabBufferDeserializer.LEGACY_BLOCK_ID_VERSION` = `8`.
pub const LEGACY_BLOCK_ID_VERSION: i32 = 8;

/// The statistics of a parsed prefab file — the parts the Rust port does
/// not carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrefabJsonStats {
    /// The number of blocks that carry a `"components"` object.
    pub blocks_with_components: usize,
    /// The number of entities of the file.
    pub entity_count: usize,
}

/// Parses a `.prefab.json` document into a [`PrefabBuffer`].
///
/// Java `BsonPrefabBufferDeserializer.deserialize(Path path, BsonDocument
/// document)`.
pub fn parse_prefab_json(
    document: &Value,
    registries: &Registries,
) -> (PrefabBuffer, PrefabJsonStats) {
    let mut stats = PrefabJsonStats::default();

    // --- the version ---------------------------------------------------
    let version = match document.get("version") {
        Some(value) => JsonNumber::as_i32(value),
        None => -1,
    };
    if version < PREFAB_FORMAT_VERSION {
        panic!("Prefab version is too old: {version}");
    }
    if version > PREFAB_FORMAT_VERSION {
        panic!("Prefab version is too new: {version} by expected {PREFAB_FORMAT_VERSION}");
    }

    // --- the anchor ----------------------------------------------------
    let anchor = (
        JsonNumber::as_i32(required(document, "anchorX")),
        JsonNumber::as_i32(required(document, "anchorY")),
        JsonNumber::as_i32(required(document, "anchorZ")),
    );

    // The block id version is read (and validated by the Java migrations);
    // the migrations themselves need the CoreServer `BlockMigration` assets,
    // which are not part of the world generator.
    let _block_id_version = match document.get("blockIdVersion") {
        Some(value) => JsonNumber::as_i32(value),
        None => LEGACY_BLOCK_ID_VERSION,
    };

    // --- the blocks ----------------------------------------------------
    // `columnIndex -> (y -> entry)`
    let mut columns: HashMap<i32, HashMap<i32, PrefabBufferBlockEntry>> = HashMap::new();
    let mut column_order: Vec<i32> = Vec::new();

    if let Some(blocks) = document.get("blocks") {
        let blocks = blocks
            .as_array()
            .unwrap_or_else(|| panic!("The 'blocks' field is not an array"));
        for block in blocks {
            let real_x = JsonNumber::as_i32(required(block, "x"));
            let real_y = JsonNumber::as_i32(required(block, "y"));
            let real_z = JsonNumber::as_i32(required(block, "z"));
            let x = real_x - anchor.0;
            let y = real_y - anchor.1;
            let z = real_z - anchor.2;

            // Java: the compact buffer stores the coordinates in a short.
            if !(-32768..=32767).contains(&x) {
                panic!("Violation X: Short.MIN_VALUE < {x} < Short.MAX_VALUE");
            }
            if !(-32768..=32767).contains(&y) {
                panic!("Violation Y: Short.MIN_VALUE < {y} < Short.MAX_VALUE");
            }
            if !(-32768..=32767).contains(&z) {
                panic!("Violation Z: Short.MIN_VALUE < {z} < Short.MAX_VALUE");
            }

            let mut entry = PrefabBufferBlockEntry::new(y, 0, "Empty");
            let name = required(block, "name");
            let name = match name.as_str() {
                Some(name) => name,
                None => panic!("The block name is not a string"),
            };
            // Java: the chance is encoded in the block name
            // ("50%Rock_Stone").
            let (block_type_key, chance) = match name.find('%') {
                Some(index) => {
                    let percent = name[..index]
                        .parse::<f32>()
                        .unwrap_or_else(|_| panic!("For input string: \"{}\"", &name[..index]));
                    if percent < 0.0 {
                        panic!("Chance is smaller than 0%. Given: {percent}");
                    }
                    if percent > 100.0 {
                        panic!("Chance is larger than 100%. Given: {percent}");
                    }
                    (&name[index + 1..], percent / 100.0)
                }
                None => (name, 1.0),
            };
            entry.block_type_key = block_type_key.to_string();
            entry.block_id = resolve_block(registries, block_type_key);
            entry.chance = chance;

            if block.get("components").is_some() {
                stats.blocks_with_components += 1;
            }

            entry.support_value = match block.get("support") {
                Some(value) => JsonNumber::as_i32(value) as i8,
                None => 0,
            };
            entry.filler = match block.get("filler") {
                Some(value) => JsonNumber::as_i32(value),
                None => 0,
            };
            entry.rotation = match block.get("rotation") {
                Some(value) => JsonNumber::as_i32(value),
                None => 0,
            };

            let column_index = hytale_math::math_util::pack_int(x, z);
            let column = columns.entry(column_index).or_insert_with(|| {
                column_order.push(column_index);
                HashMap::new()
            });
            if let Some(existing) = column.insert(y, entry) {
                panic!(
                    "Block is already present in column. Given: {real_x}, {real_y}, {real_z}, {} - Existing: {}, {}",
                    existing.block_type_key, existing.y, existing.block_type_key
                );
            }
        }
    }

    // --- the fluids ----------------------------------------------------
    if let Some(fluids) = document.get("fluids") {
        let fluids = fluids
            .as_array()
            .unwrap_or_else(|| panic!("The 'fluids' field is not an array"));
        for fluid in fluids {
            let real_x = JsonNumber::as_i32(required(fluid, "x"));
            let real_y = JsonNumber::as_i32(required(fluid, "y"));
            let real_z = JsonNumber::as_i32(required(fluid, "z"));
            let x = real_x - anchor.0;
            let y = real_y - anchor.1;
            let z = real_z - anchor.2;

            if !(-32768..=32767).contains(&x) {
                panic!("Violation X: Short.MIN_VALUE < {x} < Short.MAX_VALUE");
            }
            if !(-32768..=32767).contains(&y) {
                panic!("Violation Y: Short.MIN_VALUE < {y} < Short.MAX_VALUE");
            }
            if !(-32768..=32767).contains(&z) {
                panic!("Violation Z: Short.MIN_VALUE < {z} < Short.MAX_VALUE");
            }

            let column_index = hytale_math::math_util::pack_int(x, z);
            let column = columns.entry(column_index).or_insert_with(|| {
                column_order.push(column_index);
                HashMap::new()
            });
            let entry = column
                .entry(y)
                .or_insert_with(|| PrefabBufferBlockEntry::new(y, 0, "Empty"));
            let fluid_name = match required(fluid, "name").as_str() {
                Some(name) => name.to_string(),
                None => panic!("The fluid name is not a string"),
            };
            entry.fluid_id = resolve_fluid(registries, &fluid_name);
            entry.fluid_level = JsonNumber::as_i32(required(fluid, "level")) as i8;
        }
    }

    // --- the entities (stubbed) ----------------------------------------
    if let Some(entities) = document.get("entities") {
        stats.entity_count = entities
            .as_array()
            .map(|array| array.len())
            .unwrap_or_default();
    }

    // --- the build -----------------------------------------------------
    let mut builder = PrefabBuffer::new_builder();
    builder.set_anchor(anchor);
    for column_index in column_order {
        let column = columns
            .get(&column_index)
            .expect("the column was just inserted");
        let mut entries: Vec<PrefabBufferBlockEntry> = column.values().cloned().collect();
        // Java: `Arrays.sort(entries, Comparator.comparingInt(o -> o.y))`
        entries.sort_by_key(|entry| entry.y);
        let x = hytale_math::math_util::unpack_left_int(column_index);
        let z = hytale_math::math_util::unpack_right_int(column_index);
        builder.add_column(x, z, entries);
    }

    (builder.build(), stats)
}

/// Java `BlockType.getBlockIdOrUnknown(assetMap, blockTypeKey, "Failed to
/// find block. Given %s", blockTypeStr)`.
///
/// Deviation: the Java registers the unknown block dynamically; the Rust
/// port panics (the documented 7b deviation).
fn resolve_block(registries: &Registries, block_type_key: &str) -> i32 {
    match registries.blocks.index_of(block_type_key) {
        Some(id) => id,
        None => panic!("Failed to find block. Given {block_type_key}"),
    }
}

/// Java `Fluid.getFluidIdOrUnknown(assetMap, fluidName, "Unknown fluid
/// '%s'", fluidName)`.
fn resolve_fluid(registries: &Registries, fluid_name: &str) -> i32 {
    match registries.fluids.index_of(fluid_name) {
        Some(id) => id,
        None => panic!("Unknown fluid '{fluid_name}'"),
    }
}

/// Reads a required field.
fn required<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("The field '{key}' is missing"))
}

/// The gson-style number accessors (`BsonInt32.getValue()` — the Java
/// documents hold 32 bit integers).
mod json_number {
    use serde_json::Value;

    /// Java `BsonInt32.getValue()`.
    pub(super) fn as_i32(value: &Value) -> i32 {
        match value {
            Value::Number(number) => number
                .as_i64()
                .unwrap_or_else(|| number.as_f64().map(|value| value as i64).unwrap_or(0))
                as i32,
            Value::String(text) => text.parse::<i32>().unwrap_or(0),
            _ => 0,
        }
    }
}

use json_number as JsonNumber;

/// The shared registry handle used by the prefab store.
pub type PrefabRegistries = Rc<Registries>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_version_is_rejected() {
        let document = serde_json::json!({"version": 7});
        let result = std::panic::catch_unwind(|| parse_prefab_json(&document, &Registries::load()));
        let payload = result.unwrap_err();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_else(|| "<non-string panic>".to_string());
        assert_eq!(message, "Prefab version is too old: 7");
    }

    #[test]
    fn a_new_version_is_rejected() {
        let document = serde_json::json!({"version": 9});
        let result = std::panic::catch_unwind(|| parse_prefab_json(&document, &Registries::load()));
        let payload = result.unwrap_err();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_else(|| "<non-string panic>".to_string());
        assert_eq!(message, "Prefab version is too new: 9 by expected 8");
    }

    #[test]
    fn the_anchor_is_baked_into_the_block_coordinates() {
        let document = serde_json::json!({
            "version": 8,
            "blockIdVersion": 11,
            "anchorX": 1, "anchorY": 20, "anchorZ": 1,
            "blocks": [{"x": 1, "y": 20, "z": 1, "name": "Rock_Stone"}]
        });
        let (buffer, stats) = parse_prefab_json(&document, &Registries::load());
        assert_eq!(buffer.get_anchor_x(), 1);
        assert_eq!(buffer.get_anchor_y(), 20);
        assert_eq!(buffer.get_anchor_z(), 1);
        // The block is at the local origin.
        assert_eq!(buffer.min, (0, 0, 0));
        assert_eq!(buffer.max, (0, 0, 0));
        assert_eq!(stats.blocks_with_components, 0);
        assert_eq!(stats.entity_count, 0);
    }
}
