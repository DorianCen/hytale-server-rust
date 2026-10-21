//! `Parent` inheritance and per-store merge rules.
//!
//! Java resolves `Parent` before typed decoding
//! (`AssetStore/.../codec/AssetCodec.java` and `ContainedAssetCodec.java`). The
//! resolver here works on `serde_json::Value`: object fields merge recursively,
//! scalars and arrays are replaced by default, and metadata keys (`Parent` and
//! `$...`) are removed from the final value.
//!
//! Stores that append or otherwise merge differently supply a [`MergeHook`].

use std::collections::BTreeMap;

use crate::pack::{AssetError, ParentProvider};

type Result<T> = std::result::Result<T, AssetError>;

/// How arrays are combined when merging a parent and child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayMerge {
    /// Child arrays replace parent arrays (Java's default for most stores).
    Replace,
    /// Parent elements are kept and child elements appended.
    Append,
}

/// Per-store merge behavior.
#[derive(Debug, Clone, Copy)]
pub enum MergeHook {
    /// Recursive object merge, child scalars/arrays replace parent values.
    Replace,
    /// Recursive object merge, arrays append parent then child.
    AppendArrays,
    /// Store-specific hook. It receives the already-resolved parent and child.
    Custom(fn(&mut serde_json::Value, serde_json::Value)),
}

impl MergeHook {
    /// Merge `child` into `parent`.
    pub fn merge(
        self,
        mut parent: serde_json::Value,
        child: serde_json::Value,
    ) -> serde_json::Value {
        match self {
            Self::Custom(hook) => {
                hook(&mut parent, child);
                parent
            }
            Self::Replace => merge_values(parent, child, ArrayMerge::Replace),
            Self::AppendArrays => merge_values(parent, child, ArrayMerge::Append),
        }
    }
}

/// Recursive merge with an explicit array policy.
pub fn merge_values(
    parent: serde_json::Value,
    child: serde_json::Value,
    arrays: ArrayMerge,
) -> serde_json::Value {
    match (parent, child) {
        (serde_json::Value::Object(mut parent), serde_json::Value::Object(child)) => {
            for (key, value) in child {
                if is_metadata(&key) {
                    continue;
                }
                if let Some(existing) = parent.remove(&key) {
                    parent.insert(key, merge_values(existing, value, arrays));
                } else {
                    parent.insert(key, strip_metadata(value));
                }
            }
            serde_json::Value::Object(parent)
        }
        (serde_json::Value::Array(mut parent), serde_json::Value::Array(child)) => match arrays {
            ArrayMerge::Replace => {
                serde_json::Value::Array(child.into_iter().map(strip_metadata).collect())
            }
            ArrayMerge::Append => {
                parent.extend(child.into_iter().map(strip_metadata));
                serde_json::Value::Array(parent)
            }
        },
        (_, child) => strip_metadata(child),
    }
}

fn is_metadata(key: &str) -> bool {
    key == "Parent" || key.starts_with('$')
}

/// Remove metadata keys recursively from a newly inserted value.
fn strip_metadata(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(mut object) => {
            object.retain(|key, _| !is_metadata(key));
            serde_json::Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, strip_metadata(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(strip_metadata).collect())
        }
        other => other,
    }
}

/// Resolves `Parent` chains for a provider.
pub struct ParentResolver<'a> {
    provider: &'a dyn ParentProvider,
    hook: MergeHook,
}

impl<'a> ParentResolver<'a> {
    /// Resolve with default (array-replacing) merge behavior.
    pub fn new(provider: &'a dyn ParentProvider) -> Self {
        Self {
            provider,
            hook: MergeHook::Replace,
        }
    }

    /// Resolve with a store-specific merge hook.
    pub fn with_hook(provider: &'a dyn ParentProvider, hook: MergeHook) -> Self {
        Self { provider, hook }
    }

    /// Resolve `key` and its full parent chain.
    pub fn resolve(&self, key: &str) -> Result<serde_json::Value> {
        let mut chain = Vec::new();
        self.resolve_inner(key, &mut chain)
    }

    /// Resolve every key in a provider, returning a key → value map.
    pub fn resolve_all(&self, keys: &[String]) -> Result<BTreeMap<String, serde_json::Value>> {
        let mut assets = BTreeMap::new();
        for key in keys {
            assets.insert(key.clone(), self.resolve(key)?);
        }
        Ok(assets)
    }

    fn resolve_inner(&self, key: &str, chain: &mut Vec<String>) -> Result<serde_json::Value> {
        if chain.iter().any(|visited| visited == key) {
            let mut cycle = chain.clone();
            cycle.push(key.to_string());
            return Err(AssetError::ParentCycle {
                chain: cycle.join(" -> "),
            });
        }
        let Some(child) = self.provider.load(key) else {
            return Err(AssetError::MissingParent {
                parent: key.to_string(),
                asset: chain.last().cloned().unwrap_or_default(),
                chain: chain.join(" -> "),
            });
        };
        let Some(parent_key) = child.get("Parent").and_then(|value| value.as_str()) else {
            return Ok(strip_metadata(child));
        };
        let parent_key = parent_key.to_string();

        chain.push(key.to_string());
        let parent = self.resolve_inner(&parent_key, chain)?;
        chain.pop();

        Ok(self.hook.merge(parent, strip_metadata(child)))
    }
}
