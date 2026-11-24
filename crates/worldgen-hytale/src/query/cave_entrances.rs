//! Cave-entrance query: the placement pass of the cave stage without
//! generating a cave.
//!
//! [`CavePopulator`](crate::chunk::populator::CavePopulator) collects the
//! entry points of every zone's cave types and then, for each candidate that
//! passes its zone/threshold/height filter, compiles and pastes the cave.
//! This query runs the same collection and the shared predicate's rules
//! (`entrance_zone_result` + `entrance_height_matches`, the same helpers the
//! populator's `entrance_height` composes) over an arbitrary rectangle and
//! stops **before** `get_cave`: no cave geometry is compiled and no block is
//! generated.
//!
//! The reported `name` is the cave type's name and the reported `category` is
//! the owning zone's name (the tooltip's two labels). The Y coordinate is the
//! same column height the height condition was evaluated against. An entrance
//! is a candidate the generator would generate a cave from — a prediction,
//! not a guaranteed walkable opening; later carving can change the surface
//! around it.
//!
//! # Anchor rect, not the populator's padded rect
//!
//! The populator collects over a chunk padded by each cave type's
//! `maximum_size` because a cave whose entry point is outside the chunk can
//! still carve into it. This query reports **entry points**, so a point
//! outside `rect` is not a result; collecting over the rect padded by the
//! point generator's *interval* (plus one block) is sufficient to see every
//! point whose cell can fall inside it. The candidate walk is therefore
//! proportional to the result rect, not to
//! `(side + 2·maximum_size)²`.
//!
//! # Query-local column memo
//!
//! Every candidate asks for its column's zone result (and, when that passes,
//! its height). The generator's own column cache is small (50,000 entries) and
//! a query over thousands of blocks evicts itself; the memo keeps one zone
//! snapshot per distinct candidate column for the lifetime of the query. Its
//! size is bounded by [`COLUMN_MEMO_CAP`]; the map is cleared wholesale when
//! the cap is reached, so a huge rect trades reuse for bounded memory instead
//! of growing without limit.

use std::collections::{HashMap, HashSet};

use hytale_math::math_util;

use crate::cave::cave_type::CaveType;
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::populator::cave_populator::{entrance_height_matches, entrance_zone_result};
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::query::feature_placement::{BlockBounds, BlockRect, FeaturePlacement, PlacementKind};
use crate::zone::zone::Zone;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// How many distinct candidate columns one query memoizes before clearing
/// the map. 262,144 columns is a 512×512 area (plus slack); keeping the memo
/// bounded matters more than perfect reuse on a 10,000-block batch rect.
pub const COLUMN_MEMO_CAP: usize = 262_144;

/// One memoized column: its zone-stage result and, once a cave type's zone
/// and threshold passed, its height.
struct ColumnMemo {
    zone: ZoneGeneratorResult,
    height: Option<i32>,
}

