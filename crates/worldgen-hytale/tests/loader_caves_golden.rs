//! The **cave** golden dump comparison.
//!
//! The Java dumper (`tools/java-dumper`, `LoaderDumper.writeCaves`) writes
//! the cave configuration of every zone of the Default world into
//! `testdata/golden/loader.json` (the `caves` section):
//!
//! * the name of every cave type of the zone, and
//! * the *seed offsets* of the cave type's entry point generator — the
//!   value `PointGeneratorJsonLoader.loadSeed()` returns for the seed chain
//!   `<world seed>.Zones.Zone-<zone>.CaveGenerator-<cave type>.CaveType`.
//!
//! The seed offsets pin the **whole seed chain** of the cave loaders: a
//! missing or an extra suffix changes them, and with them the positions of
//! every cave entry point in the world.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use procedural::json::SeedString;
use serde_json::Value;
use worldgen_hytale::loader::cave::cave_generator_json_loader::CaveGeneratorJsonLoader;
use worldgen_hytale::loader::context::file_context_loader::FileContextLoader;
use worldgen_hytale::loader::seed_string_resource::SeedStringResource;
use worldgen_hytale::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use worldgen_hytale::prefab::prefab_store_root::PrefabStoreRoot;
use worldgen_model::registry::Registries;

/// The world seeds the Java dumper used.
const WORLD_SEEDS: [i32; 4] = [0, 1, 42, 1234567];

/// Whether the base-game asset pack is available (it is not part of the
/// source tree — see `.gitignore`). The cave prefab containers resolve
/// their prefab names through it.
fn assets_available() -> bool {
    PathBuf::from(format!(
        "{}/../../Assets/extracted/Server/Prefabs",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
    .is_dir()
}

fn manifest_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

fn golden_dump() -> Value {
    let path = PathBuf::from(format!(
        "{}/../../testdata/golden/loader.json",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to load file {}: {e}", path.display()));
    serde_json::from_str(&text).expect("the golden dump is a JSON document")
}

/// The zone requirement of the Default world (every zone folder with a
/// `Zone.json`).
fn default_zone_requirement() -> HashSet<String> {
    let mut requirement = HashSet::new();
    for entry in std::fs::read_dir(manifest_root().join("Zones")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() && path.join("Zone.json").exists() {
            requirement.insert(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    requirement
}

#[test]
fn the_cave_seed_offsets_match_the_java_golden_dump() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = manifest_root();
    let dump = golden_dump();
    let caves = dump.get("caves").expect("the dump has a caves section");

    let context = FileContextLoader::new("Default", &root, default_zone_requirement()).load();
    // The prefab loader of the world (the Java `SeedStringResource.loader`).
    let assets_root = PathBuf::from(format!(
        "{}/../../Assets/extracted",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ));
    context.set_prefab_loader(Rc::new(WorldGenPrefabLoader::new(
        PrefabStoreRoot::Assets,
        &root,
        &assets_root,
        Rc::new(Registries::load()),
    )));
    let zones: Vec<_> = context
        .get_zones()
        .borrow()
        .iter()
        .map(|(_, zone)| Rc::clone(zone))
        .collect();

    let registries = Rc::new(Registries::load());
    let mut checked_zones = 0;
    let mut checked_types = 0;

    for zone_context in &zones {
        let zone_name = zone_context.base.get_name().to_string();
        let Some(zone_dump) = caves.get(&zone_name) else {
            panic!("the golden dump has no caves section for the zone {zone_name}");
        };
        assert_eq!(
            zone_dump.get("present").and_then(Value::as_bool),
            Some(true),
            "the zone {zone_name} has a cave configuration"
        );

        for world_seed in WORLD_SEEDS {
            // The Java seed chain: `<seed>.Zones.Zone-<zone>` (the
            // `ZonesJsonLoader` and the `ZoneJsonLoader` append their
            // suffixes separately, the seed *string* is the same).
            let zone_seed = SeedString::new(&world_seed.to_string())
                .append(".Zones")
                .append(&format!(".Zone-{zone_name}"));

            let generator = CaveGeneratorJsonLoader::new(
                zone_seed,
                &root,
                None,
                Some(zone_context.base.get_path().join("Cave")),
                Rc::clone(zone_context),
                Rc::new(SeedStringResource::new()),
                Rc::clone(&registries),
            )
            .load()
            .expect("the zone has a cave configuration");

            let types = generator.get_cave_types();
            let dumped_types = zone_dump
                .get("types")
                .and_then(Value::as_array)
                .expect("the zone dump has the cave types");

            assert_eq!(
                types.len(),
                dumped_types.len(),
                "the zone {zone_name} has the same number of cave types"
            );

            for (cave_type, dumped) in types.iter().zip(dumped_types.iter()) {
                assert_eq!(
                    Some(cave_type.get_name()),
                    dumped.get("name").and_then(Value::as_str),
                    "the cave type names of the zone {zone_name}"
                );

                let entry_points = dumped.get("entryPoints").expect("the entry points");
                let seed_offsets = entry_points
                    .get("seedOffsets")
                    .and_then(Value::as_object)
                    .expect("the seed offsets");
                let expected = seed_offsets
                    .get(&world_seed.to_string())
                    .and_then(Value::as_i64)
                    .expect("the seed offset of the world seed")
                    as i32;

                assert_eq!(
                    cave_type.get_entry_point_seed(),
                    expected,
                    "the entry point seed offset of the cave type \"{}\" in the zone {zone_name} (world seed {world_seed})",
                    cave_type.get_name()
                );
                checked_types += 1;
            }
        }
        checked_zones += 1;
    }

    assert_eq!(checked_zones, 28, "every Default zone is checked");
    assert!(checked_types > 100, "the cave types are checked");
}
