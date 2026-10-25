//! Internal `GameplayConfig` assets (`Server/GameplayConfigs`).
//!
//! Java's asset store assigns no numeric ids and has no packet generator; the
//! world joins the selected config to a `WorldGameplayConfig` packet on demand.
//! Only the fields the protocol exposes are modelled here:
//!
//! - `World.AllowBlockBreaking` / `BlockBreakingBypassTag` /
//!   `AllowBlockGathering` / `AllowBlockPlacement` /
//!   `BlockedRootInteractionTags`;
//! - `Combat.DisplayHealthBars` / `DisplayCombatText`;
//! - `Player.ArmorVisibilityOption`.
//!
//! The day/night durations and moon phases are read by the world clock from
//! [`GameplayWorldConfig`]. Every absent section or field falls back to the
//! Java `GameplayConfig.DEFAULT` initialisers; an unparsable asset is rejected
//! by the loader and the world uses [`GameplayConfigAsset::default`].

use std::collections::BTreeMap;

use hytale_protocol::generated::common::{
    ArmorVisibilityOption, CombatConfig, PlayerConfig, WorldConfig as ProtocolWorldConfig,
};
use hytale_protocol::generated::world::WorldGameplayConfig;
use serde_json::Value;

/// Java `WorldConfig.DEFAULT_DAYTIME_DURATION_SECONDS` (28:48 real).
const DEFAULT_DAYTIME_DURATION_SECONDS: i32 = 1_728;
/// Java `WorldConfig.DEFAULT_NIGHTTIME_DURATION_SECONDS` (19:12 real).
const DEFAULT_NIGHTTIME_DURATION_SECONDS: i32 = 1_152;
/// Java `WorldConfig` default total moon phases.
const DEFAULT_TOTAL_MOON_PHASES: i32 = 5;

/// Java `GameplayConfig.DEFAULT_ID`.
pub const DEFAULT_GAMEPLAY_CONFIG_ID: &str = "Default";

/// `GameplayConfig.World`.
#[derive(Debug, Clone, PartialEq)]
pub struct GameplayWorldConfig {
    /// `AllowBlockBreaking` (Java default `true`).
    pub allow_block_breaking: bool,
    /// `BlockBreakingBypassTag`; resolved to a tag index for the packet.
    pub block_breaking_bypass_tag: Option<String>,
    /// `AllowBlockGathering` (Java default `true`).
    pub allow_block_gathering: bool,
    /// `AllowBlockPlacement` (Java default `true`).
    pub allow_block_placement: bool,
    /// `BlockedRootInteractionTags`.
    pub blocked_root_interaction_tags: Vec<String>,
    /// `BlockPlacementFragilityTimer` (seconds).
    pub block_placement_fragility_timer: f32,
    /// `DaytimeDurationSeconds` (real seconds of daylight).
    pub daytime_duration_seconds: i32,
    /// `NighttimeDurationSeconds` (real seconds of night).
    pub nighttime_duration_seconds: i32,
    /// `TotalMoonPhases`.
    pub total_moon_phases: i32,
}

impl Default for GameplayWorldConfig {
    fn default() -> Self {
        Self {
            allow_block_breaking: true,
            block_breaking_bypass_tag: None,
            allow_block_gathering: true,
            allow_block_placement: true,
            blocked_root_interaction_tags: Vec::new(),
            block_placement_fragility_timer: 0.0,
            daytime_duration_seconds: DEFAULT_DAYTIME_DURATION_SECONDS,
            nighttime_duration_seconds: DEFAULT_NIGHTTIME_DURATION_SECONDS,
            total_moon_phases: DEFAULT_TOTAL_MOON_PHASES,
        }
    }
}

/// `GameplayConfig.Combat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameplayCombatConfig {
    /// `DisplayHealthBars` (Java default `true`).
    pub display_health_bars: bool,
    /// `DisplayCombatText` (Java default `true`).
    pub display_combat_text: bool,
}

impl Default for GameplayCombatConfig {
    fn default() -> Self {
        Self {
            display_health_bars: true,
            display_combat_text: true,
        }
    }
}

/// `GameplayConfig.Player`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameplayPlayerConfig {
    /// `ArmorVisibilityOption` (Java default `ALL`).
    pub armor_visibility_option: ArmorVisibilityOption,
    /// `MovementConfig` asset id (Java default `Default`).
    pub movement_config: String,
}

impl Default for GameplayPlayerConfig {
    fn default() -> Self {
        Self {
            armor_visibility_option: ArmorVisibilityOption::All,
            movement_config: "Default".to_string(),
        }
    }
}

/// One loaded `GameplayConfig` asset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameplayConfigAsset {
    /// `World` section.
    pub world: GameplayWorldConfig,
    /// `Combat` section.
    pub combat: GameplayCombatConfig,
    /// `Player` section.
    pub player: GameplayPlayerConfig,
}

