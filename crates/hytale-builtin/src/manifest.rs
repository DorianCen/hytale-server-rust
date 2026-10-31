//! Java-compatible builtin module manifests.
//!
//! The field names and semantics mirror `PluginManifest` in the reference
//! server: `Group`, `Name`, `Version`, `Main`, `ServerVersion`, the dependency
//! maps and the disabled-by-default flag. Manifests are data; the lifecycle
//! lives in [`crate::lifecycle`].

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One `Group:Name` plugin identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PluginIdentifier {
    group: String,
    name: String,
}

impl PluginIdentifier {
    /// Build an identifier from a group and a name.
    pub fn new(group: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            group: group.into(),
            name: name.into(),
        }
    }

    /// Group part.
    pub fn group(&self) -> &str {
        &self.group
    }

    /// Name part.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl FromStr for PluginIdentifier {
    type Err = ManifestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (group, name) = value
            .split_once(':')
            .ok_or(ManifestError::InvalidIdentifier {
                value: value.to_string(),
            })?;
        if group.is_empty() || name.is_empty() || name.contains(':') {
            return Err(ManifestError::InvalidIdentifier {
                value: value.to_string(),
            });
        }
        Ok(Self::new(group, name))
    }
}

impl fmt::Display for PluginIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.group, self.name)
    }
}

/// One author entry (`AuthorInfo` in Java).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorInfo {
    /// Display name.
    #[serde(rename = "Name", default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Contact email.
    #[serde(rename = "Email", default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Website URL.
    #[serde(rename = "Url", default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A builtin/plugin manifest.
///
/// Everything except the name is optional because Java sub-plugin manifests
/// only carry `Name` and `Main`. Unknown fields are rejected so a manifest
/// typo cannot silently change module selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Plugin group.
    #[serde(rename = "Group", default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// Plugin name.
    #[serde(rename = "Name")]
    pub name: String,
    /// Plugin version.
    #[serde(rename = "Version", default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Human-readable description.
    #[serde(
        rename = "Description",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    /// Author list.
    #[serde(rename = "Authors", default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<AuthorInfo>,
    /// Website URL.
    #[serde(rename = "Website", default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// Entry-point class/type.
    #[serde(rename = "Main", default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// Compatible server version range (`*` for the vanilla builtins).
    #[serde(
        rename = "ServerVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub server_version: Option<String>,
    /// Required dependencies (`Group:Name` → version range).
    #[serde(
        rename = "Dependencies",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub dependencies: BTreeMap<String, String>,
    /// Optional dependencies (`Group:Name` → version range).
    #[serde(
        rename = "OptionalDependencies",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub optional_dependencies: BTreeMap<String, String>,
    /// Modules that must load after this one.
    #[serde(
        rename = "LoadBefore",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub load_before: BTreeMap<String, String>,
    /// Disabled unless explicitly requested.
    #[serde(
        rename = "DisabledByDefault",
        default,
        skip_serializing_if = "is_false"
    )]
    pub disabled_by_default: bool,
    /// Whether the module carries an asset pack.
    #[serde(
        rename = "IncludesAssetPack",
        default,
        skip_serializing_if = "is_false"
    )]
    pub includes_asset_pack: bool,
    /// Nested sub-plugin manifests (Java `SubPlugins`).
    #[serde(rename = "SubPlugins", default, skip_serializing_if = "Vec::is_empty")]
    pub sub_plugins: Vec<PluginManifest>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl PluginManifest {
    /// Build a vanilla-style manifest with wildcard server compatibility and
    /// no optional fields.
    pub fn new(
        group: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        main: impl Into<String>,
    ) -> Self {
        Self {
            group: group.into(),
            name: name.into(),
            version: version.into(),
            main: Some(main.into()),
            server_version: Some("*".to_string()),
            ..Self::default()
        }
    }

    /// Add a required dependency.
    pub fn with_dependency(mut self, identifier: impl Into<String>) -> Self {
        self.dependencies.insert(identifier.into(), "*".to_string());
        self
    }

    /// Add an optional dependency.
    pub fn with_optional_dependency(mut self, identifier: impl Into<String>) -> Self {
        self.optional_dependencies
            .insert(identifier.into(), "*".to_string());
        self
    }

    /// Add a sub-plugin.
    pub fn with_sub_plugin(mut self, sub_plugin: PluginManifest) -> Self {
        self.sub_plugins.push(sub_plugin);
        self
    }

    /// Add a description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark the module disabled unless explicitly requested.
    pub fn disabled_by_default(mut self) -> Self {
        self.disabled_by_default = true;
        self
    }

    /// `Group:Name` identifier.
    pub fn identifier(&self) -> PluginIdentifier {
        PluginIdentifier::new(&self.group, &self.name)
    }

    /// Validate the fields the runtime relies on.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.group.trim().is_empty() {
            return Err(ManifestError::MissingField {
                name: self.name.clone(),
                field: "Group",
            });
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::MissingField {
                name: "<unnamed>".to_string(),
                field: "Name",
            });
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::MissingField {
                name: self.identifier().to_string(),
                field: "Version",
            });
        }
        if self.main.as_deref().is_none_or(str::is_empty) {
            return Err(ManifestError::MissingField {
                name: self.identifier().to_string(),
                field: "Main",
            });
        }
        for dependency in self
            .dependencies
            .keys()
            .chain(self.optional_dependencies.keys())
        {
            PluginIdentifier::from_str(dependency)?;
        }
        Ok(())
    }

    /// Parse one manifest from JSON.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        let manifest: Self = serde_json::from_str(json).map_err(|error| ManifestError::Json {
            name: "<json>".to_string(),
            message: error.to_string(),
        })?;
        manifest.validate()?;
        Ok(manifest)
    }
}

