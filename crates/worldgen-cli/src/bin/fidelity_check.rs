//! `fidelity_check` — T05 fidelity fixture extractor/verifier.
//!
//! Manual tool behind the seed explorer's `fixtures/fidelity-v1.json`: NOT
//! part of `cargo test` and not an explorer dependency. Until the explorer's
//! `map-core` exists (T07 owns `map-core/tests/fidelity.rs`), this probe is
//! the documented harness that verifies the fixture values.
//!
//! Usage (from the workspace root):
//!
//! ```sh
//! # regenerate the fixture body from the Java golden chunks + the generator
//! cargo run --release -p worldgen-cli --bin fidelity_check -- extract \
//!   testdata/Default testdata/golden/chunks.json > fixtures-body.json
//!
//! # verify a fixture file: exits 1 with a readable diff on any mismatch
//! cargo run --release -p worldgen-cli --bin fidelity_check -- check \
//!   testdata/Default testdata/golden/chunks.json <path-to>/fidelity-v1.json
//! ```
//!
//! Sources, honestly split (see fixtures/README.md in the explorer repo):
//!
//! - `golden-chunks` — Java-authoritative: the block/fluid/biome values come
//!   from `testdata/golden/chunks.json`, produced by the real Java pipeline
//!   (`tools/java-dumper`, seeds 0/1/42, chunks incl. negative coordinates).
//!   `height` is *derived* from the same dump (topmost non-air block run's
//!   `yEnd`), so it inherits the provenance.
//! - `worldgen-test` — Rust-port self-consistency: unique-prefab and
//!   spawn-point positions from `ChunkGenerator::get_unique_prefabs` /
//!   `get_spawn_points` (names and order are additionally pinned by
//!   `crates/worldgen-hytale/tests/unique_prefabs.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_hytale::world_gen_provider::{HytaleWorldGenProvider, Semver};

/// The fixture case grid: (seed, chunk_x, chunk_z, local_x, local_z).
/// Seeds 0/1/42; chunks chosen to include negative world coordinates and the
/// three fluid columns identified in the golden dump.
const CASES: &[(i32, i32, i32, i32, i32)] = &[
    (0, 0, 0, 0, 0),
    (0, 0, 0, 16, 16),
    (0, 9, 4, 12, 31),
    (0, -12, -25, 2, 26),
    (1, 0, 0, 0, 0),
    (1, 4, -5, 7, 3),
    (1, -12, -25, 2, 26),
    (42, 0, 0, 16, 16),
    (42, 16, -8, 16, 16),
    (42, 30, 15, 3, 7),
];

const SEEDS: &[i32] = &[0, 1, 42];

fn make_loader(world: &Path) -> ChunkGeneratorJsonLoader {
    let provider = HytaleWorldGenProvider {
        name: "Default".to_string(),
        version: Semver::parse("0.0.0").expect("0.0.0 is valid semver"),
        path: None,
    };
    let path = provider
        .resolve_path(world)
        .expect("the world path resolves like the CLI default");
    let config = WorldGenConfig::new(path, "Default".to_string(), "0.0.0".to_string());
    ChunkGeneratorJsonLoader::new(procedural::json::SeedString::new("ChunkGenerator"), config)
}

/// The (id → name) palettes shipped inside the golden dump.
fn palette(dump: &Value, kind: &str) -> BTreeMap<i64, String> {
    let mut map = BTreeMap::new();
    let Some(entries) = dump["palettes"][kind].as_object() else {
        panic!("the golden dump has a {kind} palette");
    };
    for (id, name) in entries {
        map.insert(
            id.parse::<i64>()
                .unwrap_or_else(|_| panic!("palette ids are numeric: {id}")),
            name.as_str()
                .expect("palette names are strings")
                .to_string(),
        );
    }
    map
}

fn lookup(map: &BTreeMap<i64, String>, id: i64) -> Option<&str> {
    map.get(&id).map(String::as_str)
}

/// The (id → name) palettes shipped inside the golden dump.
struct Palettes {
    blocks: BTreeMap<i64, String>,
    fluids: BTreeMap<i64, String>,
    biomes: BTreeMap<i64, String>,
}

/// A golden-derived column: what the Java dump says at (seed, x, z).
#[derive(Debug, Clone)]
struct GoldenColumn {
    biome: String,
    height: i64,
    surface_block: String,
    surface_fluid: Option<String>,
    fluid_level: Option<i64>,
}

