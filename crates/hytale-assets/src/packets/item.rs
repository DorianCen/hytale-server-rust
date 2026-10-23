//! `UpdateItems` (id 54) encoder.
//!
//! Implemented from the Rust registry snapshot. Item keys are strings; block
//! references use the Rust-owned `BlockType` ids.
//!
//! The registry resolves each item's `Parent` chain before this encoder runs.
//! The large nested payloads (`tool`, `weapon`, `armor`, `gliderConfig`,
//! `ability`, `blockSelectorTool`, `builderToolData`, `trails`,
//! `itemAppearanceConditions`, `pullbackConfig`, `hudUI` and the interaction
//! ids) stay `None`/`0`/empty for now; see the T33 task's Deviations section.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::UpdateItems;
use hytale_protocol::generated::common::{
    AssetIconProperties, FormattedMessage, GameMode, InteractionConfiguration, InteractionPriority,
    InteractionType, ItemBase, ItemEntityConfig, ItemResourceType, ItemTranslationProperties,
    ItemUtility, ModelParticle, PrioritySlot, UpdateType,
};
use hytale_protocol::generated::wire::{Vector2f, Vector3f};
use serde_json::Value;

use crate::packets::{color, color_light, model_particles, AssetPacket, Result, TagRegistry};
use crate::registry::AssetRegistry;

/// Java `Item.UNKNOWN_TEXTURE`.
const UNKNOWN_ITEM_TEXTURE: &str = "Items/Unknown.png";
/// Java `ItemPlayerAnimations.DEFAULT_ID`.
const DEFAULT_PLAYER_ANIMATIONS: &str = "Item";
/// Java `ItemEntityConfig.DEFAULT_PARTICLE_SYSTEM_ID`.
const DEFAULT_PARTICLE_SYSTEM: &str = "Item";
/// Java `BlockType.UNKNOWN_ID`, used when a contained block type is missing.
const UNKNOWN_BLOCK_ID: i32 = 1;

/// Encode the `UpdateItems` init packet from the registry snapshot.
pub(crate) fn item_packet(registry: &AssetRegistry, tags: &TagRegistry) -> Result<AssetPacket> {
    let mut items = BTreeMap::new();
    for record in registry.items.iter() {
        items.insert(
            record.key.clone(),
            item_base(registry, &record.key, &record.value, tags)?,
        );
    }

    let packet = UpdateItems {
        r#type: UpdateType::Init,
        items: Some(items),
        removed_items: None,
        update_models: true,
        update_icons: true,
    };
    AssetPacket::encode("Item", &packet)
}

fn item_base(
    registry: &AssetRegistry,
    key: &str,
    json: &Value,
    tags: &TagRegistry,
) -> Result<ItemBase> {
    let has_block = json.get("BlockType").is_some_and(Value::is_object);
    let block_id = block_id(registry, key, has_block);

    let mut max_stack = integer(json, "MaxStack").unwrap_or(-1);
    if max_stack == -1 {
        // Java `Item.processConfig`.
        max_stack = if has_any(
            json,
            &[
                "Tool",
                "Weapon",
                "Armor",
                "BuilderTool",
                "BlockSelectorTool",
            ],
        ) {
            1
        } else {
            100
        };
    }

    Ok(ItemBase {
        id: Some(key.to_string()),
        model: string(json, "Model"),
        scale: number(json, "Scale").unwrap_or(1.0) as f32,
        texture: Some(string(json, "Texture").unwrap_or_else(|| UNKNOWN_ITEM_TEXTURE.to_string())),
        animation: string(json, "Animation"),
        player_animations_id: Some(
            string(json, "PlayerAnimationsId")
                .unwrap_or_else(|| DEFAULT_PLAYER_ANIMATIONS.to_string()),
        ),
        use_player_animations: boolean(json, "UsePlayerAnimations", false),
        max_stack,
        // Reticle, quality and sound indexes depend on stores that are not
        // streamed yet (T33 deviations).
        reticle_index: 0,
        icon: string(json, "Icon"),
        icon_properties: icon_properties(json.get("IconProperties")),
        translation_properties: translation_properties(json.get("TranslationProperties")),
        item_level: integer(json, "ItemLevel").unwrap_or(0),
        quality_index: 0,
        resource_types: resource_types(json.get("ResourceTypes")),
        consumable: boolean(json, "Consumable", false),
        variant: boolean(json, "Variant", false),
        block_id,
        tool: None,
        weapon: None,
        armor: None,
        glider_config: None,
        utility: Some(utility(json.get("Utility"))),
        block_selector_tool: None,
        builder_tool_data: None,
        item_entity: Some(item_entity(json.get("ItemEntity"), has_block)),
        set: string(json, "Set"),
        categories: non_empty_strings(json.get("Categories")),
        sub_category: string(json, "SubCategory"),
        particles: non_empty_particles(json.get("Particles"))?,
        first_person_particles: non_empty_particles(json.get("FirstPersonParticles"))?,
        trails: None,
        light: color_light(json.get("Light")),
        durability: number(json, "MaxDurability").unwrap_or(0.0),
        sound_event_index: 0,
        item_sound_set_index: 0,
        // Java always sends the interaction maps, empty when the item has no
        // entries. Interaction ids are not resolved until `RootInteraction` is
        // streamed.
        interactions: Some(BTreeMap::new()),
        carry_interactions: Some(BTreeMap::new()),
        carry_hud_input_bindings: Some(BTreeMap::new()),
        interaction_vars: Some(BTreeMap::new()),
        interaction_config: Some(interaction_config(json)),
        dropped_item_animation: string(json, "DroppedItemAnimation"),
        tag_indexes: tags.expanded(json.get("Tags")),
        item_appearance_conditions: None,
        display_entity_stats_hud: display_entity_stats_hud(json.get("DisplayEntityStatsHUD")),
        pullback_config: None,
        clips_geometry: boolean(json, "ClipsGeometry", false),
        render_deployable_preview: boolean(json, "RenderDeployablePreview", false),
        hud_ui: None,
        ability: None,
    })
}

