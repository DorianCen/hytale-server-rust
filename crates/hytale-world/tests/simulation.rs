//! Real-pack block-simulation metadata: support, fluid, falling and drops.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::simulation::{
    MatchSelf, RandomTickProcedure, SimulationTables, SupportDropType, SupportMatch,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
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
            .any(|rule| rule.match_self == MatchSelf::Ignored
                && rule.support == SupportMatch::Disallowed),
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

#[test]
fn fluid_tickers_parse_their_variant_and_collisions() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    use hytale_world::simulation::FluidTickerKind;

    let source = registry.fluids.id_of("Water_Source").expect("Water_Source");
    let source = tables.fluid(source).expect("source behavior");
    assert_eq!(source.kind, FluidTickerKind::Default);
    assert!(source.tags.contains("Fluid=Water"), "{:?}", source.tags);
    let collision = source
        .collisions
        .iter()
        .find(|collision| collision.other == registry.fluids.id_of("Lava").unwrap())
        .expect("water/lava collision");
    assert_eq!(
        collision.block_to_place,
        registry.block_types.id_of("Rock_Stone_Cobble")
    );
    assert!(!collision.place_fluid);
    assert_eq!(collision.sound_event.as_deref(), Some("SFX_Flame_Break"));

    let finite = registry.fluids.id_of("Water_Finite").expect("Water_Finite");
    assert_eq!(
        tables.fluid(finite).expect("finite behavior").kind,
        FluidTickerKind::Finite
    );

    let fire = registry.fluids.id_of("Fire").expect("Fire");
    let fire = tables.fluid(fire).expect("fire behavior");
    assert_eq!(fire.kind, FluidTickerKind::Fire);
    assert!(!fire.flammability.is_empty());
    assert!(fire
        .flammability
        .iter()
        .any(|config| config.pattern.is_some()));
}

#[test]
fn partial_hitboxes_apply_the_java_face_coverage_rules() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let simulation = hytale_world::simulation::BlockSimulation::new(tables, 1);

    let stairs = registry
        .block_types
        .id_of("Build_Black_Stairs")
        .expect("stairs");
    // East-west flow: 0.5 + 0.25 of the face, so water passes through.
    for (dx, dz) in [(1, 0), (-1, 0)] {
        assert!(
            !simulation.blocks_fluid_from(stairs, 0, dx, dz, 0),
            "stairs must not block x flow from ({dx}, {dz})"
        );
    }
    // North-south flow covers the full face (0.5 + 0.5), so it is blocked
    // when entering from the tall side.
    assert!(simulation.blocks_fluid_from(stairs, 0, 0, 1, 0));
    assert!(!simulation.blocks_fluid_from(stairs, 0, 0, -1, 0));

    let flat = registry
        .block_types
        .id_of("Cloth_Roof_Blue_Flat")
        .expect("flat roof");
    assert!(!simulation.blocks_fluid_from(flat, 0, 1, 0, 0));

    let vertical = registry
        .block_types
        .id_of("Deco_Bone_Skulls_Feran_Wall")
        .expect("vertical slab");
    assert!(!simulation.blocks_fluid_from(vertical, 0, 1, 0, 0));
    assert!(!simulation.blocks_fluid_from(vertical, 0, -1, 0, 0));
    assert!(simulation.blocks_fluid_from(vertical, 0, 0, 1, 0));

    let stone = registry.block_types.id_of("Rock_Stone").expect("stone");
    assert!(simulation.is_fully_solid(stone));
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        assert!(simulation.blocks_fluid_from(stone, 0, dx, dz, 0));
    }
}

#[test]
fn fire_flammability_prioritises_and_matches_block_tags() {
    let Some((registry, tables)) = registry_and_tables() else {
        return;
    };
    let fire = registry.fluids.id_of("Fire").expect("Fire");
    let fire = tables.fluid(fire).expect("fire behavior");
    let wood = registry
        .block_types
        .id_of("Wood_Trunk_Oak")
        .or_else(|| {
            registry
                .block_types
                .iter()
                .find(|record| {
                    record
                        .value
                        .json
                        .get("Tags")
                        .and_then(|tags| tags.get("Type"))
                        .and_then(|values| values.as_array())
                        .is_some_and(|values| values.iter().any(|value| value == "Wood"))
                })
                .and_then(|record| record.id)
        })
        .expect("a wood block");
    let wood = tables.block(wood).expect("wood behavior");
    let mut configs: Vec<_> = fire.flammability.iter().collect();
    configs.sort_by_key(|config| std::cmp::Reverse(config.priority));
    assert!(
        configs
            .iter()
            .any(|config| config.pattern.as_ref().is_some_and(|pattern| {
                config.matches(&wood.tags)
                    && matches!(pattern, hytale_assets::TagPatternNode::And { .. })
            })),
        "the wood flammability rule must match a wood block: {:?}",
        wood.tags
    );
}
