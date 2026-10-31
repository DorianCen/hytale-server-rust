//! T60 parity gate for the selected builtin modules.
//!
//! The golden file is the reference-server manifest set. Each compiled module
//! must match its Java manifest exactly, start in dependency order and expose a
//! runtime switch. Modules compiled out must not appear at all.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hytale_builtin::{
    compiled_manifests, feature_compiled, BuiltinHost, BuiltinRuntime, HostError, ModuleKind,
    ModuleState, PluginManifest, StartOptions,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden() -> Vec<PluginManifest> {
    let path = repo_root().join("testdata/golden/builtin/manifests.json");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let manifests: Vec<PluginManifest> = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    for manifest in &manifests {
        manifest
            .validate()
            .unwrap_or_else(|error| panic!("invalid golden manifest: {error}"));
    }
    manifests
}

fn golden_for(kind: ModuleKind) -> PluginManifest {
    let id = kind.identifier();
    golden()
        .into_iter()
        .find(|manifest| manifest.identifier() == id)
        .unwrap_or_else(|| panic!("golden is missing {id}"))
}

fn assert_module_parity(kind: ModuleKind) {
    let manifests = compiled_manifests();
    let id = kind.identifier();
    let compiled = manifests
        .iter()
        .find(|manifest| manifest.identifier() == id);
    if feature_compiled(kind) {
        let manifest = compiled.unwrap_or_else(|| panic!("{id} is compiled but missing"));
        assert_eq!(manifest, &golden_for(kind), "{id} manifest drift");
    } else {
        assert!(
            compiled.is_none(),
            "{id} is not compiled but was registered"
        );
    }
}

macro_rules! module_parity_tests {
    ($($name:ident => $kind:expr),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_module_parity($kind);
            }
        )*
    };
}

module_parity_tests! {
    block_tick_manifest_matches_java => ModuleKind::BlockTick,
    random_tick_manifest_matches_java => ModuleKind::RandomTick,
    falling_blocks_manifest_matches_java => ModuleKind::FallingBlocks,
    block_physics_manifest_matches_java => ModuleKind::BlockPhysics,
    fluid_manifest_matches_java => ModuleKind::Fluid,
    weather_manifest_matches_java => ModuleKind::Weather,
    npc_manifest_matches_java => ModuleKind::Npc,
    encounter_manager_manifest_matches_java => ModuleKind::EncounterManager,
}

#[test]
fn golden_selection_is_complete_and_sorted() {
    let golden = golden();
    assert_eq!(golden.len(), ModuleKind::ALL.len());
    let mut ids: Vec<_> = golden.iter().map(PluginManifest::identifier).collect();
    let sorted = ids.clone();
    ids.sort();
    assert_eq!(ids, sorted, "golden must stay sorted by Group:Name");
    for kind in ModuleKind::ALL {
        assert!(
            golden
                .iter()
                .any(|manifest| manifest.identifier() == kind.identifier()),
            "golden is missing {}",
            kind.identifier()
        );
    }
}

#[test]
fn compiled_selection_matches_the_feature_set() {
    let manifests = compiled_manifests();
    for kind in ModuleKind::ALL {
        let present = manifests
            .iter()
            .any(|manifest| manifest.identifier() == kind.identifier());
        assert_eq!(present, feature_compiled(kind), "{}", kind.as_str());
    }
}

