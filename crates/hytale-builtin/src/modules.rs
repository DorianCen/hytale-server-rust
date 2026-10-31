//! Selected builtin modules and their Java manifests.
//!
//! The selected set is the block-simulation/weather group needed by M5 plus
//! the NPC and EncounterManager modules needed by M6. Their native behavior is
//! already implemented by `hytale-world`/`hytale-npc`; this crate owns the
//! manifest, lifecycle and enable switches so the world runtime can gate those
//! systems without duplicating the gameplay code.

use crate::lifecycle::{BuiltinModule, ModuleContext};
use crate::manifest::PluginManifest;
use crate::runtime::ModuleKind;

/// One compiled-in module.
#[derive(Debug)]
pub struct NativeModule {
    manifest: PluginManifest,
    kind: ModuleKind,
    started: bool,
}

impl NativeModule {
    /// Build a module from its manifest and runtime switch.
    pub fn new(kind: ModuleKind, manifest: PluginManifest) -> Self {
        Self {
            manifest,
            kind,
            started: false,
        }
    }

    /// Module kind.
    pub fn kind(&self) -> ModuleKind {
        self.kind
    }
}

impl BuiltinModule for NativeModule {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn start(&mut self, context: &mut ModuleContext<'_>) -> Result<(), String> {
        context.runtime.enable(self.kind);
        self.started = true;
        Ok(())
    }

    fn shutdown(&mut self, context: &mut ModuleContext<'_>) -> Result<(), String> {
        context.runtime.disable(self.kind);
        self.started = false;
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started
    }
}

/// `builtin/BlockTick` manifest.
pub fn block_tick_manifest() -> PluginManifest {
    base(
        "BlockTick",
        "com.hypixel.hytale.builtin.blocktick.BlockTickPlugin",
    )
    .with_dependency("Hytale:LegacyModule")
}

/// `builtin/RandomTick` manifest.
pub fn random_tick_manifest() -> PluginManifest {
    base(
        "RandomTick",
        "com.hypixel.hytale.builtin.randomtick.RandomTickPlugin",
    )
    .with_dependency("Hytale:LegacyModule")
}

/// `builtin/FallingBlocks` manifest.
pub fn falling_blocks_manifest() -> PluginManifest {
    base(
        "FallingBlocks",
        "com.hypixel.hytale.builtin.fallingblocks.FallingBlocksPlugin",
    )
    .with_description("Falling Blocks")
    .with_dependency("Hytale:EntityModule")
}

/// `builtin/BlockPhysics` manifest.
pub fn block_physics_manifest() -> PluginManifest {
    base(
        "BlockPhysics",
        "com.hypixel.hytale.builtin.blockphysics.BlockPhysicsPlugin",
    )
    .with_dependency("Hytale:BlockTick")
    .with_dependency("Hytale:FallingBlocks")
    .with_dependency("Hytale:BlockTypeModule")
}

/// `builtin/Fluid` manifest.
pub fn fluid_manifest() -> PluginManifest {
    base("Fluid", "com.hypixel.hytale.builtin.fluid.FluidPlugin")
        .with_dependency("Hytale:BlockTick")
        .with_dependency("Hytale:LegacyModule")
}

/// `builtin/Weather` manifest.
pub fn weather_manifest() -> PluginManifest {
    base(
        "Weather",
        "com.hypixel.hytale.builtin.weather.WeatherPlugin",
    )
    .with_dependency("Hytale:EntityModule")
}

/// `NPC` manifest (a server plugin rather than a `builtin/` plugin).
pub fn npc_manifest() -> PluginManifest {
    base("NPC", "com.hypixel.hytale.server.npc.NPCPlugin")
        .with_dependency("Hytale:AssetModule")
        .with_dependency("Hytale:TagSet")
        .with_dependency("Hytale:Points")
        .with_dependency("Hytale:EntityModule")
        .with_dependency("Hytale:EntityStatsModule")
        .with_dependency("Hytale:EntityUIModule")
        .with_dependency("Hytale:DamageModule")
        .with_dependency("Hytale:Rewards")
        .with_optional_dependency("Hytale:Objectives")
        .with_optional_dependency("Hytale:Path")
        .with_optional_dependency("Hytale:Teleport")
        .with_sub_plugin(sub_plugin(
            "Spawning",
            "com.hypixel.hytale.server.spawning.SpawningPlugin",
        ))
        .with_sub_plugin(sub_plugin(
            "Flock",
            "com.hypixel.hytale.server.flock.FlockPlugin",
        ))
}

