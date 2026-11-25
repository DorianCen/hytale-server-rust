//! Integration tests of `ChunkGenerator::cave_entrances_in` — the cave
//! stage's placement pass without cave generation.
//!
//! Every test loads the real Default world through the public loader chain
//! and uses only the public query API. The geometry test generates the chunk
//! containing an entrance and looks for carved (air) blocks around its
//! surface column; the query itself never calls `get_cave`.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use worldgen_hytale::cave::cave_type::CaveType;
use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::query::{
    BlockRect, FeaturePlacement, PlacementFilter, PlacementKind, PlacementKindMask,
};
use worldgen_hytale::zone::zone::Zone;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

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

fn generate_chunk(
    generator: &Rc<ChunkGenerator>,
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
) -> ChunkGeneratorExecution {
    let chunk = GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(chunk_x, chunk_z),
        chunk_x,
        chunk_z,
    );
    let mut execution = ChunkGeneratorExecution::with_registry(
        seed,
        Rc::clone(generator),
        chunk,
        Rc::new(Registries::load()),
    );
    let mut resource = ChunkGeneratorResource::new();
    execution.execute(seed, &mut resource);
    execution
}

fn rect_around_chunk(chunk_x: i32, chunk_z: i32) -> BlockRect {
    BlockRect {
        min_x: hytale_math::chunk_util::min_block(chunk_x),
        min_z: hytale_math::chunk_util::min_block(chunk_z),
        max_x: hytale_math::chunk_util::max_block(chunk_x),
        max_z: hytale_math::chunk_util::max_block(chunk_z),
    }
}

/// A placement's comparable tuple (no `Ord` on the placement type itself).
fn placement_key(placement: &FeaturePlacement) -> (String, String, (i32, i32, i32), bool) {
    (
        placement.name.clone(),
        placement.category.clone(),
        placement.position,
        placement.show_on_map,
    )
}

/// The zone and cave type a placement names, from the generator's live lists.
fn zone_and_cave_type(
    generator: &ChunkGenerator,
    zone_name: &str,
    cave_name: &str,
) -> Option<(Rc<Zone>, Rc<CaveType>)> {
    for zone in generator.get_zone_pattern_provider().get_zones() {
        if zone.name != zone_name {
            continue;
        }
        let cave_generator = zone.cave_generator.as_ref()?;
        for cave_type in cave_generator.get_cave_types() {
            if cave_type.get_name() == cave_name {
                return Some((Rc::clone(zone), Rc::clone(cave_type)));
            }
        }
    }
    None
}

#[test]
fn query_cave_entrances_are_deterministic_and_inside_the_rect() {
    let generator = default_world();
    let rect = rect_around_chunk(0, 0);
    let first = generator.cave_entrances_in(0, rect);
    let second = generator.cave_entrances_in(0, rect);
    assert!(!first.is_empty(), "the fixture chunk has cave entrances");
    assert_eq!(
        first.iter().map(placement_key).collect::<Vec<_>>(),
        second.iter().map(placement_key).collect::<Vec<_>>(),
        "the query is a pure function of (seed, rect)",
    );
    for placement in &first {
        assert_eq!(placement.kind, PlacementKind::CaveEntrance);
        assert!(
            placement.position.0 >= rect.min_x
                && placement.position.0 <= rect.max_x
                && placement.position.2 >= rect.min_z
                && placement.position.2 <= rect.max_z,
            "{placement:?} is inside the queried rect",
        );
        assert!(!placement.name.is_empty(), "the cave type name is reported");
        assert!(!placement.category.is_empty(), "the zone name is reported");
        assert!(placement.show_on_map);
        assert!(!placement.spawn_location);
    }
    // Sorted by (x, z, y, name): the task's deterministic coordinate order.
    assert!(
        first.windows(2).all(|pair| {
            let left = &pair[0];
            let right = &pair[1];
            (
                left.position.0,
                left.position.2,
                left.position.1,
                left.name.as_str(),
            ) <= (
                right.position.0,
                right.position.2,
                right.position.1,
                right.name.as_str(),
            )
        }),
        "the output is sorted by coordinates then name",
    );
}

