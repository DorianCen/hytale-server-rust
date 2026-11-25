//! Integration tests of the headless feature query (`ChunkGenerator::features_in`).
//!
//! Every test loads the real Default world through the public loader chain and
//! uses only the public query API. The golden-parity test against the chunk
//! placement pass lives in the crate's `query` module (it needs `pub(crate)`
//! internals); these tests cover the public contract and the rect/filter
//! structure.
//!
//! The Default world's prefab containers reference the base-game asset pack,
//! so the tests skip when that pack is absent (CI).

use std::path::PathBuf;
use std::rc::Rc;

use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::prefab::prefab_rotation::PrefabRotation;
use worldgen_hytale::query::{
    BlockBounds, BlockRect, FeaturePlacement, PlacementFilter, PlacementKind, PlacementKindMask,
};

/// The Default world's prefab containers reference the base-game asset pack,
/// which is git-ignored and absent in CI. Skip these end-to-end tests there.
fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../Assets/extracted/Server/Prefabs")
        .is_dir()
}

/// The real Default world — the same loader chain the Java dumper and the
/// vanilla provider use.
fn default_world() -> Rc<ChunkGenerator> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/Default");
    let config = worldgen_hytale::world_gen_config::WorldGenConfig::new(
        root,
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader =
        worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new("ChunkGenerator"),
            config,
        );
    loader.load()
}

/// A total order over placements so that comparisons are independent of the
/// query's own output order.
fn sort_placements(placements: &mut [FeaturePlacement]) {
    placements.sort_by(|a, b| {
        let bounds = |placement: &FeaturePlacement| {
            placement.bounds.map_or((0, 0, 0, 0, 0, 0), |bounds| {
                (
                    bounds.min_x,
                    bounds.min_y,
                    bounds.min_z,
                    bounds.max_x,
                    bounds.max_y,
                    bounds.max_z,
                )
            })
        };
        (
            a.name.as_str(),
            a.category.as_str(),
            a.position.0,
            a.position.1,
            a.position.2,
            bounds(a),
            a.rotation.get_degrees(),
            a.show_on_map,
            a.spawn_location,
        )
            .cmp(&(
                b.name.as_str(),
                b.category.as_str(),
                b.position.0,
                b.position.1,
                b.position.2,
                bounds(b),
                b.rotation.get_degrees(),
                b.show_on_map,
                b.spawn_location,
            ))
    });
}

/// The footprint of a placement as `min_size` measures it (`None` without
/// bounds).
fn footprint(placement: &FeaturePlacement) -> Option<i64> {
    placement.bounds.map(|bounds| {
        let size_x = i64::from(bounds.max_x) - i64::from(bounds.min_x) + 1;
        let size_z = i64::from(bounds.max_z) - i64::from(bounds.min_z) + 1;
        size_x.max(size_z)
    })
}

fn unique_filter() -> PlacementFilter {
    PlacementFilter {
        kinds: PlacementKindMask::UNIQUE_PREFAB,
        ..PlacementFilter::default()
    }
}

fn spawn_filter() -> PlacementFilter {
    PlacementFilter {
        kinds: PlacementKindMask::SPAWN,
        ..PlacementFilter::default()
    }
}

/* ============================================================================================================ */
/*                                          the per-seed lists                                                  */
/* ============================================================================================================ */

#[test]
fn query_unique_prefabs_match_the_per_seed_list() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let entries = generator.get_unique_prefabs(seed);
    assert!(
        !entries.is_empty(),
        "the Default world seed {seed} has no unique prefabs; the test would be vacuous"
    );

    let mut expected = Vec::new();
    for entry in &entries {
        let position = entry.get_position();
        expected.push(FeaturePlacement {
            kind: PlacementKind::UniquePrefab,
            name: entry.get_name().to_string(),
            category: entry.get_category().name.clone(),
            position,
            // The entry bounds are absolute; the y range is the placement
            // origin (the entry records no exact y extent).
            bounds: Some(BlockBounds {
                min_x: entry.get_low_bound_x(),
                min_y: position.1,
                min_z: entry.get_low_bound_z(),
                max_x: entry.get_high_bound_x(),
                max_y: position.1,
                max_z: entry.get_high_bound_z(),
            }),
            rotation: entry.get_rotation(),
            show_on_map: entry.is_show_on_map(),
            spawn_location: entry.is_spawn_location(),
        });
    }

    // A rectangle spanning every entry's bounds.
    let rect = BlockRect {
        min_x: expected
            .iter()
            .filter_map(|placement| placement.bounds.map(|bounds| bounds.min_x))
            .min()
            .expect("the unique prefab list is non-empty"),
        min_z: expected
            .iter()
            .filter_map(|placement| placement.bounds.map(|bounds| bounds.min_z))
            .min()
            .expect("the unique prefab list is non-empty"),
        max_x: expected
            .iter()
            .filter_map(|placement| placement.bounds.map(|bounds| bounds.max_x))
            .max()
            .expect("the unique prefab list is non-empty"),
        max_z: expected
            .iter()
            .filter_map(|placement| placement.bounds.map(|bounds| bounds.max_z))
            .max()
            .expect("the unique prefab list is non-empty"),
    };

    let mut actual = generator.features_in(seed, rect, &unique_filter());
    sort_placements(&mut expected);
    sort_placements(&mut actual);
    assert_eq!(
        actual, expected,
        "the unique prefab query must match the per-seed list"
    );
}

