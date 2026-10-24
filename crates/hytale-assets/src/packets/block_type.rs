//! `UpdateBlockTypes` (id 40) encoder.
//!
//! Implemented from the Rust registry snapshot. Numeric block ids are
//! Rust-owned; every other id reference is resolved through the Rust maps.
//!
//! The large nested trees (`states`, `interactions`, `bench`,
//! `connectedBlockRuleSet`, `placementSettings`, `conditionalSounds`, `display`)
//! are intentionally not emitted yet; see the T33 task's Deviations section.

use std::collections::{BTreeMap, BTreeSet};

use hytale_protocol::generated::assets::UpdateBlockTypes;
use hytale_protocol::generated::common::{
    BlockBreaking, BlockFaceSupport, BlockGathering, BlockMaterial, BlockMovementSettings,
    BlockNeighbor, BlockSupportsRequiredForType, BlockTextures, BlockType, DrawType, Harvesting,
    ModelTexture, RailConfig, RailPoint, RandomRotation, RequiredBlockFaceSupport, Rotation,
    ShadingMode, SoftBlock, SupportMatch, Tint, UpdateType, VariantRotation, Vector3i,
};
use hytale_protocol::generated::wire::Vector3f;
use serde_json::Value;

use crate::packets::{
    block_textures, color, color_light, invalid, model_particles, next_id, shader_effects,
    AssetPacket, AssetPacketError, Result, TagRegistry, UNKNOWN_TEXTURE,
};
use crate::registry::{AssetRegistry, Face};