/// Resolve the `BlockType` id referenced by an item. A child item can inherit
/// the `BlockType` from its parent item (`appendInherited`), in which case the
/// block asset lives under the ancestor's key, not the child's.
fn block_id(registry: &AssetRegistry, key: &str, has_block: bool) -> i32 {
    if !has_block {
        return 0;
    }
    let mut current = Some(key.to_string());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(candidate) = current {
        if !seen.insert(candidate.to_ascii_lowercase()) {
            break;
        }
        if let Some(id) = registry.block_types.id_of(&candidate) {
            return id;
        }
        current = registry
            .items
            .get(&candidate)
            .and_then(|record| record.parent.clone());
    }
    UNKNOWN_BLOCK_ID
}

fn string(json: &Value, name: &str) -> Option<String> {
    json.get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn boolean(json: &Value, name: &str, fallback: bool) -> bool {
    json.get(name).and_then(Value::as_bool).unwrap_or(fallback)
}

fn number(json: &Value, name: &str) -> Option<f64> {
    json.get(name).and_then(Value::as_f64)
}

fn integer(json: &Value, name: &str) -> Option<i32> {
    let value = json.get(name).and_then(Value::as_i64)?;
    i32::try_from(value).ok()
}

fn has_any(json: &Value, names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| json.get(*name).is_some_and(|value| !value.is_null()))
}

fn non_empty_strings(value: Option<&Value>) -> Option<Vec<String>> {
    let values = value?.as_array()?;
    let strings: Vec<String> = values
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    (!strings.is_empty()).then_some(strings)
}

fn non_empty_particles(value: Option<&Value>) -> Result<Option<Vec<ModelParticle>>> {
    let particles = model_particles(value)?;
    Ok((!particles.is_empty()).then_some(particles))
}

fn icon_properties(value: Option<&Value>) -> Option<AssetIconProperties> {
    let object = value?.as_object()?;
    Some(AssetIconProperties {
        scale: object.get("Scale").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        translation: vector2(object.get("Translation")),
        rotation: vector3(object.get("Rotation")),
    })
}

