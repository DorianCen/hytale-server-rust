//! `hytale-commands` — console and in-game command framework.
//!
//! Provides argument parsing, command registration and dispatch, permission
//! checks, and the built-in commands (`stop`, `save`, `world`, `tp`, `give`,
//! `help`). Commands are executed on the owning world thread through the core
//! task queue.

#![deny(clippy::all)]
