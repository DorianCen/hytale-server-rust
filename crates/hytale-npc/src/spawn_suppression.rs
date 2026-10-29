//! NPC groups and spawn-suppression zones.
//!
//! Java builds flattened `NPCGroup` tag sets and keeps active
//! `SpawnSuppressionComponent` entities in the world. World spawning checks the
//! role against those zones before accepting a position. This module ports the
//! deterministic data side: the 114 `Server/NPC/Groups` documents (with
//! include/exclude groups, roles and globs) and the five
//! `Server/NPC/Spawn/Suppression` configurations.
//!
//! Active suppressor entities are a caller-owned map keyed by a stable id; the
//! world thread persists that map and rebuilds the zones on boot.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hytale_assets::AssetPack;
use hytale_world::collision::Vec3;
use serde_json::Value;
use thiserror::Error;

use crate::block_set::wildcard_matches;

/// One `Server/NPC/Groups` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcGroup {
    /// Asset key.
    pub key: String,
    /// `IncludeRoles` (individual role ids/globs).
    pub include_roles: Vec<String>,
    /// `ExcludeRoles`.
    pub exclude_roles: Vec<String>,
    /// `IncludeGroups` (other NPCGroup keys).
    pub include_groups: Vec<String>,
    /// `ExcludeGroups`.
    pub exclude_groups: Vec<String>,
}

impl NpcGroup {
    /// Parse one group document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Self {
        let strings = |field: &str| -> Vec<String> {
            raw.get(field)
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            key: key.into(),
            include_roles: strings("IncludeRoles"),
            exclude_roles: strings("ExcludeRoles"),
            include_groups: strings("IncludeGroups"),
            exclude_groups: strings("ExcludeGroups"),
        }
    }
}

/// All NPC group documents keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct NpcGroupCatalog {
    groups: BTreeMap<String, NpcGroup>,
}

impl NpcGroupCatalog {
    /// Load `Server/NPC/Groups/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnSuppressionError> {
        let mut groups = BTreeMap::new();
        for path in pack.list()? {
            if !path.starts_with("Server/NPC/Groups/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| SpawnSuppressionError::InvalidPath(path.clone()))?
                .to_string();
            let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
            groups.insert(key.clone(), NpcGroup::from_json(key, raw));
        }
        Ok(Self { groups })
    }

    /// Load from an extracted root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnSuppressionError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One group by key.
    pub fn get(&self, key: &str) -> Option<&NpcGroup> {
        self.groups.get(key)
    }

    /// Number of groups.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Groups in key order.
    pub fn iter(&self) -> impl Iterator<Item = &NpcGroup> {
        self.groups.values()
    }

    /// Java `TagSetLookup.tagInSet`: whether `role_key` belongs to `group_key`.
    ///
    /// The order mirrors `TagSetLookupTable`: included groups first, then
    /// excluded groups, then included roles/globs, then excluded roles/globs.
    pub fn contains_role(&self, group_key: &str, role_key: &str) -> bool {
        let mut visiting = BTreeSet::new();
        self.contains_role_inner(group_key, role_key, &mut visiting)
    }

    /// All role/group tags reachable from `group_key` (diagnostics/tests).
    pub fn role_patterns(&self, group_key: &str) -> Vec<String> {
        let mut patterns = Vec::new();
        let mut visiting = BTreeSet::new();
        self.collect_patterns(group_key, &mut patterns, &mut visiting);
        patterns
    }

    fn contains_role_inner(
        &self,
        group_key: &str,
        role_key: &str,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        let Some(group) = self.groups.get(group_key) else {
            // A leave-node group name is also a valid single-role tag in Java.
            return group_key == role_key;
        };
        if !visiting.insert(group_key.to_string()) {
            return false;
        }
        if group
            .exclude_roles
            .iter()
            .any(|pattern| wildcard_matches(pattern, role_key))
        {
            visiting.remove(group_key);
            return false;
        }

        let mut member = group
            .include_groups
            .iter()
            .any(|included| self.contains_role_inner(included, role_key, visiting));
        if member
            && group
                .exclude_groups
                .iter()
                .any(|excluded| self.contains_role_inner(excluded, role_key, visiting))
        {
            member = false;
        }
        // Java adds included roles after excluded groups, so an explicit role
        // include overrides an overlapping excluded group.
        if group
            .include_roles
            .iter()
            .any(|pattern| matches_role_pattern(pattern, role_key))
        {
            member = true;
        }
        visiting.remove(group_key);
        member
    }