/// Manifest parse/validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    /// A manifest identifier is not `Group:Name`.
    #[error("invalid plugin identifier `{value}` (expected `Group:Name`)")]
    InvalidIdentifier {
        /// Rejected value.
        value: String,
    },
    /// A required manifest field is missing or empty.
    #[error("manifest `{name}` is missing `{field}`")]
    MissingField {
        /// Manifest name.
        name: String,
        /// Field name.
        field: &'static str,
    },
    /// The JSON document could not be parsed.
    #[error("manifest `{name}` is invalid JSON: {message}")]
    Json {
        /// Manifest name (or `<json>`).
        name: String,
        /// Parser message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip_and_reject_malformed_values() {
        let id: PluginIdentifier = "Hytale:Fluid".parse().unwrap();
        assert_eq!(id.group(), "Hytale");
        assert_eq!(id.name(), "Fluid");
        assert_eq!(id.to_string(), "Hytale:Fluid");
        assert!("Hytale".parse::<PluginIdentifier>().is_err());
        assert!("Hytale:".parse::<PluginIdentifier>().is_err());
        assert!("Hytale:Fluid:Extra".parse::<PluginIdentifier>().is_err());
    }

    #[test]
    fn java_manifest_fields_parse_and_validate() {
        let manifest = PluginManifest::from_json(
            r#"{
                "Group": "Hytale",
                "Name": "Fluid",
                "Version": "1.0.0",
                "Website": "https://hypixel.com/",
                "Main": "com.hypixel.hytale.builtin.fluid.FluidPlugin",
                "ServerVersion": "*",
                "Dependencies": {
                    "Hytale:BlockTick": "*",
                    "Hytale:LegacyModule": "*"
                }
            }"#,
        )
        .unwrap();
        assert_eq!(manifest.identifier().to_string(), "Hytale:Fluid");
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.server_version.as_deref(), Some("*"));
        assert!(!manifest.disabled_by_default);
    }

    #[test]
    fn invalid_manifests_are_rejected() {
        assert!(PluginManifest::from_json(r#"{"Group":"Hytale","Name":"X"}"#).is_err());
        assert!(PluginManifest::from_json(
            r#"{"Group":"Hytale","Name":"X","Version":"1","Main":"M","Dependencies":{"Invalid":"*"}}"#
        )
        .is_err());
        assert!(PluginManifest::from_json(
            r#"{"Group":"Hytale","Name":"X","Version":"1","Main":"M","Unknown":true}"#
        )
        .is_err());
    }

    #[test]
    fn sub_plugins_may_be_partial_manifests() {
        let manifest = PluginManifest::from_json(
            r#"{
                "Group": "Hytale",
                "Name": "NPC",
                "Version": "1.0.0",
                "Main": "com.hypixel.hytale.server.npc.NPCPlugin",
                "ServerVersion": "*",
                "SubPlugins": [
                    {"Name": "Spawning", "Main": "com.hypixel.hytale.server.spawning.SpawningPlugin"}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(manifest.sub_plugins.len(), 1);
        assert_eq!(manifest.sub_plugins[0].name, "Spawning");
    }
}