/// Encode the `UpdateBlockTypes` init packet from the registry snapshot.
pub(crate) fn block_type_packet(
    registry: &AssetRegistry,
    tags: &TagRegistry,
) -> Result<AssetPacket> {
    // Java assigns group ids in load order and returns 0 for an unknown group.
    // The Rust snapshot owns its ids, so use a deterministic sorted index.
    let groups_by_name: BTreeMap<String, i32> = collect_groups(registry)
        .into_iter()
        .enumerate()
        .map(|(index, group)| (group, index as i32))
        .collect();
    // Inline `DropList` objects become Java contained assets with a synthetic
    // key. Parents are committed before their state blocks, so the first block
    // that contains an object owns its key; inherited state blocks reuse it.
    let drop_list_ids = collect_drop_list_ids(registry);

    let mut block_types = BTreeMap::new();
    for record in registry.block_types.iter() {
        let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
            store: "BlockType",
            key: record.key.clone(),
        })?;
        let json = &record.value.json;
        let key = &record.key;

        let hitbox = json
            .get("HitboxType")
            .and_then(Value::as_str)
            .unwrap_or("Full");
        let hitbox = registry.block_bounding_boxes.id_of(hitbox).unwrap_or(0);
        let interaction_hitbox = match json.get("InteractionHitboxType").and_then(Value::as_str) {
            Some(name) => registry.block_bounding_boxes.id_of(name).unwrap_or(hitbox),
            None => i32::MIN,
        };

        let gathering = parse_gathering(json.get("Gathering"), key, &drop_list_ids);
        let interaction_hint = json
            .get("InteractionHint")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                let benched = json.get("Bench").is_some_and(|value| !value.is_null());
                let door = json.get("IsDoor").and_then(Value::as_bool).unwrap_or(false);
                let harvestable = gathering
                    .as_ref()
                    .is_some_and(|gathering| gathering.harvest.is_some());
                let seated = json
                    .get("Seats")
                    .and_then(Value::as_array)
                    .is_some_and(|seats| !seats.is_empty());
                let hint = if benched || door {
                    "server.interactionHints.open"
                } else if harvestable {
                    "server.interactionHints.gather"
                } else if seated {
                    "server.interactionHints.sit"
                } else {
                    "server.interactionHints.generic"
                };
                hint.to_string()
            });

        let material = record.value.material;
        let group = json.get("Group").and_then(Value::as_str);
        let support = match json.get("Support") {
            Some(value) if !value.is_null() => Some(parse_support(value, key, registry, tags)?),
            _ if material == BlockMaterial::Empty && group != Some("@Tech") => {
                // Java `BlockType.processConfig` default: plants require support
                // from below unless explicitly configured.
                let mut support = BTreeMap::new();
                support.insert(BlockNeighbor::Down, vec![default_required_support()]);
                Some(support)
            }
            _ => None,
        };

        let supporting = match json.get("Supporting") {
            Some(value) if !value.is_null() => Some(parse_supporting(value, key)?),
            _ if record.value.material == BlockMaterial::Solid
                && matches!(
                    record.value.draw_type,
                    DrawType::Cube | DrawType::CubeWithModel | DrawType::GizmoCube
                ) =>
            {
                // Java `BlockType.processConfig`: a solid cube supports every
                // neighbouring face with a single `Full` entry.
                let all = vec![BlockFaceSupport {
                    face_type: Some("Full".to_string()),
                    filler: None,
                }];
                let mut supporting = BTreeMap::new();
                for face in Face::expand("All").unwrap_or_default() {
                    supporting.insert(face.neighbor(), all.clone());
                }
                Some(supporting)
            }
            _ => Some(BTreeMap::new()),
        };

        let tint = Some(tint(json));
        let biome_tint = Some(biome_tint(json));

        let transition_to_groups = json
            .get("TransitionToGroups")
            .and_then(Value::as_array)
            .filter(|groups| !groups.is_empty())
            .map(|groups| {
                groups
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|group| groups_index(&groups_by_name, group))
                    .collect()
            });

        block_types.insert(
            id,
            BlockType {
                item: record.value.container_item.clone(),
                name: Some(key.clone()),
                unknown: key.eq_ignore_ascii_case("Unknown"),
                draw_type: record.value.draw_type,
                material: record.value.material,
                opacity: record.value.opacity,
                shader_effect: Some(shader_effects(json.get("Effect"))?),
                hitbox,
                interaction_hitbox,
                model: opt_string(json, "CustomModel"),
                model_texture: Some(model_textures(json.get("CustomModelTexture"))),
                model_scale: f32_field(json, "CustomModelScale", 1.0),
                model_animation: opt_string(json, "CustomModelAnimation"),
                model_animation_speed: f32_field(json, "CustomModelAnimationSpeed", 1.0),
                looping: json
                    .get("Looping")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                max_support_distance: i32_field(json, "MaxSupportDistance", 0),
                block_supports_required_for: supports_required_for(json.get("SupportsRequiredFor")),
                support,
                supporting,
                requires_alpha_blending: json
                    .get("RequiresAlphaBlending")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                cube_textures: Some(cube_textures(json.get("Textures"))?),
                cube_side_mask_texture: opt_string(json, "TextureSideMask"),
                cube_shading_mode: shading_mode(json.get("CubeShadingMode")),
                random_rotation: random_rotation(json.get("RandomRotation")),
                variant_rotation: variant_rotation(json.get("VariantRotation")),
                rotation_yaw_placement_offset: rotation(json.get("RotationYawPlacementOffset")),
                // Java `BlockType.processConfig`: `EMPTY` (the field default) is
                // index 0 and an unknown set falls back to the empty set.
                block_sound_set_index: json
                    .get("BlockSoundSetId")
                    .and_then(Value::as_str)
                    .filter(|id| !id.eq_ignore_ascii_case("EMPTY"))
                    .and_then(|id| registry.block_sound_sets.id_of(id))
                    .unwrap_or(0),
                physical_material_index: registry
                    .physical_materials
                    .id_of(
                        json.get("PhysicalMaterialId")
                            .and_then(Value::as_str)
                            .unwrap_or("EMPTY"),
                    )
                    .unwrap_or(0),
                sound_occlusion_opacity: f32_field(json, "SoundOcclusionOpacity", 1.0),
                // The AmbientSoundEvent store is not streamed yet.
                ambient_sound_event_index: 0,
                // `conditionalSounds` is deferred (see T33).
                conditional_sounds: None,
                particles: {
                    let particles = model_particles(json.get("Particles"))?;
                    (!particles.is_empty()).then_some(particles)
                },
                block_particle_set_id: opt_string(json, "BlockParticleSetId"),
                block_breaking_decal_id: opt_string(json, "BlockBreakingDecalId"),
                particle_color: color(json.get("ParticleColor")),
                texture_computed_color: color(json.get("TextureComputedColor")),
                light: color_light(json.get("Light")),
                tint,
                biome_tint,
                group: group.map_or(0, |group| groups_index(&groups_by_name, group)),
                transition_texture: opt_string(json, "TransitionTexture"),
                transition_to_groups,
                movement_settings: movement_settings(json.get("MovementSettings")),
                interaction_hint: Some(interaction_hint),
                gathering,
                // Deferred stores / trees.
                placement_settings: None,
                display: None,
                rail: rail(json.get("Rail")),
                ignore_support_when_placed: json
                    .get("IgnoreSupportWhenPlaced")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                interactions: None,
                states: None,
                transition_to_tag: json
                    .get("TransitionToTag")
                    .and_then(Value::as_str)
                    .and_then(|tag| tags.index(tag))
                    .unwrap_or(i32::MIN),
                tag_indexes: tags.expanded(json.get("Tags")),
                bench: None,
                connected_block_rule_set: None,
            },
        );
    }

    let packet = UpdateBlockTypes {
        r#type: UpdateType::Init,
        max_id: next_id(&registry.block_types, "BlockType")?,
        block_types: Some(block_types),
        update_block_textures: true,
        update_model_textures: true,
        update_models: true,
        update_map_geometry: true,
    };
    AssetPacket::encode("BlockType", &packet)
}