/// `builtin/EncounterManager` manifest.
pub fn encounter_manager_manifest() -> PluginManifest {
    base(
        "EncounterManager",
        "com.hypixel.hytale.builtin.encountermanager.EncounterManagerPlugin",
    )
    .with_description("Adds Encounter Managers: data-driven instruction-list controllers.")
    .with_dependency("Hytale:NPC")
    .with_dependency("Hytale:Audio")
    .with_dependency("Hytale:Weather")
    .with_dependency("Hytale:WorldEvents")
    .with_dependency("Hytale:Rewards")
}

fn base(name: &str, main: &str) -> PluginManifest {
    let mut manifest = PluginManifest::new("Hytale", name, "1.0.0", main);
    manifest.website = Some("https://hypixel.com/".to_string());
    manifest
}

fn sub_plugin(name: &str, main: &str) -> PluginManifest {
    PluginManifest {
        name: name.to_string(),
        main: Some(main.to_string()),
        ..PluginManifest::default()
    }
}

/// The modules compiled into this binary.
///
/// A feature that is off contributes no descriptor and no trait object, so a
/// disabled module has no runtime presence.
pub fn compiled_modules() -> Vec<Box<dyn BuiltinModule>> {
    vec![
        #[cfg(feature = "block-tick")]
        Box::new(NativeModule::new(
            ModuleKind::BlockTick,
            block_tick_manifest(),
        )),
        #[cfg(feature = "random-tick")]
        Box::new(NativeModule::new(
            ModuleKind::RandomTick,
            random_tick_manifest(),
        )),
        #[cfg(feature = "falling-blocks")]
        Box::new(NativeModule::new(
            ModuleKind::FallingBlocks,
            falling_blocks_manifest(),
        )),
        #[cfg(feature = "block-physics")]
        Box::new(NativeModule::new(
            ModuleKind::BlockPhysics,
            block_physics_manifest(),
        )),
        #[cfg(feature = "fluid")]
        Box::new(NativeModule::new(ModuleKind::Fluid, fluid_manifest())),
        #[cfg(feature = "weather")]
        Box::new(NativeModule::new(ModuleKind::Weather, weather_manifest())),
        #[cfg(feature = "npc")]
        Box::new(NativeModule::new(ModuleKind::Npc, npc_manifest())),
        #[cfg(feature = "encounter-manager")]
        Box::new(NativeModule::new(
            ModuleKind::EncounterManager,
            encounter_manager_manifest(),
        )),
    ]
}

/// Manifests for every compiled module, in `Group:Name` order.
pub fn compiled_manifests() -> Vec<PluginManifest> {
    let mut manifests: Vec<PluginManifest> = compiled_modules()
        .into_iter()
        .map(|module| module.manifest().clone())
        .collect();
    manifests.sort_by_key(PluginManifest::identifier);
    manifests
}

/// Whether a module feature is compiled in.
pub fn feature_compiled(kind: ModuleKind) -> bool {
    match kind {
        ModuleKind::BlockTick => cfg!(feature = "block-tick"),
        ModuleKind::RandomTick => cfg!(feature = "random-tick"),
        ModuleKind::FallingBlocks => cfg!(feature = "falling-blocks"),
        ModuleKind::BlockPhysics => cfg!(feature = "block-physics"),
        ModuleKind::Fluid => cfg!(feature = "fluid"),
        ModuleKind::Weather => cfg!(feature = "weather"),
        ModuleKind::Npc => cfg!(feature = "npc"),
        ModuleKind::EncounterManager => cfg!(feature = "encounter-manager"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_manifests_are_valid_and_sorted() {
        let manifests = compiled_manifests();
        assert_eq!(manifests.len(), compiled_modules().len());
        let mut previous = None;
        for manifest in &manifests {
            manifest.validate().unwrap();
            let identifier = manifest.identifier();
            if let Some(previous) = previous {
                assert!(previous < identifier);
            }
            previous = Some(identifier);
        }
    }

    #[test]
    fn compiled_modules_match_the_feature_set() {
        let manifests = compiled_manifests();
        for kind in ModuleKind::ALL {
            let present = manifests
                .iter()
                .any(|manifest| manifest.identifier() == kind.identifier());
            assert_eq!(present, feature_compiled(kind), "{}", kind.as_str());
        }
    }
}
