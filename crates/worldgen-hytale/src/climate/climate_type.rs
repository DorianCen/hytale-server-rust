// Ported from: `.hytale-source/.../worldgen/climate/ClimateType.java`
//! Java `climate.ClimateType` — the recursive climate type tree with the
//! ocean/shore/island flag bits.

use std::rc::Rc;

use crate::climate::climate_color::ClimateColor;
use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_point::ClimatePoint;

/// Java `ClimateType.MAX_DEPTH`.
const MAX_DEPTH: i32 = 10;

/// Java `ClimateType.IS_ISLAND`.
pub const IS_ISLAND: i32 = 1 << 29;
/// Java `ClimateType.IS_SHORE`.
pub const IS_SHORE: i32 = 1 << 30;
/// Java `ClimateType.IS_OCEAN`.
pub const IS_OCEAN: i32 = 1 << 31;

/// Java `ClimateType.IS_MAINLAND`.
pub const IS_MAINLAND: i32 = 0;
/// Java `ClimateType.IS_MAINLAND_SHORE`.
pub const IS_MAINLAND_SHORE: i32 = IS_MAINLAND | IS_SHORE;
/// Java `ClimateType.IS_MAINLAND_SHALLOW_OCEAN`.
pub const IS_MAINLAND_SHALLOW_OCEAN: i32 = IS_MAINLAND | IS_SHORE | IS_OCEAN;

/// Java `ClimateType.IS_ISLAND_SHORE`.
pub const IS_ISLAND_SHORE: i32 = IS_ISLAND | IS_SHORE;
/// Java `ClimateType.IS_ISLAND_SHALLOW_OCEAN`.
pub const IS_ISLAND_SHALLOW_OCEAN: i32 = IS_ISLAND | IS_SHORE | IS_OCEAN;

/// Java `ClimateType.MASK`.
pub const MASK: i32 = (1 << 29) - 1;

/// Java `climate.ClimateType`.
pub struct ClimateType {
    /// Java `public final String name`.
    pub name: String,
    /// Java `public final ClimateColor color`.
    pub color: ClimateColor,
    /// Java `public final ClimateColor island`.
    pub island: ClimateColor,
    /// Java `public final ClimatePoint[] points`.
    pub points: Vec<ClimatePoint>,
    /// Java `public final ClimateType[] children` — owned recursively.
    pub children: Vec<Rc<ClimateType>>,
}

impl ClimateType {
    /// Java `ClimateType(String, ClimateColor, ClimateColor, ClimatePoint[], ClimateType[])`.
    pub fn new(
        name: impl Into<String>,
        color: ClimateColor,
        island: ClimateColor,
        points: Vec<ClimatePoint>,
        children: Vec<Rc<ClimateType>>,
    ) -> Self {
        Self {
            name: name.into(),
            color,
            island,
            points,
            children,
        }
    }

    /// Java static `name(ClimateType parent, ClimateType type)`.
    pub fn display_name(parent: Option<&ClimateType>, climate_type: &ClimateType) -> String {
        match parent {
            Some(p) if !std::ptr::eq(p, climate_type) => format!("{} / {}", p.name, climate_type.name),
            _ => climate_type.name.clone(),
        }
    }

    /// Java static `walk(ClimateType, Consumer<ClimateType>)` — visits the type
    /// and all descendants (depth-capped at Java's `MAX_DEPTH = 10`).
    pub fn walk(type_: &Rc<ClimateType>, visitor: &mut impl FnMut(&Rc<ClimateType>)) {
        Self::walk_recursive(type_, visitor, 0);
    }

    /// Java static `walk(ClimateType[], Consumer<ClimateType>)`.
    pub fn walk_all(types: &[Rc<ClimateType>], visitor: &mut impl FnMut(&Rc<ClimateType>)) {
        for type_ in types {
            Self::walk_recursive(type_, visitor, 0);
        }
    }

    fn walk_recursive(
        type_: &Rc<ClimateType>,
        visitor: &mut impl FnMut(&Rc<ClimateType>),
        depth: i32,
    ) {
        if depth >= MAX_DEPTH {
            return;
        }
        visitor(type_);
        for child in &type_.children {
            Self::walk_recursive(child, visitor, depth + 1);
        }
    }

    /// Java static `color(int id, ClimateGraph climate)` — resolves a flagged
    /// climate id to its map color.
    pub fn color(id: i32, climate: &ClimateGraph) -> i32 {
        let flags = id & !MASK;
        let type_ = climate.get_type(id & MASK);

        match flags {
            IS_OCEAN => type_.color.ocean,
            IS_MAINLAND_SHORE => type_.color.shore,
            IS_MAINLAND_SHALLOW_OCEAN => type_.color.shallow_ocean,
            IS_ISLAND => type_.island.land,
            IS_ISLAND_SHORE => type_.island.shore,
            IS_ISLAND_SHALLOW_OCEAN => type_.island.shallow_ocean,
            _ => type_.color.land,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_bit_values() {
        assert_eq!(IS_ISLAND, 1 << 29);
        assert_eq!(IS_SHORE, 1 << 30);
        assert_eq!(IS_OCEAN, 1 << 31);
        assert_eq!(MASK, (1 << 29) - 1);
        // IS_OCEAN bit lies outside the MASK (ids are masked separately)
        assert_eq!(IS_OCEAN & MASK, 0);
        assert_eq!(IS_SHORE & MASK, 0);
    }
}
