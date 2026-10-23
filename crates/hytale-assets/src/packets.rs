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

use std::collections::{BTreeMap, BTreeSet};

use hytale_protocol::generated::assets::{
    UpdateBlockHitboxes, UpdateEnvironments, UpdateFluids, UpdatePhysicalMaterials,
};
use hytale_protocol::generated::common::{
    BlockTextures, Color, ColorLight, Direction, EntityPart, Fluid, FluidDrawType, Hitbox,
    ModelParticle, Opacity, PhysicalMaterial, ShaderType, UpdateType, WorldEnvironment,
};
use hytale_protocol::generated::wire::{Packet, Vector3f, Wire};
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

/// A deterministic, Rust-owned tag index registry.
///
/// Java's global tag indexes come from the order the full 54-store asset graph
/// initializes. The Rust snapshot owns its block/item ids and its tag indexes
/// for the same reason: the client only needs the `ServerTags` map and every
/// asset packet to agree. Indexes are assigned alphabetically over every tag,
/// value and `key=value` composite found in the P0 documents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagRegistry {
    indexes: BTreeMap<String, i32>,
}

impl TagRegistry {
    /// Collect tags from every P0 asset document.
    pub fn from_assets(registry: &AssetRegistry) -> Self {
        let mut names = BTreeSet::new();
        for record in registry.environments.iter() {
            collect_tags(&record.value, &mut names);
        }
        for record in registry.fluids.iter() {
            collect_tags(&record.value.json, &mut names);
        }
        for record in registry.block_types.iter() {
            collect_tags(&record.value.json, &mut names);
        }
        for record in registry.items.iter() {
            collect_tags(&record.value, &mut names);
        }
        let indexes = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| (name, index as i32))
            .collect();
        Self { indexes }
    }

    /// Look up a tag index.
    pub fn index(&self, tag: &str) -> Option<i32> {
        self.indexes.get(tag).copied()
    }

    /// The `ServerTags` packet payload.
    pub fn map(&self) -> BTreeMap<String, i32> {
        self.indexes.clone()
    }

    /// Expand an asset's `Tags` object into its tag index set.
    pub fn expanded(&self, tags: Option<&Value>) -> Option<Vec<i32>> {
        let tags = tags?.as_object()?;
        let mut found = BTreeSet::new();
        for (tag, values) in tags {
            if let Some(index) = self.index(tag) {
                found.insert(index);
            }
            if let Some(values) = values.as_array() {
                for value in values.iter().filter_map(Value::as_str) {
                    if let Some(index) = self.index(value) {
                        found.insert(index);
                    }
                    if let Some(index) = self.index(&format!("{tag}={value}")) {
                        found.insert(index);
                    }
                }
            }
        }
        (!found.is_empty()).then(|| found.into_iter().collect())
    }
}

