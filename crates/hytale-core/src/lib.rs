//! `hytale-core` — shared server infrastructure.
//!
//! Configuration and command-line options, logging setup, the typed event bus,
//! the registry facade, the task scheduler, the plugin kernel, and module
//! manifests live here. It has no internal dependencies and is shared by the
//! asset, storage, world, network, auth, command, and server layers.

#![deny(clippy::all)]