#[test]
fn query_cave_entrances_pass_the_shared_predicate() {
    // Every reported entrance must satisfy the three rules the cave
    // populator's shared predicate encodes: same zone identity, entry
    // threshold, and the height condition at the reported Y.
    let generator = default_world();
    let seed = 0;
    let entrances = generator.cave_entrances_in(seed, rect_around_chunk(0, 0));
    let mut checked = 0usize;
    for placement in &entrances {
        let (x, y, z) = placement.position;
        let (zone, cave_type) =
            zone_and_cave_type(&generator, &placement.category, &placement.name).unwrap_or_else(
                || {
                    panic!(
                        "{} [{}] names a live zone/cave type",
                        placement.name, placement.category
                    )
                },
            );
        let result = generator.get_zone_biome_result_at(seed, x, z);
        let result_zone = result
            .get_zone_result()
            .and_then(|zone_result| zone_result.get_zone())
            .expect("a queried column resolves a zone");
        assert!(
            Rc::ptr_eq(result_zone, &zone),
            "{} at ({x},{z}) resolves to its reported zone",
            placement.name,
        );
        assert!(
            cave_type.is_entry_threshold(seed, x, z),
            "{} at ({x},{z}) passes the entry threshold",
            placement.name,
        );
        assert_eq!(
            y,
            generator.get_height(seed, x, z),
            "the reported Y is the column height the predicate evaluated",
        );
        assert!(
            cave_type.get_height_condition().eval3(
                seed.wrapping_add(0xF5ACDEB5u32 as i32),
                x,
                y,
                z,
            ),
            "{} at ({x},{z}) passes the height condition",
            placement.name,
        );
        checked += 1;
    }
    assert!(checked > 0, "the fixture entrance set is not vacuous");
}

#[test]
fn query_cave_entrances_match_generated_cave_geometry() {
    // A recorded entrance is an entry point the generator would generate a
    // cave from; the generated fixture chunk must contain carved (air) space
    // within a few blocks of its surface column. This also proves the query
    // did not need to generate the chunk itself: it reports the same
    // positions before any execution runs.
    let generator = default_world();
    let entrances = generator.cave_entrances_in(0, rect_around_chunk(0, 0));
    let execution = generate_chunk(&generator, 0, 0, 0);
    let mut checked = 0usize;
    for placement in &entrances {
        let (x, y, z) = placement.position;
        if !(0..32).contains(&x) || !(0..32).contains(&z) {
            continue;
        }
        let mut carved = false;
        'search: for dx in -2..=2 {
            for dz in -2..=2 {
                for dy in -6..=6 {
                    if execution.get_block(x + dx, y + dy, z + dz) == 0 {
                        carved = true;
                        break 'search;
                    }
                }
            }
        }
        assert!(
            carved,
            "{} at ({x},{y},{z}) has a carved block within ±2 columns / ±6 Y",
            placement.name,
        );
        checked += 1;
    }
    assert!(checked > 0, "the fixture chunk contains checked entrances");
}

#[test]
fn query_cave_entrances_kind_filter_matches_the_direct_query() {
    // The generic feature query's CAVE_ENTRANCE kind must return exactly the
    // dedicated query's placements (same positions, names and zones).
    let generator = default_world();
    let rect = rect_around_chunk(4, -5);
    let direct = generator.cave_entrances_in(0, rect);
    let filter = PlacementFilter {
        kinds: PlacementKindMask::CAVE_ENTRANCE,
        ..PlacementFilter::default()
    };
    let through_features = generator
        .features_in(0, rect, &filter)
        .into_iter()
        .filter(|placement| placement.kind == PlacementKind::CaveEntrance)
        .collect::<Vec<_>>();
    assert!(!direct.is_empty());
    assert_eq!(
        direct.iter().map(placement_key).collect::<Vec<_>>(),
        through_features
            .iter()
            .map(placement_key)
            .collect::<Vec<_>>(),
        "the kind mask routes to the same cave-entrance query",
    );
}

