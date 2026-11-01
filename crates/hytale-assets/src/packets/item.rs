//! `UpdateItems` (id 54) encoder.
//!
//! Implemented from the Rust registry snapshot. Item keys are strings; block
//! references use the Rust-owned `BlockType` ids.
//!
//! The registry resolves each item's `Parent` chain before this encoder runs.
//! The T66 slice decodes the item tool/weapon/armor/glider/trail/HUD/ability
//! trees that Java always materializes. Deeper interaction-variable and
//! damage-breakdown trees remain follow-ups and are documented in T66.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::UpdateItems;
use hytale_protocol::generated::common::{
    AbilityCostType, AbilitySlot, AssetIconProperties, BlockSelectorToolData, BoxBreakShape,
    BreakShape, BreakShapeOrientation, BuilderToolState, CalculationType, Cosmetic,
    CylinderBreakShape, Direction, EntityPart, FloatRange, FormattedMessage, GameMode,
    InteractionConfiguration, InteractionPriority, InteractionType, ItemAbility,
    ItemAppearanceCondition, ItemArmor, ItemArmorSlot, ItemBase, ItemEntityConfig, ItemGlider,
    ItemHudUI, ItemHudUIType, ItemResourceType, ItemTool, ItemToolSpec, ItemTranslationProperties,
    ItemUtility, ItemWeapon, ModelParticle, ModelTrail, Modifier, ModifierTarget, PrioritySlot,
    ResistanceCalculationType, ResistanceModifier, UpdateType, ValueType,
};
use hytale_protocol::generated::wire::{Vector2f, Vector3f};
use serde_json::Value;

use crate::packets::{
    color, color_light, model_particles, AssetPacket, Result, TagRegistry, INTERACTION_NOT_FOUND,
};
use crate::registry::AssetRegistry;

