//! Real-pack fluid ticker scenarios: face coverage, waterlogging, collisions,
//! finite flow, fire and disabled tickers.
//!
//! The oracle is the Java `FluidTicker`/`DefaultFluidTicker`/
//! `FiniteFluidTicker`/`FireFluidTicker` implementation plus the real pack
//! assets; the expected values below are the Java algorithm's outcomes, not
//! invented wire bytes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::simulation::{BlockSimulation, SimulationTables};
use hytale_world::world::{World, WorldStorage};

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

fn registry() -> Option<AssetRegistry> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    Some(AssetRegistry::load_p0(&pack).expect("load P0"))
}

fn test_world() -> World {
    let mut world = World::new(WorldStorage::new("/tmp/hytale-fluid-simulation-test"));
    for chunk_x in -1..=1 {
        for chunk_z in -1..=1 {
            world.insert_chunk(ChunkColumn::new(chunk_x, chunk_z));
        }
    }
    world
}

fn run(simulation: &mut BlockSimulation, world: &mut World, ticks: usize) {
    for _ in 0..ticks {
        simulation.tick(world, 0.05);
    }
}

#[test]
fn java_fluid_edge_cases_on_real_assets() {
    let Some(registry) = registry() else {
        return;
    };
    let tables = SimulationTables::from_assets(&registry);
    let stone = registry.block_types.id_of("Rock_Stone").expect("stone");
    let stairs = registry
        .block_types
        .id_of("Build_Black_Stairs")
        .expect("stairs");
    let water_source = registry.fluids.id_of("Water_Source").expect("Water_Source");
    let water = registry.fluids.id_of("Water").expect("Water");
    let lava = registry.fluids.id_of("Lava").expect("Lava");
    let finite = registry.fluids.id_of("Water_Finite").expect("Water_Finite");
    let fire = registry.fluids.id_of("Fire").expect("Fire");
    let burnt = registry
        .block_types
        .id_of("Wood_Burnt_Trunk")
        .expect("Wood_Burnt_Trunk");
    let trunk = registry
        .block_types
        .id_of("Wood_Aspen_Trunk")
        .expect("Wood_Aspen_Trunk");

    // 1. A water source flows into a stairs cell (face coverage < 0.9), but a
    // full cube blocks it.
    let mut world = test_world();
    world.set_block(16, 199, 16, stone).expect("floor");
    world.set_block(18, 200, 16, stone).expect("full cube");
    world.set_block(17, 200, 16, stairs).expect("stairs");
    world
        .set_fluid(16, 200, 16, water_source, 1)
        .expect("source");
    let mut simulation = BlockSimulation::new(tables.clone(), 7);
    run(&mut simulation, &mut world, 20);
    assert_eq!(
        world.get_fluid_id(17, 200, 16),
        water,
        "water flows into the stairs cell"
    );
    assert_eq!(world.get_fluid_level(17, 200, 16), 7);
    assert_eq!(
        world.get_fluid_id(18, 200, 16),
        0,
        "the full cube blocks horizontal flow"
    );

    // 2. Waterlogged stairs keep their fluid; a full solid cube clears it.
    let mut world = test_world();
    world.set_block(16, 200, 16, stairs).expect("stairs");
    world.set_block(17, 200, 16, stone).expect("stone");
    world
        .set_fluid(16, 200, 16, water_source, 1)
        .expect("waterlogged stairs");
    world
        .set_fluid(17, 200, 16, water_source, 1)
        .expect("water inside stone");
    let mut simulation = BlockSimulation::new(tables.clone(), 8);
    run(&mut simulation, &mut world, 10);
    assert_eq!(
        world.get_fluid_id(16, 200, 16),
        water_source,
        "partial blocks may retain waterlogged fluid"
    );
    assert_eq!(
        world.get_fluid_id(17, 200, 16),
        0,
        "a full solid cube clears co-located fluid"
    );

    // 3. Water meeting lava converts the cell into cobble and consumes both.
    let mut world = test_world();
    world.set_block(16, 199, 16, stone).expect("floor");
    world
        .set_fluid(16, 200, 16, water_source, 1)
        .expect("source");
    world.set_fluid(17, 200, 16, lava, 8).expect("lava");
    let mut simulation = BlockSimulation::new(tables.clone(), 9);
    run(&mut simulation, &mut world, 20);
    assert_eq!(
        world.get_block(17, 200, 16),
        registry
            .block_types
            .id_of("Rock_Stone_Cobble")
            .expect("cobble"),
        "water/lava collision places Rock_Stone_Cobble"
    );
    assert_eq!(world.get_fluid_id(17, 200, 16), 0);

    // 4. Finite water drains from the top of a column onto the floor.
    let mut world = test_world();
    world.set_block(16, 199, 16, stone).expect("floor");
    world.set_fluid(16, 202, 16, finite, 8).expect("finite top");
    let mut simulation = BlockSimulation::new(tables.clone(), 10);
    run(&mut simulation, &mut world, 25);
    assert_eq!(world.get_fluid_id(16, 201, 16), 0);
    assert_eq!(world.get_fluid_id(16, 200, 16), finite);
    assert_eq!(world.get_fluid_level(16, 200, 16), 8);

    // 5. Fire ignites a flammable neighbour, then burns it into the configured
    // result (Java `Flammability` Type=Trunk rule).
    let mut world = test_world();
    world.set_block(16, 200, 16, stone).expect("floor");
    world.set_block(16, 201, 16, trunk).expect("trunk");
    world.set_fluid(15, 201, 16, fire, 1).expect("fire");
    let mut simulation = BlockSimulation::new(tables.clone(), 11);
    run(&mut simulation, &mut world, 2000);
    assert_eq!(
        world.get_block(16, 201, 16),
        burnt,
        "fire burns a wood trunk into Wood_Burnt_Trunk"
    );

    // 6. A world-config disabled fluid tag stops the ticker.
    let mut world = test_world();
    world.set_block(16, 199, 16, stone).expect("floor");
    world
        .set_fluid(16, 200, 16, water_source, 1)
        .expect("source");
    let mut simulation = BlockSimulation::new(tables, 12);
    simulation.set_disabled_fluid_tickers(&BTreeSet::from(["Fluid=Water".to_string()]));
    run(&mut simulation, &mut world, 20);
    assert_eq!(
        world.get_fluid_id(17, 200, 16),
        0,
        "Fluid=Water disables every water ticker"
    );
}

