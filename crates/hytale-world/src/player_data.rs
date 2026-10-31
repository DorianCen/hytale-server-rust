//! Player save documents (`DiskPlayerStorageProvider`) for the runtime world.
//!
//! Java writes one `<uuid>.json` per player under `<world>/players/`. The file is
//! `EntityStore.REGISTRY.serialize(holder)` written with
//! `BsonUtil.writeDocument`: a BSON document encoded as strict extended JSON
//! with a single top-level `Components` map.
//!
//! This port types the core holder components needed by the first join path
//! (`UUID`, `Transform`, `WorldGenId`) and preserves every other component as an
//! ordered JSON value. A full Java player save therefore survives a Rust
//! load/save cycle even before inventory and gameplay components are ported.
//!
//! Ported from:
//! - `CoreServer/.../universe/playerdata/DiskPlayerStorageProvider.java`
//! - `CoreServer/.../entity/UUIDComponent.java`
//! - `CoreServer/.../modules/entity/component/TransformComponent.java`
//! - `CoreServer/.../modules/entity/component/WorldGenId.java`
//! - `Component/.../ComponentRegistry.java` (the `Components` holder codec)

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use hytale_inventory::{InventoryError, PlayerInventory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;

use crate::world_config::WorldUuid;

/// Component id of `UUIDComponent`.
pub const UUID_COMPONENT: &str = "UUID";
/// Component id of `TransformComponent`.
pub const TRANSFORM_COMPONENT: &str = "Transform";
/// Component id of `WorldGenId`.
pub const WORLD_GEN_ID_COMPONENT: &str = "WorldGenId";
/// Component id of Java `PersistentModel` (`EntityModule` registers it as
/// `Model`).
pub const MODEL_COMPONENT: &str = "Model";
/// Component id of the persisted player skin. Java keeps the live skin on the
/// auth session; this port accepts an optional saved component so a Rust- or
/// plugin-written save can carry cosmetics.
pub const SKIN_COMPONENT: &str = "PlayerSkin";
/// Component id of Java `InventoryComponent.Armor`.
pub const ARMOR_INVENTORY_COMPONENT: &str = "ArmorInventory";
/// Component id of Java `InventoryComponent.Hotbar`.
pub const HOTBAR_INVENTORY_COMPONENT: &str = "HotbarInventory";
/// Component id of Java `InventoryComponent.Utility`.
pub const UTILITY_INVENTORY_COMPONENT: &str = "UtilityInventory";
/// Component id of Java `EntityStatMap` (`EntityStats`).
pub const ENTITY_STATS_COMPONENT: &str = "EntityStats";
/// Legacy `Inventory` component (`Inventory.CODEC`, version 4).
pub const LEGACY_INVENTORY_COMPONENT: &str = "Inventory";

/// Number of armor slots (`ItemArmorSlot.VALUES.length`).
pub const ARMOR_SLOTS: usize = 4;
/// Java `BlockType.EMPTY_KEY`, used for empty held items.
pub const EMPTY_ITEM_ID: &str = "Empty";

/// Errors from the player data codec and store.
#[derive(Debug, Error)]
pub enum PlayerDataError {
    /// Reading or writing the file failed.
    #[error("player data I/O error at {path}: {source}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The document is not valid JSON or has the wrong shape.
    #[error("invalid player data {path}: {source}")]
    Json {
        /// File that failed.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A UUID argument is malformed.
    #[error("invalid player UUID `{0}`")]
    InvalidUuid(String),
    /// An inventory component is malformed.
    #[error(transparent)]
    Inventory(#[from] InventoryError),
}

/// Result alias for player data operations.
pub type Result<T> = std::result::Result<T, PlayerDataError>;

/// A 3D position in a `Transform` component (`Vector3dUtil`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate.
    #[serde(rename = "X")]
    pub x: f64,
    /// Y coordinate.
    #[serde(rename = "Y")]
    pub y: f64,
    /// Z coordinate.
    #[serde(rename = "Z")]
    pub z: f64,
}

/// A rotation in a `Transform` component (`Rotation3f`, radians).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    /// Pitch in radians.
    #[serde(rename = "Pitch")]
    pub pitch: f32,
    /// Yaw in radians.
    #[serde(rename = "Yaw")]
    pub yaw: f32,
    /// Roll in radians.
    #[serde(rename = "Roll")]
    pub roll: f32,
}

/// A `TransformComponent` document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerTransform {
    /// Entity position.
    #[serde(rename = "Position")]
    pub position: Position,
    /// Entity rotation.
    #[serde(rename = "Rotation")]
    pub rotation: Rotation,
}