#[test]
fn default_start_orders_dependencies_and_enables_switches() {
    let mut host = BuiltinHost::compiled().expect("compiled host");
    let report = host.start(&StartOptions::default()).expect("start");
    let ids: Vec<String> = report
        .enabled
        .iter()
        .map(|module| module.id.clone())
        .collect();
    let positions: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(position, id)| (id.as_str(), position))
        .collect();

    let mut kind_positions = BTreeMap::new();
    for kind in ModuleKind::ALL {
        if let Some(position) = positions.get(kind.identifier().to_string().as_str()) {
            kind_positions.insert(kind, *position);
        }
        assert_eq!(
            host.runtime().is_enabled(kind),
            feature_compiled(kind),
            "{} switch",
            kind.as_str()
        );
        assert_eq!(
            host.state(&kind.identifier().to_string()),
            feature_compiled(kind).then_some(ModuleState::Enabled)
        );
    }

    // Java manifest edges inside the compiled set must be respected.
    for (module, dependency) in [
        (ModuleKind::BlockPhysics, ModuleKind::BlockTick),
        (ModuleKind::BlockPhysics, ModuleKind::FallingBlocks),
        (ModuleKind::Fluid, ModuleKind::BlockTick),
        (ModuleKind::EncounterManager, ModuleKind::Npc),
        (ModuleKind::EncounterManager, ModuleKind::Weather),
    ] {
        if let (Some(module), Some(dependency)) =
            (kind_positions.get(&module), kind_positions.get(&dependency))
        {
            assert!(
                dependency < module,
                "{} must start before {}",
                dependency,
                module
            );
        }
    }

    let stopped = host.shutdown();
    assert_eq!(stopped.stopped.len(), ids.len());
    assert_eq!(host.runtime(), BuiltinRuntime::none());
    assert!(!host.is_started());
}

#[test]
fn disabled_leaf_is_not_started_and_adds_no_switch() {
    if !feature_compiled(ModuleKind::RandomTick) {
        eprintln!("random-tick feature is not compiled — the test is skipped");
        return;
    }
    let mut host = BuiltinHost::compiled().expect("compiled host");
    let report = host
        .start(&StartOptions {
            requested: Vec::new(),
            disabled: vec!["Hytale:RandomTick".to_string()],
        })
        .expect("start");
    assert!(!report.ids().contains(&"Hytale:RandomTick"));
    assert!(report.disabled.contains(&"Hytale:RandomTick".to_string()));
    assert!(!host.runtime().random_tick);
    assert_eq!(host.state("Hytale:RandomTick"), Some(ModuleState::Disabled));
    assert!(!host.runtime().enabled_modules().contains(&"random-tick"));
    host.shutdown();
}

#[test]
fn disabling_a_required_dependency_fails_closed() {
    if !(feature_compiled(ModuleKind::BlockTick) && feature_compiled(ModuleKind::Fluid)) {
        eprintln!("block-tick/fluid features are not compiled — the test is skipped");
        return;
    }
    let mut host = BuiltinHost::compiled().expect("compiled host");
    let error = host
        .start(&StartOptions {
            requested: Vec::new(),
            disabled: vec!["Hytale:BlockTick".to_string()],
        })
        .expect_err("fluid must not start without BlockTick");
    match error {
        HostError::DisabledDependency { module, dependency } => {
            assert_eq!(dependency, "Hytale:BlockTick");
            assert_ne!(module, dependency);
        }
        other => panic!("unexpected error: {other}"),
    }
    assert!(!host.is_started());
    assert_eq!(host.runtime(), BuiltinRuntime::none());
}

#[test]
fn disabling_every_module_yields_an_empty_runtime() {
    let mut host = BuiltinHost::compiled().expect("compiled host");
    let mut disabled: Vec<String> = host.module_ids().into_iter().map(str::to_string).collect();
    disabled.sort();
    let report = host
        .start(&StartOptions {
            requested: Vec::new(),
            disabled: disabled.clone(),
        })
        .expect("all modules disabled is a valid selection");
    assert!(report.enabled.is_empty());
    assert_eq!(report.disabled.len(), disabled.len());
    assert_eq!(host.runtime(), BuiltinRuntime::none());
    assert!(!host.runtime().any_simulation_stage());
    host.shutdown();
}

#[cfg(not(any(
    feature = "block-tick",
    feature = "random-tick",
    feature = "falling-blocks",
    feature = "block-physics",
    feature = "fluid",
    feature = "weather",
    feature = "npc",
    feature = "encounter-manager"
)))]
#[test]
fn no_default_features_registers_no_modules() {
    let host = BuiltinHost::compiled().expect("compiled host");
    assert!(host.module_ids().is_empty());
    assert_eq!(host.runtime(), BuiltinRuntime::none());
    assert!(compiled_manifests().is_empty());
}
