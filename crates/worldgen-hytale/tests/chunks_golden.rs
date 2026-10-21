//! Golden parity test for the **full chunk generation** (Phases 3 + 4 + 5).
//!
//! Reference data: `testdata/golden/chunks.json`, produced by
//! `tools/java-dumper` (`ChunkDumper.dumpChunks`) — the REAL Java pipeline
//! (the vanilla CoreServer asset pipeline boots the pack, the FULL
//! `ChunkGeneratorJsonLoader.load()` per seed loads the Default world from
//! the UNPACKED tree — `Assets/extracted/Server/World/Default`,
//! byte-identical to `testdata/Default` — and
//! `ChunkGeneratorExecution.execute(seed)` runs the real stage order:
//! tint → environment → blocks → caves → prefabs → water) for the world
//! seeds 0 / 1 / 42 and the chunks (0,0), (4,-5), (9,4), (7,3), (-12,-25),
//! (16,-8), (30,15).
//!
//! Per (seed, chunk) the dump records for every voxel of the 32×320×32
//! chunk the block id + rotation index + filler, the fluid id + level, the
//! environment id and the `BlockPriorityChunk` priority byte, compactly
//! encoded as per-column run-length lists (the runs close when ANY of the
//! eight values changes). Identical columns are deduplicated per chunk into
//! `columnDefs` (a column def is `[tint, biomeId, zoneId, envRuns,
//! blockRuns, parentBiomeId, borderDistance, biomeIndex, cellX, cellY,
//! rawTint]` — the last six are debug references; the parity target is the
//! first five); `cols[i]` references a def in `ChunkUtil.indexColumn` order
//! (`(z << 5) | x`, z-major). The ids resolve through the `palettes`
//! (id → NAME), so this test maps every dumped id through the golden
//! registries (`Registries::load()` — the name → id direction, bit-identical
//! to the Java pipeline by construction).
//!
//! ## Not compared (documented deferred scope, mirrored on BOTH sides)
//!
//! * the entities of the prefabs (`REMAINING.md` §2 — the Java dumper's
//!   shadowed `BsonPrefabBufferDeserializer` skips the entity decode), and
//! * the block "components" states (no ECS in the Rust port — the locked
//!   CoreServer-boundary decision; the Java deserializer skip is patched
//!   the same way), and
//! * the `BlockPhysics` support values written into the section holders
//!   (the Rust port documents BlockPhysics as a no-op).

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::Value;
use worldgen_hytale::biome::biome::Biome as _;
use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_model::block_priority::HEIGHT;
use worldgen_model::chunk::{block_index, ENVIRONMENT_NOT_SET};
use worldgen_model::registry::Registries;

/// The golden dump.
const GOLDEN: &str = "../../testdata/golden/chunks.json";

/// The (x, z) chunk coordinates of the dump (`meta.chunks`).
const CHUNKS: [(i32, i32); 7] = [
    (0, 0),
    (4, -5),
    (9, 4),
    (7, 3),
    (-12, -25),
    (16, -8),
    (30, 15),
];

/// The world seeds of the dump (`meta.seeds`).
#[allow(dead_code)]
const SEEDS: [i32; 3] = [0, 1, 42];

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The loaded Default world for one seed — the same chain the Java dumper
/// runs per seed (`SeedString(Integer.toString(seed))`).
fn default_world(seed: i32) -> Rc<ChunkGenerator> {
    let root = manifest_root().join("../../testdata/Default");
    let config = worldgen_hytale::world_gen_config::WorldGenConfig::new(
        root,
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader =
        worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new(&seed.to_string()),
            config,
        );
    loader.load()
}