/// Java `Item.UNKNOWN_TEXTURE`.
const UNKNOWN_ITEM_TEXTURE: &str = "Items/Unknown.png";
/// Java `ItemPlayerAnimations.DEFAULT_ID`.
const DEFAULT_PLAYER_ANIMATIONS: &str = "Default";
/// Java `ItemEntityConfig.DEFAULT_PARTICLE_SYSTEM_ID`.
const DEFAULT_PARTICLE_SYSTEM: &str = "Item";
/// Java `Item.itemSoundSetId` default.
const DEFAULT_ITEM_SOUND_SET: &str = "ISS_Default";
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

    // Java `processConfig`: `Quality` is a plain (non-inherited) field, so use
    // the item's own document rather than the parent-merged value.
    let quality_id = registry
        .item_local_json(key)
        .and_then(|json| json.get("Quality"))
        .and_then(Value::as_str);
    let quality_index = quality_id
        .and_then(|quality| registry.item_qualities.id_of(quality))
        .unwrap_or(0);

    let item_entity = Some(
        resolved_item_entity(registry, key, json, &mut std::collections::BTreeSet::new())
            .unwrap_or_else(|| default_item_entity(has_block)),
    );

    let reticle_index = json
        .get("Reticle")
        .and_then(Value::as_str)
        .and_then(|reticle| registry.item_reticles.id_of(reticle))
        .unwrap_or(0);
    // Java `preLoadAssets` constructs `Item.UNKNOWN` directly, so its
    // `processConfig` never resolves the sound-set index.
    let preloaded = registry
        .items
        .get(key)
        .is_some_and(|record| record.preloaded);
    let item_sound_set_index = if preloaded {
        0
    } else {
        let sound_set = json
            .get("ItemSoundSetId")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_ITEM_SOUND_SET);
        registry
            .item_sound_sets
            .id_of(sound_set)
            .or_else(|| registry.item_sound_sets.id_of(DEFAULT_ITEM_SOUND_SET))
            .unwrap_or(0)
    };

    let player_animations = player_animations_id(registry, key);

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
        player_animations_id: Some(player_animations.clone()),
        use_player_animations: boolean(json, "UsePlayerAnimations", false),
        max_stack,
        reticle_index,
        icon: string(json, "Icon"),
        icon_properties: icon_properties(json.get("IconProperties")),
        translation_properties: translation_properties(json.get("TranslationProperties")),
        item_level: integer(json, "ItemLevel").unwrap_or(0),
        quality_index,
        resource_types: resource_types(json.get("ResourceTypes")),
        consumable: boolean(json, "Consumable", false),
        variant: boolean(json, "Variant", false),
        block_id,
        tool: item_tool(json.get("Tool")),
        weapon: item_weapon(registry, json.get("Weapon")),
        armor: item_armor(registry, json.get("Armor")),
        glider_config: item_glider(json.get("Glider")),
        utility: Some(utility(registry, json.get("Utility"))),
        block_selector_tool: block_selector_tool(json.get("BlockSelectorTool")),
        builder_tool_data: builder_tool_data(json.get("BuilderTool")),
        item_entity,
        set: string(json, "Set"),
        categories: non_empty_strings(json.get("Categories")),
        sub_category: string(json, "SubCategory"),
        particles: non_empty_particles(json.get("Particles"))?,
        first_person_particles: non_empty_particles(json.get("FirstPersonParticles"))?,
        trails: item_trails(json.get("Trails")),
        light: color_light(json.get("Light")),
        durability: number(json, "MaxDurability").unwrap_or(0.0),
        sound_event_index: 0,
        item_sound_set_index,
        // Java `Item.processConfig` merges the item's own `Interactions`, the
        // `PlayerAnimationsId` unarmed map, and the default `Empty` unarmed map.
        // Missing that merge leaves block items without their primary/pick
        // roots and makes the client abort its item cache while preparing them.
        interactions: Some(item_interactions(registry, json, &player_animations)),
        carry_interactions: Some(item_root_map(registry, json.get("CarryInteractions"))),
        carry_hud_input_bindings: Some(item_hud_bindings(json.get("CarryHudInputBindings"))),
        interaction_vars: Some(item_interaction_vars(registry, json.get("InteractionVars"))),
        interaction_config: Some(interaction_config(json)),
        dropped_item_animation: string(json, "DroppedItemAnimation"),
        // Java materializes this list for every item; the client dereferences
        // it while building item caches.
        tag_indexes: Some(tags.expanded(json.get("Tags")).unwrap_or_default()),
        item_appearance_conditions: item_appearance_conditions(
            registry,
            json.get("ItemAppearanceConditions"),
        ),
        display_entity_stats_hud: display_entity_stats_hud(
            registry,
            json.get("DisplayEntityStatsHUD"),
        ),
        pullback_config: None,
        clips_geometry: boolean(json, "ClipsGeometry", false),
        render_deployable_preview: boolean(json, "RenderDeployablePreview", false),
        hud_ui: item_hud_ui(json.get("HudUI")),
        ability: item_ability(json.get("Ability")),
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

fn f32_value(value: Option<&Value>) -> f32 {
    value.and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn f64_value(value: Option<&Value>) -> f64 {
    value.and_then(Value::as_f64).unwrap_or(0.0)
}

fn modifiers(
    registry: &AssetRegistry,
    value: Option<&Value>,
) -> Option<BTreeMap<i32, Vec<Modifier>>> {
    let object = value?.as_object()?;
    let mut result = BTreeMap::new();
    for (stat, entries) in object {
        let Some(stat_id) = registry.entity_stat_id(stat) else {
            continue;
        };
        let list: Vec<Modifier> = entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                Some(Modifier {
                    target: match entry.get("Target").and_then(Value::as_str) {
                        Some("Max") => ModifierTarget::Max,
                        _ => ModifierTarget::Min,
                    },
                    calculation_type: match entry.get("CalculationType").and_then(Value::as_str) {
                        Some("Multiplicative") => CalculationType::Multiplicative,
                        _ => CalculationType::Additive,
                    },
                    amount: f32_value(entry.get("Amount")),
                })
            })
            .collect();
        if !list.is_empty() {
            result.insert(stat_id, list);
        }
    }
    (!result.is_empty()).then_some(result)
}