    fn collect_patterns(
        &self,
        group_key: &str,
        patterns: &mut Vec<String>,
        visiting: &mut BTreeSet<String>,
    ) {
        let Some(group) = self.groups.get(group_key) else {
            patterns.push(group_key.to_string());
            return;
        };
        if !visiting.insert(group_key.to_string()) {
            return;
        }
        for included in &group.include_groups {
            self.collect_patterns(included, patterns, visiting);
        }
        patterns.extend(group.include_roles.iter().cloned());
        visiting.remove(group_key);
    }
}

/// A role glob. Java `StringUtil` patterns are the same `*` form used by
/// block/group matching; without a `*` it is a literal role id.
fn matches_role_pattern(pattern: &str, role_key: &str) -> bool {
    if pattern.contains('*') {
        wildcard_matches(pattern, role_key)
    } else {
        pattern == role_key
    }
}

/// One `Server/NPC/Spawn/Suppression` document.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnSuppression {
    /// Asset key.
    pub key: String,
    /// Suppression radius (Java default 10).
    pub radius: f64,
    /// NPCGroup ids whose roles cannot spawn inside the zone.
    pub suppressed_groups: Vec<String>,
    /// Whether spawn markers inside the zone are disabled.
    pub suppress_spawn_markers: bool,
}

impl SpawnSuppression {
    /// Parse one suppression document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Self {
        Self {
            key: key.into(),
            radius: raw
                .get("SuppressionRadius")
                .and_then(Value::as_f64)
                .unwrap_or(10.0)
                .max(0.0),
            suppressed_groups: raw
                .get("SuppressedGroups")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            suppress_spawn_markers: raw
                .get("SuppressSpawnMarkers")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// All suppression configurations keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct SpawnSuppressionCatalog {
    suppressions: BTreeMap<String, SpawnSuppression>,
}

impl SpawnSuppressionCatalog {
    /// Load `Server/NPC/Spawn/Suppression/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnSuppressionError> {
        let mut suppressions = BTreeMap::new();
        for path in pack.list()? {
            if !path.starts_with("Server/NPC/Spawn/Suppression/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| SpawnSuppressionError::InvalidPath(path.clone()))?
                .to_string();
            let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
            suppressions.insert(key.clone(), SpawnSuppression::from_json(key, raw));
        }
        Ok(Self { suppressions })
    }

    /// Load from an extracted root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnSuppressionError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One suppression by key.
    pub fn get(&self, key: &str) -> Option<&SpawnSuppression> {
        self.suppressions.get(key)
    }

    /// Number of suppression configs.
    pub fn len(&self) -> usize {
        self.suppressions.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.suppressions.is_empty()
    }

    /// Configs in key order.
    pub fn iter(&self) -> impl Iterator<Item = &SpawnSuppression> {
        self.suppressions.values()
    }
}

/// One active suppressor entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnSuppressor {
    /// Stable caller-owned id.
    pub id: u64,
    /// Suppressor world position.
    pub position: Vec3,
}

/// A pre-resolved suppression zone for spawn-position checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnSuppressionZone {
    /// Zone center.
    pub center: Vec3,
    /// Suppression radius.
    pub radius: f64,
}

impl SpawnSuppressionZone {
    /// Whether `position` is inside this zone.
    pub fn contains(&self, position: Vec3) -> bool {
        distance_squared(self.center, position) <= self.radius * self.radius
    }
}

/// Group/suppression catalogs plus the current active suppressors.
#[derive(Debug, Clone, Default)]
pub struct SpawnSuppressionIndex {
    /// NPC group definitions.
    pub groups: NpcGroupCatalog,
    /// Suppression configurations.
    pub configs: SpawnSuppressionCatalog,
    /// Active suppressors keyed by id.
    pub suppressors: BTreeMap<u64, (String, SpawnSuppressor)>,
}