// ---------------------------------------------------------------------------
// Group registry
// ---------------------------------------------------------------------------

fn collect_groups(registry: &AssetRegistry) -> BTreeSet<String> {
    let mut groups = BTreeSet::new();
    for record in registry.block_types.iter() {
        let json = &record.value.json;
        if let Some(group) = json.get("Group").and_then(Value::as_str) {
            groups.insert(group.to_string());
        }
        if let Some(values) = json.get("TransitionToGroups").and_then(Value::as_array) {
            for group in values.iter().filter_map(Value::as_str) {
                groups.insert(group.to_string());
            }
        }
    }
    groups
}

fn groups_index(groups: &BTreeMap<String, i32>, group: &str) -> i32 {
    groups.get(group).copied().unwrap_or(0)
}

/// Map an inline `DropList` object (by content) to the Java-style contained
/// asset key generated for the block that first declares it.
fn collect_drop_list_ids(registry: &AssetRegistry) -> BTreeMap<String, String> {
    const PATHS: [(&str, &str); 3] = [
        ("Breaking", "Gathering_Breaking_DropList"),
        ("Harvest", "Gathering_Harvest_DropList"),
        ("Soft", "Gathering_Soft_DropList"),
    ];
    let mut ids = BTreeMap::new();
    for record in registry.block_types.iter() {
        let Some(gathering) = record.value.json.get("Gathering") else {
            continue;
        };
        for (kind, path) in PATHS {
            let Some(drop_list) = gathering
                .get(kind)
                .and_then(|entry| entry.get("DropList"))
                .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
            else {
                continue;
            };
            ids.entry(drop_list.to_string())
                .or_insert_with(|| format!("*{}_{path}", record.key));
        }
    }
    ids
}

