//! Module manifest registry seam.
//!
//! This is the Rust-native counterpart of the Java builtin module registry: a
//! feature-gated module registers a manifest only when it is compiled in, and
//! the registry resolves the enabled set in dependency order. It deliberately
//! does not discover or classload code; `hytale-builtin` owns the lifecycle
//! hooks and selected module switches.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Metadata for one module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleManifest {
    /// Unique module name.
    pub name: String,
    /// Module version.
    pub version: String,
    /// Module names this module needs.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Whether the module is enabled when no explicit selection is made.
    #[serde(default)]
    pub default_enabled: bool,
}

impl ModuleManifest {
    /// Create a manifest with no dependencies, disabled by default.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            dependencies: Vec::new(),
            default_enabled: false,
        }
    }

    /// Add a dependency.
    pub fn with_dependency(mut self, dependency: impl Into<String>) -> Self {
        self.dependencies.push(dependency.into());
        self
    }

    /// Mark the module enabled by default.
    pub fn enabled_by_default(mut self) -> Self {
        self.default_enabled = true;
        self
    }
}

/// Module registry errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModuleError {
    /// A manifest has an empty name.
    #[error("module name must not be empty")]
    EmptyName,
    /// Two modules registered the same name.
    #[error("module `{name}` is already registered")]
    Duplicate {
        /// Duplicated name.
        name: String,
    },
    /// A manifest names itself as a dependency.
    #[error("module `{name}` depends on itself")]
    SelfDependency {
        /// Module name.
        name: String,
    },
    /// A requested/default module is not registered.
    #[error("module `{name}` is not registered")]
    UnknownModule {
        /// Missing name.
        name: String,
    },
    /// A dependency is not registered.
    #[error("module `{module}` depends on unregistered `{dependency}`")]
    UnknownDependency {
        /// Module that declared the edge.
        module: String,
        /// Missing dependency.
        dependency: String,
    },
    /// The dependency graph has a cycle.
    #[error("module dependency cycle: {chain}")]
    Cycle {
        /// Cycle path.
        chain: String,
    },
}

/// Manifest registry.
#[derive(Debug, Clone, Default)]
pub struct ModuleRegistry {
    manifests: BTreeMap<String, ModuleManifest>,
}