#[test]
fn query_spawn_points_match_the_per_seed_list() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let transforms = generator.get_spawn_points(seed);
    assert!(
        !transforms.is_empty(),
        "the Default world seed {seed} has no spawn points; the test would be vacuous"
    );

    // Floor exactly like the query and take a rectangle generously around
    // every spawn point.
    let positions: Vec<(i32, i32, i32)> = transforms
        .iter()
        .map(|transform| {
            let position = transform.get_position();
            (
                hytale_math::math_util::floor(position.x),
                hytale_math::math_util::floor(position.y),
                hytale_math::math_util::floor(position.z),
            )
        })
        .collect();
    let rect = BlockRect {
        min_x: positions
            .iter()
            .map(|position| position.0)
            .min()
            .unwrap_or(0)
            - 2,
        min_z: positions
            .iter()
            .map(|position| position.2)
            .min()
            .unwrap_or(0)
            - 2,
        max_x: positions
            .iter()
            .map(|position| position.0)
            .max()
            .unwrap_or(0)
            + 2,
        max_z: positions
            .iter()
            .map(|position| position.2)
            .max()
            .unwrap_or(0)
            + 2,
    };

    let expected: Vec<FeaturePlacement> = positions
        .iter()
        .filter(|(x, _, z)| {
            *x >= rect.min_x && *x <= rect.max_x && *z >= rect.min_z && *z <= rect.max_z
        })
        .map(|&(x, y, z)| FeaturePlacement {
            kind: PlacementKind::Spawn,
            name: "spawn".to_string(),
            category: "spawn".to_string(),
            position: (x, y, z),
            bounds: None,
            // Spawn transforms are yaw rotations, not prefab rotations.
            rotation: PrefabRotation::VALUES[0],
            show_on_map: true,
            spawn_location: true,
        })
        .collect();

    let mut actual = generator.features_in(seed, rect, &spawn_filter());
    let mut expected = expected;
    sort_placements(&mut expected);
    sort_placements(&mut actual);
    assert_eq!(
        actual, expected,
        "the spawn query must match the per-seed spawn list"
    );
}

/* ============================================================================================================ */
/*                                          rect independence                                                  */
/* ============================================================================================================ */

#[test]
fn query_small_rect_returns_a_subset_of_a_large_rect() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    // Cave entrances have their own dense query and fixture tests; including
    // the cave pass in this 320-block subset check would time out the fast
    // CI tier for a property the cave tests pin separately.
    let filter = PlacementFilter {
        kinds: PlacementKindMask::all().without(PlacementKind::CaveEntrance),
        ..PlacementFilter::default()
    };

    let small_rect = BlockRect {
        min_x: 0,
        min_z: 0,
        max_x: 63,
        max_z: 63,
    };
    let large_rect = BlockRect {
        min_x: -32,
        min_z: -32,
        max_x: 95,
        max_z: 95,
    };

    let small = generator.features_in(seed, small_rect, &filter);
    let large = generator.features_in(seed, large_rect, &filter);
    assert!(
        !small.is_empty(),
        "the small rect has no placements; the subset property would be vacuous"
    );

    // No duplicates: the dedup by candidate index (and the one-entry-per-
    // unique/spawn mapping) must hold.
    for (i, placement) in small.iter().enumerate() {
        for other in &small[i + 1..] {
            assert_ne!(
                placement, other,
                "the small rect returned a duplicate placement: {placement:?}"
            );
        }
    }

    for placement in &small {
        assert!(
            large.contains(placement),
            "a placement of the small rect is missing from the large rect: {placement:?}"
        );
    }
}

