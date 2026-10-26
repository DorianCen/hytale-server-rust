//! `Server/NPC/Flocks` definitions and membership.
//!
//! Java `RangeSizeFlockAsset` and `WeightedSizeFlockAsset` are ported with the
//! same field names and size-selection formulas. Random selection is injected
//! through [`FlockRng`] so tests are deterministic; the server passes its
//! world RNG.

use std::collections::BTreeMap;
use std::path::Path;

use hytale_assets::AssetPack;
use serde_json::Value;
use thiserror::Error;

/// Random source used by flock size selection.
pub trait FlockRng {
    /// Uniform value in `[0, 1)`.
    fn next_f64(&mut self) -> f64;
}

/// Initial flock-size rule.
#[derive(Debug, Clone, PartialEq)]
pub enum FlockSize {
    /// Java `RangeSizeFlockAsset.Size` (inclusive range).
    Range {
        /// `[min, max]`.
        size: [i32; 2],
    },
    /// Java `WeightedSizeFlockAsset.MinSize` + `SizeWeights`.
    Weighted {
        /// Absolute minimum size.
        min_size: i32,
        /// Weight for `min_size`, `min_size + 1`, ...
        weights: Vec<f64>,
    },
}

impl FlockSize {
    /// Parse the `Size`/`Type` fields of a flock document (also used for the
    /// inline `Flock` object in spawn configurations).
    pub fn from_json(key: &str, raw: &Value) -> Result<Self, FlockError> {
        match raw.get("Type").and_then(Value::as_str) {
            Some("Weighted") => {
                let min_size = raw
                    .get("MinSize")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .ok_or_else(|| FlockError::InvalidField {
                        key: key.to_string(),
                        field: "MinSize",
                    })?;
                let weights = raw
                    .get("SizeWeights")
                    .and_then(Value::as_array)
                    .map(|weights| {
                        weights
                            .iter()
                            .filter_map(Value::as_f64)
                            .filter(|weight| *weight > 0.0)
                            .collect::<Vec<_>>()
                    })
                    .filter(|weights| !weights.is_empty())
                    .ok_or_else(|| FlockError::InvalidField {
                        key: key.to_string(),
                        field: "SizeWeights",
                    })?;
                Ok(FlockSize::Weighted { min_size, weights })
            }
            _ => {
                let size = raw
                    .get("Size")
                    .and_then(Value::as_array)
                    .filter(|size| size.len() == 2)
                    .map(|size| {
                        [
                            size[0].as_i64().unwrap_or(1) as i32,
                            size[1].as_i64().unwrap_or(1) as i32,
                        ]
                    })
                    .unwrap_or([1, 1]);
                Ok(FlockSize::Range { size })
            }
        }
    }

    /// Java `getMinFlockSize()`.
    pub fn min_size(&self) -> i32 {
        match self {
            FlockSize::Range { size } => size[0],
            FlockSize::Weighted { min_size, .. } => *min_size,
        }
    }
}

/// One flock definition.
#[derive(Debug, Clone, PartialEq)]
pub struct FlockAsset {
    /// Asset key (file stem).
    pub key: String,
    /// Maximum size the flock may grow to.
    pub max_grow_size: i32,
    /// Roles blocked from joining after the flock spawned.
    pub blocked_roles: Vec<String>,
    /// Initial size rule.
    pub size: FlockSize,
    /// The source document.
    pub raw: Value,
}

impl FlockAsset {
    /// Parse one flock document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Result<Self, FlockError> {
        let key = key.into();
        let size = FlockSize::from_json(&key, &raw)?;
        let max_grow_size = raw
            .get("MaxGrowSize")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(8);
        let blocked_roles = raw
            .get("BlockedRoles")
            .and_then(Value::as_array)
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            key,
            max_grow_size,
            blocked_roles,
            size,
            raw,
        })
    }

    /// Java `getMinFlockSize()`.
    pub fn min_size(&self) -> i32 {
        self.size.min_size()
    }

    /// Java `pickFlockSize()`.
    pub fn pick_size<R: FlockRng>(&self, rng: &mut R) -> i32 {
        match &self.size {
            FlockSize::Range { size } => {
                let low = size[0].max(1);
                let high = size[1].max(low);
                let span = i64::from(high) - i64::from(low) + 1;
                low + (rng.next_f64() * span as f64).floor() as i32
            }
            FlockSize::Weighted { min_size, weights } => {
                let index = pick_weighted_index(weights, rng);
                (*min_size).max(1) + index as i32
            }
        }
    }
}

fn pick_weighted_index<R: FlockRng>(weights: &[f64], rng: &mut R) -> usize {
    let sum: f64 = weights.iter().sum();
    if weights.is_empty() || sum <= 0.0 {
        return 0;
    }
    let mut remaining = rng.next_f64() * sum;
    for (index, weight) in weights.iter().enumerate().take(weights.len() - 1) {
        remaining -= weight;
        if remaining <= 0.0 {
            return index;
        }
    }
    weights.len() - 1
}