impl SpawnSuppressionIndex {
    /// Load both stores.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnSuppressionError> {
        Ok(Self {
            groups: NpcGroupCatalog::load(pack)?,
            configs: SpawnSuppressionCatalog::load(pack)?,
            suppressors: BTreeMap::new(),
        })
    }

    /// Load from an extracted root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnSuppressionError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// Register/replace an active suppressor.
    pub fn insert(&mut self, id: u64, suppression: impl Into<String>, position: Vec3) -> bool {
        self.suppressors
            .insert(id, (suppression.into(), SpawnSuppressor { id, position }))
            .is_none()
    }

    /// Remove an active suppressor.
    pub fn remove(&mut self, id: u64) -> Option<(String, SpawnSuppressor)> {
        self.suppressors.remove(&id)
    }

    /// Active suppressor state in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &SpawnSuppressor)> {
        self.suppressors
            .values()
            .map(|(suppression, suppressor)| (suppression, suppressor))
    }

    /// Whether `role` may not spawn at `position`.
    pub fn suppresses_role(&self, role: &str, position: Vec3) -> bool {
        self.suppressors.values().any(|(suppression, suppressor)| {
            let Some(config) = self.configs.get(suppression) else {
                return false;
            };
            SpawnSuppressionZone {
                center: suppressor.position,
                radius: config.radius,
            }
            .contains(position)
                && config
                    .suppressed_groups
                    .iter()
                    .any(|group| self.groups.contains_role(group, role))
        })
    }

    /// Zones that suppress `role` (ordered by suppressor id).
    pub fn zones_for_role(&self, role: &str) -> Vec<SpawnSuppressionZone> {
        self.suppressors
            .values()
            .filter_map(|(suppression, suppressor)| {
                let config = self.configs.get(suppression)?;
                config
                    .suppressed_groups
                    .iter()
                    .any(|group| self.groups.contains_role(group, role))
                    .then_some(SpawnSuppressionZone {
                        center: suppressor.position,
                        radius: config.radius,
                    })
            })
            .collect()
    }

    /// Whether a spawn marker at `position` is disabled.
    pub fn suppresses_marker(&self, position: Vec3) -> bool {
        self.suppressors.values().any(|(suppression, suppressor)| {
            let Some(config) = self.configs.get(suppression) else {
                return false;
            };
            config.suppress_spawn_markers
                && SpawnSuppressionZone {
                    center: suppressor.position,
                    radius: config.radius,
                }
                .contains(position)
        })
    }
}

fn distance_squared(left: Vec3, right: Vec3) -> f64 {
    let dx = right.x - left.x;
    let dy = right.y - left.y;
    let dz = right.z - left.z;
    dx * dx + dy * dy + dz * dz
}

/// Spawn suppression loading errors.
#[derive(Debug, Error)]
pub enum SpawnSuppressionError {
    /// Pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A document is invalid JSON.
    #[error("spawn suppression document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// An asset path had no file name.
    #[error("invalid spawn suppression asset path `{0}`")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> NpcGroupCatalog {
        let groups = BTreeMap::from([
            (
                "Prey".to_string(),
                NpcGroup::from_json(
                    "Prey",
                    serde_json::json!({"IncludeRoles": ["Rat", "Bunny*"]}),
                ),
            ),
            (
                "Predators".to_string(),
                NpcGroup::from_json(
                    "Predators",
                    serde_json::json!({"IncludeGroups": ["Prey"], "ExcludeRoles": ["Rat"]}),
                ),
            ),
            (
                "World".to_string(),
                NpcGroup::from_json("World", serde_json::json!({"IncludeGroups": ["Predators"]})),
            ),
        ]);
        NpcGroupCatalog { groups }
    }

    #[test]
    fn group_membership_includes_globs_groups_and_exclusions() {
        let groups = catalog();
        assert!(groups.contains_role("Prey", "Rat"));
        assert!(groups.contains_role("Prey", "Bunny_Snow"));
        assert!(!groups.contains_role("Prey", "Bear_Grizzly"));
        assert!(groups.contains_role("Predators", "Bunny_Snow"));
        assert!(!groups.contains_role("Predators", "Rat"));
        assert!(groups.contains_role("World", "Bunny_Snow"));
        assert!(!groups.contains_role("World", "Rat"));
    }

    #[test]
    fn suppression_zones_filter_roles_and_markers() {
        let mut index = SpawnSuppressionIndex {
            groups: catalog(),
            configs: SpawnSuppressionCatalog {
                suppressions: BTreeMap::from([(
                    "Camp".to_string(),
                    SpawnSuppression {
                        key: "Camp".to_string(),
                        radius: 10.0,
                        suppressed_groups: vec!["Predators".to_string()],
                        suppress_spawn_markers: true,
                    },
                )]),
            },
            suppressors: BTreeMap::new(),
        };
        index.insert(1, "Camp", Vec3::new(0.0, 64.0, 0.0));
        assert!(index.suppresses_role("Bunny_Snow", Vec3::new(1.0, 64.0, 1.0)));
        assert!(!index.suppresses_role("Bunny_Snow", Vec3::new(20.0, 64.0, 0.0)));
        assert!(!index.suppresses_role("Rat", Vec3::new(1.0, 64.0, 1.0)));
        assert!(index.suppresses_marker(Vec3::new(0.0, 64.0, 0.0)));
        assert_eq!(index.zones_for_role("Bunny_Snow").len(), 1);
    }
}