#[test]
fn java_fluid_fixture_matches_the_rust_tables() {
    let Some(registry) = registry() else {
        return;
    };
    let fixture_path = repo_root().join("testdata/golden/server/fluid/fluid-tickers.json");
    let fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture_path).expect("read fluid fixture"))
            .expect("parse fluid fixture");
    let tables = SimulationTables::from_assets(&registry);
    let simulation = BlockSimulation::new(tables.clone(), 1);

    for (key, expected) in fixture["blocks"].as_object().expect("blocks") {
        let id = registry.block_types.id_of(key).expect(key);
        assert_eq!(
            simulation.is_fully_solid(id),
            expected["fullySolid"].as_bool().unwrap(),
            "{key} fully solid"
        );
        assert_eq!(
            simulation.is_solid(id),
            expected["solid"].as_bool().unwrap(),
            "{key} solid"
        );
        for (direction, (dx, dz)) in [
            ("east", (1, 0)),
            ("west", (-1, 0)),
            ("south", (0, 1)),
            ("north", (0, -1)),
        ] {
            assert_eq!(
                simulation.blocks_fluid_from(id, 0, dx, dz, 0),
                expected["blocksFluidFrom"][direction].as_bool().unwrap(),
                "{key} blocks flow from {direction}"
            );
        }
    }

    for (key, expected) in fixture["fluids"].as_object().expect("fluids") {
        let id = registry.fluids.id_of(key).expect(key);
        let actual = tables.fluid(id).expect("fluid behavior");
        let ticker = match actual.kind {
            hytale_world::simulation::FluidTickerKind::Default => "DefaultFluidTicker",
            hytale_world::simulation::FluidTickerKind::Finite => "FiniteFluidTicker",
            hytale_world::simulation::FluidTickerKind::Fire => "FireFluidTicker",
        };
        assert_eq!(ticker, expected["ticker"].as_str().unwrap(), "{key} ticker");
        assert_eq!(
            actual.max_level,
            expected["maxFluidLevel"].as_i64().unwrap() as i8,
            "{key} max level"
        );
        assert!(
            (actual.flow_rate - expected["flowRate"].as_f64().unwrap() as f32).abs() < 1e-6,
            "{key} flow rate"
        );
        assert_eq!(
            actual.can_demote,
            expected["canDemote"].as_bool().unwrap(),
            "{key} can demote"
        );
        let actual_supported = actual
            .supported_by
            .and_then(|id| registry.fluids.get_by_id(id))
            .map(|record| record.key.as_str());
        assert_eq!(
            actual_supported,
            expected["supportedBy"].as_str(),
            "{key} supported by"
        );
        let actual_spread = actual
            .spread_fluid
            .and_then(|id| registry.fluids.get_by_id(id))
            .map(|record| record.key.as_str());
        assert_eq!(
            actual_spread,
            expected["spreadFluid"].as_str(),
            "{key} spread fluid"
        );

        let expected_collisions = expected["collisions"].as_object().unwrap();
        assert_eq!(
            actual.collisions.len(),
            expected_collisions.len(),
            "{key} collision count"
        );
        for (other, collision) in expected_collisions {
            let other_id = registry.fluids.id_of(other).expect(other);
            let actual = actual
                .collisions
                .iter()
                .find(|collision| collision.other == other_id)
                .unwrap_or_else(|| panic!("{key} collision with {other}"));
            assert_eq!(
                actual
                    .block_to_place
                    .and_then(|id| registry.block_types.get_by_id(id))
                    .map(|record| record.key.as_str()),
                collision["blockToPlace"].as_str(),
                "{key}/{other} block to place"
            );
            assert_eq!(
                actual.sound_event.as_deref(),
                collision["soundEvent"].as_str(),
                "{key}/{other} sound"
            );
            assert_eq!(
                actual.place_fluid,
                collision["placeFluid"].as_bool().unwrap(),
                "{key}/{other} place fluid"
            );
        }

        let expected_flammability = expected["flammability"].as_array().unwrap();
        assert_eq!(
            actual.flammability.len(),
            expected_flammability.len(),
            "{key} flammability count"
        );
        for (index, rule) in expected_flammability.iter().enumerate() {
            let actual = &actual.flammability[index];
            assert_eq!(
                actual.priority,
                rule["priority"].as_i64().unwrap() as i32,
                "{key} flammability {index} priority"
            );
            assert_eq!(
                actual.burn_level,
                rule["burnLevel"].as_i64().unwrap() as i8,
                "{key} flammability {index} burn level"
            );
            assert!(
                (actual.burn_chance - rule["burnChance"].as_f64().unwrap() as f32).abs() < 1e-6,
                "{key} flammability {index} burn chance"
            );
            let actual_result = actual
                .resulting_block
                .and_then(|id| registry.block_types.get_by_id(id))
                .map(|record| record.key.as_str())
                .unwrap_or("Empty");
            assert_eq!(
                actual_result,
                rule["resultingBlock"].as_str().unwrap(),
                "{key} flammability {index} resulting block"
            );
            assert_eq!(
                actual.resulting_state.as_deref(),
                rule["resultingState"].as_str(),
                "{key} flammability {index} resulting state"
            );
        }
    }
}