fn resistance_modifiers(
    value: Option<&Value>,
) -> Option<BTreeMap<String, Vec<ResistanceModifier>>> {
    let object = value?.as_object()?;
    let mut result = BTreeMap::new();
    for (cause, entries) in object {
        let list: Vec<ResistanceModifier> = entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                Some(ResistanceModifier {
                    calculation_type: match entry.get("CalculationType").and_then(Value::as_str) {
                        Some("Percent") => ResistanceCalculationType::Percent,
                        _ => ResistanceCalculationType::Flat,
                    },
                    amount: f32_value(entry.get("Amount")),
                })
            })
            .collect();
        if !list.is_empty() {
            result.insert(cause.clone(), list);
        }
    }
    (!result.is_empty()).then_some(result)
}

fn calculation_modifiers(
    value: Option<&Value>,
    lowercase_keys: bool,
) -> Option<BTreeMap<String, Vec<Modifier>>> {
    let object = value?.as_object()?;
    let mut result = BTreeMap::new();
    for (key, entries) in object {
        let list: Vec<Modifier> = entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                Some(Modifier {
                    target: ModifierTarget::Min,
                    calculation_type: match entry.get("CalculationType").and_then(Value::as_str) {
                        Some("Multiplicative") => CalculationType::Multiplicative,
                        _ => CalculationType::Additive,
                    },
                    amount: f32_value(entry.get("Amount")),
                })
            })
            .collect();
        if !list.is_empty() {
            let key = if lowercase_keys {
                key.to_ascii_lowercase()
            } else {
                key.clone()
            };
            result.insert(key, list);
        }
    }
    (!result.is_empty()).then_some(result)
}

fn item_tool(value: Option<&Value>) -> Option<ItemTool> {
    let object = value?.as_object()?;
    let specs = object
        .get("Specs")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let entry = entry.as_object()?;
                    Some(ItemToolSpec {
                        gather_type: entry
                            .get("GatherType")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        power: f32_value(entry.get("Power")),
                        quality: entry
                            .get("Quality")
                            .and_then(Value::as_i64)
                            .and_then(|value| i32::try_from(value).ok())
                            .unwrap_or(0),
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|specs| !specs.is_empty());
    Some(ItemTool {
        specs,
        speed: f32_value(object.get("Speed")),
        break_shape: break_shape(object.get("BreakShape")),
    })
}

fn break_shape(value: Option<&Value>) -> Option<BreakShape> {
    let object = value?.as_object()?;
    let width = object.get("Width").and_then(Value::as_i64).unwrap_or(0) as i32;
    let height = object.get("Height").and_then(Value::as_i64).unwrap_or(0) as i32;
    let depth = object.get("Depth").and_then(Value::as_i64).unwrap_or(0) as i32;
    let centered = object
        .get("Centered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let offset = vector3(object.get("Offset"));
    let orientation = match object.get("Orientation").and_then(Value::as_str) {
        Some("Surface") => BreakShapeOrientation::Surface,
        _ => BreakShapeOrientation::View,
    };
    Some(match object.get("Id").and_then(Value::as_str) {
        Some("Cylinder") => BreakShape::CylinderBreakShape(CylinderBreakShape {
            width,
            height,
            depth,
            centered,
            offset,
            orientation,
        }),
        _ => BreakShape::BoxBreakShape(BoxBreakShape {
            width,
            height,
            depth,
            centered,
            offset,
            orientation,
        }),
    })
}

fn item_weapon(registry: &AssetRegistry, value: Option<&Value>) -> Option<ItemWeapon> {
    let object = value?.as_object()?;
    Some(ItemWeapon {
        entity_stats_to_clear: object
            .get("EntityStatsToClear")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(|stat| registry.entity_stat_id(stat))
                    .collect::<Vec<_>>()
            })
            .filter(|ids| !ids.is_empty()),
        stat_modifiers: modifiers(registry, object.get("StatModifiers")),
        render_dual_wielded: object
            .get("RenderDualWielded")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        basic_damage_breakdown: None,
        ultimate_damage_breakdown: None,
    })
}