/// Generates the chunk `(chunk_x, chunk_z)` with the given world seed.
fn generate_chunk(
    generator: &Rc<ChunkGenerator>,
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
) -> ChunkGeneratorExecution {
    let chunk = worldgen_model::chunk::GeneratedChunk::with_coordinates(
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

/// The compact per-voxel state of the Rust chunk, run-length encoded exactly
/// like the dump: a run closes when ANY of the eight values changes.
///
/// Returns `(runs, env_runs)` per the dump format:
/// * `runs`: `[yStart, yEnd, blockId, rotation, filler, fluidId, fluidLevel,
///   priority]` (the priority unsigned, `& 0xFF`, like the dumper),
/// * `env_runs`: `[minY, maxY, environmentId]`.
fn encode_column(
    execution: &ChunkGeneratorExecution,
    x: i32,
    z: i32,
) -> (Vec<Vec<i64>>, Vec<Vec<i64>>) {
    let chunk = execution.get_chunk();
    let priority_chunk = execution.get_priority_chunk();
    let column = chunk.get_tint(x, z);
    let _ = column; // the tint index is resolved through get_tint below
    let column = tint_column(x, z);

    let mut runs: Vec<Vec<i64>> = Vec::new();
    let mut env_runs: Vec<Vec<i64>> = Vec::new();

    // --- the block runs ----------------------------------------------------
    for y in 0..HEIGHT {
        let _index = block_index(x, y, z) as usize;
        let state = [
            chunk.get_block(x, y, z) as i64,
            chunk.get_rotation_index(x, y, z) as i64,
            chunk.get_filler(x, y, z) as i64,
            chunk.get_fluid_id(x, y, z) as i64,
            chunk.get_fluid_level(x, y, z) as i64,
            (priority_chunk.get(x, y, z) as i32 & 0xFF) as i64,
        ];
        match runs.last_mut() {
            Some(last) if last[2..] == state[..] && last[1] + 1 == y as i64 => {}
            _ => {
                let mut run = vec![y as i64, y as i64];
                run.extend_from_slice(&state);
                runs.push(run);
            }
        }
        if let Some(last) = runs.last_mut() {
            last[1] = y as i64;
        }
    }

    // --- the environment runs ----------------------------------------------
    // Reconstruct the per-y environment and run-length encode it.
    for y in 0..HEIGHT {
        let _index = block_index(x, y, z) as usize;
        let env = {
            let over = chunk.environment()[_index];
            if over == ENVIRONMENT_NOT_SET {
                chunk.environment_column()[column]
            } else {
                over
            }
        } as i64;
        match env_runs.last_mut() {
            Some(last) if last[2] == env => {}
            _ => env_runs.push(vec![y as i64, y as i64, env]),
        }
        if let Some(last) = env_runs.last_mut() {
            last[1] = y as i64;
        }
    }

    (runs, env_runs)
}

/// Java `ChunkUtil.indexColumn(x, z)` — the column index in x-major /
/// z-minor order (the Rust `column_index` is private, so it is reproduced
/// here; the tint accessor uses the same layout).
fn tint_column(x: i32, z: i32) -> usize {
    ((x & 31) | ((z & 31) << 5)) as usize
}

/// Parses the dump and compares every chunk / column / run.
///
/// Returns the number of compared columns (21 chunks × 1024 columns).
///
/// **Bit-exact**: tint / raw tint / zone / biome / parent tile biome / biome
/// index / biome cell / environment base mapping, every terrain, layer,
/// cover, water, cave, cave-cover, cave-prefab and prefab voxel, the fluids
/// and the priority bytes all match the Java pipeline.
#[test]
fn the_default_world_chunk_grids_match_the_java_pipeline() {
    let path = manifest_root().join(GOLDEN);
    let dump: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read the golden chunk dump"))
            .expect("parse the golden chunk dump");

    let registries = Rc::new(Registries::load());

    // --- the palettes: id → name → Rust id --------------------------------
    let blocks = palette(&dump, "blocks");
    let fluids = palette(&dump, "fluids");
    let environments = palette(&dump, "environments");
    let biomes = palette(&dump, "biomes");
    let zones = palette(&dump, "zones");

    let mut chunk_runs = 0usize;
    let mut mismatch_report: Vec<String> = Vec::new();

    // Per-category counters and the first few examples of each. The report
    // stays bounded (a full 21k-column diff would be unreadable) but the
    // counters show how much of the divergence is which stage.
    #[derive(Default)]
    struct Stats {
        columns: usize,
        tint: usize,
        raw_tint: usize,
        biome: usize,
        parent: usize,
        index: usize,
        cell: usize,
        zone: usize,
        env: usize,
        blocks: usize,
        priority: [usize; 16],
        diff_priority: [usize; 16],
        tint_examples: Vec<String>,
        raw_tint_examples: Vec<String>,
        biome_examples: Vec<String>,
        parent_examples: Vec<String>,
        index_examples: Vec<String>,
        cell_examples: Vec<String>,
        zone_examples: Vec<String>,
        env_examples: Vec<String>,
        block_examples: Vec<String>,
        layer_examples: Vec<String>,
    }
    let mut stats = Stats::default();
    let mut chunk_summaries: Vec<String> = Vec::new();
    let record = |list: &mut Vec<String>, message: String| {
        if list.len() < 5 {
            list.push(message);
        }
    };

    // The dump is seed-major (the seeds outer, the seven chunks inner); one
    // generator per seed is loaded and reused for all of its chunks.
    let chunks = dump["chunks"].as_array().expect("chunks array");
    let mut index = 0usize;
    for seed in SEEDS {
        let generator = default_world(seed);
        for chunk in &chunks[index..index + CHUNKS.len()] {
            index += 1;
            let chunk_x = chunk["x"].as_i64().expect("x") as i32;
            let chunk_z = chunk["z"].as_i64().expect("z") as i32;

            let execution = generate_chunk(&generator, seed, chunk_x, chunk_z);

            let column_defs = chunk["columnDefs"].as_array().expect("columnDefs");
            let cols = chunk["cols"].as_array().expect("cols");
            assert_eq!(cols.len(), 1024, "the dump has every column");

            let mut chunk_columns = 0usize;
            let mut chunk_tint = 0usize;
            let mut chunk_env = 0usize;
            let mut chunk_blocks = 0usize;
            let mut chunk_priority = [0usize; 16];
            let mut chunk_layer_examples = 0usize;
            for x in 0..32i32 {
                for z in 0..32i32 {
                    let ref_index = cols[tint_column(x, z)].as_u64().expect("col ref") as usize;
                    let def = &column_defs[ref_index];
                    let def = def.as_array().expect("column def array");
                    let golden_tint = def[0].as_i64().expect("tint");
                    let golden_biome = def[1].as_i64().expect("biome");
                    let golden_zone = def[2].as_i64().expect("zone");
                    let golden_env_runs = def[3].as_array().expect("envRuns");
                    let golden_runs = def[4].as_array().expect("blockRuns");
                    let golden_parent = def
                        .get(5)
                        .and_then(|value| value.as_i64())
                        .expect("parentBiomeId");
                    let _golden_border = def
                        .get(6)
                        .and_then(|value| value.as_str())
                        .unwrap_or("<missing>");
                    let golden_biome_index = def
                        .get(7)
                        .and_then(|value| value.as_str())
                        .map(parse_java_hex_double)
                        .expect("biomeIndex");
                    let golden_cell = (
                        def.get(8).and_then(|value| value.as_i64()).expect("cellX") as i32,
                        def.get(9).and_then(|value| value.as_i64()).expect("cellY") as i32,
                    );
                    let golden_distance = def
                        .get(10)
                        .and_then(|value| value.as_f64())
                        .unwrap_or(f64::NAN);
                    let _golden_point_x = def
                        .get(11)
                        .and_then(|value| value.as_f64())
                        .unwrap_or(f64::NAN);
                    let _golden_point_y = def
                        .get(12)
                        .and_then(|value| value.as_f64())
                        .unwrap_or(f64::NAN);
                    let golden_raw_tint = def
                        .get(13)
                        .and_then(|value| value.as_i64())
                        .expect("rawTint");

                    let world_x = (chunk_x << 5) | x;
                    let world_z = (chunk_z << 5) | z;

                    // --- tint ------------------------------------------------
                    let column = tint_column(x, z);
                    let rust_tint = execution.get_chunk().tint()[column] as i64;
                    if rust_tint != golden_tint {
                        stats.tint += 1;
                        chunk_tint += 1;
                        record(
                            &mut stats.tint_examples,
                            format!(
                                "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): tint {rust_tint} != {golden_tint}"
                            ),
                        );
                    }

                    let raw_tint_result = generator.get_zone_biome_result_at(seed, world_x, world_z);
                    let rust_raw_tint = raw_tint_result
                        .get_biome()
                        .map(|biome| {
                            biome
                                .get_tint_container()
                                .get_tint_color_at(seed, world_x, world_z)
                        })
                        .unwrap_or(0) as i64;
                    if rust_raw_tint != golden_raw_tint {
                        stats.raw_tint += 1;
                        record(
                            &mut stats.raw_tint_examples,
                            format!(
                                "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): raw tint {rust_raw_tint} != {golden_raw_tint}"
                            ),
                        );
                    }

                    // --- biome / zone (by NAME) -------------------------------
                    let zone_biome = generator.get_zone_biome_result_at(seed, world_x, world_z);
                    let rust_zone = zone_biome
                        .get_zone_result()
                        .and_then(|result| result.get_zone().map(|zone| zone.name.clone()));
                    let golden_zone_name = zones.get(&(golden_zone as i32)).cloned();
                    if rust_zone.as_deref() != golden_zone_name.as_deref() {
                        stats.zone += 1;
                        record(
                            &mut stats.zone_examples,
                            format!(
                                "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): zone {:?} != {:?}",
                                rust_zone, golden_zone_name
                            ),
                        );
                    }
                    let rust_biome = zone_biome
                        .get_biome()
                        .map(|biome| biome.get_name().to_string());
                    let golden_biome_name = biomes.get(&(golden_biome as i32)).cloned();
                    if rust_biome.as_deref() != golden_biome_name.as_deref() {
                        stats.biome += 1;
                        record(
                            &mut stats.biome_examples,
                            format!(
                                "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): biome {:?} != {:?}",
                                rust_biome, golden_biome_name
                            ),
                        );
                    }
                    // The raw parent tile biome (before the custom overlay).
                    let rust_parent = zone_biome
                        .get_zone_result()
                        .and_then(|result| result.get_zone())
                        .and_then(|zone| {
                            zone.biome_pattern_generator
                                .get_biome(seed, world_x, world_z)
                        })
                        .map(|biome| biome.get_name().to_string());
                    let golden_parent_name = biomes.get(&(golden_parent as i32)).cloned();
                    if rust_parent.as_deref() != golden_parent_name.as_deref() {
                        stats.parent += 1;
                        record(
                            &mut stats.parent_examples,
                            format!(
                                "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): parent biome {:?} != {:?}",
                                rust_parent, golden_parent_name
                            ),
                        );
                    }
                    // The raw biome-pattern index value (the weighted-map pick).
                    let rust_biome_index = zone_biome
                        .get_zone_result()
                        .and_then(|result| result.get_zone())
                        .map(|zone| {
                            zone.biome_pattern_generator
                                .get_biome_index(seed, world_x, world_z)
                        });
                    if let Some(rust_biome_index) = rust_biome_index {
                        if rust_biome_index.to_bits() != golden_biome_index.to_bits() {
                            stats.index += 1;
                            record(
                                &mut stats.index_examples,
                                format!(
                                    "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): biomeIndex {} != {}",
                                    rust_biome_index, golden_biome_index
                                ),
                            );
                        }
                    }
                    let rust_cell = zone_biome
                        .get_zone_result()
                        .and_then(|result| result.get_zone())
                        .map(|zone| {
                            zone.biome_pattern_generator
                                .get_biome_cell(seed, world_x, world_z)
                        });
                    if let Some(rust_cell) = rust_cell {
                        if rust_cell != golden_cell {
                            stats.cell += 1;
                            let rust_debug = zone_biome
                                .get_zone_result()
                                .and_then(|result| result.get_zone())
                                .map(|zone| {
                                    zone.biome_pattern_generator
                                        .get_biome_point_debug(seed, world_x, world_z)
                                });
                            record(
                                &mut stats.cell_examples,
                                format!(
                                    "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): cell {:?} != {:?} (rust point {:?}, golden distance {golden_distance})",
                                    rust_cell, golden_cell, rust_debug
                                ),
                            );
                        }
                    }

                    // --- environment runs -------------------------------------
                    let (rust_runs, rust_env_runs) = encode_column(&execution, x, z);
                    // The Java dump writes the EnvironmentColumn's raw
                    // segments, including the degenerate first segment
                    // (`minY = 0`, `maxY = -1`) that covers nothing. The Rust
                    // reconstruction is per-y, so the empty segment is
                    // dropped before comparing.
                    let golden_env: Vec<Vec<i64>> = golden_env_runs
                        .iter()
                        .filter_map(|run| {
                            let run = run.as_array().expect("env run");
                            let min_y = run[0].as_i64().expect("minY");
                            let max_y = run[1].as_i64().expect("maxY");
                            (max_y >= min_y).then(|| {
                                vec![min_y, max_y, run[2].as_i64().expect("env")]
                            })
                        })
                        .collect();
                    if rust_env_runs != golden_env {
                        stats.env += 1;
                        chunk_env += 1;
                        record(
                            &mut stats.env_examples,
                            env_mismatch(
                                seed,
                                chunk_x,
                                chunk_z,
                                x,
                                z,
                                &rust_env_runs,
                                &golden_env,
                                &environments,
                            ),
                        );
                    }

                    // --- block runs -------------------------------------------
                    let golden_block_runs: Vec<Vec<i64>> = golden_runs
                        .iter()
                        .map(|run| {
                            let run = run.as_array().expect("block run");
                            vec![
                                run[0].as_i64().expect("yStart"),
                                run[1].as_i64().expect("yEnd"),
                                run[2].as_i64().expect("blockId"),
                                run[3].as_i64().expect("rotation"),
                                run[4].as_i64().expect("filler"),
                                run[5].as_i64().expect("fluidId"),
                                run[6].as_i64().expect("fluidLevel"),
                                run[7].as_i64().expect("priority"),
                            ]
                        })
                        .collect();
                    if rust_runs != golden_block_runs {
                        stats.blocks += 1;
                        chunk_blocks += 1;
                        let first = rust_runs
                            .iter()
                            .zip(golden_block_runs.iter())
                            .position(|(a, b)| a != b);
                        if let Some(first) = first {
                            let priority = rust_runs
                                .get(first)
                                .or_else(|| golden_block_runs.get(first))
                                .map(|run| run[7] as usize & 15)
                                .unwrap_or(0);
                            stats.priority[priority] += 1;
                            chunk_priority[priority] += 1;
                            // Per-y position differences by priority (cave=5/6,
                            // prefab=7/8/9, terrain=1/2/3/4).
                            let expand = |runs: &[Vec<i64>]| {
                                let mut states = vec![[0i64; 6]; HEIGHT as usize];
                                for run in runs {
                                    let start = run[0].max(0) as usize;
                                    let end = run[1].min(HEIGHT as i64 - 1).max(0) as usize;
                                    for state in states.iter_mut().take(end + 1).skip(start) {
                                        state.copy_from_slice(&run[2..8]);
                                    }
                                }
                                states
                            };
                            let rust_states = expand(&rust_runs);
                            let golden_states = expand(&golden_block_runs);
                            for (rs, gs) in rust_states.iter().zip(golden_states.iter()) {
                                if rs != gs {
                                    let p = rs[5].max(gs[5]) as usize & 15;
                                    stats.diff_priority[p] += 1;
                                }
                            }
                            if priority == 2 && chunk_layer_examples < 1 {
                                chunk_layer_examples += 1;
                                stats.layer_examples.push(block_mismatch(
                                    seed,
                                    chunk_x,
                                    chunk_z,
                                    x,
                                    z,
                                    &rust_runs,
                                    &golden_block_runs,
                                    &blocks,
                                    &fluids,
                                    &registries,
                                ));
                            }
                        }
                        record(
                            &mut stats.block_examples,
                            block_mismatch(
                                seed,
                                chunk_x,
                                chunk_z,
                                x,
                                z,
                                &rust_runs,
                                &golden_block_runs,
                                &blocks,
                                &fluids,
                                &registries,
                            ),
                        );
                    }
                    if rust_tint != golden_tint
                        || rust_zone.as_deref() != golden_zone_name.as_deref()
                        || rust_biome.as_deref() != golden_biome_name.as_deref()
                        || rust_env_runs != golden_env
                        || rust_runs != golden_block_runs
                    {
                        stats.columns += 1;
                        chunk_columns += 1;
                    }
                    chunk_runs += 1;
                }
            }
            chunk_summaries.push(format!(
                "seed {seed} chunk ({chunk_x},{chunk_z}): {chunk_columns} columns, tint {chunk_tint}, env {chunk_env}, blocks {chunk_blocks}, first-diff priorities {:?}",
                &chunk_priority[..10]
            ));
        }
    }

    assert_eq!(chunk_runs, 21 * 1024, "every column was compared");
    println!(
        "chunk parity: {} / {} columns differ (tint {}, raw tint {}, zone {}, biome {}, parent {}, index {}, cell {}, env {}, blocks {})",
        stats.columns, chunk_runs, stats.tint, stats.raw_tint, stats.zone, stats.biome, stats.parent, stats.index, stats.cell, stats.env, stats.blocks
    );
    println!("first-differing-block-run priority histogram: {:?}", stats.priority);
    println!("per-y differing positions by priority (cave=5/6, prefab=7/8/9): {:?}", stats.diff_priority);
    for summary in &chunk_summaries {
        println!("{summary}");
    }
    for example in &stats.layer_examples {
        println!("--- first layer mismatches ---\n{example}");
    }
    for (label, examples) in [
        ("tint", &stats.tint_examples),
        ("raw tint", &stats.raw_tint_examples),
        ("zone", &stats.zone_examples),
        ("biome", &stats.biome_examples),
        ("parent", &stats.parent_examples),
        ("index", &stats.index_examples),
        ("cell", &stats.cell_examples),
        ("env", &stats.env_examples),
        ("blocks", &stats.block_examples),
    ] {
        if !examples.is_empty() {
            eprintln!("--- first {label} mismatches ---\n{}", examples.join("\n"));
        }
    }
    mismatch_report.extend(stats.tint_examples);
    mismatch_report.extend(stats.raw_tint_examples);
    mismatch_report.extend(stats.zone_examples);
    mismatch_report.extend(stats.biome_examples);
    mismatch_report.extend(stats.parent_examples);
    mismatch_report.extend(stats.index_examples);
    mismatch_report.extend(stats.cell_examples);
    mismatch_report.extend(stats.env_examples);
    mismatch_report.extend(stats.block_examples);
    assert!(
        mismatch_report.is_empty(),
        "the chunk grids diverge from the Java pipeline ({} / {} columns; tint {}, zone {}, biome {}, parent {}, index {}, cell {}, env {}, blocks {})",
        stats.columns,
        chunk_runs,
        stats.tint,
        stats.zone,
        stats.biome,
        stats.parent,
        stats.index,
        stats.cell,
        stats.env,
        stats.blocks
    );
}

fn palette(dump: &Value, name: &str) -> HashMap<i32, String> {
    let mut map = HashMap::new();
    for (id, value) in dump["palettes"][name].as_object().expect("palette object") {
        let id: i32 = id.parse().expect("palette id");
        let name = value.as_str().unwrap_or("<unknown>");
        map.insert(id, name.to_string());
    }
    map
}

/// Parses a Java `Double.toHexString` value (`0x1.8p-2`, `-0x1.0p0`,
/// `0x0.0p0`, `Infinity`, `-Infinity`, `NaN`).
fn parse_java_hex_double(s: &str) -> f64 {
    match s {
        "Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        "NaN" => return f64::NAN,
        _ => {}
    }
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1.0f64, r),
        None => (1.0f64, s.trim_start_matches('+')),
    };
    let rest = rest
        .strip_prefix("0X")
        .or_else(|| rest.strip_prefix("0x"))
        .expect("Java hex double must start with 0x");
    let (mantissa_hex, exp_str) = rest
        .split_once('p')
        .or_else(|| rest.split_once('P'))
        .expect("must contain 'p'");
    let exp: i32 = exp_str.parse().expect("valid binary exponent");
    let (int_part, frac_part) = match mantissa_hex.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa_hex, ""),
    };
    let mut mantissa = 0.0f64;
    for ch in int_part.chars() {
        mantissa = mantissa * 16.0 + ch.to_digit(16).expect("hex digit") as f64;
    }
    let mut scale = 1.0 / 16.0;
    for ch in frac_part.chars() {
        mantissa += ch.to_digit(16).expect("hex digit") as f64 * scale;
        scale /= 16.0;
    }
    sign * mantissa * 2f64.powi(exp)
}

