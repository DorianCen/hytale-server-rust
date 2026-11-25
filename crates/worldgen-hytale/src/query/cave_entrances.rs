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
//! # Tiled, center-first walk
//!
//! The query walks the rect in [`CAVE_QUERY_TILE`]-block tiles, ring by ring
//! from the tile containing the rect's center. Tiling bounds the per-tile
//! column memo and makes a **capped** query possible:
//! [`ChunkGenerator::cave_entrances_in_capped`] stops after `max_results`
//! entries instead of forcing a 10,000-block rect to enumerate every dense
//! entrance. The full [`ChunkGenerator::cave_entrances_in`] is the same walk
//! with no cap and a final coordinate sort.
//!
//! # Anchor rect, not the populator's padded rect
//!
//! The populator collects over a chunk padded by each cave type's
//! `maximum_size` because a cave whose entry point is outside the chunk can
//! still carve into it. This query reports **entry points**, so a point
//! outside a tile is not a result; collecting over the tile padded by the
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
//! snapshot per distinct candidate column for the lifetime of one tile. Its
//! size is bounded by [`COLUMN_MEMO_CAP`]; the map is cleared wholesale when
//! the cap is reached, so a huge tile trades reuse for bounded memory instead
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

/// The side of one cave-query tile, in blocks.
///
/// A tile bounds the column memo and is the granularity of the capped walk.
/// The explorer's overlay cap is the same size, so an overlay query is one
/// tile.
pub const CAVE_QUERY_TILE: i32 = 256;

/// How many distinct candidate columns one tile memoizes before clearing the
/// map. A 256×256 tile has at most 65,536 columns; 262,144 is a generous
/// bound for padded collection and keeps a huge rect from growing memory.
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
    /// This is [`Self::cave_entrances_in_capped`] with no result cap. The
    /// query never calls `get_cave`: it is the cheap placement pass, not cave
    /// generation. It is still not O(1) — the point generators and
    /// per-candidate zone/height lookups scale with the rectangle's area.
    ///
    /// # Panics
    ///
    /// Panics when `rect` is not normalized (`min > max` on an axis): that is
    /// a caller bug, not a data condition.
    #[must_use]
    pub fn cave_entrances_in(&self, seed: i32, rect: BlockRect) -> Vec<FeaturePlacement> {
        self.cave_entrances_in_capped(seed, rect, usize::MAX)
    }

    /// The cave entrances inside `rect`, walked in center-first
    /// [`CAVE_QUERY_TILE`] tiles and stopped after `max_results` placements.
    ///
    /// The cap makes a huge (for example 10,000×10,000) rect usable as a
    /// sampling/batch query: the walk covers the tiles nearest the rect
    /// center first and stops when the cap is reached. The result is sorted
    /// by `(x, z, y, name)` like the uncapped query; it is a prefix of the
    /// tiled walk, not necessarily the globally nearest `max_results`.
    /// `max_results == 0` returns an empty list.
    ///
    /// # Panics
    ///
    /// Panics when `rect` is not normalized (`min > max` on an axis): that is
    /// a caller bug, not a data condition.
    #[must_use]
    pub fn cave_entrances_in_capped(
        &self,
        seed: i32,
        rect: BlockRect,
        max_results: usize,
    ) -> Vec<FeaturePlacement> {
        assert!(
            rect.min_x <= rect.max_x && rect.min_z <= rect.max_z,
            "the rect is normalized (min <= max on both axes)"
        );
        if max_results == 0 {
            return Vec::new();
        }

        let zones: Vec<_> = self.get_zone_pattern_provider().get_zones().to_vec();
        let mut placements = Vec::new();
        let mut seen: HashSet<(String, i32, i32)> = HashSet::new();
        let start_tx = rect.min_x.div_euclid(CAVE_QUERY_TILE);
        let end_tx = rect.max_x.div_euclid(CAVE_QUERY_TILE);
        let start_tz = rect.min_z.div_euclid(CAVE_QUERY_TILE);
        let end_tz = rect.max_z.div_euclid(CAVE_QUERY_TILE);
        let center_tx = (i64::from(rect.min_x) + i64::from(rect.max_x)).div_euclid(2);
        let center_tz = (i64::from(rect.min_z) + i64::from(rect.max_z)).div_euclid(2);
        let center_tx = i32::try_from(center_tx.div_euclid(i64::from(CAVE_QUERY_TILE)))
            .expect("the rect center is inside the i32 block space");
        let center_tz = i32::try_from(center_tz.div_euclid(i64::from(CAVE_QUERY_TILE)))
            .expect("the rect center is inside the i32 block space");
        let max_ring = (center_tx - start_tx)
            .max(end_tx - center_tx)
            .max(center_tz - start_tz)
            .max(end_tz - center_tz)
            .max(0);

        'rings: for ring in 0..=max_ring {
            for tx in (center_tx - ring)..=(center_tx + ring) {
                if tx < start_tx || tx > end_tx {
                    continue;
                }
                for tz in (center_tz - ring)..=(center_tz + ring) {
                    if tz < start_tz || tz > end_tz {
                        continue;
                    }
                    if ring > 0
                        && tx.abs_diff(center_tx) != ring as u32
                        && tz.abs_diff(center_tz) != ring as u32
                    {
                        continue;
                    }
                    let Some(tile) = tile_rect(rect, tx, tz) else {
                        continue;
                    };
                    collect_tile(
                        self,
                        seed,
                        tile,
                        &zones,
                        &mut placements,
                        &mut seen,
                        max_results,
                    );
                    if placements.len() >= max_results {
                        break 'rings;
                    }
                }
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
        placements.truncate(max_results);
        placements
    }
}