fn item_armor(registry: &AssetRegistry, value: Option<&Value>) -> Option<ItemArmor> {
    let object = value?.as_object()?;
    let armor_slot = match object.get("ArmorSlot").and_then(Value::as_str) {
        Some("Chest") => ItemArmorSlot::Chest,
        Some("Hands") => ItemArmorSlot::Hands,
        Some("Legs") => ItemArmorSlot::Legs,
        _ => ItemArmorSlot::Head,
    };
    let cosmetics = object
        .get("CosmeticsToHide")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .filter_map(cosmetic)
                .collect::<Vec<_>>()
        })
        .filter(|entries| !entries.is_empty());
    Some(ItemArmor {
        armor_slot,
        cosmetics_to_hide: cosmetics,
        stat_modifiers: modifiers(registry, object.get("StatModifiers")),
        base_damage_resistance: f64_value(object.get("BaseDamageResistance")),
        damage_resistance: resistance_modifiers(object.get("DamageResistance")),
        damage_enhancement: calculation_modifiers(object.get("DamageEnhancement"), false),
        damage_class_enhancement: calculation_modifiers(object.get("DamageClassEnhancement"), true),
        movement_settings: None,
    })
}

fn cosmetic(name: &str) -> Option<Cosmetic> {
    Some(match name {
        "Haircut" => Cosmetic::Haircut,
        "FacialHair" => Cosmetic::FacialHair,
        "Undertop" => Cosmetic::Undertop,
        "Overtop" => Cosmetic::Overtop,
        "Pants" => Cosmetic::Pants,
        "Overpants" => Cosmetic::Overpants,
        "Shoes" => Cosmetic::Shoes,
        "Gloves" => Cosmetic::Gloves,
        "Cape" => Cosmetic::Cape,
        "HeadAccessory" => Cosmetic::HeadAccessory,
        "FaceAccessory" => Cosmetic::FaceAccessory,
        "EarAccessory" => Cosmetic::EarAccessory,
        "Ear" => Cosmetic::Ear,
        _ => return None,
    })
}

fn item_glider(value: Option<&Value>) -> Option<ItemGlider> {
    let object = value?.as_object()?;
    Some(ItemGlider {
        terminal_velocity: f32_value(object.get("TerminalVelocity")),
        fall_speed_multiplier: f32_value(object.get("FallSpeedMultiplier")),
        horizontal_speed_multiplier: f32_value(object.get("HorizontalSpeedMultiplier")),
        speed: f32_value(object.get("Speed")),
        dive_terminal_velocity: f32_value(object.get("DiveTerminalVelocity")),
        dive_speed: f32_value(object.get("DiveSpeed")),
        dive_fall_speed_multiplier: f32_value(object.get("DiveFallSpeedMultiplier")),
        dive_horizontal_speed_multiplier: f32_value(object.get("DiveHorizontalSpeedMultiplier")),
        energy_climb_rate: f32_value(object.get("EnergyClimbRate")),
        dive_speed_to_energy_ratio: f32_value(object.get("DiveSpeedToEnergyRatio")),
    })
}

fn block_selector_tool(value: Option<&Value>) -> Option<BlockSelectorToolData> {
    let object = value?.as_object()?;
    Some(BlockSelectorToolData {
        durability_loss_on_use: f32_value(object.get("DurabilityLossOnUse")),
    })
}