fn collect_tags(value: &Value, names: &mut BTreeSet<String>) {
    let Some(tags) = value.get("Tags").and_then(Value::as_object) else {
        return;
    };
    for (tag, values) in tags {
        names.insert(tag.clone());
        if let Some(values) = values.as_array() {
            for value in values.iter().filter_map(Value::as_str) {
                names.insert(value.to_string());
                names.insert(format!("{tag}={value}"));
            }
        }
    }
}

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
    /// The config-asset packets and the matching `ServerTags` map.
    ///
    /// Java sends one init packet per store with a packet generator. This port
    /// currently encodes `BlockBoundingBoxes`, `PhysicalMaterial`, `Fluid` and
    /// `Environment`; `BlockType`/`Item` remain follow-ups. Tag indexes are
    /// owned by the Rust snapshot, so every packet and `ServerTags` agree.
    pub fn config_assets(&self) -> Result<ConfigAssets> {
        let tags = TagRegistry::from_assets(self);
        Ok(ConfigAssets {
            packets: vec![
                self.block_hitbox_packet()?,
                self.physical_material_packet()?,
                self.fluid_packet(&tags)?,
                self.environment_packet(&tags)?,
            ],
            tags: tags.map(),
        })
    }

    /// The config-asset packets without the tag map.
    pub fn config_packets(&self) -> Result<Vec<AssetPacket>> {
        Ok(self.config_assets()?.packets)
    }

    /// `UpdateFluids` (id 83) from the P0 `Fluid` store.
    pub fn fluid_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        let mut fluids = BTreeMap::new();
        for record in self.fluids.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "Fluid",
                key: record.key.clone(),
            })?;
            let json = &record.value.json;
            let textures = block_textures(json.get("Textures"), UNKNOWN_TEXTURE)?;
            let particles = model_particles(json.get("Particles"))?;
            let shader = shader_effects(json.get("Effect"))?;
            fluids.insert(
                id,
                Fluid {
                    id: Some(record.key.clone()),
                    max_fluid_level: i32::from(record.value.max_fluid_level),
                    cube_textures: Some(textures),
                    requires_alpha_blending: json
                        .get("RequiresAlphaBlending")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    opacity: opacity(json.get("Opacity"))?,
                    shader_effect: Some(shader),
                    light: color_light(json.get("Light")),
                    particles: (!particles.is_empty()).then_some(particles),
                    draw_type: fluid_draw_type(json.get("DrawType"))?,
                    // FluidFX/BlockSoundSet are non-P0 stores; the client does
                    // not receive their packets yet.
                    fluid_fx_index: 0,
                    block_sound_set_index: 0,
                    block_particle_set_id: json
                        .get("BlockParticleSetId")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    particle_color: color(json.get("ParticleColor")),
                    tag_indexes: tags.expanded(json.get("Tags")),
                },
            );
        }
        let packet = UpdateFluids {
            r#type: UpdateType::Init,
            max_id: next_id(&self.fluids, "Fluid")?,
            fluids: Some(fluids),
        };
        AssetPacket::encode("Fluid", &packet)
    }

    /// `UpdateEnvironments` (id 61) from the P0 `Environment` store.
    pub fn environment_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        let mut environments = BTreeMap::new();
        for record in self.environments.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "Environment",
                key: record.key.clone(),
            })?;
            environments.insert(
                id,
                WorldEnvironment {
                    id: Some(record.key.clone()),
                    water_tint: color(record.value.get("WaterTint")),
                    // The P0 pack has no fluid-particle environments, and the
                    // FluidFX store is not loaded yet.
                    fluid_particles: None,
                    tag_indexes: tags.expanded(record.value.get("Tags")),
                },
            );
        }
        let packet = UpdateEnvironments {
            r#type: UpdateType::Init,
            max_id: next_id(&self.environments, "Environment")?,
            environments: Some(environments),
            rebuild_map_geometry: true,
        };
        AssetPacket::encode("Environment", &packet)
    }

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

/// The config packets plus the tag map they were encoded with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAssets {
    /// Encoded init packets.
    pub packets: Vec<AssetPacket>,
    /// `ServerTags` map.
    pub tags: BTreeMap<String, i32>,
}

const UNKNOWN_TEXTURE: &str = "BlockTextures/Unknown.png";

fn color(value: Option<&Value>) -> Option<Color> {
    let text = value?.as_str()?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    let digits: Vec<char> = hex.chars().collect();
    let (red, green, blue) = match digits.len() {
        3 => (
            format!("{}{}", digits[0], digits[0]),
            format!("{}{}", digits[1], digits[1]),
            format!("{}{}", digits[2], digits[2]),
        ),
        6 | 8 => (
            digits[digits.len() - 6..digits.len() - 4].iter().collect(),
            digits[digits.len() - 4..digits.len() - 2].iter().collect(),
            digits[digits.len() - 2..].iter().collect(),
        ),
        _ => return None,
    };
    Some(Color {
        red: u8::from_str_radix(&red, 16).ok()? as i8,
        green: u8::from_str_radix(&green, 16).ok()? as i8,
        blue: u8::from_str_radix(&blue, 16).ok()? as i8,
    })
}

fn color_light(value: Option<&Value>) -> Option<ColorLight> {
    let object = value?.as_object()?;
    let text = object.get("Color")?.as_str()?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    let (red, green, blue) = if hex.len() == 3 {
        let mut digits = hex.chars();
        (
            digits.next()?.to_digit(16)? as u8,
            digits.next()?.to_digit(16)? as u8,
            digits.next()?.to_digit(16)? as u8,
        )
    } else if hex.len() == 6 {
        (
            u8::from_str_radix(&hex[0..2], 16).ok()? / 17,
            u8::from_str_radix(&hex[2..4], 16).ok()? / 17,
            u8::from_str_radix(&hex[4..6], 16).ok()? / 17,
        )
    } else {
        return None;
    };
    Some(ColorLight {
        radius: object.get("Radius").and_then(Value::as_i64).unwrap_or(0) as i8,
        red: red as i8,
        green: green as i8,
        blue: blue as i8,
    })
}

fn opacity(value: Option<&Value>) -> Result<Opacity> {
    Ok(match value.and_then(Value::as_str) {
        None | Some("Solid") => Opacity::Solid,
        Some("Semitransparent") => Opacity::Semitransparent,
        Some("Cutout") => Opacity::Cutout,
        Some("Transparent") => Opacity::Transparent,
        Some(other) => return Err(invalid("Fluid", other, "unknown Opacity")),
    })
}