/// Extract everything for one case from the golden dump: the column def
/// `[tint, biomeId, zoneId, envRuns, blockRuns, …]`, where a block run is
/// `[yStart, yEnd, blockId, rotation, filler, fluidId, fluidLevel, priority]`
/// in ascending y. Surface = the last non-air block run; topmost fluid = the
/// last run carrying a fluid id.
fn golden_case(dump: &Value, palettes: &Palettes, case: (i32, i32, i32, i32, i32)) -> GoldenColumn {
    let (seed, cx, cz, lx, lz) = case;
    let chunks = dump["chunks"].as_array().expect("chunks array");
    let chunk = chunks
        .iter()
        .find(|chunk| {
            chunk["seed"].as_i64() == Some(i64::from(seed))
                && chunk["x"].as_i64() == Some(i64::from(cx))
                && chunk["z"].as_i64() == Some(i64::from(cz))
        })
        .unwrap_or_else(|| panic!("the golden dump has no chunk for seed {seed} at ({cx},{cz})"));
    let cols = chunk["cols"].as_array().expect("cols");
    let defs = chunk["columnDefs"].as_array().expect("columnDefs");
    let reference = cols[((lz << 5) | lx) as usize].as_u64().expect("col ref") as usize;
    let def = defs[reference].as_array().expect("column def array");

    let biome_id = def[1].as_i64().expect("biomeId");
    let biome = lookup(&palettes.biomes, biome_id)
        .unwrap_or_else(|| panic!("biome id {biome_id} in palette"))
        .to_string();

    let mut height = 0i64;
    let mut surface_block = String::new();
    let mut surface_fluid = None;
    let mut fluid_level = None;
    for run in def[4].as_array().expect("blockRuns") {
        let run = run.as_array().expect("block run");
        let y_end = run[1].as_i64().expect("yEnd");
        let block_id = run[2].as_i64().expect("blockId");
        let fluid_id = run[5].as_i64().expect("fluidId");
        if block_id != 0 {
            height = y_end;
            surface_block = lookup(&palettes.blocks, block_id)
                .unwrap_or_else(|| panic!("block id {block_id} in palette"))
                .to_string();
        }
        if fluid_id != 0 {
            surface_fluid = Some(
                lookup(&palettes.fluids, fluid_id)
                    .unwrap_or_else(|| panic!("fluid id {fluid_id} in palette"))
                    .to_string(),
            );
            fluid_level = Some(run[6].as_i64().expect("fluidLevel"));
        }
    }
    GoldenColumn {
        biome,
        height,
        surface_block,
        surface_fluid,
        fluid_level,
    }
}

fn feature_entries(
    generator: &std::rc::Rc<worldgen_hytale::chunk::chunk_generator::ChunkGenerator>,
    seed: i32,
) -> Vec<Value> {
    let mut out = Vec::new();
    for entry in generator.get_unique_prefabs(seed) {
        let (x, y, z) = entry.get_position();
        out.push(json!({
            "seed": seed.to_string(),
            "kind": "unique-prefab",
            "name": entry.get_name(),
            "category": entry.get_category().name.clone(),
            "x": x, "y": y, "z": z,
            "show_on_map": entry.is_show_on_map(),
            "source": "worldgen-test",
            "notes": "name/order/rotation pinned by worldgen-hytale/tests/unique_prefabs.rs; position from the Rust generator"
        }));
    }
    for transform in generator.get_spawn_points(seed) {
        let position = transform.get_position();
        out.push(json!({
            "seed": seed.to_string(),
            "kind": "spawn-point",
            "name": "Spawn",
            "category": null,
            "x": position.x.round() as i64,
            "y": position.y.round() as i64,
            "z": position.z.round() as i64,
            "show_on_map": null,
            "source": "worldgen-test",
            "notes": "rounded from the generator transform (Vector3d); live-game value pending"
        }));
    }
    out
}

fn build_fixture(world: &Path, golden: &Path) -> Value {
    let dump: Value =
        serde_json::from_str(&std::fs::read_to_string(golden).expect("read the golden chunk dump"))
            .expect("parse the golden chunk dump");
    let palettes = Palettes {
        blocks: palette(&dump, "blocks"),
        fluids: palette(&dump, "fluids"),
        biomes: palette(&dump, "biomes"),
    };

    let mut cases = Vec::new();
    for &(seed, cx, cz, lx, lz) in CASES {
        let column = golden_case(&dump, &palettes, (seed, cx, cz, lx, lz));
        let world_x = i64::from(cx) * 32 + i64::from(lx);
        let world_z = i64::from(cz) * 32 + i64::from(lz);
        cases.push(json!({
            "seed": seed.to_string(),
            "world_seed": seed,
            "generator_seed": seed,
            "x": world_x,
            "z": world_z,
            "biome": column.biome,
            "height": column.height,
            "surface_block": column.surface_block,
            "surface_fluid": column.surface_fluid,
            "notes": format!(
                "golden chunk ({cx},{cz}) local ({lx},{lz}); fluid level {:?}",
                column.fluid_level
            ),
            "source": "golden-chunks"
        }));
    }

    let mut features = Vec::new();
    for &seed in SEEDS {
        let generator = make_loader(world).load();
        features.extend(feature_entries(&generator, seed));
    }

    json!({
        "schema": 1,
        "generator_version": worldgen_hytale::GENERATOR_VERSION,
        "world_assets": "Default@0.0.0 (testdata/Default, byte-identical to the pack's Server/World/Default)",
        "cases": cases,
        "features": features
    })
}

