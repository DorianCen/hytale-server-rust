//! Builtin module lifecycle.
//!
//! The ordering rules mirror the Java plugin loader: dependencies start first,
//! defaults are opt-out, explicitly requested modules are opt-in, and shutdown
//! runs in reverse order. Module state follows the Java `PluginState` names so
//! logs and diagnostics stay familiar.
//!
//! This is deliberately a Rust-native lifecycle. It does not discover classes
//! or load code; a module is a compiled-in `BuiltinModule` implementation.

use std::collections::{BTreeMap, BTreeSet};

use hytale_core::{ModuleError, ModuleManifest, ModuleRegistry};
use serde::Serialize;
use thiserror::Error;

use crate::manifest::{ManifestError, PluginManifest};
use crate::modules::compiled_modules;
use crate::runtime::{BuiltinRuntime, ModuleKind};

/// Lifecycle state of one module (`PluginState` in Java).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModuleState {
    /// Not started yet.
    None,
    /// `setup` is running.
    Setup,
    /// `start` is running.
    Start,
    /// Started successfully.
    Enabled,
    /// Shutting down.
    Shutdown,
    /// Stopped or never enabled.
    Disabled,
    /// A lifecycle hook failed.
    Failed,
}

impl ModuleState {
    /// Java `PluginState.isInactive` equivalent.
    pub fn is_inactive(self) -> bool {
        matches!(self, Self::None | Self::Disabled | Self::Failed)
    }

    /// Stable lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Setup => "setup",
            Self::Start => "start",
            Self::Enabled => "enabled",
            Self::Shutdown => "shutdown",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        }
    }
}

/// Context passed to lifecycle hooks.
pub struct ModuleContext<'a> {
    /// Runtime switches updated by the module.
    pub runtime: &'a mut BuiltinRuntime,
}

impl<'a> ModuleContext<'a> {
    /// Build a context around the host runtime.
    pub fn new(runtime: &'a mut BuiltinRuntime) -> Self {
        Self { runtime }
    }

    /// Enable one module switch.
    pub fn enable(&mut self, kind: ModuleKind) {
        self.runtime.enable(kind);
    }

    /// Disable one module switch.
    pub fn disable(&mut self, kind: ModuleKind) {
        self.runtime.disable(kind);
    }
}

/// One compiled-in builtin module.
pub trait BuiltinModule: Send {
    /// Java-compatible manifest.
    fn manifest(&self) -> &PluginManifest;

    /// Called once before `start`.
    fn setup(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }

    /// Called once in dependency order.
    fn start(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }

    /// Called once in reverse dependency order.
    fn shutdown(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }

    /// Whether the module is currently active.
    fn is_started(&self) -> bool {
        false
    }
}

/// Module selection for one host start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartOptions {
    /// Modules to enable in addition to the manifest defaults.
    pub requested: Vec<String>,
    /// Modules to disable. An enabled module that depends on one fails to
    /// start instead of silently running without a dependency.
    pub disabled: Vec<String>,
}

/// One started module in the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedModule {
    /// `Group:Name`.
    pub id: String,
    /// Exact manifest.
    pub manifest: PluginManifest,
    /// Final state (`Enabled` on success).
    pub state: ModuleState,
}

/// Result of one host start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartReport {
    /// Enabled modules in dependency-first order.
    pub enabled: Vec<StartedModule>,
    /// Compiled modules that were not enabled.
    pub disabled: Vec<String>,
}

impl StartReport {
    /// Enabled identifiers in start order.
    pub fn ids(&self) -> Vec<&str> {
        self.enabled
            .iter()
            .map(|module| module.id.as_str())
            .collect()
    }
}

/// Result of one host shutdown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Modules stopped, in shutdown order.
    pub stopped: Vec<String>,
    /// Shutdown hook failures.
    pub failed: Vec<(String, String)>,
}