fn builder_tool_data(value: Option<&Value>) -> Option<BuilderToolState> {
    let object = value?.as_object()?;
    Some(BuilderToolState {
        id: object
            .get("Id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        is_brush: object
            .get("IsBrush")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        args: None,
    })
}

fn item_trails(value: Option<&Value>) -> Option<Vec<ModelTrail>> {
    let entries = value?.as_array()?;
    let trails: Vec<ModelTrail> = entries
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            Some(ModelTrail {
                trail_id: entry
                    .get("TrailId")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                target_entity_part: match entry.get("TargetEntityPart").and_then(Value::as_str) {
                    Some("Entity") => EntityPart::Entity,
                    Some("PrimaryItem") => EntityPart::PrimaryItem,
                    Some("SecondaryItem") => EntityPart::SecondaryItem,
                    _ => EntityPart::Self_,
                },
                target_node_name: entry
                    .get("TargetNodeName")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                position_offset: vector3(entry.get("PositionOffset")),
                rotation_offset: entry.get("RotationOffset").and_then(Value::as_object).map(
                    |rotation| Direction {
                        yaw: f32_value(rotation.get("Yaw")),
                        pitch: f32_value(rotation.get("Pitch")),
                        roll: f32_value(rotation.get("Roll")),
                    },
                ),
                fixed_rotation: entry
                    .get("FixedRotation")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    (!trails.is_empty()).then_some(trails)
}

fn item_hud_ui(value: Option<&Value>) -> Option<Vec<ItemHudUI>> {
    let entries = value?.as_array()?;
    let entries: Vec<ItemHudUI> = entries
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            Some(ItemHudUI {
                path: entry
                    .get("Path")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                r#type: match entry.get("Type").and_then(Value::as_str) {
                    Some("Legend") => ItemHudUIType::Legend,
                    _ => ItemHudUIType::Hud,
                },
            })
        })
        .collect();
    (!entries.is_empty()).then_some(entries)
}

fn item_appearance_conditions(
    registry: &AssetRegistry,
    value: Option<&Value>,
) -> Option<BTreeMap<i32, Vec<ItemAppearanceCondition>>> {
    let object = value?.as_object()?;
    let mut result = BTreeMap::new();
    for (stat, entries) in object {
        let Some(stat_id) = registry.entity_stat_id(stat) else {
            continue;
        };
        let conditions: Vec<ItemAppearanceCondition> = entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.as_object()?;
                Some(ItemAppearanceCondition {
                    particles: None,
                    first_person_particles: None,
                    model: entry
                        .get("Model")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    texture: entry
                        .get("Texture")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    model_vfx_id: entry
                        .get("ModelVFXId")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    condition: entry
                        .get("Condition")
                        .and_then(Value::as_array)
                        .map(|range| FloatRange {
                            inclusive_min: range.first().and_then(Value::as_f64).unwrap_or(0.0)
                                as f32,
                            inclusive_max: range.get(1).and_then(Value::as_f64).unwrap_or(0.0)
                                as f32,
                        }),
                    condition_value_type: match entry
                        .get("ConditionValueType")
                        .and_then(Value::as_str)
                    {
                        Some("Absolute") => ValueType::Absolute,
                        _ => ValueType::Percent,
                    },
                    local_sound_event_id: 0,
                    world_sound_event_id: 0,
                })
            })
            .collect();
        if !conditions.is_empty() {
            result.insert(stat_id, conditions);
        }
    }
    (!result.is_empty()).then_some(result)
}

fn item_ability(value: Option<&Value>) -> Option<ItemAbility> {
    let object = value?.as_object()?;
    let attributes = object
        .get("Attributes")
        .and_then(Value::as_object)
        .map(|attributes| {
            attributes
                .iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_f64()? as f32)))
                .collect::<BTreeMap<_, _>>()
        })
        .filter(|attributes| !attributes.is_empty());
    Some(ItemAbility {
        slot: match object.get("Slot").and_then(Value::as_str) {
            Some("Support") => AbilitySlot::Support,
            _ => AbilitySlot::Core,
        },
        cooldown: f32_value(object.get("Cooldown")),
        cost: f32_value(object.get("Cost")),
        cost_type: match object.get("CostType").and_then(Value::as_str) {
            Some("Mana") => AbilityCostType::Mana,
            Some("Stamina") => AbilityCostType::Stamina,
            Some("Health") => AbilityCostType::Health,
            _ => AbilityCostType::None,
        },
        weapons: None,
        weapon_tag_indexes: None,
        ability_keyword_indexes: None,
        requires_any_ability_keyword_indexes: None,
        requires_all_ability_keyword_indexes: None,
        excludes_any_ability_keyword_indexes: None,
        on_hit_effects: None,
        attributes,
        attribute_modifiers: None,
        cast: 0,
    })
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

