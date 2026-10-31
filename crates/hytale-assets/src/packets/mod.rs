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

mod block_type;
mod child;
mod item;
mod tag_pattern;

use std::collections::{BTreeMap, BTreeSet};

use hytale_protocol::generated::assets::{
    UpdateAbilityKeywords, UpdateAmbienceFX, UpdateBeamConfigs, UpdateBlockHitboxes,
    UpdateBlockSets, UpdateConnectedBlockRuleSets, UpdateEntityEffects, UpdateEntityStatTypes,
    UpdateEntityUIComponents, UpdateEnvironments, UpdateEqualizerEffects,
    UpdateFieldcraftCategories, UpdateFluids, UpdateHitboxCollisionConfig, UpdateInteractions,
    UpdateItemCategories, UpdateItemPlayerAnimations, UpdateItemQualities, UpdateItemReticles,
    UpdateItemSoundSets, UpdateModelvfxs, UpdateMusicContainers, UpdateMusicTransitions,
    UpdateParticleSpawners, UpdateParticleSystems, UpdatePhysicalMaterials,
    UpdateProjectileConfigs, UpdateRecipes, UpdateRepulsionConfig, UpdateResourceTypes,
    UpdateReverbEffects, UpdateRootInteractions, UpdateSoundSets, UpdateStingers, UpdateTrails,
    UpdateUnarmedInteractions, UpdateWeathers,
};
use hytale_protocol::generated::common::{
    AbilityKeyword, BeamConfig, BlockSet, BlockTextures, CameraAxis, CameraNode, CameraSettings,
    CollisionType, Color, ColorAlpha, ColorLight, CombatTextEntityUIAnimationEventType,
    CombatTextEntityUIComponentAnimationEvent, CurveType, Direction, Edge, EffectDirection,
    EmitShape, EntityEffect, EntityPart, EntityStatEffects, EntityStatResetBehavior,
    EntityStatType, EntityUIComponent, EntityUIType, EqualizerEffect, FXRenderMode, Fluid,
    FluidDrawType, FluidParticle, Hitbox, HitboxCollisionConfig, InitialVelocity,
    IntersectionHighlight, ItemAnimation, ItemCategory, ItemGridInfoDisplayMode,
    ItemPlayerAnimations, ItemPullbackConfiguration, ItemQuality, ItemReticle,
    ItemReticleClientEvent, ItemReticleConfig, ItemSoundEvent, ItemSoundSet, LoopOption, Model,
    ModelParticle, ModelVFX, Opacity, OverlapBehavior as ProtocolOverlapBehavior, Particle,
    ParticleAnimationFrame, ParticleAttractor, ParticleCollision, ParticleCollisionAction,
    ParticleCollisionBlockType, ParticleRotationInfluence, ParticleScaleRatioConstraint,
    ParticleSpawner, ParticleSpawnerGroup, ParticleSystem, ParticleUVOption, PhysicalMaterial,
    PhysicsConfig, PhysicsType, ProjectileConfig, Range, RangeVector2f, RangeVector3f, Rangef,
    RepulsionConfig, ResourceType, ReverbEffect, RotationMode, ShaderType, Size, SoftParticle,
    SoundCategory, SoundSet, SubCategoryDefinition, SwitchTo, Trail, UVMotion, UVMotionCurveType,
    UpdateType, ValueType, Vector2i, Weather, WiggleWeights, WorldEnvironment,
};
use hytale_protocol::generated::wire::{Packet, Vector2f, Vector3f, Wire};
use hytale_protocol::io::{ProtocolError, Writer};
use serde_json::Value;
use thiserror::Error;

use crate::registry::{AssetCollection, AssetRegistry, TagPatternNode};

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
/// value, `key=value` composite and `TagPattern` `Equals` tag found in the
/// loaded documents. This differs from Java's creation order but is
/// deterministic and identical for repeated calls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagRegistry {
    indexes: BTreeMap<String, i32>,
}

