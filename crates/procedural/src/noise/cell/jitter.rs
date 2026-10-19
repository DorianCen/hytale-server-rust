// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/CellJitter.java`,
//              `.../ConstantCellJitter.java`, `.../DefaultCellJitter.java`
//! Java package `logic.cell.jitter`.
//!
//! The Java `CellJitter` interface has exactly two implementations
//! (`DefaultCellJitter`, `ConstantCellJitter`) and is created via the static
//! `CellJitter.of(x, y, z)`. The Rust port models the closed set as a `Copy`
//! enum that delegates to the per-class structs (1:1 class files kept).

pub mod cell_jitter;
pub mod constant_cell_jitter;
pub mod default_cell_jitter;

pub use cell_jitter::CellJitter;
pub use constant_cell_jitter::ConstantCellJitter;
pub use default_cell_jitter::DefaultCellJitter;
