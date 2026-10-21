//! `procedural` — Rust port of the Hytale `ProceduralLibrary`
//! (`com.hypixel.hytale.procedurallib.**`).
//!
//! Currently contains the noise-generator core
//! (`com.hypixel.hytale.procedurallib.logic.**` including the `cell/`
//! subpackage) plus the root noise interfaces. Later phases add
//! `NoiseProperty` DSL, point generators, conditions and JSON loaders.

// This is a faithful port of Java code: keep Java signatures verbatim even
// when Rust idiom would differ, and keep expressions that Java semantics
// require (identity operations, shadowed locals, lookup tables).
#![allow(clippy::too_many_arguments)]
#![allow(clippy::identity_op)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::redundant_locals)]
#![allow(dead_code)]

pub mod json;
pub mod noise;