/// Host errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HostError {
    /// A manifest is invalid.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// Two modules registered the same identifier.
    #[error("module `{0}` is already registered")]
    Duplicate(String),
    /// A requested/default module has no compiled implementation.
    #[error("module `{0}` is not compiled into this binary")]
    UnknownModule(String),
    /// Requested and disabled at the same time.
    #[error("module `{module}` was both requested and disabled")]
    ConflictingSelection {
        /// Selected module.
        module: String,
    },
    /// A module depends on a module disabled for this start.
    #[error("module `{module}` depends on disabled module `{dependency}`")]
    DisabledDependency {
        /// Module that declared the edge.
        module: String,
        /// Disabled dependency.
        dependency: String,
    },
    /// `setup` failed.
    #[error("module `{module}` failed to setup: {message}")]
    SetupFailed {
        /// Module identifier.
        module: String,
        /// Hook error.
        message: String,
    },
    /// `start` failed.
    #[error("module `{module}` failed to start: {message}")]
    StartFailed {
        /// Module identifier.
        module: String,
        /// Hook error.
        message: String,
    },
    /// Start was called twice.
    #[error("the builtin host is already started")]
    AlreadyStarted,
    /// Dependency resolution failed (cycles/unknown dependencies).
    #[error(transparent)]
    Registry(#[from] ModuleError),
}

/// Compiled builtin module host.
pub struct BuiltinHost {
    modules: BTreeMap<String, Box<dyn BuiltinModule>>,
    states: BTreeMap<String, ModuleState>,
    runtime: BuiltinRuntime,
    running: Vec<String>,
    started: bool,
}

impl std::fmt::Debug for BuiltinHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuiltinHost")
            .field("modules", &self.modules.keys().collect::<Vec<_>>())
            .field("states", &self.states)
            .field("runtime", &self.runtime)
            .field("started", &self.started)
            .finish()
    }
}

impl BuiltinHost {
    /// Host containing every module compiled into this binary.
    pub fn compiled() -> Result<Self, HostError> {
        Self::with_modules(compiled_modules())
    }

    /// Host over an explicit module list (test/embedding seam).
    pub fn with_modules(modules: Vec<Box<dyn BuiltinModule>>) -> Result<Self, HostError> {
        let mut by_id = BTreeMap::new();
        let mut states = BTreeMap::new();
        for module in modules {
            module.manifest().validate()?;
            let id = module.manifest().identifier().to_string();
            if by_id.contains_key(&id) {
                return Err(HostError::Duplicate(id));
            }
            states.insert(id.clone(), ModuleState::None);
            by_id.insert(id, module);
        }
        Ok(Self {
            modules: by_id,
            states,
            runtime: BuiltinRuntime::none(),
            running: Vec::new(),
            started: false,
        })
    }

    /// All compiled manifests, sorted by identifier.
    pub fn manifests(&self) -> Vec<PluginManifest> {
        self.modules
            .values()
            .map(|module| module.manifest().clone())
            .collect()
    }

    /// One module's manifest.
    pub fn manifest(&self, id: &str) -> Option<&PluginManifest> {
        self.modules.get(id).map(|module| module.manifest())
    }

    /// All compiled identifiers, sorted.
    pub fn module_ids(&self) -> Vec<&str> {
        self.modules.keys().map(String::as_str).collect()
    }

    /// Current state of one module.
    pub fn state(&self, id: &str) -> Option<ModuleState> {
        self.states.get(id).copied()
    }

    /// Current runtime switches.
    pub fn runtime(&self) -> BuiltinRuntime {
        self.runtime
    }

