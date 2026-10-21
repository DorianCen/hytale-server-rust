//! `hytale-assets` — asset pack loading and typed Hytale assets.
//!
//! This crate owns the read-only asset VFS over `Assets.zip` and mod directories,
//! the store dependency graph and `Parent` inheritance resolver, the typed asset
//! structs, deterministic index-map id assignment, and the asset packet encoders.
//! It consumes protocol packet types but never depends on the world, network, or
//! server layers.

#![deny(clippy::all)]

pub mod order;
pub mod pack;
pub mod parent;

pub use order::{order_stores, StoreNode};
pub use pack::{AssetError, AssetKey, AssetPack, ParentProvider};
pub use parent::{merge_values, ArrayMerge, MergeHook, ParentResolver};