impl PlayerTransform {
    /// Build a transform from position and rotation.
    pub const fn new(position: Position, rotation: Rotation) -> Self {
        Self { position, rotation }
    }
}

/// A saved `PersistentModel.Model` reference (`Model.ModelReference`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerModelReference {
    /// Model asset id (`Id`).
    pub id: String,
    /// Scale (`Scale`); non-positive values are normalized to `1.0`.
    pub scale: f32,
    /// Static model flag (`Static`).
    pub static_model: bool,
    /// Random attachment selections (`RandomAttachments`).
    pub random_attachments: BTreeMap<String, String>,
}

impl Default for PlayerModelReference {
    fn default() -> Self {
        Self {
            // `TempAssetIdUtil.DEFAULT_PLAYER_MODEL_NAME`.
            id: "Player".to_string(),
            scale: 1.0,
            static_model: false,
            random_attachments: BTreeMap::new(),
        }
    }
}

/// A persisted player skin (the protocol `PlayerSkin` fields).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerSkinData {
    /// `bodyCharacteristic`.
    pub body_characteristic: Option<String>,
    /// `underwear`.
    pub underwear: Option<String>,
    /// `face`.
    pub face: Option<String>,
    /// `eyes`.
    pub eyes: Option<String>,
    /// `ears`.
    pub ears: Option<String>,
    /// `mouth`.
    pub mouth: Option<String>,
    /// `facialHair`.
    pub facial_hair: Option<String>,
    /// `haircut`.
    pub haircut: Option<String>,
    /// `eyebrows`.
    pub eyebrows: Option<String>,
    /// `pants`.
    pub pants: Option<String>,
    /// `overpants`.
    pub overpants: Option<String>,
    /// `undertop`.
    pub undertop: Option<String>,
    /// `overtop`.
    pub overtop: Option<String>,
    /// `shoes`.
    pub shoes: Option<String>,
    /// `headAccessory`.
    pub head_accessory: Option<String>,
    /// `faceAccessory`.
    pub face_accessory: Option<String>,
    /// `earAccessory`.
    pub ear_accessory: Option<String>,
    /// `skinFeature`.
    pub skin_feature: Option<String>,
    /// `gloves`.
    pub gloves: Option<String>,
    /// `cape`.
    pub cape: Option<String>,
}

/// Player appearance and equipment decoded from a save.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerAppearance {
    /// Saved model reference (`Model` component), if any.
    pub model: Option<PlayerModelReference>,
    /// Saved skin (`PlayerSkin` component), if any.
    pub skin: Option<PlayerSkinData>,
    /// Armor item ids by `ItemArmorSlot` (Head, Chest, Hands, Legs).
    pub armor: [Option<String>; ARMOR_SLOTS],
    /// Active hotbar item, used as the right-hand item.
    pub right_hand: Option<String>,
    /// Active utility item, used as the left-hand item.
    pub left_hand: Option<String>,
}

impl PlayerAppearance {
    /// Drop item ids the caller's asset registry does not know.
    ///
    /// This is the "unknown id" fallback: the join still succeeds and the
    /// self-spawn uses Java's empty armor/held-item values.
    pub fn resolve_items<F>(&mut self, exists: F)
    where
        F: Fn(&str) -> bool,
    {
        for slot in &mut self.armor {
            if let Some(id) = slot {
                if !exists(id) {
                    *slot = None;
                }
            }
        }
        if let Some(id) = &self.right_hand {
            if !exists(id) {
                self.right_hand = None;
            }
        }
        if let Some(id) = &self.left_hand {
            if !exists(id) {
                self.left_hand = None;
            }
        }
    }
}

/// One player's `EntityStore` holder document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    /// The `Components` map keyed by Java component id.
    #[serde(rename = "Components", default)]
    components: Map<String, Value>,
}

impl Default for PlayerData {
    fn default() -> Self {
        Self {
            components: Map::new(),
        }
    }
}

