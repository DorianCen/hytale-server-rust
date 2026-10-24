//! Loads `RootInteraction` and `Interaction` assets from the base pack.
//!
//! Java registers `Item/Interactions` and `Item/RootInteractions` as separate
//! asset stores. The full graph executor is not a codec port; this catalog keeps
//! the merged JSON so the runtime can walk the node types needed by block
//! interactions while unknown node types remain inert.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hytale_assets::{AssetError, AssetPack};
use serde_json::{Map, Value};
use thiserror::Error;

/// Interaction catalog errors.
#[derive(Debug, Error)]
pub enum InteractionError {
    /// Pack read/parse failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// A JSON document is malformed.
    #[error("{path}: invalid JSON: {source}")]
    Json {
        /// Pack-relative path.
        path: String,
        /// Serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A `Parent` chain is cyclic or missing.
    #[error("interaction `{key}`: {message}")]
    Parent {
        /// Asset key.
        key: String,
        /// What failed.
        message: String,
    },
}

/// Root interaction and interaction nodes.
#[derive(Debug, Clone, Default)]
pub struct InteractionCatalog {
    interactions: BTreeMap<String, Value>,
    roots: BTreeMap<String, Value>,
}

impl InteractionCatalog {
    /// Load both stores from a base-game pack.
    pub fn load(pack: &AssetPack) -> Result<Self, InteractionError> {
        Ok(Self {
            interactions: load_json_store(pack, "Server/Item/Interactions")?,
            roots: load_json_store(pack, "Server/Item/RootInteractions")?,
        })
    }

    /// Load from an unpacked asset tree root.
    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self, InteractionError> {
        let pack = AssetPack::open(root.as_ref())?;
        Self::load(&pack)
    }

    /// Build a catalog from already-loaded documents (tests and plugin seams).
    pub fn from_parts(
        interactions: BTreeMap<String, Value>,
        roots: BTreeMap<String, Value>,
    ) -> Self {
        Self {
            interactions: interactions
                .into_iter()
                .map(|(key, value)| (key.to_ascii_lowercase(), value))
                .collect(),
            roots: roots
                .into_iter()
                .map(|(key, value)| (key.to_ascii_lowercase(), value))
                .collect(),
        }
    }

    /// One merged interaction node document.
    pub fn interaction(&self, key: &str) -> Option<&Value> {
        self.interactions.get(&key.to_ascii_lowercase())
    }

    /// One merged root interaction document.
    pub fn root(&self, key: &str) -> Option<&Value> {
        self.roots.get(&key.to_ascii_lowercase())
    }

    /// Number of interaction nodes.
    pub fn interaction_count(&self) -> usize {
        self.interactions.len()
    }

    /// Number of root interactions.
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Resolve an inline node's `Parent` against the interaction store.
    pub fn resolve_node(&self, value: &Value) -> Option<Value> {
        let parent = value.get("Parent").and_then(Value::as_str);
        let mut resolved = match parent {
            Some(parent) => self.interaction(parent)?.clone(),
            None => Value::Object(Map::new()),
        };
        merge_value(&mut resolved, value);
        Some(resolved)
    }
}

fn load_json_store(
    pack: &AssetPack,
    prefix: &str,
) -> Result<BTreeMap<String, Value>, InteractionError> {
    let paths: Vec<String> = pack
        .list()?
        .into_iter()
        .filter(|path| path.starts_with(prefix) && path.ends_with(".json"))
        .collect();
    let mut raw: BTreeMap<String, (Option<String>, Value)> = BTreeMap::new();
    for (path, bytes) in paths.iter().zip(pack.read_many(&paths)?) {
        let key = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .strip_suffix(".json")
            .unwrap_or(path)
            .to_ascii_lowercase();
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|source| InteractionError::Json {
                path: path.clone(),
                source,
            })?;
        let parent = value
            .get("Parent")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase);
        raw.insert(key, (parent, value));
    }

    let mut resolved = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    for key in raw.keys() {
        resolve_raw(key, &raw, &mut resolved, &mut visiting)?;
    }
    Ok(resolved)
}

fn resolve_raw(
    key: &str,
    raw: &BTreeMap<String, (Option<String>, Value)>,
    resolved: &mut BTreeMap<String, Value>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), InteractionError> {
    if resolved.contains_key(key) {
        return Ok(());
    }
    let Some((parent, value)) = raw.get(key) else {
        return Err(InteractionError::Parent {
            key: key.to_string(),
            message: "missing parent".to_string(),
        });
    };
    if !visiting.insert(key.to_string()) {
        return Err(InteractionError::Parent {
            key: key.to_string(),
            message: "parent cycle".to_string(),
        });
    }
    let mut merged = if let Some(parent) = parent {
        let parent = parent.to_ascii_lowercase();
        resolve_raw(&parent, raw, resolved, visiting)?;
        resolved.get(&parent).cloned().unwrap_or(Value::Null)
    } else {
        Value::Object(Map::new())
    };
    merge_value(&mut merged, value);
    visiting.remove(key);
    resolved.insert(key.to_string(), merged);
    Ok(())
}

/// Java `BuilderCodec` inheritance: objects merge recursively, arrays/scalars
/// replace.
pub fn merge_value(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                if key == "Parent" {
                    continue;
                }
                match target.get_mut(key) {
                    Some(existing) => merge_value(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, source) => *target = source.clone(),
    }
}
