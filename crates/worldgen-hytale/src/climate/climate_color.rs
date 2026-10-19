// Ported from: `.hytale-source/.../worldgen/climate/ClimateColor.java`
//! Java `climate.ClimateColor`.

/// Java `climate.ClimateColor` — the four color slots used by map rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClimateColor {
    /// Java `public final int land`.
    pub land: i32,
    /// Java `public final int shore`.
    pub shore: i32,
    /// Java `public final int ocean`.
    pub ocean: i32,
    /// Java `public final int shallowOcean`.
    pub shallow_ocean: i32,
}

impl ClimateColor {
    /// Java `ClimateColor.UNSET`.
    pub const UNSET: i32 = -1;
    /// Java `ClimateColor.ISLAND`.
    pub const ISLAND: i32 = 0x554433;
    /// Java `ClimateColor.ISLAND_SHORE`.
    pub const ISLAND_SHORE: i32 = 0x775544;
    /// Java `ClimateColor.SHORE`.
    pub const SHORE: i32 = 0xddcc99;
    /// Java `ClimateColor.OCEAN`.
    pub const OCEAN: i32 = 0x008479;
    /// Java `ClimateColor.SHALLOW_OCEAN`.
    pub const SHALLOW_OCEAN: i32 = 0x3388cc;

    /// Java `ClimateColor(int, int, int, int)`.
    pub fn new(land: i32, shore: i32, ocean: i32, shallow_ocean: i32) -> Self {
        Self {
            land,
            shore,
            ocean,
            shallow_ocean,
        }
    }
}
