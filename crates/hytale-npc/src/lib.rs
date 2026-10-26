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
//!
//! Motion controllers, the builder expression language, blackboard sensors and
//! world spawning remain later T58/T59 work.

#![deny(clippy::all)]

pub mod expr;
pub mod flock;
pub mod navigation;
pub mod path;
pub mod perception;
pub mod role;
pub mod steering;

pub use expr::{evaluate, EvalContext, ExprError, Value as ExprValue};
pub use flock::{FlockAsset, FlockCatalog, FlockSize, PersistentFlockData};
pub use navigation::{
    search_directions, AStarConfig, AStarPath, AStarPlanner, BlockPos, SearchDirection,
};
pub use path::PathFollower;
pub use perception::is_in_view_cone;
pub use role::{NpcRole, RoleCatalog, RoleType};
pub use steering::{Steering, SteeringForceEvade, SteeringForcePursue, SteeringForceRotate};