fn utility(registry: &AssetRegistry, value: Option<&Value>) -> ItemUtility {
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
        entity_stats_to_clear: object
            .and_then(|object| object.get("EntityStatsToClear"))
            .and_then(|value| resolve_entity_stats(registry, value)),
        stat_modifiers: None,
    }
}

/// Decode an explicit/inherited `ItemEntityConfig`, or `None` when the field
/// is absent (Java's `null`, which `processConfig` later replaces with a
/// default).
fn item_entity_config(value: Option<&Value>) -> Option<ItemEntityConfig> {
    let object = value?.as_object()?;
    Some(ItemEntityConfig {
        // An explicit `null` decodes as Java `null`; an absent key keeps the
        // codec field initializer `DEFAULT_PARTICLE_SYSTEM_ID`.
        particle_system_id: match object.get("ParticleSystemId") {
            Some(Value::Null) => None,
            Some(Value::String(id)) => Some(id.clone()),
            Some(_) => return None,
            None => Some(DEFAULT_PARTICLE_SYSTEM.to_string()),
        },
        particle_color: color(object.get("ParticleColor")),
        show_item_particles: object
            .get("ShowItemParticles")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

/// Java inherits `ItemEntity` from the parent *after* the parent's
/// `processConfig` ran, so a parent's quality-driven config propagates to
/// children that do not define their own field. The child's own `Quality` is
/// not inherited and overrides the chain when present.
fn resolved_item_entity(
    registry: &AssetRegistry,
    key: &str,
    json: &Value,
    seen: &mut std::collections::BTreeSet<String>,
) -> Option<ItemEntityConfig> {
    if !seen.insert(key.to_ascii_lowercase()) {
        return None;
    }
    let local = registry.item_local_json(key);
    let has_block = json.get("BlockType").is_some_and(Value::is_object);
    let mut entity = match local.and_then(|local| local.get("ItemEntity")) {
        Some(value) => item_entity_config(Some(value)),
        None => registry
            .items
            .get(key)
            .and_then(|record| record.parent.clone())
            .and_then(|parent| {
                registry
                    .items
                    .get(&parent)
                    .map(|record| resolved_item_entity(registry, &parent, &record.value, seen))
            })
            .flatten(),
    };

    if let Some(quality) = local
        .and_then(|local| local.get("Quality"))
        .and_then(Value::as_str)
    {
        let quality_index = registry.item_qualities.id_of(quality).unwrap_or(0);
        if let Some(record) = registry.item_qualities.get_by_id(quality_index) {
            entity = item_entity_config(record.value.get("ItemEntityConfig"));
        }
    }

    entity.or_else(|| Some(default_item_entity(has_block)))
}

/// Java `ItemEntityConfig.DEFAULT_BLOCK` / `ItemEntityConfig.DEFAULT`.
fn default_item_entity(has_block: bool) -> ItemEntityConfig {
    ItemEntityConfig {
        particle_system_id: (!has_block).then(|| DEFAULT_PARTICLE_SYSTEM.to_string()),
        particle_color: None,
        show_item_particles: true,
    }
}

/// Resolve `Item.PlayerAnimationsId` through the item's own document and parent
/// chain. A contained animation object resolves to the id Java generates for
/// the document that defines it (`*<document>_PlayerAnimationsId`); otherwise
/// the field is inherited as a plain string.
fn player_animations_id(registry: &AssetRegistry, key: &str) -> String {
    let mut current = Some(key.to_string());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(candidate) = current {
        if !seen.insert(candidate.to_ascii_lowercase()) {
            break;
        }
        if let Some(local) = registry.item_local_json(&candidate) {
            match local.get("PlayerAnimationsId") {
                Some(Value::String(id)) => return id.clone(),
                Some(Value::Object(_)) => {
                    return format!("*{candidate}_PlayerAnimationsId");
                }
                _ => {}
            }
        }
        current = registry
            .items
            .get(&candidate)
            .and_then(|record| record.parent.clone());
    }
    DEFAULT_PLAYER_ANIMATIONS.to_string()
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

/// Map an item `Interactions`/`CarryInteractions`/`InteractionVars` object to
/// root-interaction indices (Java `RootInteraction.getRootInteractionIdOrUnknown`).
fn item_root_map(
    registry: &AssetRegistry,
    value: Option<&Value>,
) -> BTreeMap<InteractionType, i32> {
    let mut map = BTreeMap::new();
    let Some(entries) = value.and_then(Value::as_object) else {
        return map;
    };
    for (type_name, value) in entries {
        let Some(interaction_type) = interaction_type(type_name) else {
            continue;
        };
        let Some(root) = value.as_str() else {
            continue;
        };
        map.insert(
            interaction_type,
            registry
                .root_interactions
                .id_of(root)
                .unwrap_or(INTERACTION_NOT_FOUND),
        );
    }
    map
}

/// Java `Item.processConfig`: the item's own `Interactions`, then the unarmed
/// interactions for its `PlayerAnimationsId`, then the default `Empty` map.
/// `putIfAbsent` semantics: earlier entries win.
fn item_interactions(
    registry: &AssetRegistry,
    json: &Value,
    player_animations: &str,
) -> BTreeMap<InteractionType, i32> {
    let mut interactions = item_root_map(registry, json.get("Interactions"));
    for id in [player_animations, "Empty"] {
        let Some(unarmed) = registry.unarmed_interactions_by_id.get(id) else {
            continue;
        };
        for (interaction_type, root) in unarmed {
            interactions.entry(*interaction_type).or_insert_with(|| {
                registry
                    .root_interactions
                    .id_of(root)
                    .unwrap_or(INTERACTION_NOT_FOUND)
            });
        }
    }
    interactions
}

/// `InteractionVars` is a plain `string` -> root-interaction map.
fn item_interaction_vars(registry: &AssetRegistry, value: Option<&Value>) -> BTreeMap<String, i32> {
    let mut map = BTreeMap::new();
    let Some(entries) = value.and_then(Value::as_object) else {
        return map;
    };
    for (name, value) in entries {
        let Some(root) = value.as_str() else {
            continue;
        };
        map.insert(
            name.clone(),
            registry
                .root_interactions
                .id_of(root)
                .unwrap_or(INTERACTION_NOT_FOUND),
        );
    }
    map
}

/// `CarryHudInputBindings` is a plain `InteractionType` -> string map.
fn item_hud_bindings(value: Option<&Value>) -> BTreeMap<InteractionType, String> {
    let mut map = BTreeMap::new();
    let Some(entries) = value.and_then(Value::as_object) else {
        return map;
    };
    for (type_name, value) in entries {
        let Some(interaction_type) = interaction_type(type_name) else {
            continue;
        };
        let Some(binding) = value.as_str() else {
            continue;
        };
        map.insert(interaction_type, binding.to_string());
    }
    map
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
fn display_entity_stats_hud(registry: &AssetRegistry, value: Option<&Value>) -> Option<Vec<i32>> {
    let ids = resolve_entity_stats(registry, value?)?;
    (!ids.is_empty()).then_some(ids)
}

/// Java `EntityStatsModule.resolveEntityStats`: map names to the loaded
/// `EntityStatType` ids; numeric ids are accepted for already-resolved data.
fn resolve_entity_stats(registry: &AssetRegistry, value: &Value) -> Option<Vec<i32>> {
    let values = value.as_array()?;
    values
        .iter()
        .map(|value| {
            if let Some(id) = value.as_i64() {
                return i32::try_from(id).ok();
            }
            registry.entity_stat_id(value.as_str()?)
        })
        .collect()
}
