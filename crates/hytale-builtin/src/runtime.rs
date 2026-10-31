//! Runtime switches derived from the enabled builtin module set.
//!
//! A switch is only `true` when the module is compiled in *and* its lifecycle
//! reached `ENABLED`. The server maps the block switches onto
//! `SimulationStages`, so a disabled module performs no work in the tick loop.

use serde::{Deserialize, Serialize};

use crate::manifest::PluginIdentifier;

/// One selected builtin/native module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKind {
    /// Scheduled block ticks (`builtin/BlockTick`).
    BlockTick,
    /// Random block ticks (`builtin/RandomTick`).
    RandomTick,
    /// Falling-block entities (`builtin/FallingBlocks`).
    FallingBlocks,
    /// Support/filler physics (`builtin/BlockPhysics`).
    BlockPhysics,
    /// Fluid tickers (`builtin/Fluid`).
    Fluid,
    /// Weather state and transitions (`builtin/Weather`).
    Weather,
    /// NPC runtime (`NPC`).
    Npc,
    /// Encounter manager evaluator (`builtin/EncounterManager`).
    EncounterManager,
}

impl ModuleKind {
    /// All kinds known to the runtime.
    pub const ALL: [Self; 8] = [
        Self::BlockTick,
        Self::RandomTick,
        Self::FallingBlocks,
        Self::BlockPhysics,
        Self::Fluid,
        Self::Weather,
        Self::Npc,
        Self::EncounterManager,
    ];

    /// Java `Group:Name` identifier.
    pub fn identifier(self) -> PluginIdentifier {
        match self {
            Self::BlockTick => PluginIdentifier::new("Hytale", "BlockTick"),
            Self::RandomTick => PluginIdentifier::new("Hytale", "RandomTick"),
            Self::FallingBlocks => PluginIdentifier::new("Hytale", "FallingBlocks"),
            Self::BlockPhysics => PluginIdentifier::new("Hytale", "BlockPhysics"),
            Self::Fluid => PluginIdentifier::new("Hytale", "Fluid"),
            Self::Weather => PluginIdentifier::new("Hytale", "Weather"),
            Self::Npc => PluginIdentifier::new("Hytale", "NPC"),
            Self::EncounterManager => PluginIdentifier::new("Hytale", "EncounterManager"),
        }
    }

    /// Stable kebab-case name used in logs and CLI selection.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BlockTick => "block-tick",
            Self::RandomTick => "random-tick",
            Self::FallingBlocks => "falling-blocks",
            Self::BlockPhysics => "block-physics",
            Self::Fluid => "fluid",
            Self::Weather => "weather",
            Self::Npc => "npc",
            Self::EncounterManager => "encounter-manager",
        }
    }
}

/// Enabled module switches.
///
/// The derived `Default` reflects the compile-time feature set, so a build
/// without a feature starts with that switch disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuiltinRuntime {
    /// `builtin/BlockTick` is enabled.
    pub block_tick: bool,
    /// `builtin/RandomTick` is enabled.
    pub random_tick: bool,
    /// `builtin/FallingBlocks` is enabled.
    pub falling_blocks: bool,
    /// `builtin/BlockPhysics` is enabled.
    pub block_physics: bool,
    /// `builtin/Fluid` is enabled.
    pub fluid: bool,
    /// `builtin/Weather` is enabled.
    pub weather: bool,
    /// `NPC` is enabled.
    pub npc: bool,
    /// `builtin/EncounterManager` is enabled.
    pub encounter_manager: bool,
}

impl Default for BuiltinRuntime {
    fn default() -> Self {
        Self::compiled()
    }
}

impl BuiltinRuntime {
    /// All switches off.
    pub const fn none() -> Self {
        Self {
            block_tick: false,
            random_tick: false,
            falling_blocks: false,
            block_physics: false,
            fluid: false,
            weather: false,
            npc: false,
            encounter_manager: false,
        }
    }

