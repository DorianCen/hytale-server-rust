//! Asset registry → world generator registry bridge.

use std::path::Path;

use hytale_assets::AssetPack;
use hytale_world::registry::registries_from_assets;
use worldgen_model::registry::{DrawType, Opacity};

fn fixture_registries() -> worldgen_model::registry::Registries {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../hytale-assets/tests/fixtures/registry");
    let pack = AssetPack::open(path).expect("open the fixture pack");
    let assets = hytale_assets::AssetRegistry::load_p0(&pack).expect("load the P0 registry");
    registries_from_assets(&assets).expect("convert the registry")
}

#[test]
fn blocks_keep_their_names_fields_and_rotated_support() {
    let registries = fixture_registries();

    let rock = registries
        .blocks
        .index_of("Rock_Stone")
        .and_then(|id| registries.blocks.get(id))
        .expect("Rock_Stone is registered");
    assert_eq!(rock.draw_type, DrawType::Cube);
    assert_eq!(rock.opacity, Opacity::Solid);
    assert_eq!(rock.support, None);

    let plant_id = registries
        .blocks
        .index_of("Plant")
        .expect("Plant is registered");
    let plant = registries.blocks.get(plant_id).expect("Plant info");
    assert_eq!(plant.draw_type, DrawType::Model);
    let base = plant.support.expect("plants require support");
    assert!(base.down);
    assert!(!base.up);
    assert_eq!(plant.rotated_support.len(), 64);
    assert_eq!(plant.get_support(0), Some(base));

    let bench_open = registries
        .blocks
        .index_of("*Bench_State_Definitions_Open")
        .and_then(|id| registries.blocks.get(id))
        .expect("nested state block is registered");
    let bench_support = bench_open.support.expect("explicit support");
    assert!(!bench_support.down);
    assert!(bench_support.up);
}

#[test]
fn fluids_and_environments_keep_their_ids_and_levels() {
    let registries = fixture_registries();

    let lava = registries.fluids.index_of("Lava").expect("Lava");
    assert_eq!(registries.fluids.get_max_fluid_level(lava), 4);
    let source = registries
        .fluids
        .index_of("Lava_Source")
        .expect("Lava_Source");
    assert_eq!(registries.fluids.get_max_fluid_level(source), 8);
    let empty = registries.fluids.index_of("Empty").expect("Empty");
    assert_eq!(registries.fluids.get_max_fluid_level(empty), 8);

    let default = registries
        .environments
        .index_of("Default")
        .expect("Default environment");
    assert_eq!(registries.environments.name(default), Some("Default"));
}
