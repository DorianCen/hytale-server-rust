//! Real-pack block-simulation metadata: support, fluid, falling and drops.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::simulation::{MatchSelf, RandomTickProcedure, SimulationTables, SupportDropType};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn registry_and_tables() -> Option<(AssetRegistry, SimulationTables)> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let tables = SimulationTables::from_assets(&registry);
    Some((registry, tables))
}

#[test]
fn stone_keeps_its_breaking_drop_and_tags() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let stone = registry
        .block_types
        .id_of("Rock_Stone")
        .expect("Rock_Stone");
    let block = tables.block(stone).expect("stone behavior");
    assert!(block.fluid_solid);
    assert!(block.tags.contains("Type=Rock"), "{:?}", block.tags);
    let drops = tables.breaking_drops(stone);
    let drop = drops.first().expect("stone drop");
    assert_eq!(drop.item_id.as_deref(), Some("Rock_Stone_Cobble"));
    assert_eq!(drop.quantity, 1);
}

#[test]
fn grass_random_tick_and_plant_support_match_the_assets() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let grass = registry
        .block_types
        .id_of("Soil_Grass")
        .expect("Soil_Grass");
    let block = tables.block(grass).expect("grass behavior");
    let Some(RandomTickProcedure::SpreadTo {
        allowed_tag,
        min_y,
        max_y,
        revert_block,
        ..
    }) = &block.random_tick
    else {
        panic!("grass must use SpreadTo: {:?}", block.random_tick);
    };
    assert_eq!(allowed_tag, "Spreadable=Grass");
    assert_eq!((*min_y, *max_y), (-1, 1));
    assert!(revert_block.is_some());

    let dirt = registry.block_types.id_of("Soil_Dirt").expect("Soil_Dirt");
    let dirt = tables.block(dirt).expect("dirt behavior");
    assert!(
        dirt.tags.contains("Spreadable=Grass"),
        "the target soil carries the spread tag: {:?}",
        dirt.tags
    );

    let bush = registry
        .block_types
        .id_of("Plant_Bush")
        .expect("Plant_Bush");
    let bush = tables.block(bush).expect("bush behavior");
    assert!(
        bush.support
            .iter()
            .any(|rule| rule.face == hytale_world::simulation::Dir::DOWN
                && rule.face_type.as_deref() == Some("Full")
                && rule.tag.as_deref() == Some("Type=Soil")),
        "{:?}",
        bush.support
    );
    assert!(
        bush.support
            .iter()
            .any(|rule| rule.match_self == MatchSelf::Ignored && !rule.required),
        "the Tilled disallowed rule is parsed"
    );
}

#[test]
fn fluids_expose_source_and_flow_tickers() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let source = registry.fluids.id_of("Water_Source").expect("Water_Source");
    let source = tables.fluid(source).expect("source behavior");
    assert_eq!(source.max_level, 1);
    assert!(!source.can_demote);
    let flow_id = source.spread_fluid.expect("source spreads Water");
    let flow = tables.fluid(flow_id).expect("flowing water");
    assert_eq!(flow.max_level, 8);
    assert!(flow.can_demote);
    assert_eq!(
        flow.supported_by,
        Some(registry.fluids.id_of("Water_Source").unwrap())
    );
}

#[test]
fn falling_barrels_are_parsed_as_fall_on_lost_support() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let barrel = registry
        .block_types
        .id_of("Debug_Falling_Barrel")
        .expect("Debug_Falling_Barrel");
    let barrel = tables.block(barrel).expect("barrel behavior");
    assert_eq!(barrel.support_drop, SupportDropType::Fall);
    assert!(barrel.falling, "FallingBlockSettings is present");
    assert!(
        barrel
            .support
            .iter()
            .any(|rule| rule.face == hytale_world::simulation::Dir::DOWN
                && rule.face_type.as_deref() == Some("Full")),
        "{:?}",
        barrel.support
    );
}
