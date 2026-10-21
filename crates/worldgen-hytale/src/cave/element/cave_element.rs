// Ported from: `.hytale-source/.../worldgen/cave/element/CaveElement.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.element.CaveElement` — the
//! common interface of the elements a cave is built from.

use crate::util::bounds::WorldBounds;

/// Java `interface CaveElement`.
pub trait CaveElement {
    /// Java `IWorldBounds getBounds()`.
    ///
    /// Deviation: the Java interface returns the live `IWorldBounds` object;
    /// the Rust `WorldBounds` is a `Copy` value, so the bounds are handed out
    /// by value.
    fn get_bounds(&self) -> WorldBounds;
}
