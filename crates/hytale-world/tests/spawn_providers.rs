//! T75: Java `ISpawnProvider` fixtures.

use std::path::PathBuf;

use hytale_world::player_data::Position;
use hytale_world::spawn::SpawnProvider;
use hytale_world::world_config::WorldUuid;

fn fixture(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/spawn_providers")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).expect("provider fixture"))
        .expect("provider fixture JSON")
}

fn uuid(value: &str) -> WorldUuid {
    value.parse().expect("uuid")
}

#[test]
fn global_provider_returns_one_static_spawn() {
    let provider = SpawnProvider::from_value(&fixture("global.json")).expect("global provider");
    let point = provider
        .spawn_point(uuid("00000000-0000-0000-0000-000000000001"))
        .expect("spawn");
    assert_eq!(point.position.x, 0.5);
    assert_eq!(point.position.y, 149.0);
    assert_eq!(point.position.z, 290.5);
    assert!(provider.is_within_spawn_distance(
        &Position {
            x: 0.5,
            y: 149.0,
            z: 290.5
        },
        1.0
    ));
    assert!(!provider.is_within_spawn_distance(
        &Position {
            x: 20.0,
            y: 149.0,
            z: 290.5
        },
        7.0
    ));
}

#[test]
fn individual_provider_is_stable_per_uuid() {
    let provider = SpawnProvider::from_value(&fixture("individual.json")).expect("individual");
    let first = uuid("00000000-0000-0000-0000-000000000001");
    let second = uuid("00000000-0000-0000-0000-000000000002");
    let first_point = provider.spawn_point(first).expect("first");
    assert_eq!(first_point, provider.spawn_point(first).expect("repeat"));
    let second_point = provider.spawn_point(second).expect("second");
    assert!([1.5, 3.5].contains(&first_point.position.x));
    assert!([1.5, 3.5].contains(&second_point.position.x));
    assert!(provider.is_within_spawn_distance(&first_point.position, 0.1));
}

#[test]
fn fit_to_heightmap_replaces_the_sentinel_with_the_column_top() {
    let provider =
        SpawnProvider::from_value(&fixture("fit_to_heightmap.json")).expect("fit provider");
    let resolved = provider
        .resolve_for(uuid("00000000-0000-0000-0000-000000000001"), |x, z| {
            assert_eq!((x, z), (4, 7));
            Some(90)
        })
        .expect("resolved spawn");
    assert_eq!(resolved.position.y, 91.0);
    // Without a loaded column Java falls back to Y 1.
    let missing = provider
        .resolve_for(uuid("00000000-0000-0000-0000-000000000001"), |_, _| None)
        .expect("fallback spawn");
    assert_eq!(missing.position.y, 1.0);
}
