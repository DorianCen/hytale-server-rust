//! NPC core: role/flock assets, navigation, steering and perception.
//!
//! This is the first T58 slice. Java's NPC builder/blackboard/ECS runtime is
//! not ported; this crate owns the deterministic pieces the later NPC systems
//! share:
//!
//! * [`role`] — the `Server/NPC/Roles` document catalog (type/reference/raw
//!   parameters), preserving the builder input without evaluating it.
//! * [`flock`] — `Server/NPC/Flocks` definitions, size selection and flock
//!   membership.
//! * [`steering`] — Java `Steering` output plus the pursue/evade/rotate/wander
//!   forces.
//! * [`navigation`] — Java `SearchDirections` tables and a block-grid A*.
//! * [`path`] — waypoint following, waypoint advancement and path rejection.
//! * [`perception`] — the Java view-cone test.
//! * [`spawn`] — `Server/NPC/Spawn/World` configurations, time/moon gates and
//!   weighted role selection.
//! * [`spawn_position`] — deterministic walk/wade/underwater/dive/fly position
//!   search over a real world.
//! * [`block_set`] — `Server/Item/Block/Sets` membership for spawn floors.
//! * [`fluid_tags`] — fluid tag membership for `SpawnFluidTag`.
//! * [`light`] — `LightRanges` spawn constraints over packed world light.
//! * [`spawn_special`] — beacon and spawn-marker catalogs, weighted selection
//!   and ISO-8601 cooldowns.
//! * [`combat`] — resolved combat profiles, interaction damage ranges and a
//!   scripted NPC actor (targeting, chase, melee, damage, death).
//! * [`builder`] — parsed role `Instructions` graph and sensor/action
//!   evaluation.
//! * [`ecs`] — `bevy_ecs` actor storage used by the server world thread.
//!
//! Motion controllers, the full builder action set, blackboard sensors and
//! spawn-position search remain follow-ups. T60b adds the parsed role
//! instruction graph and the `bevy_ecs` actor store used by the server.

#![deny(clippy::all)]

pub mod blackboard;
pub mod block_set;
pub mod builder;
pub mod combat;
pub mod ecs;
pub mod encounter;
pub mod expr;
pub mod flock;
pub mod fluid_tags;
pub mod group;
pub mod light;
pub mod motion;
pub mod navigation;
pub mod path;
pub mod perception;
pub mod role;
pub mod spawn;
pub mod spawn_position;
pub mod spawn_special;
pub mod spawn_suppression;
pub mod steering;
pub mod worldgrid;

pub use blackboard::NpcBlackboard;
pub use block_set::{wildcard_matches, BlockSet, BlockSetCatalog, BlockSetError};
pub use builder::{BuilderAction, BuilderContext, BuilderGraph, InstructionNode, Sensor};
pub use combat::{
    npc_stat_map, AttackAction, AttackType, BallisticMode, CombatError, CombatProfile,
    DamageCalculator, DamageEffects, DamageType, NpcActor, NpcAttack, NpcTarget, NpcTickReport,
};
pub use ecs::{NpcActorComponent, NpcEcs, NpcEntityId};
pub use encounter::{
    BeaconMessage, BossBarState, DeferredEncounterAction, EncounterAction, EncounterCatalog,
    EncounterContext, EncounterDocument, EncounterError, EncounterRuntime, EncounterTarget,
    EncounterTickReport, PortalState, StateTransition,
};
pub use expr::{evaluate, EvalContext, ExprError, Value as ExprValue};
pub use flock::{FlockAsset, FlockCatalog, FlockSize, PersistentFlockData};
pub use fluid_tags::FluidTagIndex;
pub use group::{GroupMember, GroupSteeringAccumulator};
pub use light::{LightChannel, LightRanges};
pub use motion::{MotionController, MotionMode};
pub use navigation::{
    search_directions, AStarConfig, AStarPath, AStarPlanner, BlockPos, SearchDirection,
};
pub use path::PathFollower;
pub use perception::is_in_view_cone;
pub use role::{NpcRole, RoleCatalog, RoleType};
pub use spawn::{
    RoleSpawnParameters, SpawnError, SpawnFlock, SpawnTimeWindow, WorldSpawn, WorldSpawnCatalog,
    WorldSpawnController,
};
pub use spawn_position::{
    SpawnMovementMode, SpawnPosition, SpawnPositionSearch, SpawnSearchOptions,
};
pub use spawn_special::{
    parse_iso_duration, BeaconCatalog, BeaconController, BeaconSpawn, BeaconSpawnRequest,
    MarkerNpcConfig, MarkerSpawnRequest, SpawnMarker, SpawnMarkerCatalog, SpawnMarkerController,
    SpawnSpecialError,
};
pub use spawn_suppression::{
    NpcGroup, NpcGroupCatalog, SpawnSuppression, SpawnSuppressionCatalog, SpawnSuppressionError,
    SpawnSuppressionIndex, SpawnSuppressionZone, SpawnSuppressor,
};
pub use steering::{Steering, SteeringForceEvade, SteeringForcePursue, SteeringForceRotate};
pub use worldgrid::WorldGrid;