impl ChunkGenerator {
    /// The cave entrances this generator would use inside `rect` for `seed`,
    /// sorted deterministically by `(x, z, y, name)`.
    ///
    /// Every zone with a cave generator is walked exactly like the
    /// [`CavePopulator`](crate::chunk::populator::CavePopulator): each cave
    /// type's entry-point generator is collected over `rect`, and a candidate
    /// is kept only when the shared predicate's rules accept it (same zone by
    /// identity, entry threshold, height condition — the helpers the
    /// populator's `entrance_height` composes). Candidates are filtered to
    /// the rect after collection.
    ///
    /// The query never calls `get_cave`: this is the cheap placement pass,
    /// not cave generation. It is still not O(1) — the point generators and
    /// per-candidate zone/height lookups scale with the rectangle's area.
    ///
    /// # Panics
    ///
    /// Panics when `rect` is not normalized (`min > max` on an axis): that is
    /// a caller bug, not a data condition.
    #[must_use]
    pub fn cave_entrances_in(&self, seed: i32, rect: BlockRect) -> Vec<FeaturePlacement> {
        assert!(
            rect.min_x <= rect.max_x && rect.min_z <= rect.max_z,
            "the rect is normalized (min <= max on both axes)"
        );

        let zones: Vec<_> = self.get_zone_pattern_provider().get_zones().to_vec();
        let mut placements = Vec::new();
        // Multiple cave types (or overlapping padded collections) can report
        // the same candidate; the cave type name is part of the identity, so
        // two different cave types at one column stay two entrances.
        let mut seen: HashSet<(String, i32, i32)> = HashSet::new();
        let mut columns: HashMap<(i32, i32), ColumnMemo> = HashMap::new();

        for zone in zones {
            let Some(cave_generator) = &zone.cave_generator else {
                continue;
            };
            let cave_types: Vec<_> = cave_generator.get_cave_types().to_vec();
            for cave_type in cave_types {
                let entry_points = cave_type.get_entry_point_generator();
                // `collect` walks whole cells; padding by the point
                // generator's interval (plus one) covers every cell that can
                // contain a point inside the caller's rect, without the
                // populator's `maximum_size` reach (that reach exists to
                // find caves crossing into a chunk, not anchors inside a
                // query rect).
                let interval = entry_points.get_interval();
                let pad = if interval.is_finite() {
                    interval.max(1.0).ceil() + 1.0
                } else {
                    1.0
                };
                entry_points.collect(
                    seed,
                    f64::from(rect.min_x) - pad,
                    f64::from(rect.min_z) - pad,
                    f64::from(rect.max_x) + pad,
                    f64::from(rect.max_z) + pad,
                    &mut |dx: f64, dz: f64| {
                        let x = math_util::floor(dx);
                        let z = math_util::floor(dz);
                        if x < rect.min_x || x > rect.max_x || z < rect.min_z || z > rect.max_z {
                            return;
                        }
                        if columns.len() >= COLUMN_MEMO_CAP {
                            columns.clear();
                        }
                        let column = columns.entry((x, z)).or_insert_with(|| ColumnMemo {
                            // The predicate only needs the zone identity, so
                            // the cheap zone stage suffices; the full
                            // zone+biome result would run the biome and
                            // height-context work for nothing.
                            zone: self.get_zone_pattern_generator(seed).generate(
                                seed,
                                f64::from(x),
                                f64::from(z),
                            ),
                            height: None,
                        });
                        if !entrance_zone_result(seed, x, z, &column.zone, &zone, &cave_type) {
                            return;
                        }
                        let height = match column.height {
                            Some(height) => height,
                            None => {
                                let height = self.get_height(seed, x, z);
                                column.height = Some(height);
                                height
                            }
                        };
                        if !entrance_height_matches(seed, x, z, height, &cave_type) {
                            return;
                        }
                        if seen.insert((cave_type.get_name().to_string(), x, z)) {
                            placements.push(cave_placement(&zone, &cave_type, x, height, z));
                        }
                    },
                );
            }
        }

        placements.sort_by(|left, right| {
            (
                left.position.0,
                left.position.2,
                left.position.1,
                left.name.as_str(),
            )
                .cmp(&(
                    right.position.0,
                    right.position.2,
                    right.position.1,
                    right.name.as_str(),
                ))
        });
        placements
    }
}

/// Builds the plain placement for one accepted entrance.
///
/// The bounds are a one-column marker box (the surface column plus two blocks
/// above it): an entrance has no prefab footprint, so a size filter above one
/// block correctly excludes it.
fn cave_placement(
    zone: &Zone,
    cave_type: &CaveType,
    x: i32,
    height: i32,
    z: i32,
) -> FeaturePlacement {
    FeaturePlacement {
        kind: PlacementKind::CaveEntrance,
        name: cave_type.get_name().to_string(),
        category: zone.name.clone(),
        position: (x, height, z),
        bounds: Some(BlockBounds {
            min_x: x,
            min_y: height,
            min_z: z,
            max_x: x,
            max_y: height.saturating_add(2),
            max_z: z,
        }),
        // Entrances have no prefab rotation.
        rotation: PrefabRotation::VALUES[0],
        show_on_map: true,
        spawn_location: false,
    }
}