impl GameplayConfigAsset {
    /// Decode one asset document.
    ///
    /// Unknown fields are ignored (Java plugins add their own sections); a
    /// non-object document or a malformed modelled field is an error so the
    /// loader reports the asset instead of silently using defaults.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "GameplayConfig must be a JSON object".to_string())?;
        let mut config = Self::default();

        if let Some(world) = object.get("World") {
            let world = world
                .as_object()
                .ok_or_else(|| "GameplayConfig.World must be an object".to_string())?;
            if let Some(value) = world.get("AllowBlockBreaking") {
                config.world.allow_block_breaking = boolean(value, "World.AllowBlockBreaking")?;
            }
            if let Some(value) = world.get("BlockBreakingBypassTag") {
                config.world.block_breaking_bypass_tag =
                    optional_string(value, "World.BlockBreakingBypassTag")?;
            }
            if let Some(value) = world.get("AllowBlockGathering") {
                config.world.allow_block_gathering = boolean(value, "World.AllowBlockGathering")?;
            }
            if let Some(value) = world.get("AllowBlockPlacement") {
                config.world.allow_block_placement = boolean(value, "World.AllowBlockPlacement")?;
            }
            if let Some(value) = world.get("BlockedRootInteractionTags") {
                config.world.blocked_root_interaction_tags =
                    string_array(value, "World.BlockedRootInteractionTags")?;
            }
            if let Some(value) = world.get("BlockPlacementFragilityTimer") {
                config.world.block_placement_fragility_timer =
                    number(value, "World.BlockPlacementFragilityTimer")? as f32;
            }
            if let Some(value) = world.get("DaytimeDurationSeconds") {
                config.world.daytime_duration_seconds =
                    integer(value, "World.DaytimeDurationSeconds")?;
            }
            if let Some(value) = world.get("NighttimeDurationSeconds") {
                config.world.nighttime_duration_seconds =
                    integer(value, "World.NighttimeDurationSeconds")?;
            }
            if let Some(value) = world.get("TotalMoonPhases") {
                config.world.total_moon_phases = integer(value, "World.TotalMoonPhases")?;
            }
        }

        if let Some(combat) = object.get("Combat") {
            let combat = combat
                .as_object()
                .ok_or_else(|| "GameplayConfig.Combat must be an object".to_string())?;
            if let Some(value) = combat.get("DisplayHealthBars") {
                config.combat.display_health_bars = boolean(value, "Combat.DisplayHealthBars")?;
            }
            if let Some(value) = combat.get("DisplayCombatText") {
                config.combat.display_combat_text = boolean(value, "Combat.DisplayCombatText")?;
            }
        }

        if let Some(player) = object.get("Player") {
            let player = player
                .as_object()
                .ok_or_else(|| "GameplayConfig.Player must be an object".to_string())?;
            if let Some(value) = player.get("ArmorVisibilityOption") {
                let value = value.as_str().ok_or_else(|| {
                    "GameplayConfig.Player.ArmorVisibilityOption must be a string".to_string()
                })?;
                config.player.armor_visibility_option =
                    parse_armor_visibility(value).ok_or_else(|| {
                        format!("unknown GameplayConfig.Player.ArmorVisibilityOption `{value}`")
                    })?;
            }
            if let Some(value) = player.get("MovementConfig") {
                config.player.movement_config = value
                    .as_str()
                    .ok_or_else(|| {
                        "GameplayConfig.Player.MovementConfig must be a string".to_string()
                    })?
                    .to_string();
            }
        }

        Ok(config)
    }

    /// Build the `WorldGameplayConfig` packet.
    ///
    /// Tag references resolve through the server tag map; unknown tags become
    /// Java's `AssetRegistry.TAG_NOT_FOUND` (`-1`) for the bypass tag and are
    /// dropped from the blocked-tag list, matching `WorldConfig.afterDecode`.
    pub fn to_packet(&self, tags: &BTreeMap<String, i32>) -> WorldGameplayConfig {
        let bypass_tag_index = self
            .world
            .block_breaking_bypass_tag
            .as_deref()
            .and_then(|tag| tags.get(tag).copied())
            .unwrap_or(-1);
        let blocked_root_interaction_tag_indexes = self
            .world
            .blocked_root_interaction_tags
            .iter()
            .filter_map(|tag| tags.get(tag).copied())
            .collect();

        WorldGameplayConfig {
            world_config: ProtocolWorldConfig {
                allow_block_breaking: self.world.allow_block_breaking,
                block_breaking_bypass_tag_index: bypass_tag_index,
                allow_block_gathering: self.world.allow_block_gathering,
                allow_block_placement: self.world.allow_block_placement,
                blocked_root_interaction_tag_indexes: Some(blocked_root_interaction_tag_indexes),
            },
            combat_config: CombatConfig {
                display_health_bars: self.combat.display_health_bars,
                display_combat_text: self.combat.display_combat_text,
            },
            player_config: PlayerConfig {
                armor_visibility_option: self.player.armor_visibility_option,
            },
        }
    }
}

