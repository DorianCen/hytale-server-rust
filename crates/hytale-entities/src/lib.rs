//! `hytale-entities` — entity and player component store.
//!
//! A thin Rust-native wrapper over `bevy_ecs` providing the component registry,
//! systems/schedules, and player/entity lifecycle used by worlds. Components are
//! serializable Rust structs with stable ids; block entities reference their
//! position in the owning world. Exact Java ECS API parity is deliberately not a
//! goal.

#![deny(clippy::all)]