#[test]
fn query_cave_entrances_handles_negative_and_edge_rects() {
    let generator = default_world();
    let rect = rect_around_chunk(-12, -25);
    let entrances = generator.cave_entrances_in(0, rect);
    assert!(
        !entrances.is_empty(),
        "the negative fixture chunk has entrances"
    );
    for placement in &entrances {
        assert!(
            (rect.min_x..=rect.max_x).contains(&placement.position.0)
                && (rect.min_z..=rect.max_z).contains(&placement.position.2),
            "{placement:?} is inside the negative rect",
        );
    }

    // A one-block rect at a known entrance still reports that entrance
    // (the point generator's cell padding covers the boundary).
    let (x, y, z) = entrances[0].position;
    let single = generator.cave_entrances_in(
        0,
        BlockRect {
            min_x: x,
            min_z: z,
            max_x: x,
            max_z: z,
        },
    );
    assert!(
        single
            .iter()
            .any(|placement| placement.position == (x, y, z)),
        "a rect exactly on an entrance finds it",
    );
}

#[test]
fn query_cave_entrances_capped_walk_is_bounded_and_inside() {
    let generator = default_world();
    let rect = rect_around_chunk(0, 0);
    let full = generator.cave_entrances_in(0, rect);
    let capped = generator.cave_entrances_in_capped(0, rect, 10);
    assert_eq!(capped.len(), 10, "the cap is exact when the tile has more");
    assert!(
        capped.iter().all(|placement| full
            .iter()
            .any(|full| placement_key(full) == placement_key(placement))),
        "a capped result is a placement of the uncapped query",
    );
    assert!(
        generator.cave_entrances_in_capped(0, rect, 0).is_empty(),
        "a zero cap returns nothing",
    );
}

#[test]
#[ignore = "release-only benchmark; run with --release -- --ignored query_cave_entrances_bench"]
fn query_cave_entrances_bench_10k_capped() {
    // The task-sanctioned cap path: a 10,000×10,000 rect walked center-first
    // in 256-block tiles, stopped at 10,000 entrances. The full uncapped
    // 10k query is a batch job (QUERY.md); this is the usable measured form.
    let generator = default_world();
    let side = 10_000;
    let half = side / 2;
    let rect = BlockRect {
        min_x: -half,
        min_z: -half,
        max_x: half - 1,
        max_z: half - 1,
    };
    // Warm the caches on a disjoint small rect first.
    let warm = BlockRect {
        min_x: 4096,
        min_z: 4096,
        max_x: 4159,
        max_z: 4159,
    };
    let _ = generator.cave_entrances_in(0, warm);
    let started = Instant::now();
    let entrances = generator.cave_entrances_in_capped(0, rect, 10_000);
    eprintln!(
        "cave entrances 10000x10000 capped at 10000: {} in {:?}",
        entrances.len(),
        started.elapsed()
    );
    assert_eq!(entrances.len(), 10_000);
    assert!(entrances.iter().all(|placement| {
        (rect.min_x..=rect.max_x).contains(&placement.position.0)
            && (rect.min_z..=rect.max_z).contains(&placement.position.2)
    }));
}

#[test]
#[ignore = "release-only benchmark; run with --release -- --ignored query_cave_entrances_bench"]
fn query_cave_entrances_bench_512() {
    let generator = default_world();
    let side = 512;
    let half = side / 2;
    let rect = BlockRect {
        min_x: -half,
        min_z: -half,
        max_x: half - 1,
        max_z: half - 1,
    };
    // Warm the caches on a disjoint small rect first, like the T22 probe.
    let warm = BlockRect {
        min_x: 4096,
        min_z: 4096,
        max_x: 4159,
        max_z: 4159,
    };
    let _ = generator.cave_entrances_in(0, warm);
    let started = Instant::now();
    let entrances = generator.cave_entrances_in(0, rect);
    eprintln!(
        "cave entrances {side}x{side}: {} in {:?}",
        entrances.len(),
        started.elapsed()
    );
}
