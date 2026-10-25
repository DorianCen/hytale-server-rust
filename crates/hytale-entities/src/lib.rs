//! `hytale-entities` — entity and player gameplay components.
//!
//! Deterministic Rust-native components and systems used by worlds: movement
//! collision integration, entity stats, active effects and damage. The world
//! thread currently owns its entities directly; later NPC/combat producers
//! attach [`components::EntityGameplay`] to them. Exact Java ECS API parity is
//! deliberately not a goal.

#![deny(clippy::all)]

pub mod components;
pub mod damage;
pub mod effects;
pub mod movement;
pub mod projectile;
pub mod stats;
