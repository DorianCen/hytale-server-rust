//! `Server/NPC/Roles` document catalog.
//!
//! The Java NPC role store is built from `Builder` documents: `Type`,
//! `Reference`, `Parameters` and expression placeholders (`Compute`). This
//! first slice preserves the raw document and indexes it by file stem; the
//! builder expression language and the fully evaluated `Role` object remain
//! later T58 work.

use std::collections::BTreeMap;
use std::path::Path;

use hytale_assets::AssetPack;
use serde_json::Value;
use thiserror::Error;

/// Java `Role` builder category.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoleType {
    /// Abstract template.
    Abstract,
    /// Generic template.
    Generic,
    /// Variant of a template.
    Variant,
    /// Component fragment.
    Component,
    /// A category this build does not model.
    Other(String),
}

impl RoleType {
    /// Parse the Java `Type` value.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Abstract" => Self::Abstract,
            "Generic" => Self::Generic,
            "Variant" => Self::Variant,
            "Component" => Self::Component,
            other => Self::Other(other.to_string()),
        }
    }
}

/// One role builder document.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcRole {
    /// Asset key (file stem).
    pub key: String,
    /// Builder category.
    pub role_type: RoleType,
    /// `Reference` template name, if any.
    pub reference: Option<String>,
    /// Raw `Parameters` map.
    pub parameters: BTreeMap<String, Value>,
    /// The merged source document.
    pub raw: Value,
}

impl NpcRole {
    /// Parse one role document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Self {
        let key = key.into();
        let role_type = raw
            .get("Type")
            .and_then(Value::as_str)
            .map(RoleType::from_name)
            .unwrap_or_else(|| RoleType::Other(String::new()));
        let reference = raw
            .get("Reference")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let parameters = raw
            .get("Parameters")
            .and_then(Value::as_object)
            .map(|parameters| {
                parameters
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            key,
            role_type,
            reference,
            parameters,
            raw,
        }
    }

    /// One parameter's raw entry (`{"Value": ...}` or an expression).
    pub fn parameter(&self, name: &str) -> Option<&Value> {
        self.parameters.get(name)
    }

    /// One parameter's literal `Value`, if it has one.
    pub fn parameter_value(&self, name: &str) -> Option<&Value> {
        self.parameter(name).and_then(|entry| entry.get("Value"))
    }

    /// The model/appearance key when the parameter is a literal.
    pub fn appearance(&self) -> Option<&str> {
        self.parameter_value("Appearance").and_then(Value::as_str)
    }

    /// The literal max health, if the role document sets one directly.
    pub fn max_health(&self) -> Option<f64> {
        self.parameter_value("MaxHealth")
            .and_then(Value::as_f64)
            .or_else(|| self.raw.get("MaxHealth").and_then(Value::as_f64))
    }

    /// Raw `MotionControllerList` entries.
    pub fn motion_controllers(&self) -> &[Value] {
        self.raw
            .get("MotionControllerList")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Follow `Reference` links through `catalog`.
    pub fn reference_chain<'a>(&'a self, catalog: &'a RoleCatalog) -> Vec<&'a NpcRole> {
        let mut chain = Vec::new();
        let mut current = self.reference.as_deref();
        while let Some(key) = current {
            let Some(role) = catalog.get(key) else {
                break;
            };
            if chain.iter().any(|entry: &&NpcRole| entry.key == role.key) {
                break;
            }
            chain.push(role);
            current = role.reference.as_deref();
        }
        chain
    }
}

/// All NPC role documents keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct RoleCatalog {
    roles: BTreeMap<String, NpcRole>,
    duplicates: Vec<(String, String)>,
}

impl RoleCatalog {
    /// Load every `Server/NPC/Roles/**/*.json` document from a pack.
    ///
    /// Directory packs are listed in sorted order. If two files share a stem
    /// (the base pack has one such development duplicate), the later path wins
    /// and the duplicate is reported by [`Self::duplicates`].
    pub fn load(pack: &AssetPack) -> Result<Self, RoleError> {
        let mut catalog = Self::default();
        for path in pack.list()? {
            if !path.starts_with("Server/NPC/Roles/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| RoleError::InvalidPath(path.clone()))?
                .to_string();
            let bytes = pack.read(&path)?;
            let raw: Value = serde_json::from_slice(&bytes)?;
            if let Some(previous) = catalog.roles.get(&key) {
                catalog
                    .duplicates
                    .push((previous.key.clone(), path.clone()));
            }
            catalog
                .roles
                .insert(key.clone(), NpcRole::from_json(key, raw));
        }
        Ok(catalog)
    }

    /// Load from an extracted asset root (`<root>/Server/NPC/Roles/...`).
    pub fn load_root(root: &Path) -> Result<Self, RoleError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One role by key.
    pub fn get(&self, key: &str) -> Option<&NpcRole> {
        self.roles.get(key)
    }

    /// Number of unique role keys.
    pub fn len(&self) -> usize {
        self.roles.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// Roles in key order.
    pub fn iter(&self) -> impl Iterator<Item = &NpcRole> {
        self.roles.values()
    }

    /// Duplicate keys as `(key, overridden_path)`.
    pub fn duplicates(&self) -> &[(String, String)] {
        &self.duplicates
    }
}

/// Role loading errors.
#[derive(Debug, Error)]
pub enum RoleError {
    /// Asset pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A role document is not valid JSON.
    #[error("role document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// An asset path had no file name.
    #[error("invalid role asset path `{0}`")]
    InvalidPath(String),
}
