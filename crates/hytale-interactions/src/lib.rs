//! Interaction graph loading and block-interaction dispatch.
//!
//! The crate is a focused port of the Java `RootInteraction`/`Interaction`
//! graph: it loads the merged JSON, walks the structural nodes, resolves
//! interaction variables and turns block/UI nodes into [`WorldAction`]s. Server
//! code applies those actions to the world; UI windows remain later work.

#![deny(clippy::all)]

pub mod block;
pub mod catalog;
pub mod combat;
pub mod executor;

pub use block::{interaction_type_from_name, state_from_block_key, BlockInteractions};
pub use catalog::{merge_value, InteractionCatalog, InteractionError};
pub use combat::{
    collect_attack_hits, collect_value_hits, AngledDamage, AttackHit, DamageCalculator,
    DamageEffects, DamageEntity, DamageType, Knockback, TargetedDamage,
};
pub use executor::{
    execute_interactions, execute_root, execute_value, ExecutionContext, ExecutionOutcome,
    WorldAction,
};
