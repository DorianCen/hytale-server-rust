//! P0 asset packet encoders.
//!
//! Java sends one `Update*` packet per asset store on join; `AssetDumper` dumped
//! the exact payloads to `testdata/golden/server/assets/`. This module builds
//! the same packets from the Rust [`AssetRegistry`].
//!
//! The file-backed stores whose ids match Java (BlockBoundingBoxes,
//! Environment, Fluid, PhysicalMaterial) can be byte-compared. `BlockType` and
//! `Item` contain Rust-owned ids for contained assets (see
//! [`crate::registry`]), so their envelopes are built from the same packet types
//! but cannot be byte-compared until the Java contained-asset order is modelled.
//!
//! Java serializes the packet maps in ascending integer key order because
//! `AssetDumper.canonicalizeMaps` sorts every protocol map first; the Rust
//! encoders use `BTreeMap` for the same order.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::{UpdateBlockHitboxes, UpdatePhysicalMaterials};
use hytale_protocol::generated::common::{Hitbox, PhysicalMaterial, UpdateType};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::{ProtocolError, Writer};
use serde_json::Value;
use thiserror::Error;

use crate::registry::{AssetCollection, AssetRegistry};

/// Errors from the asset packet encoders.
#[derive(Debug, Error)]
pub enum AssetPacketError {
    /// An indexed asset is missing its numeric id.
    #[error("asset `{key}` in {store} has no numeric id")]
    MissingId {
        /// Store name.
        store: &'static str,
        /// Asset key.
        key: String,
    },
    /// A decoded asset is not the expected JSON object/shape.
    #[error("invalid asset `{key}` in {store}: {message}")]
    InvalidAsset {
        /// Store name.
        store: &'static str,
        /// Asset key.
        key: String,
        /// What is wrong.
        message: String,
    },
    /// The wire encoder failed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// Result alias for the asset packet encoders.
pub type Result<T> = std::result::Result<T, AssetPacketError>;

/// One encoded, unframed asset packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetPacket {
    /// Store name that produced the packet.
    pub store: &'static str,
    /// Packet registry id.
    pub id: u32,
    /// Uncompressed, unframed wire payload.
    pub payload: Vec<u8>,
}

impl AssetPacket {
    fn encode<T: Packet + Wire>(store: &'static str, packet: &T) -> Result<Self> {
        let mut writer = Writer::new();
        packet.encode(&mut writer)?;
        Ok(Self {
            store,
            id: T::ID,
            payload: writer.into_inner(),
        })
    }
}

impl AssetRegistry {
    /// `UpdatePhysicalMaterials` (id 87) from the P0 `PhysicalMaterial` store.
    pub fn physical_material_packet(&self) -> Result<AssetPacket> {
        let mut materials = BTreeMap::new();
        for record in self.physical_materials.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "PhysicalMaterial",
                key: record.key.clone(),
            })?;
            let object = record
                .value
                .as_object()
                .ok_or_else(|| invalid("PhysicalMaterial", &record.key, "expected an object"))?;
            let field = |name: &str| -> f32 {
                object.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32
            };
            materials.insert(
                id,
                PhysicalMaterial {
                    id: Some(record.key.clone()),
                    reflection_coeff: field("ReflectionCoeff"),
                    attenuation_per_block: field("AttenuationPerBlock"),
                    h_f_attenuation_per_block: field("HFAttenuationPerBlock"),
                    shelter_opacity: field("ShelterOpacity"),
                },
            );
        }
        let packet = UpdatePhysicalMaterials {
            r#type: UpdateType::Init,
            max_id: next_id(&self.physical_materials, "PhysicalMaterial")?,
            physical_materials: Some(materials),
        };
        AssetPacket::encode("PhysicalMaterial", &packet)
    }

    /// `UpdateBlockHitboxes` (id 41) from the P0 `BlockBoundingBoxes` store.
    pub fn block_hitbox_packet(&self) -> Result<AssetPacket> {
        let mut hitboxes = BTreeMap::new();
        for record in self.block_bounding_boxes.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "BlockBoundingBoxes",
                key: record.key.clone(),
            })?;
            let boxes = record
                .value
                .get("Boxes")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    invalid(
                        "BlockBoundingBoxes",
                        &record.key,
                        "missing the `Boxes` array",
                    )
                })?;
            let mut boxes_out = Vec::with_capacity(boxes.len());
            for entry in boxes {
                // A missing `Min`/`Max` vector decodes as Java's zero vector.
                let min = entry.get("Min").cloned().unwrap_or(Value::Null);
                let max = entry.get("Max").cloned().unwrap_or(Value::Null);
                // Java's `Vector3d` starts at (0, 0, 0); a missing axis is 0.
                let component = |vector: &Value, name: &str| -> f32 {
                    vector.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32
                };
                boxes_out.push(Hitbox {
                    min_x: component(&min, "X"),
                    min_y: component(&min, "Y"),
                    min_z: component(&min, "Z"),
                    max_x: component(&max, "X"),
                    max_y: component(&max, "Y"),
                    max_z: component(&max, "Z"),
                });
            }
            hitboxes.insert(id, boxes_out);
        }
        let packet = UpdateBlockHitboxes {
            r#type: UpdateType::Init,
            max_id: next_id(&self.block_bounding_boxes, "BlockBoundingBoxes")?,
            block_base_hitboxes: Some(hitboxes),
        };
        AssetPacket::encode("BlockBoundingBoxes", &packet)
    }
}

fn next_id<T>(collection: &AssetCollection<T>, store: &'static str) -> Result<i32> {
    let mut max = -1;
    for record in collection.iter() {
        let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
            store,
            key: record.key.clone(),
        })?;
        max = max.max(id);
    }
    Ok(max + 1)
}

fn invalid(store: &'static str, key: &str, message: impl Into<String>) -> AssetPacketError {
    AssetPacketError::InvalidAsset {
        store,
        key: key.to_string(),
        message: message.into(),
    }
}