#[test]
fn query_negative_coordinates_and_chunk_edges() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    // The cave pass is dense enough to dominate this negative-rect check;
    // the cave query has its own edge/negative tests.
    let filter = PlacementFilter {
        kinds: PlacementKindMask::all().without(PlacementKind::CaveEntrance),
        ..PlacementFilter::default()
    };

    // Straddles negative chunk borders on both axes.
    let rect = BlockRect {
        min_x: -80,
        min_z: -48,
        max_x: -17,
        max_z: -15,
    };
    let bigger = BlockRect {
        min_x: -112,
        min_z: -80,
        max_x: 47,
        max_z: 47,
    };

    let first = generator.features_in(seed, rect, &filter);
    let second = generator.features_in(seed, rect, &filter);
    assert_eq!(
        first, second,
        "a negative-coordinate rect must be deterministic"
    );
    assert!(
        !first.is_empty(),
        "the negative rect has no placements; the subset property would be vacuous"
    );

    let large = generator.features_in(seed, bigger, &filter);
    for placement in &first {
        assert!(
            large.contains(placement),
            "a placement of the negative rect is missing from the covering rect: {placement:?}"
        );
    }
}

#[test]
fn query_is_deterministic() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let rect = BlockRect {
        min_x: 0,
        min_z: 0,
        max_x: 63,
        max_z: 63,
    };
    // A filter that exercises every rule at once.
    let filter = PlacementFilter {
        kinds: PlacementKindMask::UNIQUE_PREFAB | PlacementKindMask::BIOME_PREFAB,
        categories: None,
        name_contains: Some("a".to_string()),
        min_size: Some(3),
    };

    let first = generator.features_in(seed, rect, &filter);
    let second = generator.features_in(seed, rect, &filter);
    assert_eq!(
        first, second,
        "the query must be deterministic, including the order"
    );
}

/* ============================================================================================================ */
/*                                          filters                                                           */
/* ============================================================================================================ */

/// The rect of the filter tests: a populated area with only biome prefabs.
fn filter_rect() -> BlockRect {
    BlockRect {
        min_x: 0,
        min_z: 0,
        max_x: 95,
        max_z: 95,
    }
}

#[test]
fn query_filters_kinds() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let rect = filter_rect();
    let all = generator.features_in(seed, rect, &PlacementFilter::default());
    assert!(!all.is_empty(), "the filter rect is empty");

    let filter = PlacementFilter {
        kinds: PlacementKindMask::BIOME_PREFAB,
        ..PlacementFilter::default()
    };
    let actual = generator.features_in(seed, rect, &filter);
    let expected: Vec<FeaturePlacement> = all
        .iter()
        .filter(|placement| placement.kind == PlacementKind::BiomePrefab)
        .cloned()
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn query_filters_category() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let rect = filter_rect();
    let all = generator.features_in(seed, rect, &PlacementFilter::default());
    let category = all
        .first()
        .expect("the filter rect is empty")
        .category
        .clone();

    let filter = PlacementFilter {
        categories: Some(vec![category.clone()]),
        ..PlacementFilter::default()
    };
    let actual = generator.features_in(seed, rect, &filter);
    let expected: Vec<FeaturePlacement> = all
        .iter()
        .filter(|placement| placement.category == category)
        .cloned()
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn query_filters_name_contains() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let rect = filter_rect();
    let all = generator.features_in(seed, rect, &PlacementFilter::default());
    assert!(!all.is_empty(), "the filter rect is empty");
    // A common lowercase fragment; matching is case-insensitive.
    let needle = "stage";

    let filter = PlacementFilter {
        name_contains: Some(needle.to_string()),
        ..PlacementFilter::default()
    };
    let actual = generator.features_in(seed, rect, &filter);
    let expected: Vec<FeaturePlacement> = all
        .iter()
        .filter(|placement| placement.name.to_lowercase().contains(needle))
        .cloned()
        .collect();
    assert_eq!(actual, expected);
    assert!(
        !expected.is_empty() && expected.len() < all.len(),
        "the name filter should select some, but not all, placements"
    );
}

#[test]
fn query_filters_min_size() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let seed = 0;
    let rect = filter_rect();
    let all = generator.features_in(seed, rect, &PlacementFilter::default());
    let min_size = 10;

    let filter = PlacementFilter {
        min_size: Some(min_size),
        ..PlacementFilter::default()
    };
    let actual = generator.features_in(seed, rect, &filter);
    let expected: Vec<FeaturePlacement> = all
        .iter()
        .filter(|placement| footprint(placement).is_some_and(|size| size >= i64::from(min_size)))
        .cloned()
        .collect();
    assert_eq!(actual, expected);
    assert!(
        !expected.is_empty() && expected.len() < all.len(),
        "the min-size filter should select some, but not all, placements"
    );
}
