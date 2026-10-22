//! `hytale-server` library surface.
//!
//! The binary is a thin CLI; the reusable boot paths live here so integration
//! tests can drive them without spawning a process.

#![deny(clippy::all)]

pub mod headless;

pub use headless::{run as run_headless, HeadlessError, HeadlessOptions, HeadlessReport};
