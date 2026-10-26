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
//! * [`combat`] — resolved combat profiles, interaction damage ranges and a
//!   scripted NPC actor (targeting, chase, melee, damage, death).
//!
//! Motion controllers, the builder graph, blackboard sensors, spawn-position
//! search and the server ECS producer remain T59b/T60 work.

#![deny(clippy::all)]

pub mod block_set;
pub mod combat;
pub mod expr;
pub mod flock;
pub mod fluid_tags;
pub mod group;
pub mod navigation;
pub mod path;
pub mod perception;
pub mod role;
pub mod spawn;
pub mod spawn_position;
pub mod steering;
pub mod worldgrid;

pub use block_set::{wildcard_matches, BlockSet, BlockSetCatalog, BlockSetError};
pub use combat::{
    npc_stat_map, CombatError, CombatProfile, DamageCalculator, DamageType, NpcActor, NpcAttack,
    NpcTarget, NpcTickReport,
};
pub use expr::{evaluate, EvalContext, ExprError, Value as ExprValue};
pub use flock::{FlockAsset, FlockCatalog, FlockSize, PersistentFlockData};
pub use fluid_tags::FluidTagIndex;
pub use group::{GroupMember, GroupSteeringAccumulator};
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
pub use steering::{Steering, SteeringForceEvade, SteeringForcePursue, SteeringForceRotate};
pub use worldgrid::WorldGrid;