fn fluid_draw_type(value: Option<&Value>) -> Result<FluidDrawType> {
    Ok(match value.and_then(Value::as_str) {
        None | Some("Liquid") => FluidDrawType::Liquid,
        Some("None") => FluidDrawType::None,
        Some(other) => return Err(invalid("Fluid", other, "unknown DrawType")),
    })
}

fn shader_effects(value: Option<&Value>) -> Result<Vec<ShaderType>> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(vec![ShaderType::None]);
    };
    let mut shader = Vec::with_capacity(values.len());
    for value in values {
        let name = value
            .as_str()
            .ok_or_else(|| invalid("Fluid", "Effect", "shader names must be strings"))?;
        shader.push(match name {
            "None" => ShaderType::None,
            "Wind" => ShaderType::Wind,
            "WindAttached" => ShaderType::WindAttached,
            "WindRandom" => ShaderType::WindRandom,
            "WindFractal" => ShaderType::WindFractal,
            "Ice" => ShaderType::Ice,
            "Water" => ShaderType::Water,
            "Lava" => ShaderType::Lava,
            "Slime" => ShaderType::Slime,
            "Ripple" => ShaderType::Ripple,
            other => return Err(invalid("Fluid", other, "unknown shader effect")),
        });
    }
    if shader.is_empty() {
        shader.push(ShaderType::None);
    }
    Ok(shader)
}

fn block_textures(value: Option<&Value>, fallback: &str) -> Result<Vec<BlockTextures>> {
    let entries: Vec<&Value> = match value {
        Some(Value::Array(values)) => values.iter().collect(),
        Some(Value::Object(_)) => vec![value.unwrap()],
        _ => {
            return Ok(vec![BlockTextures {
                top: Some(fallback.to_string()),
                bottom: Some(fallback.to_string()),
                front: Some(fallback.to_string()),
                back: Some(fallback.to_string()),
                left: Some(fallback.to_string()),
                right: Some(fallback.to_string()),
                weight: 1.0,
            }])
        }
    };
    let weights: Vec<f64> = entries
        .iter()
        .map(|entry| entry.get("Weight").and_then(Value::as_f64).unwrap_or(1.0))
        .collect();
    let total: f64 = weights.iter().sum();
    let mut textures = Vec::with_capacity(entries.len());
    for (entry, weight) in entries.iter().zip(weights) {
        let all = entry.get("All").and_then(Value::as_str);
        let sides = entry.get("Sides").and_then(Value::as_str);
        let up_down = entry.get("UpDown").and_then(Value::as_str);
        let field = |name: &str, fallback: Option<&str>| {
            entry
                .get(name)
                .and_then(Value::as_str)
                .or(fallback)
                .map(ToOwned::to_owned)
        };
        let horizontal = sides.or(all);
        let north = field("North", horizontal);
        let south = field("South", horizontal);
        let west = field("West", horizontal);
        let east = field("East", horizontal);
        textures.push(BlockTextures {
            top: field("Up", up_down.or(all)),
            bottom: field("Down", up_down.or(all)),
            front: south,
            back: north,
            left: west,
            right: east,
            weight: if total > 0.0 {
                (weight / total) as f32
            } else {
                weight as f32
            },
        });
    }
    Ok(textures)
}

fn model_particles(value: Option<&Value>) -> Result<Vec<ModelParticle>> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut particles = Vec::with_capacity(values.len());
    for value in values {
        let target = value
            .get("TargetEntityPart")
            .and_then(Value::as_str)
            .unwrap_or("Self");
        let target_entity_part = match target {
            "Self" => EntityPart::Self_,
            "Entity" => EntityPart::Entity,
            "PrimaryItem" => EntityPart::PrimaryItem,
            "SecondaryItem" => EntityPart::SecondaryItem,
            other => return Err(invalid("Fluid", other, "unknown TargetEntityPart")),
        };
        let offset = value.get("PositionOffset").and_then(Value::as_object);
        let position_offset = offset.map(|offset| Vector3f {
            x: offset.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            y: offset.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            z: offset.get("Z").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        });
        let rotation = value.get("RotationOffset").and_then(Value::as_object);
        let rotation_offset = rotation.map(|rotation| Direction {
            yaw: rotation.get("Yaw").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            pitch: rotation.get("Pitch").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            roll: rotation.get("Roll").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        });
        particles.push(ModelParticle {
            system_id: value
                .get("SystemId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            scale: value.get("Scale").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            color: color(value.get("Color")),
            target_entity_part,
            target_node_name: value
                .get("TargetNodeName")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            position_offset,
            rotation_offset,
            detached_from_model: value
                .get("DetachedFromModel")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            clear_particles_on_remove: value
                .get("ClearParticlesOnRemove")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(particles)
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