fn drop_list_id(
    entry: &Value,
    owner: &str,
    path: &str,
    drop_list_ids: &BTreeMap<String, String>,
) -> Option<String> {
    match entry.get("DropList") {
        Some(Value::String(id)) => Some(id.clone()),
        Some(value @ Value::Object(_)) => Some(
            drop_list_ids
                .get(&value.to_string())
                .cloned()
                .unwrap_or_else(|| format!("*{owner}_{path}")),
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Field helpers
// ---------------------------------------------------------------------------

fn opt_string(json: &Value, name: &str) -> Option<String> {
    json.get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn f32_field(json: &Value, name: &str, default: f32) -> f32 {
    json.get(name)
        .and_then(Value::as_f64)
        .map_or(default, |value| value as f32)
}

fn i32_field(json: &Value, name: &str, default: i32) -> i32 {
    json.get(name)
        .and_then(Value::as_i64)
        .map_or(default, |value| value as i32)
}

fn supports_required_for(value: Option<&Value>) -> BlockSupportsRequiredForType {
    match value.and_then(Value::as_str) {
        Some("Any") => BlockSupportsRequiredForType::Any,
        _ => BlockSupportsRequiredForType::All,
    }
}

fn shading_mode(value: Option<&Value>) -> ShadingMode {
    match value.and_then(Value::as_str) {
        Some("Flat") => ShadingMode::Flat,
        Some("Fullbright") => ShadingMode::Fullbright,
        Some("Reflective") => ShadingMode::Reflective,
        _ => ShadingMode::Standard,
    }
}

fn random_rotation(value: Option<&Value>) -> RandomRotation {
    match value.and_then(Value::as_str) {
        Some("YawPitchRollStep1") => RandomRotation::YawPitchRollStep1,
        Some("YawStep1") => RandomRotation::YawStep1,
        Some("YawStep1XZ") => RandomRotation::YawStep1XZ,
        Some("YawStep90") => RandomRotation::YawStep90,
        _ => RandomRotation::None,
    }
}

fn variant_rotation(value: Option<&Value>) -> VariantRotation {
    match value.and_then(Value::as_str) {
        Some("Wall") => VariantRotation::Wall,
        Some("UpDown") => VariantRotation::UpDown,
        Some("Pipe") => VariantRotation::Pipe,
        Some("DoublePipe") => VariantRotation::DoublePipe,
        Some("NESW") => VariantRotation::NESW,
        Some("UpDownNESW") => VariantRotation::UpDownNESW,
        Some("All") => VariantRotation::All,
        _ => VariantRotation::None,
    }
}

fn rotation(value: Option<&Value>) -> Rotation {
    match value.and_then(Value::as_str) {
        Some("Ninety") => Rotation::Ninety,
        Some("OneEighty") => Rotation::OneEighty,
        Some("TwoSeventy") => Rotation::TwoSeventy,
        _ => Rotation::None,
    }
}

/// Java `ColorParseUtil.colorToARGBInt` on the first color of a JSON array.
fn tint_value(value: Option<&Value>) -> i32 {
    let element = match value {
        Some(Value::Array(values)) => values.first(),
        value => value,
    };
    let Some(parsed) = color(element) else {
        return -1;
    };
    let red = u32::from(parsed.red as u8);
    let green = u32::from(parsed.green as u8);
    let blue = u32::from(parsed.blue as u8);
    (0xFF00_0000 | (red << 16) | (green << 8) | blue) as i32
}

/// Java decodes the `Tint` and per-face `Tint*` keys in document order, so the
/// last matching key wins (the merged parent/child object order mirrors Java's
/// inherited-field decode order).
fn tint(json: &Value) -> Tint {
    let mut tint = Tint {
        top: -1,
        bottom: -1,
        front: -1,
        back: -1,
        left: -1,
        right: -1,
    };
    let Some(object) = json.as_object() else {
        return tint;
    };
    for (name, value) in object {
        match name.as_str() {
            "Tint" => {
                let channel = tint_value(Some(value));
                tint = Tint {
                    top: channel,
                    bottom: channel,
                    front: channel,
                    back: channel,
                    left: channel,
                    right: channel,
                };
            }
            "TintUp" => tint.top = tint_value(Some(value)),
            "TintDown" => tint.bottom = tint_value(Some(value)),
            "TintSouth" => tint.front = tint_value(Some(value)),
            "TintNorth" => tint.back = tint_value(Some(value)),
            "TintWest" => tint.left = tint_value(Some(value)),
            "TintEast" => tint.right = tint_value(Some(value)),
            _ => {}
        }
    }
    tint
}

/// Java `BiomeTint`/`BiomeTint*` decode order, mirroring [`tint`].
fn biome_tint(json: &Value) -> Tint {
    let mut tint = Tint::default();
    let Some(object) = json.as_object() else {
        return tint;
    };
    for (name, value) in object {
        match name.as_str() {
            "BiomeTint" => {
                let channel = value.as_i64().unwrap_or(0) as i32;
                tint = Tint {
                    top: channel,
                    bottom: channel,
                    front: channel,
                    back: channel,
                    left: channel,
                    right: channel,
                };
            }
            "BiomeTintUp" => tint.top = value.as_i64().unwrap_or(0) as i32,
            "BiomeTintDown" => tint.bottom = value.as_i64().unwrap_or(0) as i32,
            "BiomeTintSouth" => tint.front = value.as_i64().unwrap_or(0) as i32,
            "BiomeTintNorth" => tint.back = value.as_i64().unwrap_or(0) as i32,
            "BiomeTintWest" => tint.left = value.as_i64().unwrap_or(0) as i32,
            "BiomeTintEast" => tint.right = value.as_i64().unwrap_or(0) as i32,
            _ => {}
        }
    }
    tint
}

// ---------------------------------------------------------------------------
// Model textures / movement / gathering / rail
// ---------------------------------------------------------------------------

/// Java `BlockTypeTextures` defaults every unspecified side to the unknown
/// texture; the shared helper only supplies the fallback for a missing array.
fn cube_textures(value: Option<&Value>) -> Result<Vec<BlockTextures>> {
    let mut textures = block_textures(value, UNKNOWN_TEXTURE)?;
    for texture in &mut textures {
        texture
            .top
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
        texture
            .bottom
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
        texture
            .front
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
        texture
            .back
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
        texture
            .left
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
        texture
            .right
            .get_or_insert_with(|| UNKNOWN_TEXTURE.to_string());
    }
    Ok(textures)
}

fn model_textures(value: Option<&Value>) -> Vec<ModelTexture> {
    let Some(entries) = value.and_then(Value::as_array).filter(|e| !e.is_empty()) else {
        return vec![ModelTexture {
            texture: Some(UNKNOWN_TEXTURE.to_string()),
            weight: 1.0,
        }];
    };
    let total: f32 = entries
        .iter()
        .map(|entry| entry.get("Weight").and_then(Value::as_i64).unwrap_or(0) as f32)
        .sum();
    entries
        .iter()
        .map(|entry| {
            let weight = entry.get("Weight").and_then(Value::as_i64).unwrap_or(0) as f32;
            ModelTexture {
                texture: opt_string(entry, "Texture"),
                weight: if total > 0.0 { weight / total } else { 0.0 },
            }
        })
        .collect()
}

fn movement_settings(value: Option<&Value>) -> BlockMovementSettings {
    let json = value.unwrap_or(&Value::Null);
    BlockMovementSettings {
        is_climbable: json
            .get("IsClimbable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        climb_up_speed_multiplier: f32_field(json, "ClimbUpSpeedMultiplier", 1.0),
        climb_down_speed_multiplier: f32_field(json, "ClimbDownSpeedMultiplier", 1.0),
        climb_lateral_speed_multiplier: f32_field(json, "ClimbLateralSpeedMultiplier", 1.0),
        is_bouncy: json
            .get("IsBouncy")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        bounce_velocity: f32_field(json, "BounceVelocity", 0.0),
        drag: f32_field(json, "Drag", 0.82),
        friction: f32_field(json, "Friction", 0.18),
        terminal_velocity_modifier: f32_field(json, "TerminalVelocityModifier", 1.0),
        horizontal_speed_multiplier: f32_field(json, "HorizontalSpeedMultiplier", 1.0),
        // The Java codec has no `Acceleration` field.
        acceleration: 0.0,
        jump_force_multiplier: f32_field(json, "JumpForceMultiplier", 1.0),
        disable_auto_step: json
            .get("DisableAutoStep")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_gathering(
    value: Option<&Value>,
    owner: &str,
    drop_list_ids: &BTreeMap<String, String>,
) -> Option<BlockGathering> {
    let json = value?.as_object()?;
    let breaking = json.get("Breaking").map(|breaking| BlockBreaking {
        gather_type: opt_string(breaking, "GatherType"),
        health: f32_field(breaking, "Health", 0.0),
        quantity: i32_field(breaking, "Quantity", 1),
        quality: i32_field(breaking, "Quality", 0),
        item_id: opt_string(breaking, "ItemId"),
        drop_list_id: drop_list_id(
            breaking,
            owner,
            "Gathering_Breaking_DropList",
            drop_list_ids,
        ),
    });
    let harvest = json.get("Harvest").map(|harvest| Harvesting {
        item_id: opt_string(harvest, "ItemId"),
        drop_list_id: drop_list_id(harvest, owner, "Gathering_Harvest_DropList", drop_list_ids),
    });
    let soft = json.get("Soft").map(|soft| SoftBlock {
        item_id: opt_string(soft, "ItemId"),
        drop_list_id: drop_list_id(soft, owner, "Gathering_Soft_DropList", drop_list_ids),
        is_weapon_breakable: soft
            .get("IsWeaponBreakable")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    });
    Some(BlockGathering {
        breaking,
        harvest,
        soft,
    })
}

fn rail(value: Option<&Value>) -> Option<RailConfig> {
    let json = value?;
    let points = json.get("Points").and_then(Value::as_array).map(|points| {
        points
            .iter()
            .map(|point| RailPoint {
                point: vector3f(point.get("Point")),
                // Java `RailPoint.afterDecode` normalizes the normal.
                normal: normalize(vector3f(point.get("Normal"))),
            })
            .collect()
    });
    Some(RailConfig { points })
}

fn vector3f(value: Option<&Value>) -> Vector3f {
    Vector3f {
        x: value
            .and_then(|value| value.get("X"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        y: value
            .and_then(|value| value.get("Y"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        z: value
            .and_then(|value| value.get("Z"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
    }
}

fn normalize(value: Vector3f) -> Vector3f {
    let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
    if length > 0.0 && length.is_finite() {
        Vector3f {
            x: value.x / length,
            y: value.y / length,
            z: value.z / length,
        }
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// Support / supporting
// ---------------------------------------------------------------------------

fn default_required_support() -> RequiredBlockFaceSupport {
    RequiredBlockFaceSupport {
        face_type: Some("Full".to_string()),
        self_face_type: None,
        block_set_id: None,
        block_type_id: -1,
        tag_index: i32::MIN,
        fluid_id: -1,
        support: SupportMatch::Required,
        match_self: SupportMatch::Ignored,
        allow_support_propagation: true,
        rotate: true,
        filler: None,
    }
}

fn parse_support(
    value: &Value,
    key: &str,
    registry: &AssetRegistry,
    tags: &TagRegistry,
) -> Result<BTreeMap<BlockNeighbor, Vec<RequiredBlockFaceSupport>>> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid("BlockType", key, "Support must be an object"))?;
    let mut support = BTreeMap::new();
    for (name, entries) in map {
        let faces =
            Face::expand(name).ok_or_else(|| invalid("BlockType", key, "unknown Support face"))?;
        let entries = entries
            .as_array()
            .ok_or_else(|| invalid("BlockType", key, "Support entries must be arrays"))?;
        let parsed = entries
            .iter()
            .map(|entry| required_support(entry, registry, tags))
            .collect::<Result<Vec<_>>>()?;
        for face in faces {
            support
                .entry(face.neighbor())
                .or_insert_with(Vec::new)
                .extend(parsed.iter().cloned());
        }
    }
    Ok(support)
}

fn required_support(
    value: &Value,
    registry: &AssetRegistry,
    tags: &TagRegistry,
) -> Result<RequiredBlockFaceSupport> {
    if !value.is_object() {
        return Err(invalid(
            "BlockType",
            "Support",
            "required support entries must be objects",
        ));
    }
    let block_type_id = value
        .get("BlockTypeId")
        .and_then(Value::as_str)
        .and_then(|block_type| registry.block_types.id_of(block_type))
        .unwrap_or(-1);
    let fluid_id = value
        .get("FluidId")
        .and_then(Value::as_str)
        .and_then(|fluid| registry.fluids.id_of(fluid))
        .unwrap_or(-1);
    let tag_index = value
        .get("TagId")
        .and_then(Value::as_str)
        .and_then(|tag| tags.index(tag))
        .unwrap_or(i32::MIN);
    Ok(RequiredBlockFaceSupport {
        face_type: opt_string(value, "FaceType"),
        self_face_type: opt_string(value, "SelfFaceType"),
        block_set_id: opt_string(value, "BlockSetId"),
        block_type_id,
        tag_index,
        fluid_id,
        support: support_match(value.get("Support")).unwrap_or(SupportMatch::Required),
        match_self: support_match(value.get("MatchSelf")).unwrap_or(SupportMatch::Ignored),
        allow_support_propagation: value
            .get("AllowSupportPropagation")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        rotate: value.get("Rotate").and_then(Value::as_bool).unwrap_or(true),
        filler: filler(value.get("Filler")),
    })
}

fn support_match(value: Option<&Value>) -> Option<SupportMatch> {
    match value.and_then(Value::as_str) {
        Some("Ignored") => Some(SupportMatch::Ignored),
        Some("Required") => Some(SupportMatch::Required),
        Some("Disallowed") => Some(SupportMatch::Disallowed),
        _ => None,
    }
}

fn parse_supporting(
    value: &Value,
    key: &str,
) -> Result<BTreeMap<BlockNeighbor, Vec<BlockFaceSupport>>> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid("BlockType", key, "Supporting must be an object"))?;
    let mut supporting = BTreeMap::new();
    for (name, entries) in map {
        let faces = Face::expand(name)
            .ok_or_else(|| invalid("BlockType", key, "unknown Supporting face"))?;
        let entries = entries
            .as_array()
            .ok_or_else(|| invalid("BlockType", key, "Supporting entries must be arrays"))?;
        let parsed = entries
            .iter()
            .map(|entry| BlockFaceSupport {
                face_type: Some(
                    opt_string(entry, "FaceType").unwrap_or_else(|| "Full".to_string()),
                ),
                filler: filler(entry.get("Filler")),
            })
            .collect::<Vec<_>>();
        for face in faces {
            supporting
                .entry(face.neighbor())
                .or_insert_with(Vec::new)
                .extend(parsed.iter().cloned());
        }
    }
    Ok(supporting)
}

fn filler(value: Option<&Value>) -> Option<Vec<Vector3i>> {
    value.and_then(Value::as_array).map(|entries| {
        entries
            .iter()
            .map(|entry| Vector3i {
                x: entry.get("X").and_then(Value::as_i64).unwrap_or(0) as i32,
                y: entry.get("Y").and_then(Value::as_i64).unwrap_or(0) as i32,
                z: entry.get("Z").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
            .collect()
    })
}