impl TagRegistry {
    /// Collect tags from every loaded asset document and tag pattern.
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
        for record in registry.block_types.iter() {
            let json = &record.value.json;
            if let Some(name) = json.get("TransitionToTag").and_then(Value::as_str) {
                names.insert(name.to_string());
            }
            collect_support_tags(json.get("Support"), &mut names);
            collect_support_tags(json.get("Supporting"), &mut names);
        }
        // `EqualsTagOp.afterDecode` calls `getOrCreateTagIndex`, so a tag used
        // only by a pattern still receives a deterministic client index.
        for record in registry.tag_patterns.iter() {
            collect_pattern_tags(&record.value.pattern, &mut names);
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

fn collect_pattern_tags(pattern: &TagPatternNode, names: &mut BTreeSet<String>) {
    match pattern {
        TagPatternNode::Equals { tag } => {
            names.insert(tag.clone());
        }
        TagPatternNode::And { operands } | TagPatternNode::Or { operands } => {
            for operand in operands {
                collect_pattern_tags(operand, names);
            }
        }
        TagPatternNode::Not { pattern } => collect_pattern_tags(pattern, names),
    }
}

fn collect_support_tags(value: Option<&Value>, names: &mut BTreeSet<String>) {
    let Some(map) = value.and_then(Value::as_object) else {
        return;
    };
    for entries in map.values().filter_map(Value::as_array) {
        for entry in entries {
            if let Some(tag) = entry.get("TagId").and_then(Value::as_str) {
                names.insert(tag.to_string());
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

/// Encode an empty (`Init`) asset-type packet. The caller supplies explicit
/// empty collections so the client's handlers never see a null map/vector.
fn empty_packet<T>(store: &'static str, packet: &T) -> Result<AssetPacket>
where
    T: Packet + Wire,
{
    AssetPacket::encode(store, packet)
}

impl AssetRegistry {
    /// The config-asset packets and the matching `ServerTags` map.
    ///
    /// Java sends one init packet per store with a packet generator. This port
    /// encodes every enabled store. The order is dependency-driven: every
    /// numeric index reference arrives before its consumer.
    ///
    /// `BlockBoundingBoxes`/`PhysicalMaterial` → `SoundEvent` → `BlockSoundSet`
    /// → `FluidFX` → `Fluid` → `Environment` → `TagPattern` → `BlockType` →
    /// `BlockParticleSet`/`BlockBreakingDecal` → item child stores → `Item`.
    /// Tag indexes are owned by the Rust snapshot, so every packet and
    /// `ServerTags` agree.
    pub fn config_assets(&self) -> Result<ConfigAssets> {
        let tags = TagRegistry::from_assets(self);
        Ok(ConfigAssets {
            packets: vec![
                self.block_hitbox_packet()?,
                self.physical_material_packet()?,
                self.sound_event_packet()?,
                self.block_sound_set_packet()?,
                self.fluid_fx_packet()?,
                self.fluid_packet(&tags)?,
                self.environment_packet(&tags)?,
                self.tag_pattern_packet(&tags)?,
                self.block_type_packet(&tags)?,
                self.block_particle_set_packet()?,
                self.block_breaking_decal_packet()?,
                self.item_sound_set_packet()?,
                self.item_quality_packet()?,
                self.item_reticle_packet()?,
                self.item_category_packet()?,
                self.item_player_animations_packet()?,
                self.entity_stat_types_packet()?,
                self.entity_effects_packet()?,
                self.projectile_configs_packet()?,
                self.weathers_packet(&tags)?,
                self.item_packet(&tags)?,
                // Release clients require every asset type to be initialized
                // before `WorldLoadFinished`, even when this port does not yet
                // model that store's content. Java always sends one `Update*`
                // packet per enabled store. The maps/vectors must be present
                // (empty), not null: the client dereferences them while
                // handling the packet.
                self.block_sets_packet()?,
                self.trails_packet()?,
                self.particle_systems_packet()?,
                self.particle_spawners_packet()?,
                self.model_vfxs_packet()?,
                self.fieldcraft_categories_packet()?,
                self.resource_types_packet()?,
                // The gameplay phase sends the real `UpdateRecipes`; this is
                // the required asset-type initialization.
                empty_packet(
                    "Recipes",
                    &UpdateRecipes {
                        recipes: Some(BTreeMap::new()),
                        removed_recipes: Some(Vec::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "AmbienceFX",
                    &UpdateAmbienceFX {
                        ambience_fx: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "Interactions",
                    &UpdateInteractions {
                        interactions: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "RootInteractions",
                    &UpdateRootInteractions {
                        interactions: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "UnarmedInteractions",
                    &UpdateUnarmedInteractions {
                        interactions: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                self.entity_ui_components_packet()?,
                self.hitbox_collision_configs_packet()?,
                self.repulsion_configs_packet()?,
                self.sound_sets_packet()?,
                self.audio_category_packet()?,
                self.reverb_effects_packet()?,
                self.equalizer_effects_packet()?,
                empty_packet(
                    "MusicContainers",
                    &UpdateMusicContainers {
                        music_containers: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                self.audio_state_packet()?,
                empty_packet(
                    "ConnectedBlockRuleSets",
                    &UpdateConnectedBlockRuleSets {
                        rule_sets: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "MusicTransitions",
                    &UpdateMusicTransitions {
                        music_transitions: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                empty_packet(
                    "Stingers",
                    &UpdateStingers {
                        stingers: Some(BTreeMap::new()),
                        ..Default::default()
                    },
                )?,
                self.beam_configs_packet()?,
                self.ability_keywords_packet()?,
            ],
            tags: tags.map(),
        })
    }

    /// The config-asset packets without the tag map.
    pub fn config_packets(&self) -> Result<Vec<AssetPacket>> {
        Ok(self.config_assets()?.packets)
    }

    /// `UpdateTagPatterns` (id 84) from the `TagPattern` store.
    pub fn tag_pattern_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        tag_pattern::tag_pattern_packet(self, tags)
    }

    /// `UpdateBlockTypes` (id 40) from the P0 `BlockType` store.
    pub fn block_type_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        block_type::block_type_packet(self, tags)
    }

    /// `UpdateItems` (id 54) from the P0 `Item` store.
    pub fn item_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        item::item_packet(self, tags)
    }

    /// `UpdateBlockSets` (id 46) from the `BlockSet` store.
    pub fn block_sets_packet(&self) -> Result<AssetPacket> {
        let block_ids: BTreeMap<String, i32> = self
            .block_types
            .iter()
            .filter_map(|record| Some((record.key.clone(), record.id?)))
            .collect();
        let all_blocks: BTreeSet<i32> = block_ids.values().copied().collect();
        let mut groups: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        let mut hitboxes: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        let mut categories: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        for record in self.block_types.iter() {
            let Some(id) = record.id else { continue };
            let json = &record.value.json;
            if let Some(group) = json.get("Group").and_then(Value::as_str) {
                groups.entry(group.to_string()).or_default().insert(id);
            }
            if let Some(hitbox) = json.get("HitboxType").and_then(Value::as_str) {
                let hitbox = hitbox.split('|').next().unwrap_or(hitbox);
                hitboxes.entry(hitbox.to_string()).or_default().insert(id);
            }
            if let Some(item) = self.items.get(&record.key) {
                for category in item
                    .value
                    .get("Categories")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    categories
                        .entry(category.to_string())
                        .or_default()
                        .insert(id);
                }
            }
        }

        let resolve_names = |values: Option<&Value>,
                             names: &BTreeMap<String, i32>,
                             result: &mut BTreeSet<i32>,
                             add: bool| {
            for pattern in values.and_then(Value::as_array).into_iter().flatten() {
                let Some(pattern) = pattern.as_str() else {
                    continue;
                };
                for (name, id) in names {
                    if glob_matches(pattern, name) {
                        if add {
                            result.insert(*id);
                        } else {
                            result.remove(id);
                        }
                    }
                }
            }
        };
        let resolve_groups = |values: Option<&Value>, result: &mut BTreeSet<i32>, add: bool| {
            for pattern in values.and_then(Value::as_array).into_iter().flatten() {
                let Some(pattern) = pattern.as_str() else {
                    continue;
                };
                for (name, ids) in &groups {
                    if glob_matches(pattern, name) {
                        for id in ids {
                            if add {
                                result.insert(*id);
                            } else {
                                result.remove(id);
                            }
                        }
                    }
                }
            }
        };
        let resolve_hitboxes = |values: Option<&Value>, result: &mut BTreeSet<i32>, add: bool| {
            for pattern in values.and_then(Value::as_array).into_iter().flatten() {
                let Some(pattern) = pattern.as_str() else {
                    continue;
                };
                for (name, ids) in &hitboxes {
                    if glob_matches(pattern, name) {
                        for id in ids {
                            if add {
                                result.insert(*id);
                            } else {
                                result.remove(id);
                            }
                        }
                    }
                }
            }
        };
        let resolve_categories = |values: Option<&Value>, result: &mut BTreeSet<i32>, add: bool| {
            for category_list in values.and_then(Value::as_array).into_iter().flatten() {
                let Some(list) = category_list.as_array() else {
                    continue;
                };
                let mut intersection: Option<BTreeSet<i32>> = None;
                for category in list.iter().filter_map(Value::as_str) {
                    let Some(ids) = categories.get(category) else {
                        intersection = Some(BTreeSet::new());
                        break;
                    };
                    intersection = Some(match intersection {
                        Some(current) => current.intersection(ids).copied().collect(),
                        None => ids.clone(),
                    });
                }
                for id in intersection.unwrap_or_default() {
                    if add {
                        result.insert(id);
                    } else {
                        result.remove(&id);
                    }
                }
            }
        };

        let mut block_sets = BTreeMap::new();
        for record in self.block_sets.iter() {
            let json = &record.value;
            let mut set = BTreeSet::new();
            if json
                .get("IncludeAll")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                set.extend(&all_blocks);
            }
            resolve_names(json.get("IncludeBlockTypes"), &block_ids, &mut set, true);
            resolve_groups(json.get("IncludeBlockGroups"), &mut set, true);
            resolve_hitboxes(json.get("IncludeHitboxTypes"), &mut set, true);
            resolve_categories(json.get("IncludeCategories"), &mut set, true);
            resolve_names(json.get("ExcludeBlockTypes"), &block_ids, &mut set, false);
            resolve_groups(json.get("ExcludeBlockGroups"), &mut set, false);
            resolve_hitboxes(json.get("ExcludeHitboxTypes"), &mut set, false);
            resolve_categories(json.get("ExcludeCategories"), &mut set, false);
            block_sets.insert(
                record.key.clone(),
                BlockSet {
                    name: Some(record.key.clone()),
                    blocks: Some(set.into_iter().collect()),
                },
            );
        }
        let packet = UpdateBlockSets {
            r#type: UpdateType::Init,
            block_sets: Some(block_sets),
        };
        AssetPacket::encode("BlockSet", &packet)
    }

    /// `UpdateParticleSystems` (id 49) from the `ParticleSystem` store.
    pub fn particle_systems_packet(&self) -> Result<AssetPacket> {
        let mut systems = BTreeMap::new();
        for record in self.particle_systems.iter() {
            systems.insert(
                record.key.clone(),
                particle_system(&record.key, &record.value),
            );
        }
        let packet = UpdateParticleSystems {
            r#type: UpdateType::Init,
            particle_systems: Some(systems),
            // Java's init packet leaves the removed list null.
            removed_particle_systems: None,
        };
        AssetPacket::encode("ParticleSystem", &packet)
    }

    /// `UpdateParticleSpawners` (id 50) from the `ParticleSpawner` store.
    pub fn particle_spawners_packet(&self) -> Result<AssetPacket> {
        let mut spawners = BTreeMap::new();
        for record in self.particle_spawners.iter() {
            spawners.insert(
                record.key.clone(),
                particle_spawner(&record.key, &record.value),
            );
        }
        let packet = UpdateParticleSpawners {
            r#type: UpdateType::Init,
            particle_spawners: Some(spawners),
            removed_particle_spawners: None,
        };
        AssetPacket::encode("ParticleSpawner", &packet)
    }

    /// `UpdateEntityUIComponents` (id 73) from the `EntityUIComponent` store.
    pub fn entity_ui_components_packet(&self) -> Result<AssetPacket> {
        let mut components = BTreeMap::new();
        for record in self.entity_ui_components.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "EntityUIComponent",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let entity_type = match json.get("Type").and_then(Value::as_str) {
                Some("CombatText") => EntityUIType::CombatText,
                _ => EntityUIType::EntityStat,
            };
            let hitbox_offset = json
                .get("HitboxOffset")
                .and_then(Value::as_object)
                .map(|offset| Vector2f {
                    x: offset.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    y: offset.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                })
                .unwrap_or_default();
            let offset_range = json
                .get("RandomPositionOffsetRange")
                .and_then(Value::as_object)
                .map(|offset| {
                    let range_of = |name: &str| -> Option<Rangef> {
                        let range = offset.get(name)?.as_object()?;
                        Some(Rangef {
                            min: range.get("Min").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            max: range.get("Max").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        })
                    };
                    RangeVector2f {
                        x: range_of("X"),
                        y: range_of("Y"),
                    }
                })
                .unwrap_or_default();
            let animation_events = json
                .get("AnimationEvents")
                .and_then(Value::as_array)
                .map(|events| {
                    events
                        .iter()
                        .filter_map(|event| {
                            let event = event.as_object()?;
                            let position_offset = event
                                .get("PositionOffset")
                                .and_then(Value::as_object)
                                .map(|offset| Vector2f {
                                    x: offset.get("X").and_then(Value::as_f64).unwrap_or(0.0)
                                        as f32,
                                    y: offset.get("Y").and_then(Value::as_f64).unwrap_or(0.0)
                                        as f32,
                                })
                                .unwrap_or_default();
                            Some(CombatTextEntityUIComponentAnimationEvent {
                                r#type: match event.get("Type").and_then(Value::as_str) {
                                    Some("Position") => {
                                        CombatTextEntityUIAnimationEventType::Position
                                    }
                                    Some("Opacity") => {
                                        CombatTextEntityUIAnimationEventType::Opacity
                                    }
                                    _ => CombatTextEntityUIAnimationEventType::Scale,
                                },
                                start_at: event
                                    .get("StartAt")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                                end_at: event.get("EndAt").and_then(Value::as_f64).unwrap_or(0.0)
                                    as f32,
                                start_scale: event
                                    .get("StartScale")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                                end_scale: event
                                    .get("EndScale")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                                position_offset,
                                start_opacity: event
                                    .get("StartOpacity")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                                end_opacity: event
                                    .get("EndOpacity")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            components.insert(
                id,
                EntityUIComponent {
                    r#type: entity_type,
                    hitbox_offset,
                    unknown: false,
                    entity_stat_index: json
                        .get("EntityStat")
                        .and_then(Value::as_str)
                        .and_then(|stat| self.entity_stat_id(stat))
                        .unwrap_or(0),
                    combat_text_random_position_offset_range: offset_range,
                    combat_text_viewport_margin: number(json, "ViewportMargin") as f32,
                    combat_text_duration: number(json, "Duration") as f32,
                    combat_text_hit_angle_modifier_strength: number(
                        json,
                        "HitAngleModifierStrength",
                    ) as f32,
                    combat_text_font_size: number(json, "FontSize") as f32,
                    combat_text_color: color(json.get("TextColor")).unwrap_or_default(),
                    combat_text_animation_events: animation_events,
                },
            );
        }
        let packet = UpdateEntityUIComponents {
            r#type: UpdateType::Init,
            max_id: next_id(&self.entity_ui_components, "EntityUIComponent")?,
            components: Some(components),
        };
        AssetPacket::encode("EntityUIComponent", &packet)
    }

    /// `UpdateModelvfxs` (id 53) from the `ModelVFX` store.
    pub fn model_vfxs_packet(&self) -> Result<AssetPacket> {
        let mut model_vfxs = BTreeMap::new();
        for record in self.model_vfxs.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "ModelVFX",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            model_vfxs.insert(
                id,
                ModelVFX {
                    id: Some(record.key.clone()),
                    switch_to: match json.get("SwitchTo").and_then(Value::as_str) {
                        Some("PostColor") => SwitchTo::PostColor,
                        Some("Distortion") => SwitchTo::Distortion,
                        Some("Transparency") => SwitchTo::Transparency,
                        Some("AlphaTransparency") => SwitchTo::AlphaTransparency,
                        _ => SwitchTo::Disappear,
                    },
                    effect_direction: match json.get("EffectDirection").and_then(Value::as_str) {
                        Some("BottomUp") => EffectDirection::BottomUp,
                        Some("TopDown") => EffectDirection::TopDown,
                        Some("ToCenter") => EffectDirection::ToCenter,
                        Some("FromCenter") => EffectDirection::FromCenter,
                        _ => EffectDirection::None,
                    },
                    animation_duration: number(json, "AnimationDuration") as f32,
                    animation_range: json.get("AnimationRange").and_then(Value::as_object).map(
                        |range| Vector2f {
                            x: range.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            y: range.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        },
                    ),
                    loop_option: match json.get("LoopOption").and_then(Value::as_str) {
                        Some("Loop") => LoopOption::Loop,
                        Some("LoopMirror") => LoopOption::LoopMirror,
                        _ => LoopOption::PlayOnce,
                    },
                    curve_type: match json.get("CurveType").and_then(Value::as_str) {
                        Some("QuartIn") => CurveType::QuartIn,
                        Some("QuartOut") => CurveType::QuartOut,
                        Some("QuartInOut") => CurveType::QuartInOut,
                        _ => CurveType::Linear,
                    },
                    highlight_color: color(json.get("HighlightColor")),
                    highlight_thickness: number(json, "HighlightThickness") as f32,
                    use_bloom_on_highlight: json
                        .get("UseBloomOnHighlight")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    use_progessive_highlight: json
                        .get("UseProgessiveHighlight")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    noise_scale: json
                        .get("NoiseScale")
                        .and_then(Value::as_object)
                        .map(|scale| Vector2f {
                            x: scale.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            y: scale.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        }),
                    noise_scroll_speed: json
                        .get("NoiseScrollSpeed")
                        .and_then(Value::as_object)
                        .map(|speed| Vector2f {
                            x: speed.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                            y: speed.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        }),
                    post_color: color(json.get("PostColor")),
                    post_color_opacity: number(json, "PostColorOpacity") as f32,
                    opacity: number(json, "Opacity") as f32,
                },
            );
        }
        let packet = UpdateModelvfxs {
            r#type: UpdateType::Init,
            max_id: next_id(&self.model_vfxs, "ModelVFX")?,
            model_vf_xs: Some(model_vfxs),
        };
        AssetPacket::encode("ModelVFX", &packet)
    }

    /// `UpdateTrails` (id 48) from the `Trail` store.
    pub fn trails_packet(&self) -> Result<AssetPacket> {
        let mut trails = BTreeMap::new();
        for record in self.trails.iter() {
            let json = &record.value;
            let start = json.get("Start").and_then(edge);
            let end = json.get("End").and_then(edge);
            let animation = json.get("Animation").and_then(Value::as_object);
            trails.insert(
                record.key.clone(),
                Trail {
                    id: Some(record.key.clone()),
                    texture: json
                        .get("TexturePath")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    life_span: json.get("LifeSpan").and_then(Value::as_i64).unwrap_or(0) as i32,
                    roll: number(json, "Roll") as f32,
                    start,
                    end,
                    light_influence: number(json, "LightInfluence") as f32,
                    render_mode: match json.get("RenderMode").and_then(Value::as_str) {
                        Some("BlendAdd") => FXRenderMode::BlendAdd,
                        Some("Erosion") => FXRenderMode::Erosion,
                        Some("Distortion") => FXRenderMode::Distortion,
                        _ => FXRenderMode::BlendLinear,
                    },
                    intersection_highlight: None,
                    smooth: json.get("Smooth").and_then(Value::as_bool).unwrap_or(false),
                    frame_size: animation
                        .and_then(|animation| animation.get("FrameSize"))
                        .and_then(Value::as_object)
                        .map(|size| Vector2i {
                            x: size.get("X").and_then(Value::as_i64).unwrap_or(0) as i32,
                            y: size.get("Y").and_then(Value::as_i64).unwrap_or(0) as i32,
                        }),
                    frame_range: animation
                        .and_then(|animation| animation.get("FrameRange"))
                        .and_then(Value::as_object)
                        .map(|range| Range {
                            min: range.get("Min").and_then(Value::as_i64).unwrap_or(0) as i32,
                            max: range.get("Max").and_then(Value::as_i64).unwrap_or(0) as i32,
                        }),
                    frame_life_span: animation
                        .and_then(|animation| animation.get("FrameLifeSpan"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i32,
                },
            );
        }
        let packet = UpdateTrails {
            r#type: UpdateType::Init,
            trails: Some(trails),
        };
        AssetPacket::encode("Trail", &packet)
    }

    /// `UpdateHitboxCollisionConfig` (id 74).
    pub fn hitbox_collision_configs_packet(&self) -> Result<AssetPacket> {
        let mut configs = BTreeMap::new();
        for record in self.hitbox_collision_configs.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "HitboxCollisionConfig",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            configs.insert(
                id,
                HitboxCollisionConfig {
                    collision_type: match json.get("CollisionType").and_then(Value::as_str) {
                        Some("Soft") => CollisionType::Soft,
                        _ => CollisionType::Hard,
                    },
                    soft_collision_offset_ratio: number(json, "SoftCollisionOffsetRatio") as f32,
                    rotate_hitbox: json
                        .get("RotateHitbox")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    allow_entity_anchoring: json
                        .get("AllowEntityAnchoring")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    rotate_anchored_entities: json
                        .get("RotateAnchoredEntities")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            );
        }
        let packet = UpdateHitboxCollisionConfig {
            r#type: UpdateType::Init,
            max_id: next_id(&self.hitbox_collision_configs, "HitboxCollisionConfig")?,
            hitbox_collision_configs: Some(configs),
        };
        AssetPacket::encode("HitboxCollisionConfig", &packet)
    }

    /// `UpdateRepulsionConfig` (id 75).
    pub fn repulsion_configs_packet(&self) -> Result<AssetPacket> {
        let mut configs = BTreeMap::new();
        for record in self.repulsion_configs.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "RepulsionConfig",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            configs.insert(
                id,
                RepulsionConfig {
                    radius: number(json, "Radius") as f32,
                    min_force: number(json, "MinForce") as f32,
                    max_force: number(json, "MaxForce") as f32,
                },
            );
        }
        let packet = UpdateRepulsionConfig {
            r#type: UpdateType::Init,
            max_id: next_id(&self.repulsion_configs, "RepulsionConfig")?,
            repulsion_configs: Some(configs),
        };
        AssetPacket::encode("RepulsionConfig", &packet)
    }

    /// `UpdateAbilityKeywords` (id 94).
    pub fn ability_keywords_packet(&self) -> Result<AssetPacket> {
        let mut keywords = BTreeMap::new();
        for record in self.ability_keywords.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "AbilityKeyword",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            keywords.insert(
                id,
                AbilityKeyword {
                    id: Some(record.key.clone()),
                    translation_key: json
                        .get("Name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    show_on_tooltip: json
                        .get("ShowOnTooltip")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                },
            );
        }
        let packet = UpdateAbilityKeywords {
            r#type: UpdateType::Init,
            max_id: next_id(&self.ability_keywords, "AbilityKeyword")?,
            ability_keywords: Some(keywords),
        };
        AssetPacket::encode("AbilityKeyword", &packet)
    }

    /// `UpdateReverbEffects` (id 81) from the `ReverbEffect` store.
    pub fn reverb_effects_packet(&self) -> Result<AssetPacket> {
        let mut effects = BTreeMap::new();
        for record in self.reverb_effects.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "ReverbEffect",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            effects.insert(
                id,
                ReverbEffect {
                    id: Some(record.key.clone()),
                    dry_gain: number(json, "DryGain") as f32,
                    modal_density: number(json, "ModalDensity") as f32,
                    diffusion: number(json, "Diffusion") as f32,
                    gain: number(json, "Gain") as f32,
                    high_frequency_gain: number(json, "HighFrequencyGain") as f32,
                    decay_time: number(json, "DecayTime") as f32,
                    high_frequency_decay_ratio: number(json, "HighFrequencyDecayRatio") as f32,
                    reflection_gain: number(json, "ReflectionGain") as f32,
                    reflection_delay: number(json, "ReflectionDelay") as f32,
                    late_reverb_gain: number(json, "LateReverbGain") as f32,
                    late_reverb_delay: number(json, "LateReverbDelay") as f32,
                    room_rolloff_factor: number(json, "RoomRolloffFactor") as f32,
                    air_absorption_high_frequency_gain: number(
                        json,
                        "AirAbsorbptionHighFrequencyGain",
                    ) as f32,
                    limit_decay_high_frequency: json
                        .get("LimitDecayHighFrequency")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            );
        }
        let packet = UpdateReverbEffects {
            r#type: UpdateType::Init,
            max_id: next_id(&self.reverb_effects, "ReverbEffect")?,
            effects: Some(effects),
        };
        AssetPacket::encode("ReverbEffect", &packet)
    }

    /// `UpdateEqualizerEffects` (id 82) from the `EqualizerEffect` store.
    pub fn equalizer_effects_packet(&self) -> Result<AssetPacket> {
        let mut effects = BTreeMap::new();
        for record in self.equalizer_effects.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "EqualizerEffect",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            effects.insert(
                id,
                EqualizerEffect {
                    id: Some(record.key.clone()),
                    low_gain: number(json, "LowGain") as f32,
                    low_cut_off: number(json, "LowCutOff") as f32,
                    low_mid_gain: number(json, "LowMidGain") as f32,
                    low_mid_center: number(json, "LowMidCenter") as f32,
                    low_mid_width: number(json, "LowMidWidth") as f32,
                    high_mid_gain: number(json, "HighMidGain") as f32,
                    high_mid_center: number(json, "HighMidCenter") as f32,
                    high_mid_width: number(json, "HighMidWidth") as f32,
                    high_gain: number(json, "HighGain") as f32,
                    high_cut_off: number(json, "HighCutOff") as f32,
                },
            );
        }
        let packet = UpdateEqualizerEffects {
            r#type: UpdateType::Init,
            max_id: next_id(&self.equalizer_effects, "EqualizerEffect")?,
            effects: Some(effects),
        };
        AssetPacket::encode("EqualizerEffect", &packet)
    }

    /// `UpdateSoundSets` (id 79) from the `SoundSet` store.
    pub fn sound_sets_packet(&self) -> Result<AssetPacket> {
        let mut sound_sets = BTreeMap::new();
        for record in self.sound_sets.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "SoundSet",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let sounds = json
                .get("SoundEvents")
                .and_then(Value::as_object)
                .map(|events| {
                    events
                        .iter()
                        .filter_map(|(name, value)| {
                            let event = value.as_str()?;
                            Some((name.clone(), self.sound_events.id_of(event).unwrap_or(0)))
                        })
                        .collect::<BTreeMap<_, _>>()
                })
                .filter(|sounds| !sounds.is_empty());
            let category = match json.get("Category").and_then(Value::as_str) {
                Some("Music") => SoundCategory::Music,
                Some("Ambient") => SoundCategory::Ambient,
                Some("UI") => SoundCategory::UI,
                Some("Voice") => SoundCategory::Voice,
                _ => SoundCategory::SFX,
            };
            sound_sets.insert(
                id,
                SoundSet {
                    id: Some(record.key.clone()),
                    sounds,
                    category,
                },
            );
        }
        let packet = UpdateSoundSets {
            r#type: UpdateType::Init,
            max_id: next_id(&self.sound_sets, "SoundSet")?,
            sound_sets: Some(sound_sets),
        };
        AssetPacket::encode("SoundSet", &packet)
    }

    /// `UpdateResourceTypes` (id 59) from the `ResourceType` store.
    pub fn resource_types_packet(&self) -> Result<AssetPacket> {
        let mut resource_types = BTreeMap::new();
        for record in self.resource_types.iter() {
            resource_types.insert(
                record.key.clone(),
                ResourceType {
                    id: Some(record.key.clone()),
                    icon: record
                        .value
                        .get("Icon")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                },
            );
        }
        let packet = UpdateResourceTypes {
            r#type: UpdateType::Init,
            resource_types: Some(resource_types),
        };
        AssetPacket::encode("ResourceType", &packet)
    }

    /// `UpdateFieldcraftCategories` (id 58) from the `FieldcraftCategory` store.
    pub fn fieldcraft_categories_packet(&self) -> Result<AssetPacket> {
        let categories = self
            .fieldcraft_categories
            .iter()
            .map(|record| ItemCategory {
                id: Some(record.key.clone()),
                name: record
                    .value
                    .get("Name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                icon: record
                    .value
                    .get("Icon")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                order: record
                    .value
                    .get("Order")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                info_display_mode: ItemGridInfoDisplayMode::default(),
                children: None,
                sub_categories: None,
            })
            .collect();
        let packet = UpdateFieldcraftCategories {
            r#type: UpdateType::Init,
            item_categories: Some(categories),
        };
        AssetPacket::encode("FieldcraftCategory", &packet)
    }

    /// `UpdateBeamConfigs` (id 93) from the `Beam` store.
    pub fn beam_configs_packet(&self) -> Result<AssetPacket> {
        let mut beam_configs = BTreeMap::new();
        for record in self.beams.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "Beam",
                key: record.key.clone(),
            })?;
            beam_configs.insert(
                id,
                BeamConfig {
                    texture: record
                        .value
                        .get("TexturePath")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                },
            );
        }
        let packet = UpdateBeamConfigs {
            r#type: UpdateType::Init,
            max_id: next_id(&self.beams, "Beam")?,
            beam_configs: Some(beam_configs),
        };
        AssetPacket::encode("BeamConfig", &packet)
    }

    /// `UpdateItemSoundSets` (id 43) from the `ItemSoundSet` store.
    pub fn item_sound_set_packet(&self) -> Result<AssetPacket> {
        let mut sets = BTreeMap::new();
        for record in self.item_sound_sets.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "ItemSoundSet",
                key: record.key.clone(),
            })?;
            let mut indices = BTreeMap::new();
            if let Some(events) = record.value.get("SoundEvents").and_then(Value::as_object) {
                for (name, value) in events {
                    let event = match name.as_str() {
                        "Drag" => ItemSoundEvent::Drag,
                        "Drop" => ItemSoundEvent::Drop,
                        other => {
                            return Err(invalid(
                                "ItemSoundSet",
                                &record.key,
                                format!("unknown ItemSoundEvent `{other}`"),
                            ))
                        }
                    };
                    let sound = value.as_str().ok_or_else(|| {
                        invalid(
                            "ItemSoundSet",
                            &record.key,
                            "SoundEvents values must be strings",
                        )
                    })?;
                    let index = self.sound_event_index(sound).ok_or_else(|| {
                        invalid(
                            "ItemSoundSet",
                            &record.key,
                            format!("missing SoundEvent `{sound}`"),
                        )
                    })?;
                    indices.insert(event, index);
                }
            }
            sets.insert(
                id,
                ItemSoundSet {
                    // Java `processConfig` always assigns a (possibly empty)
                    // index map.
                    id: Some(record.key.clone()),
                    sound_event_indices: Some(indices),
                },
            );
        }
        let packet = UpdateItemSoundSets {
            r#type: UpdateType::Init,
            max_id: next_id(&self.item_sound_sets, "ItemSoundSet")?,
            item_sound_sets: Some(sets),
        };
        AssetPacket::encode("ItemSoundSet", &packet)
    }

    /// `UpdateItemQualities` (id 55) from the `ItemQuality` store.
    pub fn item_quality_packet(&self) -> Result<AssetPacket> {
        let mut qualities = BTreeMap::new();
        for record in self.item_qualities.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "ItemQuality",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            qualities.insert(
                id,
                ItemQuality {
                    id: Some(record.key.clone()),
                    item_tooltip_texture: text(json, "ItemTooltipTexture"),
                    item_tooltip_arrow_texture: text(json, "ItemTooltipArrowTexture"),
                    slot_texture: text(json, "SlotTexture"),
                    block_slot_texture: text(json, "BlockSlotTexture"),
                    special_slot_texture: text(json, "SpecialSlotTexture"),
                    text_color: color(json.get("TextColor")),
                    localization_key: text(json, "LocalizationKey"),
                    visible_quality_label: boolean(json, "VisibleQualityLabel", false),
                    render_special_slot: boolean(json, "RenderSpecialSlot", false),
                    hide_from_search: boolean(json, "HideFromSearch", false),
                },
            );
        }
        let packet = UpdateItemQualities {
            r#type: UpdateType::Init,
            max_id: next_id(&self.item_qualities, "ItemQuality")?,
            item_qualities: Some(qualities),
        };
        AssetPacket::encode("ItemQuality", &packet)
    }

    /// `UpdateItemReticles` (id 57) from the `ItemReticleConfig` store.
    pub fn item_reticle_packet(&self) -> Result<AssetPacket> {
        let event_indexes = self.reticle_event_indexes();
        let mut reticles = BTreeMap::new();
        for record in self.item_reticles.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "ItemReticleConfig",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let base = json.get("Base").and_then(Value::as_array).map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            });

            let server_events = match json.get("ServerEvents").and_then(Value::as_object) {
                Some(events) if !events.is_empty() => {
                    let mut map = BTreeMap::new();
                    for (event, value) in events {
                        let index = event_indexes.get(event).copied().ok_or_else(|| {
                            invalid(
                                "ItemReticleConfig",
                                &record.key,
                                format!("missing server event `{event}`"),
                            )
                        })?;
                        map.insert(index, reticle(value, 0.25));
                    }
                    Some(map)
                }
                _ => None,
            };

            let client_events = match json.get("ClientEvents").and_then(Value::as_object) {
                Some(events) if !events.is_empty() => {
                    let mut map = BTreeMap::new();
                    for (event, value) in events {
                        let event = reticle_client_event(event).ok_or_else(|| {
                            invalid(
                                "ItemReticleConfig",
                                &record.key,
                                format!("unknown ItemReticleClientEvent `{event}`"),
                            )
                        })?;
                        map.insert(event, reticle(value, 0.0));
                    }
                    Some(map)
                }
                _ => None,
            };

            reticles.insert(
                id,
                ItemReticleConfig {
                    // Java `toPacket` does not set the id.
                    id: None,
                    base,
                    server_events,
                    client_events,
                },
            );
        }
        let packet = UpdateItemReticles {
            r#type: UpdateType::Init,
            max_id: next_id(&self.item_reticles, "ItemReticleConfig")?,
            item_reticle_configs: Some(reticles),
        };
        AssetPacket::encode("ItemReticleConfig", &packet)
    }

    /// `UpdateItemCategories` (id 56) from the `ItemCategory` store.
    pub fn item_category_packet(&self) -> Result<AssetPacket> {
        let mut records: Vec<_> = self.item_categories.iter().collect();
        // The Java dumper hands the generator assets in sorted key order, and
        // `ItemCategoryPacketGenerator` preserves that array order.
        records.sort_by(|a, b| a.key.cmp(&b.key));
        let categories = records
            .into_iter()
            .map(|record| item_category(&record.key, &record.value, true))
            .collect::<Result<Vec<_>>>()?;
        let packet = UpdateItemCategories {
            r#type: UpdateType::Init,
            item_categories: Some(categories),
        };
        AssetPacket::encode("ItemCategory", &packet)
    }

    /// `UpdateItemPlayerAnimations` (id 52) from the `ItemPlayerAnimations`
    /// store.
    pub fn item_player_animations_packet(&self) -> Result<AssetPacket> {
        let mut animations = BTreeMap::new();
        for record in self.item_player_animations.iter() {
            animations.insert(
                record.key.clone(),
                item_player_animations(&record.key, &record.value)?,
            );
        }
        let packet = UpdateItemPlayerAnimations {
            r#type: UpdateType::Init,
            item_player_animations: Some(animations),
        };
        AssetPacket::encode("ItemPlayerAnimations", &packet)
    }

    /// `UpdateEntityStatTypes` from the `EntityStatType` store.
    pub fn entity_stat_types_packet(&self) -> Result<AssetPacket> {
        let mut types = BTreeMap::new();
        for record in self.entity_stat_types.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "EntityStatType",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let number = |name: &str| -> f32 {
                json.get(name)
                    .and_then(Value::as_f64)
                    .map(|value| value as f32)
                    .unwrap_or(0.0)
            };
            types.insert(
                id,
                EntityStatType {
                    id: Some(record.key.clone()),
                    value: number("InitialValue"),
                    min: number("Min"),
                    max: number("Max"),
                    min_value_effects: stat_effects(json.get("MinValueEffects")),
                    max_value_effects: stat_effects(json.get("MaxValueEffects")),
                    reset_behavior: match json.get("ResetType").and_then(Value::as_str) {
                        Some("MaxValue") => EntityStatResetBehavior::MaxValue,
                        _ => EntityStatResetBehavior::InitialValue,
                    },
                    hide_from_tooltip: json
                        .get("HideFromTooltip")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            );
        }
        let packet = UpdateEntityStatTypes {
            r#type: UpdateType::Init,
            max_id: next_id(&self.entity_stat_types, "EntityStatType")?,
            types: Some(types),
        };
        AssetPacket::encode("EntityStatType", &packet)
    }

    /// `UpdateEntityEffects` from the `EntityEffect` store.
    pub fn entity_effects_packet(&self) -> Result<AssetPacket> {
        let mut effects = BTreeMap::new();
        for record in self.entity_effects.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "EntityEffect",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let sound = |name: &str| -> i32 {
                json.get(name)
                    .and_then(Value::as_str)
                    .and_then(|id| self.sound_events.id_of(id))
                    .unwrap_or(0)
            };
            let overlap = match json.get("OverlapBehavior").and_then(Value::as_str) {
                Some("Overwrite") => ProtocolOverlapBehavior::Overwrite,
                Some("Extend") => ProtocolOverlapBehavior::Extend,
                _ => ProtocolOverlapBehavior::Ignore,
            };
            let value_type = match json.get("ValueType").and_then(Value::as_str) {
                Some("Percent") | Some("PERCENT") => ValueType::Percent,
                _ => ValueType::Absolute,
            };
            let mut stat_modifiers = BTreeMap::new();
            for (stat, amount) in json
                .get("StatModifiers")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                if let Some(stat) = self.entity_stat_id(stat) {
                    stat_modifiers.insert(stat, amount.as_f64().unwrap_or(0.0) as f32);
                }
            }
            effects.insert(
                id,
                EntityEffect {
                    id: Some(record.key.clone()),
                    name: text(json, "Name"),
                    application_effects: None,
                    world_removal_sound_event_index: sound("WorldRemovalSoundEventId"),
                    local_removal_sound_event_index: sound("LocalRemovalSoundEventId"),
                    model_override: None,
                    duration: json.get("Duration").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    infinite: json
                        .get("Infinite")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    debuff: json.get("Debuff").and_then(Value::as_bool).unwrap_or(false),
                    status_effect_icon: text(json, "StatusEffectIcon"),
                    overlap_behavior: overlap,
                    damage_calculator_cooldown: json
                        .get("DamageCalculatorCooldown")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                    stat_modifiers: (!stat_modifiers.is_empty()).then_some(stat_modifiers),
                    value_type,
                },
            );
        }
        let packet = UpdateEntityEffects {
            r#type: UpdateType::Init,
            max_id: next_id(&self.entity_effects, "EntityEffect")?,
            entity_effects: Some(effects),
        };
        AssetPacket::encode("EntityEffect", &packet)
    }

    /// `UpdateProjectileConfigs` (id 85) from `ProjectileConfig`.
    pub fn projectile_configs_packet(&self) -> Result<AssetPacket> {
        let mut configs = BTreeMap::new();
        for record in self.projectile_configs.iter() {
            let json = &record.value;
            let physics = json.get("Physics").map(|physics| PhysicsConfig {
                r#type: PhysicsType::Standard,
                density: number(physics, "Density"),
                gravity: number(physics, "Gravity"),
                bounciness: number(physics, "Bounciness"),
                bounce_count: number(physics, "BounceCount") as i32,
                bounce_limit: number(physics, "BounceLimit"),
                sticks_vertically: boolean(physics, "SticksVertically", false),
                compute_yaw: boolean(physics, "ComputeYaw", false),
                compute_pitch: boolean(physics, "ComputePitch", false),
                rotation_mode: match physics
                    .get("RotationMode")
                    .and_then(Value::as_str)
                    .unwrap_or("None")
                {
                    "Velocity" => RotationMode::Velocity,
                    "VelocityDamped" => RotationMode::VelocityDamped,
                    "VelocityRoll" => RotationMode::VelocityRoll,
                    _ => RotationMode::None,
                },
                move_out_of_solid_speed: number(physics, "MoveOutOfSolidSpeed"),
                terminal_velocity_air: number(physics, "TerminalVelocityAir"),
                density_air: number(physics, "DensityAir"),
                terminal_velocity_water: number(physics, "TerminalVelocityWater"),
                density_water: number(physics, "DensityWater"),
                hit_water_impulse_loss: number(physics, "HitWaterImpulseLoss"),
                rotation_force: number(physics, "RotationForce"),
                speed_rotation_factor: number(physics, "SpeedRotationFactor") as f32,
                swimming_damping_factor: number(physics, "SwimmingDampingFactor"),
                allow_rolling: boolean(physics, "AllowRolling", false),
                rolling_friction_factor: number(physics, "RollingFrictionFactor"),
                rolling_speed: number(physics, "RollingSpeed") as f32,
            });
            let model = text(json, "Model").map(|asset_id| Model {
                asset_id: Some(asset_id),
                scale: 1.0,
                ..Model::default()
            });
            configs.insert(
                record.key.clone(),
                ProjectileConfig {
                    physics_config: physics,
                    model,
                    launch_force: number(json, "LaunchForce"),
                    spawn_offset: json.get("SpawnOffset").map(|offset| Vector3f {
                        x: number(offset, "X") as f32,
                        y: number(offset, "Y") as f32,
                        z: number(offset, "Z") as f32,
                    }),
                    rotation_offset: json.get("RotationOffset").map(|rotation| Direction {
                        pitch: number(rotation, "Pitch") as f32,
                        yaw: number(rotation, "Yaw") as f32,
                        roll: number(rotation, "Roll") as f32,
                    }),
                    rotate_spawn_offset_by_pitch: boolean(json, "RotateSpawnOffsetByPitch", false),
                    rotate_spawn_offset_by_yaw: boolean(json, "RotateSpawnOffsetByYaw", false),
                    interactions: None,
                    launch_local_sound_event_index: sound_index(
                        self,
                        json,
                        "LaunchLocalSoundEventId",
                    ),
                    launch_world_sound_event_index: sound_index(
                        self,
                        json,
                        "LaunchWorldSoundEventId",
                    ),
                    projectile_sound_event_index: sound_index(self, json, "ProjectileSoundEventId"),
                    min_scale: number(json, "MinScale") as f32,
                    max_scale: number(json, "MaxScale") as f32,
                },
            );
        }
        let packet = UpdateProjectileConfigs {
            r#type: UpdateType::Init,
            configs: Some(configs),
            removed_configs: None,
        };
        AssetPacket::encode("ProjectileConfig", &packet)
    }

    /// `UpdateWeathers` (id 47) from the `Weather` store.
    pub fn weathers_packet(&self, tags: &TagRegistry) -> Result<AssetPacket> {
        let mut weathers = BTreeMap::new();
        for record in self.weathers.iter() {
            let id = record.id.ok_or_else(|| AssetPacketError::MissingId {
                store: "Weather",
                key: record.key.clone(),
            })?;
            let json = &record.value;
            let moons = json.get("Moons").and_then(Value::as_array).map(|moons| {
                moons
                    .iter()
                    .filter_map(|moon| {
                        Some((
                            moon.get("Day").and_then(Value::as_i64)? as i32,
                            moon.get("Texture").and_then(Value::as_str)?.to_string(),
                        ))
                    })
                    .collect()
            });
            weathers.insert(
                id,
                Weather {
                    id: Some(record.key.clone()),
                    tag_indexes: tags.expanded(json.get("Tags")),
                    stars: text(json, "Stars"),
                    moons,
                    ..Weather::default()
                },
            );
        }
        let packet = UpdateWeathers {
            r#type: UpdateType::Init,
            max_id: next_id(&self.weathers, "Weather")?,
            weathers: Some(weathers),
        };
        AssetPacket::encode("Weather", &packet)
    }

    /// Java's `ItemReticleConfig` keys server events by `AssetRegistry`'s global
    /// tag index. That index depends on the whole 54-store load order, so this
    /// snapshot assigns deterministic indexes over the reticle event names.
    fn reticle_event_indexes(&self) -> BTreeMap<String, i32> {
        let mut names = BTreeSet::new();
        for record in self.item_reticles.iter() {
            if let Some(events) = record.value.get("ServerEvents").and_then(Value::as_object) {
                names.extend(events.keys().cloned());
            }
        }
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| (name, index as i32))
            .collect()
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
                    // Java `Fluid.processConfig`: the authored ids default to
                    // the `Empty`/`EMPTY` code-defined assets. Java leaves the
                    // `NOT_FOUND` sentinel for an unknown non-empty id.
                    fluid_fx_index: resolve_fluid_fx(self, json),
                    block_sound_set_index: resolve_block_sound_set(self, json),
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
                    // Java `Environment.toPacket` only emits the map when the
                    // authored `FluidParticles` map is non-empty, and drops keys
                    // whose FluidFX id is unknown.
                    fluid_particles: environment_fluid_particles(self, &record.value)?,
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

fn text(json: &Value, name: &str) -> Option<String> {
    json.get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn boolean(json: &Value, name: &str, fallback: bool) -> bool {
    json.get(name).and_then(Value::as_bool).unwrap_or(fallback)
}

fn ranged_f32(value: Option<&Value>) -> Option<Rangef> {
    let object = value?.as_object()?;
    Some(Rangef {
        min: object.get("Min").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        max: object.get("Max").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn ranged_i32(value: Option<&Value>) -> Option<Range> {
    let object = value?.as_object()?;
    Some(Range {
        min: object.get("Min").and_then(Value::as_i64).unwrap_or(0) as i32,
        max: object.get("Max").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
}

fn range_vector2f(value: Option<&Value>) -> Option<RangeVector2f> {
    let object = value?.as_object()?;
    Some(RangeVector2f {
        x: ranged_f32(object.get("X")),
        y: ranged_f32(object.get("Y")),
    })
}

fn range_vector3f(value: Option<&Value>) -> Option<RangeVector3f> {
    let object = value?.as_object()?;
    Some(RangeVector3f {
        x: ranged_f32(object.get("X")),
        y: ranged_f32(object.get("Y")),
        z: ranged_f32(object.get("Z")),
    })
}

fn direction(value: Option<&Value>) -> Option<Direction> {
    let object = value?.as_object()?;
    Some(Direction {
        yaw: object.get("Yaw").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        pitch: object.get("Pitch").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        roll: object.get("Roll").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn size(value: Option<&Value>) -> Option<Size> {
    let object = value?.as_object()?;
    Some(Size {
        width: object.get("Width").and_then(Value::as_i64).unwrap_or(0) as i32,
        height: object.get("Height").and_then(Value::as_i64).unwrap_or(0) as i32,
    })
}

fn map_number(object: &serde_json::Map<String, Value>, name: &str) -> f64 {
    object.get(name).and_then(Value::as_f64).unwrap_or(0.0)
}

fn particle_animation_frame(value: &Value) -> Option<ParticleAnimationFrame> {
    let object = value.as_object()?;
    Some(ParticleAnimationFrame {
        frame_index: ranged_i32(object.get("FrameIndex")),
        scale: range_vector2f(object.get("Scale")),
        rotation: range_vector3f(object.get("Rotation")),
        color: color(object.get("Color")),
        // Java's `ParticleAnimationFrame.UNASSIGNED_OPACITY`.
        opacity: object
            .get("Opacity")
            .and_then(Value::as_f64)
            .unwrap_or(-1.0) as f32,
    })
}

fn particle(value: Option<&Value>) -> Option<Particle> {
    let object = value?.as_object()?;
    let animation_frames = object
        .get("Animation")
        .and_then(Value::as_object)
        .map(|frames| {
            frames
                .iter()
                .filter_map(|(index, frame)| {
                    Some((index.parse::<i32>().ok()?, particle_animation_frame(frame)?))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .filter(|frames| !frames.is_empty());
    Some(Particle {
        texture_path: object
            .get("Texture")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        frame_size: size(object.get("FrameSize")),
        uv_option: match object.get("UVOption").and_then(Value::as_str) {
            Some("RandomFlipU") => ParticleUVOption::RandomFlipU,
            Some("RandomFlipV") => ParticleUVOption::RandomFlipV,
            Some("RandomFlipUV") => ParticleUVOption::RandomFlipUV,
            Some("FlipU") => ParticleUVOption::FlipU,
            Some("FlipV") => ParticleUVOption::FlipV,
            Some("FlipUV") => ParticleUVOption::FlipUV,
            _ => ParticleUVOption::None,
        },
        scale_ratio_constraint: match object.get("ScaleRatioConstraint").and_then(Value::as_str) {
            Some("Preserved") => ParticleScaleRatioConstraint::Preserved,
            Some("None") => ParticleScaleRatioConstraint::None,
            _ => ParticleScaleRatioConstraint::OneToOne,
        },
        soft_particles: match object.get("SoftParticles").and_then(Value::as_str) {
            Some("Disable") => SoftParticle::Disable,
            Some("Require") => SoftParticle::Require,
            _ => SoftParticle::Enable,
        },
        soft_particles_fade_factor: object
            .get("SoftParticlesFadeFactor")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32,
        camera_near_fade_start_distance: map_number(object, "CameraNearFadeStartDistance") as f32,
        camera_near_fade_end_distance: map_number(object, "CameraNearFadeEndDistance") as f32,
        camera_far_fade_start_distance: map_number(object, "CameraFarFadeStartDistance") as f32,
        camera_far_fade_end_distance: map_number(object, "CameraFarFadeEndDistance") as f32,
        use_sprite_blending: object
            .get("UseSpriteBlending")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        initial_animation_frame: object
            .get("InitialAnimationFrame")
            .and_then(particle_animation_frame),
        collision_animation_frame: object
            .get("CollisionAnimationFrame")
            .and_then(particle_animation_frame),
        animation_frames,
    })
}

fn particle_attractor(value: &Value) -> Option<ParticleAttractor> {
    let object = value.as_object()?;
    Some(ParticleAttractor {
        position: object.get("Position").and_then(vector3),
        radial_axis: object.get("RadialAxis").and_then(vector3),
        trail_position_multiplier: map_number(object, "TrailPositionMultiplier") as f32,
        radius: map_number(object, "Radius") as f32,
        radial_acceleration: map_number(object, "RadialAcceleration") as f32,
        radial_tangent_acceleration: map_number(object, "RadialTangentAcceleration") as f32,
        linear_acceleration: object.get("LinearAcceleration").and_then(vector3),
        radial_impulse: map_number(object, "RadialImpulse") as f32,
        radial_tangent_impulse: map_number(object, "RadialTangentImpulse") as f32,
        linear_impulse: object.get("LinearImpulse").and_then(vector3),
        damping_multiplier: object.get("DampingMultiplier").and_then(vector3),
    })
}

fn particle_collision(value: Option<&Value>) -> Option<ParticleCollision> {
    let object = value?.as_object()?;
    Some(ParticleCollision {
        block_type: match object.get("BlockType").and_then(Value::as_str) {
            Some("Air") => ParticleCollisionBlockType::Air,
            Some("Solid") => ParticleCollisionBlockType::Solid,
            Some("All") => ParticleCollisionBlockType::All,
            _ => ParticleCollisionBlockType::None,
        },
        action: match object.get("Action").and_then(Value::as_str) {
            Some("LastFrame") => ParticleCollisionAction::LastFrame,
            Some("Linger") => ParticleCollisionAction::Linger,
            _ => ParticleCollisionAction::Expire,
        },
        particle_rotation_influence: particle_rotation_influence(
            object.get("ParticleRotationInfluence"),
        ),
    })
}

fn particle_rotation_influence(value: Option<&Value>) -> ParticleRotationInfluence {
    match value.and_then(Value::as_str) {
        Some("Billboard") => ParticleRotationInfluence::Billboard,
        Some("BillboardY") => ParticleRotationInfluence::BillboardY,
        Some("BillboardVelocity") => ParticleRotationInfluence::BillboardVelocity,
        Some("Velocity") => ParticleRotationInfluence::Velocity,
        _ => ParticleRotationInfluence::None,
    }
}

fn initial_velocity(value: Option<&Value>) -> Option<InitialVelocity> {
    let object = value?.as_object()?;
    Some(InitialVelocity {
        yaw: ranged_f32(object.get("Yaw")),
        pitch: ranged_f32(object.get("Pitch")),
        speed: ranged_f32(object.get("Speed")),
    })
}

fn uv_motion(value: Option<&Value>) -> Option<UVMotion> {
    let object = value?.as_object()?;
    Some(UVMotion {
        texture: object
            .get("Texture")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        add_random_uv_offset: object
            .get("AddRandomUVOffset")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        speed_x: map_number(object, "SpeedX") as f32,
        speed_y: map_number(object, "SpeedY") as f32,
        scale: map_number(object, "Scale") as f32,
        strength: map_number(object, "Strength") as f32,
        strength_curve_type: match object.get("StrengthCurveType").and_then(Value::as_str) {
            Some("IncreaseLinear") => UVMotionCurveType::IncreaseLinear,
            Some("IncreaseQuartIn") => UVMotionCurveType::IncreaseQuartIn,
            Some("IncreaseQuartInOut") => UVMotionCurveType::IncreaseQuartInOut,
            Some("IncreaseQuartOut") => UVMotionCurveType::IncreaseQuartOut,
            Some("DecreaseLinear") => UVMotionCurveType::DecreaseLinear,
            Some("DecreaseQuartIn") => UVMotionCurveType::DecreaseQuartIn,
            Some("DecreaseQuartInOut") => UVMotionCurveType::DecreaseQuartInOut,
            Some("DecreaseQuartOut") => UVMotionCurveType::DecreaseQuartOut,
            _ => UVMotionCurveType::Constant,
        },
    })
}

fn intersection_highlight(value: Option<&Value>) -> Option<IntersectionHighlight> {
    let object = value?.as_object()?;
    Some(IntersectionHighlight {
        highlight_threshold: map_number(object, "HighlightThreshold") as f32,
        highlight_color: color(object.get("HighlightColor")),
    })
}

fn particle_spawner_group(value: &Value) -> Option<ParticleSpawnerGroup> {
    let object = value.as_object()?;
    Some(ParticleSpawnerGroup {
        spawner_id: object
            .get("SpawnerId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        position_offset: object.get("PositionOffset").and_then(vector3),
        rotation_offset: direction(object.get("RotationOffset")),
        fixed_rotation: object
            .get("FixedRotation")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        start_delay: map_number(object, "StartDelay") as f32,
        spawn_rate: ranged_f32(object.get("SpawnRate")),
        wave_delay: ranged_f32(object.get("WaveDelay")),
        total_spawners: object
            .get("TotalSpawners")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32,
        max_concurrent: object
            .get("MaxConcurrent")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        initial_velocity: initial_velocity(object.get("InitialVelocity")),
        emit_offset: range_vector3f(object.get("EmitOffset")),
        life_span: ranged_f32(object.get("LifeSpan")),
        attractors: object
            .get("Attractors")
            .and_then(Value::as_array)
            .map(|attractors| {
                attractors
                    .iter()
                    .filter_map(particle_attractor)
                    .collect::<Vec<_>>()
            })
            .filter(|attractors| !attractors.is_empty()),
    })
}

fn particle_system(key: &str, json: &Value) -> ParticleSystem {
    ParticleSystem {
        id: Some(key.to_string()),
        spawners: json
            .get("Spawners")
            .and_then(Value::as_array)
            .map(|spawners| {
                spawners
                    .iter()
                    .filter_map(particle_spawner_group)
                    .collect::<Vec<_>>()
            })
            .filter(|spawners| !spawners.is_empty()),
        life_span: number(json, "LifeSpan") as f32,
        cull_distance: number(json, "CullDistance") as f32,
        bounding_radius: number(json, "BoundingRadius") as f32,
        is_important: json
            .get("IsImportant")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn particle_spawner(key: &str, json: &Value) -> ParticleSpawner {
    let rotation_influence = particle_rotation_influence(json.get("ParticleRotationInfluence"));
    let collision = particle_collision(json.get("ParticleCollision")).map(|mut collision| {
        // Java falls back to the spawner's rotation influence when the
        // collision block does not define one.
        if json
            .get("ParticleCollision")
            .and_then(Value::as_object)
            .and_then(|collision| collision.get("ParticleRotationInfluence"))
            .is_none()
        {
            collision.particle_rotation_influence = rotation_influence;
        }
        collision
    });
    ParticleSpawner {
        id: Some(key.to_string()),
        particle: particle(json.get("Particle")),
        shape: match json.get("Shape").and_then(Value::as_str) {
            Some("Cube") => EmitShape::Cube,
            _ => EmitShape::Sphere,
        },
        emit_offset: range_vector3f(json.get("EmitOffset")),
        camera_offset: number(json, "CameraOffset") as f32,
        use_emit_direction: json
            .get("UseEmitDirection")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        life_span: number(json, "LifeSpan") as f32,
        spawn_rate: ranged_f32(json.get("SpawnRate")),
        spawn_burst: json
            .get("SpawnBurst")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        wave_delay: ranged_f32(json.get("WaveDelay")),
        total_particles: ranged_i32(json.get("TotalParticles")),
        max_concurrent_particles: json
            .get("MaxConcurrentParticles")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        initial_velocity: initial_velocity(json.get("InitialVelocity")),
        velocity_stretch_multiplier: number(json, "VelocityStretchMultiplier") as f32,
        particle_rotation_influence: rotation_influence,
        particle_rotate_with_spawner: json
            .get("ParticleRotateWithSpawner")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_low_res: json
            .get("IsLowRes")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        trail_spawner_position_multiplier: number(json, "TrailSpawnerPositionMultiplier") as f32,
        trail_spawner_rotation_multiplier: number(json, "TrailSpawnerRotationMultiplier") as f32,
        particle_collision: collision,
        render_mode: match json.get("RenderMode").and_then(Value::as_str) {
            Some("BlendAdd") => FXRenderMode::BlendAdd,
            Some("Erosion") => FXRenderMode::Erosion,
            Some("Distortion") => FXRenderMode::Distortion,
            _ => FXRenderMode::BlendLinear,
        },
        light_influence: number(json, "LightInfluence") as f32,
        linear_filtering: json
            .get("LinearFiltering")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        particle_life_span: ranged_f32(json.get("ParticleLifeSpan")),
        uv_motion: uv_motion(json.get("UVMotion")),
        attractors: json
            .get("Attractors")
            .and_then(Value::as_array)
            .map(|attractors| {
                attractors
                    .iter()
                    .filter_map(particle_attractor)
                    .collect::<Vec<_>>()
            })
            .filter(|attractors| !attractors.is_empty()),
        intersection_highlight: intersection_highlight(json.get("IntersectionHighlight")),
    }
}

fn number(json: &Value, name: &str) -> f64 {
    json.get(name).and_then(Value::as_f64).unwrap_or(0.0)
}

fn sound_index(registry: &AssetRegistry, json: &Value, name: &str) -> i32 {
    json.get(name)
        .and_then(Value::as_str)
        .and_then(|id| registry.sound_events.id_of(id))
        .unwrap_or(0)
}

fn reticle(value: &Value, default_duration: f32) -> ItemReticle {
    ItemReticle {
        hide_base: value
            .get("HideBase")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        parts: value.get("Parts").and_then(Value::as_array).map(|parts| {
            parts
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        }),
        duration: value
            .get("Duration")
            .and_then(Value::as_f64)
            .map(|duration| duration as f32)
            .unwrap_or(default_duration),
    }
}

fn reticle_client_event(name: &str) -> Option<ItemReticleClientEvent> {
    Some(match name {
        "OnHit" => ItemReticleClientEvent::OnHit,
        "Wielding" => ItemReticleClientEvent::Wielding,
        "OnMovementLeft" => ItemReticleClientEvent::OnMovementLeft,
        "OnMovementRight" => ItemReticleClientEvent::OnMovementRight,
        "OnMovementBack" => ItemReticleClientEvent::OnMovementBack,
        _ => return None,
    })
}

fn item_category(key: &str, value: &Value, top_level: bool) -> Result<ItemCategory> {
    let id = value
        .get("Id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| top_level.then(|| key.to_string()));
    let info_display_mode = match value.get("InfoDisplayMode").and_then(Value::as_str) {
        None | Some("Tooltip") => ItemGridInfoDisplayMode::Tooltip,
        Some("Adjacent") => ItemGridInfoDisplayMode::Adjacent,
        Some("None") => ItemGridInfoDisplayMode::None,
        Some(other) => {
            return Err(invalid(
                "ItemCategory",
                key,
                format!("unknown ItemGridInfoDisplayMode `{other}`"),
            ))
        }
    };
    let mut children = Vec::new();
    if let Some(values) = value.get("Children").and_then(Value::as_array) {
        let mut ordered: Vec<(&Value, i32)> = values
            .iter()
            .map(|child| {
                (
                    child,
                    child.get("Order").and_then(Value::as_i64).unwrap_or(0) as i32,
                )
            })
            .collect();
        // Java `afterDecode` sorts children by `Order`.
        ordered.sort_by_key(|(_, order)| *order);
        for (child, _) in ordered {
            children.push(item_category(key, child, false)?);
        }
    }
    let sub_categories = value
        .get("SubCategories")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|sub| SubCategoryDefinition {
                    id: sub.get("Id").and_then(Value::as_str).map(ToOwned::to_owned),
                    name: sub
                        .get("Name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    description: sub
                        .get("Description")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    order: sub.get("Order").and_then(Value::as_i64).unwrap_or(0) as i32,
                })
                .collect()
        });
    Ok(ItemCategory {
        id,
        name: value
            .get("Name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        icon: value
            .get("Icon")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        order: value.get("Order").and_then(Value::as_i64).unwrap_or(0) as i32,
        info_display_mode,
        children: (!children.is_empty()).then_some(children),
        sub_categories,
    })
}

fn item_player_animations(key: &str, value: &Value) -> Result<ItemPlayerAnimations> {
    let mut animations = BTreeMap::new();
    if let Some(entries) = value.get("Animations").and_then(Value::as_object) {
        for (name, animation) in entries {
            animations.insert(name.clone(), item_animation(key, name, animation)?);
        }
    }
    Ok(ItemPlayerAnimations {
        id: Some(key.to_string()),
        animations,
        wiggle_weights: value
            .get("WiggleWeights")
            .and_then(Value::as_object)
            .map(wiggle_weights),
        camera: value
            .get("Camera")
            .and_then(Value::as_object)
            .map(camera_settings),
        pullback_config: value
            .get("PullbackConfig")
            .and_then(Value::as_object)
            .map(pullback_config),
        use_first_person_override: value
            .get("UseFirstPersonOverrides")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn item_animation(store_key: &str, name: &str, value: &Value) -> Result<ItemAnimation> {
    let Some(object) = value.as_object() else {
        return Err(invalid(
            "ItemPlayerAnimations",
            store_key,
            format!("animation `{name}` must be an object"),
        ));
    };
    let field = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    };
    Ok(ItemAnimation {
        third_person: field("ThirdPerson"),
        third_person_moving: field("ThirdPersonMoving"),
        third_person_face: field("ThirdPersonFace"),
        first_person: field("FirstPerson"),
        first_person_override: field("FirstPersonOverride"),
        keep_previous_first_person_animation: object
            .get("KeepPreviousFirstPersonAnimation")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        speed: object.get("Speed").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        // Java `ItemAnimation.blendingDuration` defaults to 0.2.
        blending_duration: object
            .get("BlendingDuration")
            .and_then(Value::as_f64)
            .unwrap_or(0.2) as f32,
        looping: object
            .get("Looping")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        clips_geometry: object
            .get("ClipsGeometry")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn wiggle_weights(object: &serde_json::Map<String, Value>) -> WiggleWeights {
    let field =
        |name: &str| -> f32 { object.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32 };
    WiggleWeights {
        x: field("X"),
        x_deceleration: field("XDeceleration"),
        y: field("Y"),
        y_deceleration: field("YDeceleration"),
        z: field("Z"),
        z_deceleration: field("ZDeceleration"),
        roll: field("Roll"),
        roll_deceleration: field("RollDeceleration"),
        pitch: field("Pitch"),
        pitch_deceleration: field("PitchDeceleration"),
    }
}

fn camera_settings(object: &serde_json::Map<String, Value>) -> CameraSettings {
    CameraSettings {
        position_offset: object.get("PositionOffset").and_then(vector3),
        yaw: object
            .get("Yaw")
            .and_then(Value::as_object)
            .map(camera_axis),
        pitch: object
            .get("Pitch")
            .and_then(Value::as_object)
            .map(camera_axis),
    }
}

fn camera_axis(object: &serde_json::Map<String, Value>) -> CameraAxis {
    CameraAxis {
        angle_range: object
            .get("AngleRange")
            .and_then(Value::as_object)
            .map(|range| Rangef {
                min: range.get("Min").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                max: range.get("Max").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            }),
        target_nodes: object
            .get("TargetNodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(camera_node)
                    .collect()
            }),
    }
}

fn camera_node(name: &str) -> Option<CameraNode> {
    Some(match name {
        "None" => CameraNode::None,
        "Head" => CameraNode::Head,
        "LShoulder" => CameraNode::LShoulder,
        "RShoulder" => CameraNode::RShoulder,
        "Belly" => CameraNode::Belly,
        _ => return None,
    })
}

fn pullback_config(object: &serde_json::Map<String, Value>) -> ItemPullbackConfiguration {
    ItemPullbackConfiguration {
        left_offset_override: object.get("LeftOffsetOverride").and_then(vector3),
        left_rotation_override: object.get("LeftRotationOverride").and_then(vector3),
        right_offset_override: object.get("RightOffsetOverride").and_then(vector3),
        right_rotation_override: object.get("RightRotationOverride").and_then(vector3),
    }
}

fn vector3(value: &Value) -> Option<Vector3f> {
    if let Some(values) = value.as_array() {
        return Some(Vector3f {
            x: values.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            y: values.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            z: values.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        });
    }
    let object = value.as_object()?;
    Some(Vector3f {
        x: object.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        y: object.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        z: object.get("Z").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    let (mut p, mut v) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            mark = v;
        } else if let Some(star) = star {
            p = star + 1;
            mark += 1;
            v = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn edge(value: &Value) -> Option<Edge> {
    let object = value.as_object()?;
    Some(Edge {
        color: object.get("Color").and_then(color_alpha),
        width: object.get("Width").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn color_alpha(value: &Value) -> Option<ColorAlpha> {
    let text = value.as_str()?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    let (alpha, rgb) = if hex.len() == 8 {
        (u8::from_str_radix(&hex[0..2], 16).ok()?, &hex[2..])
    } else {
        (255, hex)
    };
    let red = u8::from_str_radix(&rgb[0..2], 16).ok()?;
    let green = u8::from_str_radix(&rgb[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&rgb[4..6], 16).ok()?;
    Some(ColorAlpha {
        alpha: alpha as i8,
        red: red as i8,
        green: green as i8,
        blue: blue as i8,
    })
}

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

/// Java `AssetMapWithIndexes.NOT_FOUND`.
const NOT_FOUND: i32 = i32::MIN;

/// Java `Fluid.processConfig` FluidFX index.
fn resolve_fluid_fx(registry: &AssetRegistry, json: &Value) -> i32 {
    let id = json
        .get("FluidFXId")
        .and_then(Value::as_str)
        .unwrap_or("Empty");
    if id.eq_ignore_ascii_case("Empty") {
        0
    } else {
        registry.fluid_fx.id_of(id).unwrap_or(NOT_FOUND)
    }
}

/// Java `Fluid.processConfig` BlockSoundSet index.
fn resolve_block_sound_set(registry: &AssetRegistry, json: &Value) -> i32 {
    let id = json
        .get("BlockSoundSetId")
        .and_then(Value::as_str)
        .unwrap_or("EMPTY");
    if id.eq_ignore_ascii_case("EMPTY") {
        0
    } else {
        registry.block_sound_sets.id_of(id).unwrap_or(NOT_FOUND)
    }
}

/// Java `Environment.toPacket` fluid-particle map.
fn environment_fluid_particles(
    registry: &AssetRegistry,
    json: &Value,
) -> Result<Option<BTreeMap<i32, FluidParticle>>> {
    let Some(particles) = json.get("FluidParticles").and_then(Value::as_object) else {
        return Ok(None);
    };
    if particles.is_empty() {
        return Ok(None);
    }
    let mut out = BTreeMap::new();
    for (key, value) in particles {
        if let Some(index) = registry.fluid_fx.id_of(key) {
            out.insert(index, child::fluid_particle(value)?);
        }
    }
    Ok(Some(out))
}

fn stat_effects(value: Option<&Value>) -> Option<EntityStatEffects> {
    let value = value?;
    Some(EntityStatEffects {
        trigger_at_zero: value
            .get("TriggerAtZero")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sound_event_index: 0,
        particles: None,
    })
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