impl ModuleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a manifest. Names are exact (case-sensitive).
    pub fn register(&mut self, manifest: ModuleManifest) -> Result<(), ModuleError> {
        if manifest.name.trim().is_empty() {
            return Err(ModuleError::EmptyName);
        }
        if manifest
            .dependencies
            .iter()
            .any(|dependency| dependency == &manifest.name)
        {
            return Err(ModuleError::SelfDependency {
                name: manifest.name,
            });
        }
        if self.manifests.contains_key(&manifest.name) {
            return Err(ModuleError::Duplicate {
                name: manifest.name,
            });
        }
        self.manifests.insert(manifest.name.clone(), manifest);
        Ok(())
    }

    /// Look up one manifest.
    pub fn get(&self, name: &str) -> Option<&ModuleManifest> {
        self.manifests.get(name)
    }

    /// All manifests in name order.
    pub fn iter(&self) -> impl Iterator<Item = &ModuleManifest> {
        self.manifests.values()
    }

    /// Number of registered modules.
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Resolve the enabled set: the explicitly requested modules plus every
    /// `default_enabled` module, with dependencies first.
    ///
    /// The result is deterministic: dependencies and defaults are visited in
    /// name order.
    pub fn resolve_enabled(
        &self,
        requested: &[String],
    ) -> Result<Vec<&ModuleManifest>, ModuleError> {
        let mut roots: BTreeSet<String> = requested.iter().cloned().collect();
        for manifest in self.manifests.values() {
            if manifest.default_enabled {
                roots.insert(manifest.name.clone());
            }
        }

        for name in &roots {
            if !self.manifests.contains_key(name) {
                return Err(ModuleError::UnknownModule { name: name.clone() });
            }
        }

        let mut resolved = Vec::new();
        let mut done = BTreeSet::new();
        let mut visiting = Vec::new();
        for name in &roots {
            self.visit(name, &mut visiting, &mut done, &mut resolved)?;
        }
        Ok(resolved)
    }

    fn visit<'a>(
        &'a self,
        name: &str,
        visiting: &mut Vec<String>,
        done: &mut BTreeSet<String>,
        resolved: &mut Vec<&'a ModuleManifest>,
    ) -> Result<(), ModuleError> {
        if done.contains(name) {
            return Ok(());
        }
        if let Some(position) = visiting.iter().position(|candidate| candidate == name) {
            let mut chain = visiting[position..].to_vec();
            chain.push(name.to_string());
            return Err(ModuleError::Cycle {
                chain: chain.join(" -> "),
            });
        }
        let Some(manifest) = self.manifests.get(name) else {
            return Err(ModuleError::UnknownModule {
                name: name.to_string(),
            });
        };

        visiting.push(name.to_string());
        let mut dependencies = manifest.dependencies.clone();
        dependencies.sort();
        for dependency in dependencies {
            if !self.manifests.contains_key(&dependency) {
                return Err(ModuleError::UnknownDependency {
                    module: name.to_string(),
                    dependency,
                });
            }
            self.visit(&dependency, visiting, done, resolved)?;
        }
        visiting.pop();
        done.insert(name.to_string());
        resolved.push(manifest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str) -> ModuleManifest {
        ModuleManifest::new(name, "0.1.0")
    }

    #[test]
    fn duplicate_and_invalid_manifests_are_rejected() {
        let mut registry = ModuleRegistry::new();
        registry.register(manifest("core")).unwrap();
        assert_eq!(
            registry.register(manifest("core")).unwrap_err(),
            ModuleError::Duplicate {
                name: "core".to_string()
            }
        );
        assert_eq!(
            registry
                .register(ModuleManifest::new("", "0.1.0"))
                .unwrap_err(),
            ModuleError::EmptyName
        );
        assert_eq!(
            registry
                .register(manifest("self").with_dependency("self"))
                .unwrap_err(),
            ModuleError::SelfDependency {
                name: "self".to_string()
            }
        );
    }

    #[test]
    fn resolve_orders_dependencies_first_and_includes_defaults() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(manifest("base").enabled_by_default())
            .unwrap();
        registry
            .register(manifest("world").with_dependency("base"))
            .unwrap();
        registry
            .register(
                manifest("npc")
                    .with_dependency("world")
                    .with_dependency("base"),
            )
            .unwrap();
        registry.register(manifest("unused")).unwrap();

        let names: Vec<&str> = registry
            .resolve_enabled(&["npc".to_string()])
            .unwrap()
            .iter()
            .map(|manifest| manifest.name.as_str())
            .collect();
        assert_eq!(names, vec!["base", "world", "npc"]);

        let defaults: Vec<&str> = registry
            .resolve_enabled(&[])
            .unwrap()
            .iter()
            .map(|manifest| manifest.name.as_str())
            .collect();
        assert_eq!(defaults, vec!["base"]);
    }

    #[test]
    fn unknown_modules_and_dependencies_are_rejected() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(manifest("world").with_dependency("missing"))
            .unwrap();
        assert_eq!(
            registry
                .resolve_enabled(&["world".to_string()])
                .unwrap_err(),
            ModuleError::UnknownDependency {
                module: "world".to_string(),
                dependency: "missing".to_string()
            }
        );
        assert_eq!(
            registry
                .resolve_enabled(&["absent".to_string()])
                .unwrap_err(),
            ModuleError::UnknownModule {
                name: "absent".to_string()
            }
        );
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(manifest("a").with_dependency("b"))
            .unwrap();
        registry
            .register(manifest("b").with_dependency("a"))
            .unwrap();
        let error = registry.resolve_enabled(&["a".to_string()]).unwrap_err();
        assert!(matches!(error, ModuleError::Cycle { .. }));
        assert!(error.to_string().contains("a -> b -> a"));
    }
}