    /// Switches for the modules compiled into this binary.
    pub const fn compiled() -> Self {
        Self {
            block_tick: cfg!(feature = "block-tick"),
            random_tick: cfg!(feature = "random-tick"),
            falling_blocks: cfg!(feature = "falling-blocks"),
            block_physics: cfg!(feature = "block-physics"),
            fluid: cfg!(feature = "fluid"),
            weather: cfg!(feature = "weather"),
            npc: cfg!(feature = "npc"),
            encounter_manager: cfg!(feature = "encounter-manager"),
        }
    }

    /// Whether one module switch is on.
    pub fn is_enabled(&self, kind: ModuleKind) -> bool {
        match kind {
            ModuleKind::BlockTick => self.block_tick,
            ModuleKind::RandomTick => self.random_tick,
            ModuleKind::FallingBlocks => self.falling_blocks,
            ModuleKind::BlockPhysics => self.block_physics,
            ModuleKind::Fluid => self.fluid,
            ModuleKind::Weather => self.weather,
            ModuleKind::Npc => self.npc,
            ModuleKind::EncounterManager => self.encounter_manager,
        }
    }

    /// Turn one module switch on.
    pub fn enable(&mut self, kind: ModuleKind) {
        match kind {
            ModuleKind::BlockTick => self.block_tick = true,
            ModuleKind::RandomTick => self.random_tick = true,
            ModuleKind::FallingBlocks => self.falling_blocks = true,
            ModuleKind::BlockPhysics => self.block_physics = true,
            ModuleKind::Fluid => self.fluid = true,
            ModuleKind::Weather => self.weather = true,
            ModuleKind::Npc => self.npc = true,
            ModuleKind::EncounterManager => self.encounter_manager = true,
        }
    }

    /// Turn one module switch off.
    pub fn disable(&mut self, kind: ModuleKind) {
        match kind {
            ModuleKind::BlockTick => self.block_tick = false,
            ModuleKind::RandomTick => self.random_tick = false,
            ModuleKind::FallingBlocks => self.falling_blocks = false,
            ModuleKind::BlockPhysics => self.block_physics = false,
            ModuleKind::Fluid => self.fluid = false,
            ModuleKind::Weather => self.weather = false,
            ModuleKind::Npc => self.npc = false,
            ModuleKind::EncounterManager => self.encounter_manager = false,
        }
    }

    /// Names of the enabled modules, in [`ModuleKind::ALL`] order.
    pub fn enabled_modules(&self) -> Vec<&'static str> {
        ModuleKind::ALL
            .into_iter()
            .filter(|kind| self.is_enabled(*kind))
            .map(ModuleKind::as_str)
            .collect()
    }

    /// Whether any block-simulation stage is enabled.
    pub fn any_simulation_stage(&self) -> bool {
        self.block_tick
            || self.random_tick
            || self.falling_blocks
            || self.block_physics
            || self.fluid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_runtime_matches_the_feature_set() {
        let runtime = BuiltinRuntime::compiled();
        assert_eq!(runtime.block_tick, cfg!(feature = "block-tick"));
        assert_eq!(runtime.random_tick, cfg!(feature = "random-tick"));
        assert_eq!(runtime.falling_blocks, cfg!(feature = "falling-blocks"));
        assert_eq!(runtime.block_physics, cfg!(feature = "block-physics"));
        assert_eq!(runtime.fluid, cfg!(feature = "fluid"));
        assert_eq!(runtime.weather, cfg!(feature = "weather"));
        assert_eq!(runtime.npc, cfg!(feature = "npc"));
        assert_eq!(
            runtime.encounter_manager,
            cfg!(feature = "encounter-manager")
        );
        assert_eq!(BuiltinRuntime::default(), runtime);
    }

    #[test]
    fn switches_enable_disable_and_round_trip_through_json() {
        let mut runtime = BuiltinRuntime::none();
        assert!(runtime.enabled_modules().is_empty());
        for kind in ModuleKind::ALL {
            runtime.enable(kind);
            assert!(runtime.is_enabled(kind));
        }
        assert_eq!(runtime.enabled_modules().len(), ModuleKind::ALL.len());

        let json = serde_json::to_string(&runtime).unwrap();
        let decoded: BuiltinRuntime = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, runtime);

        for kind in ModuleKind::ALL {
            runtime.disable(kind);
            assert!(!runtime.is_enabled(kind));
        }
        assert!(!runtime.any_simulation_stage());
    }
}
