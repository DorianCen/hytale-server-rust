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
use serde_json::{Map, Value};

use crate::expr::{evaluate, EvalContext, ExprError, Value as ExprValue};
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

    /// Resolve one role's reference chain, parameters and `Compute`
    /// expressions. Random builtins fail with [`RoleError::MissingRng`].
    pub fn resolve(&self, key: &str) -> Result<Option<ResolvedRole>, RoleError> {
        self.resolve_inner(key, None)
    }

    /// Resolve one role with an RNG for `random`/`randomInRange`.
    pub fn resolve_with_rng(
        &self,
        key: &str,
        rng: &mut dyn FnMut(f64, f64) -> f64,
    ) -> Result<Option<ResolvedRole>, RoleError> {
        self.resolve_inner(key, Some(rng))
    }

    fn resolve_inner(
        &self,
        key: &str,
        mut rng: Option<&mut dyn FnMut(f64, f64) -> f64>,
    ) -> Result<Option<ResolvedRole>, RoleError> {
        let Some(role) = self.get(key) else {
            return Ok(None);
        };
        let mut chain = role.reference_chain(self);
        chain.reverse();
        chain.push(role);

        let mut parameter_entries: BTreeMap<String, Value> = BTreeMap::new();
        for role in &chain {
            for (name, entry) in &role.parameters {
                parameter_entries.insert(name.clone(), entry.clone());
            }
        }

        let mut vars: BTreeMap<String, ExprValue> = BTreeMap::new();
        let mut parameters: BTreeMap<String, Value> = BTreeMap::new();
        let mut pending: Vec<(String, Value)> = Vec::new();
        for (name, entry) in parameter_entries {
            let Some(value) = entry.get("Value") else {
                continue;
            };
            if contains_compute(value) {
                pending.push((name, value.clone()));
            } else {
                if let Some(expr_value) = ExprValue::from_json(value) {
                    vars.insert(name.clone(), expr_value);
                }
                parameters.insert(name, value.clone());
            }
        }
        // Computed parameters can depend on other computed parameters; repeat
        // until every expression resolves or only unknown-variable failures
        // remain.
        while !pending.is_empty() {
            let mut progressed = false;
            let mut remaining = Vec::new();
            for (name, value) in pending {
                let mut resolved = value.clone();
                match resolve_expressions(&mut resolved, &vars, &mut rng, &mut Vec::new(), true) {
                    Ok(()) => {
                        if let Some(expr_value) = ExprValue::from_json(&resolved) {
                            vars.insert(name.clone(), expr_value);
                        }
                        parameters.insert(name, resolved);
                        progressed = true;
                    }
                    Err(RoleError::Expression(ExprError::UnknownVariable(_))) => {
                        remaining.push((name, value));
                    }
                    Err(error) => return Err(error),
                }
            }
            if !progressed {
                let (name, _) = remaining.first().expect("non-empty pending list");
                return Err(RoleError::Expression(ExprError::UnknownVariable(
                    name.clone(),
                )));
            }
            pending = remaining;
        }

        let mut merged = Map::new();
        for role in &chain {
            if let Some(object) = role.raw.as_object() {
                for (name, value) in object {
                    merged.insert(name.clone(), value.clone());
                }
            }
        }
        let mut document = Value::Object(merged);
        let mut unresolved_expressions = Vec::new();
        resolve_expressions(
            &mut document,
            &vars,
            &mut rng,
            &mut unresolved_expressions,
            false,
        )?;
        if let Some(object) = document.as_object_mut() {
            // Java role variants override the inherited document through
            // `Modify`. The builder consumes the modified slots; this pass
            // flattens the keys so consumers can read `MaxHealth`,
            // `_InteractionVars`, etc. directly.
            if let Some(Value::Object(modify)) = object.get("Modify").cloned() {
                for (name, value) in modify {
                    object.insert(name, value);
                }
            }
            object.insert(
                "Parameters".to_string(),
                Value::Object(
                    parameters
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            );
        }

        Ok(Some(ResolvedRole {
            key: role.key.clone(),
            role_type: role.role_type.clone(),
            reference: role.reference.clone(),
            parameters,
            raw: document,
            unresolved_expressions,
        }))
    }
}

/// A role after reference-template merge and `Compute` expression evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRole {
    /// Asset key.
    pub key: String,
    /// Builder category.
    pub role_type: RoleType,
    /// `Reference` template name.
    pub reference: Option<String>,
    /// Literal parameter values after inheritance.
    pub parameters: BTreeMap<String, Value>,
    /// The inherited/resolved document.
    pub raw: Value,
    /// Nested `Compute` expressions left for a reference-scope builder pass
    /// (the `_Core/Tests/ReferenceSelector` development roles).
    pub unresolved_expressions: Vec<String>,
}

impl ResolvedRole {
    /// One resolved parameter.
    pub fn parameter(&self, name: &str) -> Option<&Value> {
        self.parameters.get(name)
    }

    /// Resolved model/appearance.
    pub fn appearance(&self) -> Option<&str> {
        self.raw.get("Appearance").and_then(Value::as_str)
    }

    /// Resolved max health.
    pub fn max_health(&self) -> Option<f64> {
        self.raw.get("MaxHealth").and_then(Value::as_f64)
    }

    /// Resolved `MotionControllerList`.
    pub fn motion_controllers(&self) -> &[Value] {
        self.raw
            .get("MotionControllerList")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Resolved view range (the builder parameter is the source when the
    /// document has no top-level `ViewRange` field).
    pub fn view_range(&self) -> Option<f64> {
        self.raw
            .get("ViewRange")
            .and_then(Value::as_f64)
            .or_else(|| self.parameter("ViewRange").and_then(Value::as_f64))
    }
}

fn contains_compute(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key("Compute") || object.values().any(contains_compute)
        }
        Value::Array(values) => values.iter().any(contains_compute),
        _ => false,
    }
}

fn resolve_expressions(
    value: &mut Value,
    vars: &BTreeMap<String, ExprValue>,
    rng: &mut Option<&mut dyn FnMut(f64, f64) -> f64>,
    unresolved: &mut Vec<String>,
    strict_unknown: bool,
) -> Result<(), RoleError> {
    match value {
        Value::Object(object) => {
            if let Some(expression) = object.get("Compute").and_then(Value::as_str) {
                let expression = expression.to_string();
                let mut context = match rng {
                    Some(rng) => EvalContext::with_rng(vars, *rng),
                    None => EvalContext::new(vars),
                };
                match evaluate(&expression, &mut context) {
                    Ok(resolved) => {
                        *value = resolved.to_json();
                    }
                    Err(ExprError::UnknownVariable(_)) if !strict_unknown => {
                        unresolved.push(expression);
                    }
                    Err(error) => return Err(RoleError::Expression(error)),
                }
                return Ok(());
            }
            for value in object.values_mut() {
                resolve_expressions(value, vars, rng, unresolved, strict_unknown)?;
            }
            Ok(())
        }
        Value::Array(values) => {
            for value in values {
                resolve_expressions(value, vars, rng, unresolved, strict_unknown)?;
            }
            Ok(())
        }
        _ => Ok(()),
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
    /// A `Compute` expression failed.
    #[error(transparent)]
    Expression(#[from] ExprError),
}