fn vector2(value: Option<&Value>) -> Option<Vector2f> {
    let value = value?;
    if let Some(array) = value.as_array() {
        return Some(Vector2f {
            x: array.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            y: array.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        });
    }
    let object = value.as_object()?;
    Some(Vector2f {
        x: object.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        y: object.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn vector3(value: Option<&Value>) -> Option<Vector3f> {
    let value = value?;
    if let Some(array) = value.as_array() {
        return Some(Vector3f {
            x: array.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            y: array.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            z: array.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        });
    }
    let object = value.as_object()?;
    Some(Vector3f {
        x: object.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        y: object.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        z: object.get("Z").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn translation_properties(value: Option<&Value>) -> Option<ItemTranslationProperties> {
    let object = value?.as_object()?;
    Some(ItemTranslationProperties {
        name: object
            .get("Name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        name_arguments: translation_arguments(object.get("NameArguments")),
        description: object
            .get("Description")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        description_arguments: translation_arguments(object.get("DescriptionArguments")),
    })
}

/// Java `ItemTranslationProperties.wrapKeys`: every argument key becomes a
/// translation `Message` with the key as its `messageId`.
fn translation_arguments(value: Option<&Value>) -> Option<BTreeMap<String, FormattedMessage>> {
    let object = value?.as_object()?;
    if object.is_empty() {
        return None;
    }
    let arguments: BTreeMap<String, FormattedMessage> = object
        .iter()
        .filter_map(|(name, value)| {
            value.as_str().map(|key| {
                (
                    name.clone(),
                    FormattedMessage {
                        message_id: Some(key.to_string()),
                        ..FormattedMessage::default()
                    },
                )
            })
        })
        .collect();
    (!arguments.is_empty()).then_some(arguments)
}

fn resource_types(value: Option<&Value>) -> Option<Vec<ItemResourceType>> {
    let values = value?.as_array()?;
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                Some(ItemResourceType {
                    id: object
                        .get("Id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    // Java's protocol `ItemResourceType.quantity` defaults to 1.
                    quantity: object
                        .get("Quantity")
                        .and_then(Value::as_i64)
                        .and_then(|quantity| i32::try_from(quantity).ok())
                        .unwrap_or(1),
                })
            })
            .collect(),
    )
}

fn utility(value: Option<&Value>) -> ItemUtility {
    let object = value.and_then(Value::as_object);
    ItemUtility {
        usable: object
            .and_then(|object| object.get("Usable"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        compatible: object
            .and_then(|object| object.get("Compatible"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        // `EntityStatsToClear` is a string array resolved through the
        // `EntityStatType` store, which is not loaded yet. Numeric ids are
        // accepted for fixtures that already use resolved indexes.
        entity_stats_to_clear: object
            .and_then(|object| object.get("EntityStatsToClear"))
            .and_then(numeric_ids),
        stat_modifiers: None,
    }
}

fn numeric_ids(value: &Value) -> Option<Vec<i32>> {
    let values = value.as_array()?;
    values
        .iter()
        .map(|value| {
            let id = value.as_i64()?;
            i32::try_from(id).ok()
        })
        .collect()
}

fn item_entity(value: Option<&Value>, has_block: bool) -> ItemEntityConfig {
    match value.and_then(Value::as_object) {
        Some(object) => ItemEntityConfig {
            particle_system_id: Some(
                object
                    .get("ParticleSystemId")
                    .and_then(Value::as_str)
                    .unwrap_or(DEFAULT_PARTICLE_SYSTEM)
                    .to_string(),
            ),
            particle_color: color(object.get("ParticleColor")),
            show_item_particles: object
                .get("ShowItemParticles")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
        None => ItemEntityConfig {
            // Java `ItemEntityConfig.DEFAULT_BLOCK` / `DEFAULT`.
            particle_system_id: (!has_block).then(|| DEFAULT_PARTICLE_SYSTEM.to_string()),
            particle_color: None,
            show_item_particles: true,
        },
    }
}

/// Java `InteractionPriorityCodec`: an integer is the `Default` slot, an object
/// carries named priority slots, and zero means “no priority map”.
fn priorities(value: Option<&Value>) -> Option<BTreeMap<InteractionType, InteractionPriority>> {
    let object = value?.as_object()?;
    let mut priorities = BTreeMap::new();
    for (name, value) in object {
        let Some(interaction) = interaction_type(name) else {
            continue;
        };
        let priority = match value {
            Value::Number(number) => {
                let value = i32::try_from(number.as_i64()?).ok()?;
                InteractionPriority {
                    values: (value != 0).then(|| BTreeMap::from([(PrioritySlot::Default, value)])),
                }
            }
            Value::Object(slots) => {
                let mut values = BTreeMap::new();
                for (slot, value) in slots {
                    let slot = match slot.as_str() {
                        "Default" => PrioritySlot::Default,
                        "MainHand" => PrioritySlot::MainHand,
                        "OffHand" => PrioritySlot::OffHand,
                        _ => continue,
                    };
                    if let Some(value) = value.as_i64().and_then(|v| i32::try_from(v).ok()) {
                        values.insert(slot, value);
                    }
                }
                InteractionPriority {
                    values: (!values.is_empty()).then_some(values),
                }
            }
            _ => continue,
        };
        priorities.insert(interaction, priority);
    }
    (!priorities.is_empty()).then_some(priorities)
}

fn interaction_type(name: &str) -> Option<InteractionType> {
    Some(match name {
        "Primary" => InteractionType::Primary,
        "Secondary" => InteractionType::Secondary,
        "Ability1" => InteractionType::Ability1,
        "Ability2" => InteractionType::Ability2,
        "Ability3" => InteractionType::Ability3,
        "Ability4" => InteractionType::Ability4,
        "Use" => InteractionType::Use,
        "Pick" => InteractionType::Pick,
        "Pickup" => InteractionType::Pickup,
        "CollisionEnter" => InteractionType::CollisionEnter,
        "CollisionLeave" => InteractionType::CollisionLeave,
        "Collision" => InteractionType::Collision,
        "EntityStatEffect" => InteractionType::EntityStatEffect,
        "SwapTo" => InteractionType::SwapTo,
        "SwapFrom" => InteractionType::SwapFrom,
        "Death" => InteractionType::Death,
        "Wielding" => InteractionType::Wielding,
        "ProjectileSpawn" => InteractionType::ProjectileSpawn,
        "ProjectileHit" => InteractionType::ProjectileHit,
        "ProjectileMiss" => InteractionType::ProjectileMiss,
        "ProjectileBounce" => InteractionType::ProjectileBounce,
        "Held" => InteractionType::Held,
        "HeldOffhand" => InteractionType::HeldOffhand,
        "Equipped" => InteractionType::Equipped,
        "Dodge" => InteractionType::Dodge,
        "GameModeSwap" => InteractionType::GameModeSwap,
        "OnBreak" => InteractionType::OnBreak,
        "OnBreakImpact" => InteractionType::OnBreakImpact,
        _ => return None,
    })
}

fn interaction_config(json: &Value) -> InteractionConfiguration {
    if let Some(object) = json.get("InteractionConfig").and_then(Value::as_object) {
        // The Java codec's `UseDistance` supplier starts from the two default
        // mode distances and overlays the document's entries.
        let mut use_distance = BTreeMap::new();
        use_distance.insert(GameMode::Adventure, 5.0);
        use_distance.insert(GameMode::Creative, 6.0);
        if let Some(overrides) = object.get("UseDistance").and_then(Value::as_object) {
            for (name, value) in overrides {
                let mode = match name.as_str() {
                    "Adventure" => Some(GameMode::Adventure),
                    "Creative" => Some(GameMode::Creative),
                    _ => None,
                };
                if let (Some(mode), Some(distance)) = (mode, value.as_f64()) {
                    use_distance.insert(mode, distance as f32);
                }
            }
        }
        InteractionConfiguration {
            // `InteractionConfiguration` field defaults.
            display_outlines: object
                .get("DisplayOutlines")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            debug_outlines: object
                .get("DebugOutlines")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            use_distance: Some(use_distance),
            all_entities: object
                .get("AllEntities")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            priorities: priorities(object.get("Priorities")),
        }
    } else {
        // `InteractionConfiguration.DEFAULT` is built during static
        // initialization before `DEFAULT_USE_DISTANCE` is populated, so its
        // `useDistance` is null. `DEFAULT_WEAPON` differs only in
        // `displayOutlines`.
        InteractionConfiguration {
            display_outlines: !has_any(json, &["Weapon"]),
            debug_outlines: false,
            use_distance: None,
            all_entities: false,
            priorities: None,
        }
    }
}

/// Java `EntityStatsModule.resolveEntityStats` maps stat names to ids. The
/// `EntityStatType` store is not loaded yet, so only numeric arrays can be
/// emitted; string-only arrays are left unset.
fn display_entity_stats_hud(value: Option<&Value>) -> Option<Vec<i32>> {
    let ids = numeric_ids(value?)?;
    (!ids.is_empty()).then_some(ids)
}