/// Formats an environment-run mismatch (resolved names, the first differing
/// run, the surrounding context).
#[allow(clippy::too_many_arguments)]
fn env_mismatch(
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
    x: i32,
    z: i32,
    rust: &[Vec<i64>],
    golden: &[Vec<i64>],
    environments: &HashMap<i32, String>,
) -> String {
    let first = rust.iter().zip(golden.iter()).position(|(a, b)| a != b);
    let context = |runs: &[Vec<i64>], at: Option<usize>| -> String {
        let at = at.unwrap_or(rust.len().min(golden.len()));
        runs.iter()
            .enumerate()
            .skip(at.saturating_sub(2))
            .take(5)
            .map(|(i, run)| {
                format!(
                    "  [{}]: y {}..={} env {} ({:?})",
                    i,
                    run[0],
                    run[1],
                    run[2],
                    environments.get(&(run[2] as i32))
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): env runs diverge (first differing run {first:?})\n\
         rust:\n{}\ngolden:\n{}",
        context(rust, first),
        context(golden, first)
    )
}

/// Formats a block-run mismatch (resolved names, the first differing run).
#[allow(clippy::too_many_arguments)]
fn block_mismatch(
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
    x: i32,
    z: i32,
    rust: &[Vec<i64>],
    golden: &[Vec<i64>],
    blocks: &HashMap<i32, String>,
    fluids: &HashMap<i32, String>,
    registries: &Registries,
) -> String {
    let resolve_block = |golden_id: i64| -> String {
        match blocks.get(&(golden_id as i32)) {
            Some(name) => match registries.blocks.index_of(name) {
                Some(id) => format!("{name} (rust id {id})"),
                None => format!("{name} (NOT IN THE RUST REGISTRY)"),
            },
            None => format!("golden id {golden_id}"),
        }
    };
    let run_string =
        |run: &Vec<i64>, blocks: &HashMap<i32, String>, fluids: &HashMap<i32, String>| -> String {
            let block_name = blocks
                .get(&(run[2] as i32))
                .map(|name| name.as_str())
                .unwrap_or("<air>");
            let fluid_name = if run[5] == 0 {
                "<none>".to_string()
            } else {
                fluids.get(&(run[5] as i32)).cloned().unwrap_or_default()
            };
            format!(
                "y {}..={} block {} rot {} filler {} fluid {} level {} priority {}",
                run[0], run[1], block_name, run[3], run[4], fluid_name, run[6], run[7]
            )
        };
    let first = rust.iter().zip(golden.iter()).position(|(a, b)| a != b);
    let count = |at: usize, total: &[Vec<i64>]| -> String {
        (0..at.min(total.len()))
            .map(|i| format!("{} identical runs before", i))
            .next()
            .unwrap_or_else(|| "no identical runs before".to_string())
            .to_string()
    };
    format!(
        "seed {seed} chunk ({chunk_x},{chunk_z}) column ({x},{z}): block runs diverge \
         (first differing run {first:?}; {})\n  rust:     {}\n  golden:   {} ({} / rust id via registries)",
        count(first.unwrap_or(0), golden),
        first.map_or("—".to_string(), |i| run_string(&rust[i], blocks, fluids)),
        first.map_or("—".to_string(), |i| {
            let run = &golden[i];
            format!(
                "{} block {}",
                run_string(run, blocks, fluids),
                resolve_block(run[2])
            )
        }),
        "resolved",
    )
}