/// Persistent flock membership (`PersistentFlockData`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentFlockData {
    /// Java `MaxGrowSize`.
    pub max_grow_size: i32,
    /// Sorted allowed role keys after blocked roles are removed.
    pub allowed_roles: Vec<String>,
    /// Current member count.
    pub size: i32,
}

impl PersistentFlockData {
    /// Java `new PersistentFlockData(flockDefinition, allowedRoles)`.
    pub fn new(asset: Option<&FlockAsset>, allowed_roles: &[String]) -> Self {
        let mut allowed = allowed_roles.to_vec();
        allowed.sort();
        let Some(asset) = asset else {
            return Self {
                max_grow_size: i32::MAX,
                allowed_roles: allowed,
                size: 0,
            };
        };
        if !asset.blocked_roles.is_empty() && !allowed.is_empty() {
            allowed.retain(|role| !asset.blocked_roles.contains(role));
        }
        Self {
            max_grow_size: asset.max_grow_size,
            allowed_roles: allowed,
            size: 0,
        }
    }

    /// Java `isFlockAllowedRole` (binary search over the sorted array).
    pub fn is_allowed(&self, role: &str) -> bool {
        self.allowed_roles
            .binary_search_by(|candidate| candidate.as_str().cmp(role))
            .is_ok()
    }

    /// Java `increaseSize`.
    pub fn increase(&mut self) {
        self.size += 1;
    }

    /// Java `decreaseSize`.
    pub fn decrease(&mut self) {
        self.size -= 1;
    }
}

/// All flock definitions keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct FlockCatalog {
    assets: BTreeMap<String, FlockAsset>,
}

impl FlockCatalog {
    /// Load `Server/NPC/Flocks/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, FlockError> {
        let mut catalog = Self::default();
        for path in pack.list()? {
            if !path.starts_with("Server/NPC/Flocks/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| FlockError::InvalidPath(path.clone()))?
                .to_string();
            let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
            catalog
                .assets
                .insert(key.clone(), FlockAsset::from_json(key, raw)?);
        }
        Ok(catalog)
    }

    /// Load from an extracted asset root.
    pub fn load_root(root: &Path) -> Result<Self, FlockError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One flock by key.
    pub fn get(&self, key: &str) -> Option<&FlockAsset> {
        self.assets.get(key)
    }

    /// Number of flock assets.
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Whether no flock asset exists.
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Assets in key order.
    pub fn iter(&self) -> impl Iterator<Item = &FlockAsset> {
        self.assets.values()
    }
}

/// Flock loading errors.
#[derive(Debug, Error)]
pub enum FlockError {
    /// Asset pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A flock document is not valid JSON.
    #[error("flock document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A required field is missing or invalid.
    #[error("flock `{key}` has an invalid `{field}`")]
    InvalidField {
        /// Asset key.
        key: String,
        /// Field name.
        field: &'static str,
    },
    /// An asset path had no file name.
    #[error("invalid flock asset path `{0}`")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedRng(f64);

    impl FlockRng for FixedRng {
        fn next_f64(&mut self) -> f64 {
            self.0
        }
    }

    #[test]
    fn range_and_weighted_sizes_use_the_java_formulas() {
        let range = FlockAsset {
            key: "Range".to_string(),
            max_grow_size: 8,
            blocked_roles: Vec::new(),
            size: FlockSize::Range { size: [2, 3] },
            raw: Value::Null,
        };
        assert_eq!(range.pick_size(&mut FixedRng(0.0)), 2);
        assert_eq!(range.pick_size(&mut FixedRng(0.5)), 3);
        assert_eq!(range.pick_size(&mut FixedRng(0.999)), 3);

        let weighted = FlockAsset {
            key: "Weighted".to_string(),
            max_grow_size: 8,
            blocked_roles: Vec::new(),
            size: FlockSize::Weighted {
                min_size: 1,
                weights: vec![75.0, 25.0],
            },
            raw: Value::Null,
        };
        assert_eq!(weighted.pick_size(&mut FixedRng(0.74)), 1);
        assert_eq!(weighted.pick_size(&mut FixedRng(0.76)), 2);
    }

    #[test]
    fn blocked_roles_are_removed_from_the_sorted_allow_list() {
        let asset = FlockAsset {
            key: "Test".to_string(),
            max_grow_size: 4,
            blocked_roles: vec!["Wolf".to_string()],
            size: FlockSize::Range { size: [1, 1] },
            raw: Value::Null,
        };
        let data =
            PersistentFlockData::new(Some(&asset), &["Wolf".to_string(), "Bear".to_string()]);
        assert_eq!(data.allowed_roles, vec!["Bear".to_string()]);
        assert!(data.is_allowed("Bear"));
        assert!(!data.is_allowed("Wolf"));
        assert_eq!(data.max_grow_size, 4);
        let mut data = data;
        data.increase();
        data.decrease();
        assert_eq!(data.size, 0);
    }
}