    /// Whether `start` completed successfully.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// Resolve, set up and start the selected module set.
    pub fn start(&mut self, options: &StartOptions) -> Result<StartReport, HostError> {
        if self.started {
            return Err(HostError::AlreadyStarted);
        }

        let disabled: BTreeSet<String> = options.disabled.iter().cloned().collect();
        let requested: BTreeSet<String> = options.requested.iter().cloned().collect();
        for name in requested.iter().chain(disabled.iter()) {
            if !self.modules.contains_key(name) {
                return Err(HostError::UnknownModule(name.clone()));
            }
        }
        if let Some(name) = requested.intersection(&disabled).next() {
            return Err(HostError::ConflictingSelection {
                module: name.clone(),
            });
        }

        // Defaults are opt-out; requested modules are opt-in.
        let mut enabled: BTreeSet<String> = self
            .modules
            .iter()
            .filter(|(id, module)| {
                !module.manifest().disabled_by_default && !disabled.contains(*id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for name in &requested {
            enabled.insert(name.clone());
        }

        // Close over compiled dependencies. Dependencies outside the compiled
        // set are provided by the server core and are intentionally ignored.
        let mut worklist: Vec<String> = enabled.iter().cloned().collect();
        while let Some(id) = worklist.pop() {
            let module = self
                .modules
                .get(&id)
                .expect("enabled ids come from the module map");
            for dependency in module.manifest().dependencies.keys() {
                if disabled.contains(dependency) {
                    return Err(HostError::DisabledDependency {
                        module: id.clone(),
                        dependency: dependency.clone(),
                    });
                }
                if self.modules.contains_key(dependency) && enabled.insert(dependency.clone()) {
                    worklist.push(dependency.clone());
                }
            }
        }

        let order = self.resolve_order(&enabled)?;
        let start_report = self.run_lifecycle(&order, &disabled)?;
        self.started = true;
        Ok(start_report)
    }

    fn resolve_order(&self, enabled: &BTreeSet<String>) -> Result<Vec<String>, HostError> {
        let mut registry = ModuleRegistry::new();
        for id in enabled {
            let manifest = self
                .modules
                .get(id)
                .expect("enabled ids come from the module map")
                .manifest();
            let mut module = ModuleManifest::new(id.clone(), manifest.version.clone());
            for dependency in manifest.dependencies.keys() {
                if enabled.contains(dependency) {
                    module = module.with_dependency(dependency.clone());
                }
            }
            registry.register(module)?;
        }
        let roots: Vec<String> = enabled.iter().cloned().collect();
        let resolved = registry.resolve_enabled(&roots)?;
        Ok(resolved
            .into_iter()
            .map(|manifest| manifest.name.clone())
            .collect())
    }

    fn run_lifecycle(
        &mut self,
        order: &[String],
        disabled: &BTreeSet<String>,
    ) -> Result<StartReport, HostError> {
        let Self {
            modules,
            states,
            running,
            ..
        } = self;
        let mut runtime = BuiltinRuntime::none();

        for id in order {
            states.insert(id.clone(), ModuleState::Setup);
            let module = modules
                .get_mut(id)
                .expect("order comes from the module map");
            let mut context = ModuleContext::new(&mut runtime);
            if let Err(message) = module.setup(&mut context) {
                states.insert(id.clone(), ModuleState::Failed);
                shutdown_running(modules, states, running, &mut runtime);
                return Err(HostError::SetupFailed {
                    module: id.clone(),
                    message,
                });
            }

            states.insert(id.clone(), ModuleState::Start);
            if let Err(message) = module.start(&mut context) {
                states.insert(id.clone(), ModuleState::Failed);
                shutdown_running(modules, states, running, &mut runtime);
                return Err(HostError::StartFailed {
                    module: id.clone(),
                    message,
                });
            }
            states.insert(id.clone(), ModuleState::Enabled);
            running.push(id.clone());
        }
        self.runtime = runtime;

        for id in modules.keys() {
            if !order.contains(id) {
                states.insert(id.clone(), ModuleState::Disabled);
            }
        }

        let enabled: Vec<StartedModule> = order
            .iter()
            .map(|id| StartedModule {
                id: id.clone(),
                manifest: modules
                    .get(id)
                    .expect("order comes from the module map")
                    .manifest()
                    .clone(),
                state: ModuleState::Enabled,
            })
            .collect();
        let disabled: Vec<String> = modules
            .keys()
            .filter(|id| !order.contains(id) || disabled.contains(*id))
            .cloned()
            .collect();
        Ok(StartReport { enabled, disabled })
    }

    /// Stop every running module in reverse order.
    pub fn shutdown(&mut self) -> ShutdownReport {
        let Self {
            modules,
            states,
            runtime,
            running,
            started,
        } = self;
        let mut report = ShutdownReport::default();
        if !*started && running.is_empty() {
            return report;
        }
        for id in running.iter().rev() {
            if let Some(module) = modules.get_mut(id) {
                states.insert(id.clone(), ModuleState::Shutdown);
                let mut context = ModuleContext::new(runtime);
                match module.shutdown(&mut context) {
                    Ok(()) => {
                        states.insert(id.clone(), ModuleState::Disabled);
                        report.stopped.push(id.clone());
                    }
                    Err(message) => {
                        states.insert(id.clone(), ModuleState::Failed);
                        report.failed.push((id.clone(), message));
                    }
                }
            }
        }
        running.clear();
        *runtime = BuiltinRuntime::none();
        *started = false;
        report
    }
}

fn shutdown_running(
    modules: &mut BTreeMap<String, Box<dyn BuiltinModule>>,
    states: &mut BTreeMap<String, ModuleState>,
    running: &mut Vec<String>,
    runtime: &mut BuiltinRuntime,
) {
    for id in running.iter().rev() {
        if let Some(module) = modules.get_mut(id) {
            states.insert(id.clone(), ModuleState::Shutdown);
            let mut context = ModuleContext::new(runtime);
            let result = module.shutdown(&mut context);
            states.insert(
                id.clone(),
                if result.is_ok() {
                    ModuleState::Disabled
                } else {
                    ModuleState::Failed
                },
            );
        }
    }
    running.clear();
    *runtime = BuiltinRuntime::none();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeModule {
        manifest: PluginManifest,
        fail_setup: bool,
        fail_start: bool,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl FakeModule {
        fn new(
            id: &str,
            dependencies: &[&str],
            log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        ) -> Self {
            let (group, name) = id.split_once(':').unwrap();
            let mut manifest = PluginManifest::new(group, name, "1.0.0", "Example");
            for dependency in dependencies {
                manifest = manifest.with_dependency(*dependency);
            }
            Self {
                manifest,
                fail_setup: false,
                fail_start: false,
                log,
            }
        }

        fn failing(mut self, setup: bool, start: bool) -> Self {
            self.fail_setup = setup;
            self.fail_start = start;
            self
        }

        fn log(&self, event: &str) {
            self.log.lock().unwrap().push(event.to_string());
        }

        fn kind(&self) -> ModuleKind {
            match self.manifest.name.as_str() {
                "BlockTick" => ModuleKind::BlockTick,
                "RandomTick" => ModuleKind::RandomTick,
                "FallingBlocks" => ModuleKind::FallingBlocks,
                "BlockPhysics" => ModuleKind::BlockPhysics,
                "Fluid" => ModuleKind::Fluid,
                "Weather" => ModuleKind::Weather,
                "NPC" => ModuleKind::Npc,
                "EncounterManager" => ModuleKind::EncounterManager,
                _ => unreachable!(),
            }
        }
    }

    impl BuiltinModule for FakeModule {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        fn setup(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
            if self.fail_setup {
                return Err("setup failed".to_string());
            }
            Ok(())
        }

        fn start(&mut self, context: &mut ModuleContext<'_>) -> Result<(), String> {
            if self.fail_start {
                return Err("start failed".to_string());
            }
            self.log("start");
            context.enable(self.kind());
            Ok(())
        }

        fn shutdown(&mut self, context: &mut ModuleContext<'_>) -> Result<(), String> {
            self.log("shutdown");
            context.disable(self.kind());
            Ok(())
        }
    }

    fn log() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
    }

    #[test]
    fn dependencies_start_first_and_shutdown_in_reverse() {
        let log = log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![
            Box::new(FakeModule::new(
                "Hytale:BlockPhysics",
                &["Hytale:BlockTick", "Hytale:FallingBlocks"],
                log.clone(),
            )),
            Box::new(FakeModule::new("Hytale:BlockTick", &[], log.clone())),
            Box::new(FakeModule::new("Hytale:FallingBlocks", &[], log.clone())),
        ];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        let report = host.start(&StartOptions::default()).unwrap();
        assert_eq!(
            report.ids(),
            vec![
                "Hytale:BlockTick",
                "Hytale:FallingBlocks",
                "Hytale:BlockPhysics"
            ]
        );
        assert_eq!(*log.lock().unwrap(), vec!["start"; 3]);
        assert!(host.runtime().block_tick);
        assert!(host.runtime().block_physics);

        let stopped = host.shutdown();
        assert_eq!(stopped.stopped.len(), 3);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["start", "start", "start", "shutdown", "shutdown", "shutdown"]
        );
        assert_eq!(host.runtime(), BuiltinRuntime::none());
        assert_eq!(
            host.state("Hytale:BlockPhysics"),
            Some(ModuleState::Disabled)
        );
    }

    #[test]
    fn disabled_dependency_is_reported_and_disabled_leaf_is_skipped() {
        let log = log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![
            Box::new(FakeModule::new("Hytale:BlockTick", &[], log.clone())),
            Box::new(FakeModule::new(
                "Hytale:Fluid",
                &["Hytale:BlockTick"],
                log.clone(),
            )),
            Box::new(FakeModule::new("Hytale:RandomTick", &[], log.clone())),
        ];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        let error = host
            .start(&StartOptions {
                requested: Vec::new(),
                disabled: vec!["Hytale:BlockTick".to_string()],
            })
            .unwrap_err();
        assert_eq!(
            error,
            HostError::DisabledDependency {
                module: "Hytale:Fluid".to_string(),
                dependency: "Hytale:BlockTick".to_string(),
            }
        );
        assert!(!host.is_started());
        assert_eq!(host.runtime(), BuiltinRuntime::none());

        let report = host
            .start(&StartOptions {
                requested: vec!["Hytale:BlockTick".to_string()],
                disabled: vec!["Hytale:RandomTick".to_string()],
            })
            .unwrap();
        assert_eq!(report.ids(), vec!["Hytale:BlockTick", "Hytale:Fluid"]);
        assert_eq!(host.state("Hytale:RandomTick"), Some(ModuleState::Disabled));
        assert!(host.runtime().block_tick);
        assert!(host.runtime().fluid);
        assert!(!host.runtime().random_tick);
    }

    #[test]
    fn setup_and_start_failures_roll_back_the_running_set() {
        let log = log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![
            Box::new(FakeModule::new("Hytale:BlockTick", &[], log.clone())),
            Box::new(
                FakeModule::new("Hytale:Fluid", &["Hytale:BlockTick"], log.clone())
                    .failing(false, true),
            ),
        ];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        let error = host.start(&StartOptions::default()).unwrap_err();
        assert!(matches!(error, HostError::StartFailed { .. }));
        assert_eq!(host.state("Hytale:BlockTick"), Some(ModuleState::Disabled));
        assert_eq!(host.state("Hytale:Fluid"), Some(ModuleState::Failed));
        assert_eq!(host.runtime(), BuiltinRuntime::none());
        assert!(!host.is_started());

        let setup_log = self::log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![Box::new(
            FakeModule::new("Hytale:BlockTick", &[], setup_log).failing(true, false),
        )];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        assert!(matches!(
            host.start(&StartOptions::default()),
            Err(HostError::SetupFailed { .. })
        ));
        assert_eq!(host.state("Hytale:BlockTick"), Some(ModuleState::Failed));
    }

    #[test]
    fn duplicate_unknown_and_conflicting_selection_are_rejected() {
        let log = log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![
            Box::new(FakeModule::new("Hytale:BlockTick", &[], log.clone())),
            Box::new(FakeModule::new("Hytale:BlockTick", &[], log.clone())),
        ];
        assert!(matches!(
            BuiltinHost::with_modules(modules),
            Err(HostError::Duplicate(_))
        ));

        let modules: Vec<Box<dyn BuiltinModule>> =
            vec![Box::new(FakeModule::new("Hytale:BlockTick", &[], log))];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        assert!(matches!(
            host.start(&StartOptions {
                requested: vec!["Hytale:Missing".to_string()],
                disabled: Vec::new(),
            }),
            Err(HostError::UnknownModule(_))
        ));
        assert!(matches!(
            host.start(&StartOptions {
                requested: vec!["Hytale:BlockTick".to_string()],
                disabled: vec!["Hytale:BlockTick".to_string()],
            }),
            Err(HostError::ConflictingSelection { .. })
        ));
    }

    #[test]
    fn cycles_are_rejected() {
        let log = log();
        let modules: Vec<Box<dyn BuiltinModule>> = vec![
            Box::new(FakeModule::new(
                "Hytale:BlockTick",
                &["Hytale:Fluid"],
                log.clone(),
            )),
            Box::new(FakeModule::new(
                "Hytale:Fluid",
                &["Hytale:BlockTick"],
                log.clone(),
            )),
        ];
        let mut host = BuiltinHost::with_modules(modules).unwrap();
        let error = host.start(&StartOptions::default()).unwrap_err();
        assert!(matches!(
            error,
            HostError::Registry(ModuleError::Cycle { .. })
        ));
    }

    #[test]
    fn manifest_validation_runs_before_start() {
        let manifest = PluginManifest {
            group: String::new(),
            name: "X".to_string(),
            ..PluginManifest::default()
        };
        let module = FakeModule {
            manifest,
            fail_setup: false,
            fail_start: false,
            log: log(),
        };
        assert!(matches!(
            BuiltinHost::with_modules(vec![Box::new(module)]),
            Err(HostError::Manifest(ManifestError::MissingField {
                field: "Group",
                ..
            }))
        ));
    }
}