impl PlayerData {
    /// A holder containing the core components this port owns.
    pub fn new(uuid: WorldUuid, transform: PlayerTransform) -> Self {
        let mut data = Self::default();
        data.set_uuid(uuid);
        data.set_transform(transform);
        data
    }

    /// Build from an existing `Components` map (unknown components included).
    pub fn from_components(components: Map<String, Value>) -> Self {
        Self { components }
    }

    /// The raw `Components` map, in document order.
    pub fn components(&self) -> &Map<String, Value> {
        &self.components
    }

    /// Numerical component count (known and unknown).
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Component ids in document order.
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }

    /// One raw component document.
    pub fn component(&self, id: &str) -> Option<&Value> {
        self.components.get(id)
    }

    /// Replace a component document; returns the previous value.
    pub fn insert_component(&mut self, id: impl Into<String>, value: Value) -> Option<Value> {
        self.components.insert(id.into(), value)
    }

    /// Remove a component document.
    pub fn remove_component(&mut self, id: &str) -> Option<Value> {
        self.components.remove(id)
    }

    /// Rust-owned component holding the per-player revealed world-map markers.
    ///
    /// Java stores the reveal set inside `PlayerWorldData`; this port keeps the
    /// same information in a small explicit component until that codec is
    /// modelled.
    pub const REVEALED_MARKERS_COMPONENT: &'static str = "RustRevealedMarkers";

    /// Revealed world-map marker ids.
    pub fn revealed_markers(&self) -> BTreeSet<String> {
        self.components
            .get(Self::REVEALED_MARKERS_COMPONENT)
            .and_then(|value| value.get("Markers"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Replace the revealed world-map marker ids.
    pub fn set_revealed_markers(&mut self, markers: impl IntoIterator<Item = String>) {
        let mut markers: Vec<String> = markers.into_iter().collect();
        markers.sort();
        markers.dedup();
        self.components.insert(
            Self::REVEALED_MARKERS_COMPONENT.to_string(),
            json!({ "Markers": markers }),
        );
    }

    /// The `UUID` component, if present.
    pub fn uuid(&self) -> Option<WorldUuid> {
        let value = self.components.get(UUID_COMPONENT)?.get("UUID")?;
        serde_json::from_value(value.clone()).ok()
    }

    /// Set or replace the `UUID` component.
    pub fn set_uuid(&mut self, uuid: WorldUuid) {
        let encoded = serde_json::to_value(uuid).expect("WorldUuid always serializes");
        self.components
            .insert(UUID_COMPONENT.to_string(), json!({ "UUID": encoded }));
    }

    /// The `Transform` component, if present and well-formed.
    pub fn transform(&self) -> Option<PlayerTransform> {
        let value = self.components.get(TRANSFORM_COMPONENT)?;
        serde_json::from_value(value.clone()).ok()
    }

    /// Set or replace the `Transform` component.
    pub fn set_transform(&mut self, transform: PlayerTransform) {
        let encoded = serde_json::to_value(transform).expect("PlayerTransform always serializes");
        self.components
            .insert(TRANSFORM_COMPONENT.to_string(), encoded);
    }

    /// The `WorldGenId` component, if present.
    pub fn world_gen_id(&self) -> Option<i32> {
        self.components
            .get(WORLD_GEN_ID_COMPONENT)?
            .get("WorldGenId")?
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
    }

    /// Set or replace the `WorldGenId` component.
    pub fn set_world_gen_id(&mut self, world_gen_id: i32) {
        self.components.insert(
            WORLD_GEN_ID_COMPONENT.to_string(),
            json!({ "WorldGenId": world_gen_id }),
        );
    }

    /// Decode the full player inventory (modern components, legacy fallback).
    pub fn inventory(&self) -> std::result::Result<PlayerInventory, InventoryError> {
        PlayerInventory::from_components(&self.components)
    }

    /// Write the full player inventory back into the component map, preserving
    /// every non-inventory component.
    pub fn set_inventory(&mut self, inventory: &PlayerInventory) {
        inventory.write_components(&mut self.components);
    }

    /// Saved `EntityStats.Stats.Health.Value`, when the component exists.
    pub fn health(&self) -> Option<f32> {
        self.components
            .get(ENTITY_STATS_COMPONENT)?
            .get("Stats")?
            .get("Health")?
            .get("Value")?
            .as_f64()
            .map(|value| value as f32)
    }

    /// Update the saved health value while preserving other stats, modifiers
    /// and unknown `EntityStats` fields.
    pub fn set_health(&mut self, health: f32) {
        let component = self
            .components
            .entry(ENTITY_STATS_COMPONENT.to_string())
            .or_insert_with(|| json!({ "Stats": {} }));
        if !component.is_object() {
            *component = json!({ "Stats": {} });
        }
        let object = component.as_object_mut().expect("component replaced above");
        let stats = object
            .entry("Stats".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !stats.is_object() {
            *stats = Value::Object(Map::new());
        }
        let stats = stats.as_object_mut().expect("stats replaced above");
        let entry = stats
            .entry("Health".to_string())
            .or_insert_with(|| json!({ "Id": "Health" }));
        if !entry.is_object() {
            *entry = json!({ "Id": "Health" });
        }
        entry
            .as_object_mut()
            .expect("health replaced above")
            .insert("Value".to_string(), json!(health));
    }

    /// Saved crafting knowledge/progression, when present.
    pub fn crafting(&self) -> Option<crate::crafting::CraftingKnowledge> {
        self.components
            .get("Crafting")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    /// Persist crafting knowledge/progression while preserving all other
    /// components.
    pub fn set_crafting(&mut self, crafting: &crate::crafting::CraftingKnowledge) {
        if let Ok(value) = serde_json::to_value(crafting) {
            self.components.insert("Crafting".to_string(), value);
        }
    }

    /// Decode the saved model, skin, armor and active held items.
    ///
    /// Modern per-inventory components (`ArmorInventory`, `HotbarInventory`,
    /// `UtilityInventory`) win; the legacy `Inventory` component is used as a
    /// fallback when the modern component is absent. Missing or malformed
    /// components produce [`PlayerAppearance::default`] values instead of an
    /// error.
    pub fn appearance(&self) -> PlayerAppearance {
        let mut appearance = PlayerAppearance::default();
        if let Some(value) = self.components.get(MODEL_COMPONENT) {
            appearance.model = parse_model(value);
        }
        if let Some(value) = self.components.get(SKIN_COMPONENT) {
            appearance.skin = parse_skin(value);
        }

        let mut has_modern_armor = false;
        let mut has_modern_hotbar = false;
        let mut has_modern_utility = false;

        if let Some(value) = self.components.get(ARMOR_INVENTORY_COMPONENT) {
            if let Some(container) = parse_component_container(value) {
                has_modern_armor = true;
                copy_armor(&container, &mut appearance.armor);
            }
        }
        if let Some(value) = self.components.get(HOTBAR_INVENTORY_COMPONENT) {
            if let Some(container) = parse_component_container(value) {
                has_modern_hotbar = true;
                appearance.right_hand = active_item(&container);
            }
        }
        if let Some(value) = self.components.get(UTILITY_INVENTORY_COMPONENT) {
            if let Some(container) = parse_component_container(value) {
                has_modern_utility = true;
                appearance.left_hand = active_item(&container);
            }
        }

        if let Some(legacy) = self.components.get(LEGACY_INVENTORY_COMPONENT) {
            if !has_modern_armor {
                if let Some(value) = legacy.get("Armor") {
                    if let Some(container) = parse_container(value) {
                        copy_armor(&container, &mut appearance.armor);
                    }
                }
            }
            if !has_modern_hotbar {
                if let Some(value) = legacy.get("HotBar") {
                    if let Some(container) = parse_container(value) {
                        appearance.right_hand =
                            legacy_active_item(&container, legacy.get("ActiveHotbarSlot"));
                    }
                }
            }
            if !has_modern_utility {
                if let Some(value) = legacy.get("Utility") {
                    if let Some(container) = parse_container(value) {
                        appearance.left_hand =
                            legacy_active_item(&container, legacy.get("ActiveUtilitySlot"));
                    }
                }
            }
        }

        appearance
    }

    /// Parse a document string (`{"Components": ...}`).
    pub fn from_json(text: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Render in Java's pretty JSON style.
    pub fn to_json_pretty(&self) -> std::result::Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

/// A parsed `ItemContainer` (the `Inventory` field of an inventory component).
#[derive(Debug, Clone, Default)]
struct ParsedContainer {
    items: BTreeMap<usize, String>,
    active_slot: Option<i32>,
}

fn parse_component_container(value: &Value) -> Option<ParsedContainer> {
    let mut container = parse_container(value.get("Inventory")?)?;
    if let Some(slot) = int_field(value, "ActiveSlot") {
        container.active_slot = Some(slot);
    }
    Some(container)
}

fn parse_container(value: &Value) -> Option<ParsedContainer> {
    let mut container = ParsedContainer::default();
    match value.get("Items") {
        Some(Value::Object(items)) => {
            for (slot, item) in items {
                let Ok(slot) = slot.parse::<usize>() else {
                    continue;
                };
                if let Some(id) = item_id(item) {
                    container.items.insert(slot, id);
                }
            }
        }
        Some(Value::Array(items)) => {
            for (slot, item) in items.iter().enumerate() {
                if let Some(id) = item_id(item) {
                    container.items.insert(slot, id);
                }
            }
        }
        _ => {}
    }
    Some(container)
}

fn item_id(value: &Value) -> Option<String> {
    let id = value.get("Id")?.as_str()?;
    if id.is_empty() || id == EMPTY_ITEM_ID {
        return None;
    }
    Some(id.to_string())
}

fn int_field(value: &Value, name: &str) -> Option<i32> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn active_item(container: &ParsedContainer) -> Option<String> {
    let slot = container.active_slot.unwrap_or(0);
    if slot < 0 {
        return None;
    }
    container.items.get(&(slot as usize)).cloned()
}

fn legacy_active_item(container: &ParsedContainer, value: Option<&Value>) -> Option<String> {
    let slot = value
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0);
    if slot < 0 {
        return None;
    }
    container.items.get(&(slot as usize)).cloned()
}

fn copy_armor(container: &ParsedContainer, armor: &mut [Option<String>; ARMOR_SLOTS]) {
    for (slot, target) in armor.iter_mut().enumerate() {
        *target = container.items.get(&slot).cloned();
    }
}

fn parse_model(value: &Value) -> Option<PlayerModelReference> {
    // Java `PersistentModel.CODEC` stores `{"Model": {...}}`; accept a direct
    // reference too for hand-written or future saves.
    let reference = value.get("Model").unwrap_or(value);
    if !reference.is_object() {
        return None;
    }
    let id = reference.get("Id")?.as_str()?;
    if id.is_empty() {
        return None;
    }
    let scale = reference
        .get("Scale")
        .and_then(Value::as_f64)
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0) as f32;
    let static_model = reference
        .get("Static")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let random_attachments = reference
        .get("RandomAttachments")
        .and_then(Value::as_object)
        .map(|attachments| {
            attachments
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(PlayerModelReference {
        id: id.to_string(),
        scale,
        static_model,
        random_attachments,
    })
}

fn parse_skin(value: &Value) -> Option<PlayerSkinData> {
    let skin = value.get("Skin").unwrap_or(value);
    if !skin.is_object() {
        return None;
    }
    Some(PlayerSkinData {
        body_characteristic: string_field(skin, "bodyCharacteristic"),
        underwear: string_field(skin, "underwear"),
        face: string_field(skin, "face"),
        eyes: string_field(skin, "eyes"),
        ears: string_field(skin, "ears"),
        mouth: string_field(skin, "mouth"),
        facial_hair: string_field(skin, "facialHair"),
        haircut: string_field(skin, "haircut"),
        eyebrows: string_field(skin, "eyebrows"),
        pants: string_field(skin, "pants"),
        overpants: string_field(skin, "overpants"),
        undertop: string_field(skin, "undertop"),
        overtop: string_field(skin, "overtop"),
        shoes: string_field(skin, "shoes"),
        head_accessory: string_field(skin, "headAccessory"),
        face_accessory: string_field(skin, "faceAccessory"),
        ear_accessory: string_field(skin, "earAccessory"),
        skin_feature: string_field(skin, "skinFeature"),
        gloves: string_field(skin, "gloves"),
        cape: string_field(skin, "cape"),
    })
}

fn string_field(value: &Value, name: &str) -> Option<String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// Player save files under `<world>/players/`.
#[derive(Debug, Clone)]
pub struct PlayerStorage {
    players_dir: PathBuf,
}

impl PlayerStorage {
    /// A store rooted at an explicit `players/` directory.
    pub fn new(players_dir: impl Into<PathBuf>) -> Self {
        Self {
            players_dir: players_dir.into(),
        }
    }

    /// The Java layout: `<world save>/players/`.
    pub fn for_save_dir(save_dir: impl AsRef<Path>) -> Self {
        Self::new(save_dir.as_ref().join("players"))
    }

    /// The directory holding `<uuid>.json`.
    pub fn players_dir(&self) -> &Path {
        &self.players_dir
    }

    /// Path of one player file.
    pub fn path_for(&self, uuid: WorldUuid) -> PathBuf {
        self.players_dir.join(format!("{uuid}.json"))
    }

    /// Load a player, or `None` when the file does not exist.
    pub fn load(&self, uuid: WorldUuid) -> Result<Option<PlayerData>> {
        let path = self.path_for(uuid);
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|source| PlayerDataError::Io {
            path: path.clone(),
            source,
        })?;
        let data =
            serde_json::from_str(&text).map_err(|source| PlayerDataError::Json { path, source })?;
        Ok(Some(data))
    }

    /// Delete one player file (and its backup). Returns whether it existed.
    pub fn delete(&self, uuid: WorldUuid) -> Result<bool> {
        let path = self.path_for(uuid);
        let existed = path.is_file();
        if existed {
            fs::remove_file(&path).map_err(|source| PlayerDataError::Io {
                path: path.clone(),
                source,
            })?;
        }
        let backup = backup_path(&path);
        if backup.is_file() {
            fs::remove_file(&backup).map_err(|source| PlayerDataError::Io {
                path: backup,
                source,
            })?;
        }
        Ok(existed)
    }

    /// Save atomically, keeping Java's `.bak` copy of a replaced file.
    pub fn save(&self, uuid: WorldUuid, data: &PlayerData) -> Result<()> {
        fs::create_dir_all(&self.players_dir).map_err(|source| PlayerDataError::Io {
            path: self.players_dir.clone(),
            source,
        })?;
        let path = self.path_for(uuid);
        if path.is_file() {
            let backup = backup_path(&path);
            fs::copy(&path, &backup).map_err(|source| PlayerDataError::Io {
                path: backup,
                source,
            })?;
        }
        let text = data
            .to_json_pretty()
            .map_err(|source| PlayerDataError::Json {
                path: path.clone(),
                source,
            })?;
        let temporary = temporary_path(&path);
        fs::write(&temporary, text).map_err(|source| PlayerDataError::Io {
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, &path).map_err(|source| PlayerDataError::Io { path, source })?;
        Ok(())
    }

    /// Delete a player file; returns whether it existed.
    pub fn remove(&self, uuid: WorldUuid) -> Result<bool> {
        match fs::remove_file(self.path_for(uuid)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(PlayerDataError::Io {
                path: self.path_for(uuid),
                source,
            }),
        }
    }

    /// Every player UUID stored in the directory, sorted.
    pub fn players(&self) -> Result<Vec<WorldUuid>> {
        if !self.players_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut players = Vec::new();
        for entry in fs::read_dir(&self.players_dir).map_err(|source| PlayerDataError::Io {
            path: self.players_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| PlayerDataError::Io {
                path: self.players_dir.clone(),
                source,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            if let Ok(uuid) = stem.parse() {
                players.push(uuid);
            }
        }
        players.sort_by_key(ToString::to_string);
        Ok(players)
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid() -> WorldUuid {
        "12345678-1234-5678-9abc-def012345678".parse().unwrap()
    }

    fn transform() -> PlayerTransform {
        PlayerTransform::new(
            Position {
                x: 10.5,
                y: 80.0,
                z: -3.5,
            },
            Rotation {
                pitch: 0.25,
                yaw: 1.570_796_4,
                roll: -0.125,
            },
        )
    }

    #[test]
    fn core_components_round_trip() {
        let mut data = PlayerData::new(uuid(), transform());
        data.set_world_gen_id(42);
        let text = data.to_json_pretty().unwrap();
        assert!(text.contains("\"Components\""));
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded, data);
        assert_eq!(reloaded.uuid(), Some(uuid()));
        assert_eq!(reloaded.transform(), Some(transform()));
        assert_eq!(reloaded.world_gen_id(), Some(42));
        assert_eq!(reloaded.component_count(), 3);
    }

    #[test]
    fn health_preserves_other_stats_and_unknown_fields() {
        let mut data = PlayerData::new(uuid(), transform());
        data.insert_component(
            ENTITY_STATS_COMPONENT,
            json!({
                "Stats": {
                    "Health": {"Id": "Health", "Value": 100.0, "Modifiers": {"set": {"Amount": 5.0}}},
                    "Stamina": {"Id": "Stamina", "Value": 12.0}
                },
                "Unknown": true
            }),
        );
        assert_eq!(data.health(), Some(100.0));
        data.set_health(73.5);
        assert_eq!(data.health(), Some(73.5));
        let component = data.component(ENTITY_STATS_COMPONENT).expect("stats");
        assert_eq!(component["Stats"]["Stamina"]["Value"], 12.0);
        assert_eq!(
            component["Stats"]["Health"]["Modifiers"]["set"]["Amount"],
            5.0
        );
        assert_eq!(component["Unknown"], true);

        let text = data.to_json_pretty().unwrap();
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded.health(), Some(73.5));
    }

    #[test]
    fn crafting_knowledge_round_trips_and_preserves_components() {
        let mut data = PlayerData::new(uuid(), transform());
        data.insert_component("Unknown", json!({"x": 1}));
        let mut crafting = crate::crafting::CraftingKnowledge::default();
        crafting.learn("Armor_Bronze_Chest");
        crafting.memories_level = 4;
        crafting.bench_tiers.insert("Armor_Bench".to_string(), 3);
        crafting.augment_tags.insert("goblin_lab_power".to_string());
        data.set_crafting(&crafting);
        assert_eq!(data.crafting(), Some(crafting.clone()));
        let text = data.to_json_pretty().unwrap();
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded.crafting(), Some(crafting));
        assert_eq!(reloaded.component("Unknown"), Some(&json!({"x": 1})));
    }

    #[test]
    fn unknown_components_are_preserved() {
        let source = r#"{
            "Components": {
                "UUID": {"UUID": {"$binary": "EjRWeBI0VniavN7wEjRWeA==", "$type": "04"}},
                "Inventory": {"Items": [], "Version": 3}
            }
        }"#;
        let data = PlayerData::from_json(source).unwrap();
        assert_eq!(data.uuid(), Some(uuid()));
        assert_eq!(data.component("Inventory").unwrap()["Version"], 3);
        let text = data.to_json_pretty().unwrap();
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded.component("Inventory").unwrap()["Version"], 3);
    }

    #[test]
    fn appearance_decodes_model_armor_and_active_items() {
        let source = r#"{
            "Components": {
                "Model": {"Model": {"Id": "Player", "Scale": 1.0, "Static": false,
                    "RandomAttachments": {"Head": "VariantA"}}},
                "ArmorInventory": {
                    "Inventory": {
                        "Id": "Simple",
                        "Capacity": 4,
                        "Items": {
                            "0": {"Id": "Armor_Adamantite_Head"},
                            "1": {"Id": "Armor_Adamantite_Chest"},
                            "2": {"Id": "Armor_Adamantite_Hands"},
                            "3": {"Id": "Armor_Adamantite_Legs"}
                        }
                    }
                },
                "HotbarInventory": {
                    "Inventory": {
                        "Id": "Simple",
                        "Capacity": 9,
                        "Items": {"0": {"Id": "Weapon_Longsword_Adamantite"}}
                    },
                    "ActiveSlot": 0
                },
                "UtilityInventory": {
                    "Inventory": {
                        "Id": "Simple",
                        "Capacity": 4,
                        "Items": {"2": {"Id": "Potion_Health"}}
                    },
                    "ActiveSlot": 2
                }
            }
        }"#;
        let appearance = PlayerData::from_json(source).unwrap().appearance();
        let model = appearance.model.expect("model");
        assert_eq!(model.id, "Player");
        assert_eq!(model.scale, 1.0);
        assert_eq!(
            model.random_attachments.get("Head").map(String::as_str),
            Some("VariantA")
        );
        assert_eq!(
            appearance.armor[0].as_deref(),
            Some("Armor_Adamantite_Head")
        );
        assert_eq!(
            appearance.armor[3].as_deref(),
            Some("Armor_Adamantite_Legs")
        );
        assert_eq!(
            appearance.right_hand.as_deref(),
            Some("Weapon_Longsword_Adamantite")
        );
        assert_eq!(appearance.left_hand.as_deref(), Some("Potion_Health"));
    }

    #[test]
    fn appearance_falls_back_to_the_legacy_inventory_component() {
        let source = r#"{
            "Components": {
                "Inventory": {
                    "Armor": {
                        "Id": "Simple",
                        "Capacity": 4,
                        "Items": {"0": {"Id": "Armor_Bronze_Head"}}
                    },
                    "HotBar": {
                        "Id": "Simple",
                        "Capacity": 9,
                        "Items": {"3": {"Id": "Weapon_Longsword_Adamantite"}}
                    },
                    "ActiveHotbarSlot": 3,
                    "Utility": {
                        "Id": "Simple",
                        "Capacity": 4,
                        "Items": {"1": {"Id": "Potion_Health"}}
                    },
                    "ActiveUtilitySlot": 1
                }
            }
        }"#;
        let appearance = PlayerData::from_json(source).unwrap().appearance();
        assert_eq!(appearance.armor[0].as_deref(), Some("Armor_Bronze_Head"));
        assert_eq!(
            appearance.right_hand.as_deref(),
            Some("Weapon_Longsword_Adamantite")
        );
        assert_eq!(appearance.left_hand.as_deref(), Some("Potion_Health"));
    }

    #[test]
    fn empty_save_has_java_compatible_empty_appearance() {
        let appearance = PlayerData::default().appearance();
        assert_eq!(appearance, PlayerAppearance::default());
        assert_eq!(appearance.armor, [None, None, None, None]);
        assert!(appearance.model.is_none());
        assert!(appearance.skin.is_none());
        assert!(appearance.right_hand.is_none());
        assert!(appearance.left_hand.is_none());
    }

    #[test]
    fn resolve_items_drops_unknown_ids() {
        let mut appearance = PlayerAppearance {
            armor: [
                Some("Known".to_string()),
                Some("Unknown".to_string()),
                None,
                None,
            ],
            right_hand: Some("Unknown".to_string()),
            left_hand: Some("Known".to_string()),
            ..PlayerAppearance::default()
        };
        appearance.resolve_items(|id| id == "Known");
        assert_eq!(appearance.armor[0].as_deref(), Some("Known"));
        assert_eq!(appearance.armor[1], None);
        assert_eq!(appearance.right_hand, None);
        assert_eq!(appearance.left_hand.as_deref(), Some("Known"));
    }

    #[test]
    fn revealed_map_markers_round_trip_as_a_sorted_component() {
        let mut player = PlayerData::new(
            "00000000-0000-0000-0000-000000000004".parse().unwrap(),
            PlayerTransform::new(
                Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                Rotation {
                    pitch: 0.0,
                    yaw: 0.0,
                    roll: 0.0,
                },
            ),
        );
        player.set_revealed_markers(["marker-b".to_string(), "marker-a".to_string()]);
        assert_eq!(
            player.revealed_markers().into_iter().collect::<Vec<_>>(),
            vec!["marker-a".to_string(), "marker-b".to_string()]
        );
        let json = player.to_json_pretty().expect("serialize");
        let loaded = PlayerData::from_json(&json).expect("reload");
        assert_eq!(loaded.revealed_markers(), player.revealed_markers());
    }

    #[test]
    fn saved_skin_component_is_decoded() {
        let source = r#"{
            "Components": {
                "PlayerSkin": {
                    "Skin": {
                        "bodyCharacteristic": "BodyA",
                        "face": "FaceA",
                        "haircut": "HairA",
                        "cape": "CapeA"
                    }
                }
            }
        }"#;
        let skin = PlayerData::from_json(source)
            .unwrap()
            .appearance()
            .skin
            .expect("skin");
        assert_eq!(skin.body_characteristic.as_deref(), Some("BodyA"));
        assert_eq!(skin.face.as_deref(), Some("FaceA"));
        assert_eq!(skin.haircut.as_deref(), Some("HairA"));
        assert_eq!(skin.cape.as_deref(), Some("CapeA"));
        assert!(skin.gloves.is_none());
    }
}