/// Compares a fixture file against freshly derived values. Returns the
/// number of mismatches; every mismatch prints a readable diff line.
fn check_fixture(world: &Path, golden: &Path, fixture_path: &Path) -> usize {
    let fixture: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path).expect("read the fixture"))
            .expect("parse the fixture JSON");
    let fresh = build_fixture(world, golden);

    let mut failures = 0usize;
    let report = |what: &str, expected: &Value, actual: &Value| {
        println!("FAIL {what}\n      fixture: {expected}\n      actual:  {actual}");
    };

    for key in ["schema", "generator_version", "world_assets"] {
        if fixture[key] != fresh[key] {
            failures += 1;
            report(&format!("header.{key}"), &fixture[key], &fresh[key]);
        }
    }

    let empty = Vec::new();
    let fixture_cases = fixture["cases"].as_array().unwrap_or(&empty);
    let fresh_cases = fresh["cases"].as_array().unwrap_or(&empty);
    if fixture_cases.len() != fresh_cases.len() {
        failures += 1;
        report(
            "cases.count",
            &json!(fixture_cases.len()),
            &json!(fresh_cases.len()),
        );
    }
    for (index, fresh_case) in fresh_cases.iter().enumerate() {
        let Some(fixture_case) = fixture_cases.get(index) else {
            continue;
        };
        let label = format!(
            "case[{}] seed={} x={} z={}",
            index,
            fresh_case["seed"].as_str().unwrap_or("?"),
            fresh_case["x"],
            fresh_case["z"]
        );
        for field in [
            "seed",
            "world_seed",
            "generator_seed",
            "x",
            "z",
            "biome",
            "height",
            "surface_block",
            "surface_fluid",
            "source",
        ] {
            if fixture_case[field] != fresh_case[field] {
                failures += 1;
                report(
                    &format!("{label} .{field}"),
                    &fixture_case[field],
                    &fresh_case[field],
                );
            }
        }
        if fixture_case["source"].as_str() != Some("golden-chunks") {
            failures += 1;
            println!("FAIL {label} .source must be golden-chunks in the committed fixture");
        }
    }

    let fixture_features = fixture["features"].as_array().unwrap_or(&empty);
    let fresh_features = fresh["features"].as_array().unwrap_or(&empty);
    if fixture_features.len() != fresh_features.len() {
        failures += 1;
        report(
            "features.count",
            &json!(fixture_features.len()),
            &json!(fresh_features.len()),
        );
    }
    for (index, fresh_feature) in fresh_features.iter().enumerate() {
        let Some(fixture_feature) = fixture_features.get(index) else {
            continue;
        };
        let label = format!(
            "feature[{}] kind={} name={}",
            index, fresh_feature["kind"], fresh_feature["name"]
        );
        for field in [
            "seed",
            "kind",
            "name",
            "x",
            "y",
            "z",
            "show_on_map",
            "source",
        ] {
            if fixture_feature[field] != fresh_feature[field] {
                failures += 1;
                report(
                    &format!("{label} .{field}"),
                    &fixture_feature[field],
                    &fresh_feature[field],
                );
            }
        }
    }

    if failures == 0 {
        println!(
            "OK: {} cases and {} features match the golden dump and the generator",
            fresh_cases.len(),
            fresh_features.len()
        );
    } else {
        println!("FAILED: {failures} mismatch(es)");
    }
    failures
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let world = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/Default"));
    let golden = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/golden/chunks.json"));

    match mode {
        "extract" => {
            let fixture = build_fixture(&world, &golden);
            println!(
                "{}",
                serde_json::to_string_pretty(&fixture).expect("serialize")
            );
        }
        "check" => {
            let Some(fixture_path) = args.get(4).map(PathBuf::from) else {
                eprintln!("usage: fidelity_check check <world> <golden> <fixture.json>");
                std::process::exit(2);
            };
            let failures = check_fixture(&world, &golden, &fixture_path);
            std::process::exit(if failures == 0 { 0 } else { 1 });
        }
        _ => {
            eprintln!(
                "usage: fidelity_check <extract|check> [world] [golden-chunks] [fixture.json]"
            );
            std::process::exit(2);
        }
    }
}

// Silence unused-import machinery for Map (kept for future schema checks).
#[allow(unused)]
fn _unused(_: Option<Map<String, Value>>) {}