fn boolean(value: &Value, field: &str) -> Result<bool, String> {
    value
        .as_bool()
        .ok_or_else(|| format!("GameplayConfig.{field} must be a boolean"))
}

fn integer(value: &Value, field: &str) -> Result<i32, String> {
    let value = value
        .as_i64()
        .ok_or_else(|| format!("GameplayConfig.{field} must be an integer"))?;
    i32::try_from(value).map_err(|_| format!("GameplayConfig.{field} is out of range"))
}

fn number(value: &Value, field: &str) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("GameplayConfig.{field} must be a number"))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, String> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| format!("GameplayConfig.{field} must be a string"))
}

fn string_array(value: &Value, field: &str) -> Result<Vec<String>, String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("GameplayConfig.{field} must be an array"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| format!("GameplayConfig.{field} entries must be strings"))
        })
        .collect()
}

fn parse_armor_visibility(value: &str) -> Option<ArmorVisibilityOption> {
    match value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
        .as_str()
    {
        "all" => Some(ArmorVisibilityOption::All),
        "helmetonly" => Some(ArmorVisibilityOption::HelmetOnly),
        "none" => Some(ArmorVisibilityOption::None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_sections_use_java_defaults() {
        let config = GameplayConfigAsset::from_value(&json!({})).expect("empty config");
        assert_eq!(config, GameplayConfigAsset::default());
        assert!(config.world.allow_block_breaking);
        assert!(config.world.allow_block_gathering);
        assert!(config.world.allow_block_placement);
        assert_eq!(config.world.daytime_duration_seconds, 1_728);
        assert_eq!(config.world.nighttime_duration_seconds, 1_152);
        assert_eq!(config.world.total_moon_phases, 5);
        assert!(config.combat.display_health_bars);
        assert!(config.combat.display_combat_text);
        assert_eq!(
            config.player.armor_visibility_option,
            ArmorVisibilityOption::All
        );
    }

    #[test]
    fn default_json_subset_is_parsed() {
        let config = GameplayConfigAsset::from_value(&json!({
            "World": {
                "DaytimeDurationSeconds": 1728,
                "NighttimeDurationSeconds": 1152,
                "BlockPlacementFragilityTimer": 0,
                "Sleep": { "WakeUpHour": 4.79 }
            },
            "Player": {
                "MovementConfig": "Default",
                "ArmorVisibilityOption": "All"
            }
        }))
        .expect("config");
        assert_eq!(config.world.daytime_duration_seconds, 1_728);
        assert_eq!(config.world.nighttime_duration_seconds, 1_152);
        assert_eq!(
            config.player.armor_visibility_option,
            ArmorVisibilityOption::All
        );
    }

    #[test]
    fn packet_resolves_tags_and_flags() {
        let config = GameplayConfigAsset::from_value(&json!({
            "World": {
                "AllowBlockBreaking": false,
                "BlockBreakingBypassTag": "Type=Bypass",
                "AllowBlockGathering": false,
                "AllowBlockPlacement": true,
                "BlockedRootInteractionTags": ["Type=Bed", "Missing"]
            },
            "Combat": {
                "DisplayHealthBars": false,
                "DisplayCombatText": true
            },
            "Player": {
                "ArmorVisibilityOption": "HelmetOnly"
            }
        }))
        .expect("config");
        let mut tags = BTreeMap::new();
        tags.insert("Type=Bypass".to_string(), 42);
        tags.insert("Type=Bed".to_string(), 7);

        let packet = config.to_packet(&tags);
        assert!(!packet.world_config.allow_block_breaking);
        assert_eq!(packet.world_config.block_breaking_bypass_tag_index, 42);
        assert!(!packet.world_config.allow_block_gathering);
        assert!(packet.world_config.allow_block_placement);
        assert_eq!(
            packet.world_config.blocked_root_interaction_tag_indexes,
            Some(vec![7])
        );
        assert!(!packet.combat_config.display_health_bars);
        assert!(packet.combat_config.display_combat_text);
        assert_eq!(
            packet.player_config.armor_visibility_option,
            ArmorVisibilityOption::HelmetOnly
        );
    }

    #[test]
    fn unknown_bypass_tag_becomes_not_found() {
        let config = GameplayConfigAsset::from_value(&json!({
            "World": { "BlockBreakingBypassTag": "Type=Unknown" }
        }))
        .expect("config");
        let packet = config.to_packet(&BTreeMap::new());
        assert_eq!(packet.world_config.block_breaking_bypass_tag_index, -1);
        assert_eq!(
            packet.world_config.blocked_root_interaction_tag_indexes,
            Some(Vec::new())
        );
    }
}
