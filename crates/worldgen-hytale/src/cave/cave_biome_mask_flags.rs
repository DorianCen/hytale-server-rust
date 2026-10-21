// Ported from: `.hytale-source/.../worldgen/cave/CaveBiomeMaskFlags.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveBiomeMaskFlags` — the
//! bit flags the cave biome mask evaluates to.
//!
//! The flags decide whether a cave node may continue to grow, whether its
//! geometry is added to the world and whether the node may retry with
//! another direction/shape.

use crate::util::condition::flag::ConstantInt2Flags;

/// Java `CaveBiomeMaskFlags.GENERATE`.
///
/// Allows the CaveNode to continue to generate (and spawn child nodes) at a
/// given position/region. If left unset then generation for the CaveNode
/// will terminate immediately.
pub const GENERATE: i32 = 1;

/// Java `CaveBiomeMaskFlags.POPULATE`.
///
/// Allows the CaveNode to populate at a given position/region. If left unset
/// then the CaveNode will continue to generate child nodes but will not add
/// its own geometry to the world.
pub const POPULATE: i32 = 1 << 1;

/// Java `CaveBiomeMaskFlags.CONTINUE`.
///
/// Allows the CaveNode to attempt to generate in another random
/// direction/shape when the current attempt has failed. If left unset then
/// generation for the node will terminate immediately.
pub const CONTINUE: i32 = 1 << 2;

/// Java `CaveBiomeMaskFlags.Defaults` — the default flag values.
pub mod defaults {
    /// An int value that contains the default biome-mask flags.
    ///
    /// The default behaviour blocks generation and population of the current
    /// node but allows it to attempt a different direction/shape.
    pub const DEFAULT_RESULT: i32 = super::CONTINUE;

    /// An int value that contains no valid cave biome-mask flags.
    ///
    /// The default behaviour blocks further generation of the current node,
    /// prevents its geometry from being added to the cave system, and
    /// prevents further attempts to generate in a different direction/shape.
    pub const DISALLOW_ALL: i32 = 0;

    /// An int value that contains all cave biome-mask flags.
    ///
    /// The default behaviour is to allow node generation to continue, to add
    /// the node geometry to the cave system and, should generation be
    /// blocked, allow the current node to attempt a new direction/shape.
    pub const ALLOW_ALL: i32 = super::GENERATE | super::POPULATE | super::CONTINUE;
}

/// Java `CaveBiomeMaskFlags.DEFAULT_ALLOW` — `new
/// ConstantInt2Flags(Defaults.ALLOW_ALL)`.
///
/// Deviation: Java compares this constant *by identity* (the cave generator
/// short-circuits the biome lookup when the mask is the default one). The
/// Rust port exposes the same value as a `ConstantInt2Flags` constant, so the
/// comparison is a value comparison — a mask that was built separately but
/// holds the same flags behaves like the default one.
pub const DEFAULT_ALLOW: ConstantInt2Flags = ConstantInt2Flags::new(defaults::ALLOW_ALL);

/// Java `CaveBiomeMaskFlags.DEFAULT_DENY` — `new
/// ConstantInt2Flags(Defaults.DISALLOW_ALL)`.
///
/// Deviation: see [`DEFAULT_ALLOW`].
pub const DEFAULT_DENY: ConstantInt2Flags = ConstantInt2Flags::new(defaults::DISALLOW_ALL);

/// Java `static boolean canGenerate(int value)` — whether the value contains
/// the [`GENERATE`] flag.
#[inline]
pub fn can_generate(value: i32) -> bool {
    test(value, GENERATE)
}

/// Java `static boolean canPopulate(int value)` — whether the value contains
/// the [`POPULATE`] flag.
#[inline]
pub fn can_populate(value: i32) -> bool {
    test(value, POPULATE)
}

/// Java `static boolean canContinue(int value)` — whether the value contains
/// the [`CONTINUE`] flag.
#[inline]
pub fn can_continue(value: i32) -> bool {
    test(value, CONTINUE)
}

/// Java `static boolean test(int value, int flag)` — whether the flag is set
/// on the value.
#[inline]
pub fn test(value: i32, flag: i32) -> bool {
    (value & flag) == flag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flags_have_the_java_values() {
        assert_eq!(GENERATE, 1);
        assert_eq!(POPULATE, 2);
        assert_eq!(CONTINUE, 4);
        assert_eq!(defaults::ALLOW_ALL, 7);
        assert_eq!(defaults::DISALLOW_ALL, 0);
        assert_eq!(defaults::DEFAULT_RESULT, CONTINUE);
    }

    #[test]
    fn the_flag_test_requires_every_bit_of_the_flag() {
        assert!(test(0b0111, GENERATE));
        assert!(test(0b0111, POPULATE));
        assert!(test(0b0111, CONTINUE));
        assert!(!test(0b0110, GENERATE));
        assert!(!test(0b0101, POPULATE));
        assert!(!test(0b0011, CONTINUE));
        assert!(!test(0, GENERATE));
    }

    #[test]
    fn the_helpers_read_the_matching_flag() {
        assert!(can_generate(defaults::ALLOW_ALL));
        assert!(can_populate(defaults::ALLOW_ALL));
        assert!(can_continue(defaults::ALLOW_ALL));

        assert!(!can_generate(defaults::DEFAULT_RESULT));
        assert!(!can_populate(defaults::DEFAULT_RESULT));
        assert!(can_continue(defaults::DEFAULT_RESULT));

        assert!(!can_generate(defaults::DISALLOW_ALL));
        assert!(!can_populate(defaults::DISALLOW_ALL));
        assert!(!can_continue(defaults::DISALLOW_ALL));
    }

    #[test]
    fn the_default_masks_hold_the_default_flags() {
        assert_eq!(DEFAULT_ALLOW.result, defaults::ALLOW_ALL);
        assert_eq!(DEFAULT_DENY.result, defaults::DISALLOW_ALL);
    }
}