/// The part of `(tx, tz)`'s [`CAVE_QUERY_TILE`] square that intersects the
/// caller's rect; [`None`] when that tile is entirely outside the rect.
fn tile_rect(rect: BlockRect, tx: i32, tz: i32) -> Option<BlockRect> {
    let tile = i64::from(CAVE_QUERY_TILE);
    let min_x = (i64::from(tx) * tile).max(i64::from(rect.min_x));
    let max_x = (i64::from(tx) * tile + tile - 1).min(i64::from(rect.max_x));
    let min_z = (i64::from(tz) * tile).max(i64::from(rect.min_z));
    let max_z = (i64::from(tz) * tile + tile - 1).min(i64::from(rect.max_z));
    if min_x > max_x || min_z > max_z {
        return None;
    }
    Some(BlockRect {
        min_x: i32::try_from(min_x).expect("tile coordinates are inside the rect"),
        min_z: i32::try_from(min_z).expect("tile coordinates are inside the rect"),
        max_x: i32::try_from(max_x).expect("tile coordinates are inside the rect"),
        max_z: i32::try_from(max_z).expect("tile coordinates are inside the rect"),
    })
}

/// Collects one tile's entrances into the query-wide vectors, stopping short
/// once `max_results` is reached (the point generator itself cannot be
/// interrupted; the callback skips the remaining work).
fn collect_tile(
    generator: &ChunkGenerator,
    seed: i32,
    rect: BlockRect,
    zones: &[std::rc::Rc<Zone>],
    placements: &mut Vec<FeaturePlacement>,
    seen: &mut HashSet<(String, i32, i32)>,
    max_results: usize,
) {
    let mut columns: HashMap<(i32, i32), ColumnMemo> = HashMap::new();
    for zone in zones {
        let Some(cave_generator) = &zone.cave_generator else {
            continue;
        };
        let cave_types: Vec<_> = cave_generator.get_cave_types().to_vec();
        for cave_type in cave_types {
            if placements.len() >= max_results {
                return;
            }
            let entry_points = cave_type.get_entry_point_generator();
            // `collect` walks whole cells; padding by the point generator's
            // interval (plus one) covers every cell that can contain a point
            // inside the tile, without the populator's `maximum_size` reach
            // (that reach exists to find caves crossing into a chunk, not
            // anchors inside a query tile).
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
                    if placements.len() >= max_results {
                        return;
                    }
                    let x = math_util::floor(dx);
                    let z = math_util::floor(dz);
                    if x < rect.min_x || x > rect.max_x || z < rect.min_z || z > rect.max_z {
                        return;
                    }
                    if columns.len() >= COLUMN_MEMO_CAP {
                        columns.clear();
                    }
                    let column = columns.entry((x, z)).or_insert_with(|| ColumnMemo {
                        // The predicate only needs the zone identity, so the
                        // cheap zone stage suffices; the full zone+biome
                        // result would run the biome and height-context work
                        // for nothing.
                        zone: generator.get_zone_pattern_generator(seed).generate(
                            seed,
                            f64::from(x),
                            f64::from(z),
                        ),
                        height: None,
                    });
                    if !entrance_zone_result(seed, x, z, &column.zone, zone, &cave_type) {
                        return;
                    }
                    let height = match column.height {
                        Some(height) => height,
                        None => {
                            let height = generator.get_height(seed, x, z);
                            column.height = Some(height);
                            height
                        }
                    };
                    if !entrance_height_matches(seed, x, z, height, &cave_type) {
                        return;
                    }
                    if seen.insert((cave_type.get_name().to_string(), x, z)) {
                        placements.push(cave_placement(zone, &cave_type, x, height, z));
                    }
                },
            );
        }
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
