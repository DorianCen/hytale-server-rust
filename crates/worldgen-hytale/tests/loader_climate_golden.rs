//! Golden parity test for the **world-gen loader layer** (Subtask 7a): the
//! real `testdata/Default/Mask.json` configuration is loaded through the
//! ported `ClimateMaskJsonLoader` (and the climate loaders below it) and the
//! procedural climate mask is sampled on the dumper's 16×16 grid.
//!
//! Reference data: `testdata/golden/climate.json` (`noise.seeds.<seed>` →
//! `id` / `continent` / `temperature` / `intensity` / `fade`), produced by
//! `tools/java-dumper` `Phase2Dumper.dumpClimate`:
//!
//! ```java
//! final ClimateNoise noise = new ClimateNoiseJsonLoader<>(seedString, worldDir, maskJson.get("Noise")).load();
//! final ClimateGraph graph = new ClimateGraphJsonLoader<>(graphSeed, worldDir, maskJson.get("Climate")).load();
//! final int id = noise.generate(seed, x, y, buffer, graph);
//! ```
//!
//! Comparisons are **bit-exact** (Java `Double.toHexString` strings).
//!
//! The climate graph and the noise properties are already golden-verified
//! (`crates/worldgen-hytale/tests/golden.rs`); this test proves that the
//! *loaders* rebuild exactly the same objects — any drift in the seed
//! derivation, the defaults or the `"File"` reference resolution shows up
//! here as a sample mismatch.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use procedural::json::seed_string::SeedString;
use serde_json::Value;

use worldgen_hytale::climate::climate_mask_provider::ClimateMaskProvider;
use worldgen_hytale::climate::climate_noise::Buffer;
use worldgen_hytale::climate::climate_type::ClimateType;
use worldgen_hytale::climate::FadeMode;
use worldgen_hytale::loader::chunk_generator_json_loader::{
    ChunkGeneratorJsonLoader, LoadedMaskProvider,
};
use worldgen_hytale::loader::climate::climate_mask_json_loader::ClimateMaskJsonLoader;
use worldgen_hytale::prefab::prefab_store_root::PrefabStoreRoot;
use worldgen_hytale::world_gen_config::WorldGenConfig;

// ---------------------------------------------------------------------
// Java hex float parsing ("0x1.8p-2", "-0x1.0p0", "0x0.0p0", "Infinity")
// ---------------------------------------------------------------------

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

    // mantissa = int_part.frac_part in hex -> integer m and scale 16^-frac_len,
    // both exactly representable in f64 for <=13 hex frac digits (52 bits).
    let mut m: f64 = 0.0;
    for c in int_part.chars() {
        m = m * 16.0 + c.to_digit(16).unwrap() as f64;
    }
    let mut scale = 1.0f64;
    for c in frac_part.chars() {
        scale *= 16.0;
        m = m * 16.0 + c.to_digit(16).unwrap() as f64;
    }

    // value = sign * m / scale * 2^exp -- computed exactly for normal doubles.
    let v = m / scale * (2.0f64).powi(exp);
    sign * v
}

/// JSON value -> f64 (hex strings are parsed, plain numbers converted).
fn as_f64(v: &Value) -> f64 {
    if let Some(s) = v.as_str() {
        parse_java_hex_double(s)
    } else if let Some(i) = v.as_i64() {
        i as f64
    } else if let Some(f) = v.as_f64() {
        f
    } else {
        panic!("not a number: {v}");
    }
}

fn as_i32(v: &Value) -> i32 {
    v.as_i64().expect("integer") as i32
}

// ---------------------------------------------------------------------
// Paths / golden data
// ---------------------------------------------------------------------

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn default_world_folder() -> PathBuf {
    manifest_dir().join("../../testdata/Default")
}

fn load_golden() -> Option<Value> {
    let path = manifest_dir().join("../../testdata/golden/climate.json");
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(serde_json::from_str(&text).expect("valid JSON")),
        Err(e) => {
            eprintln!(
                "skipping: cannot read {} ({e}) — run tools/java-dumper/run.sh first",
                path.display()
            );
            None
        }
    }
}

/// The world seeds of the dumper (`Phase2Dumper.WORLD_SEEDS`).
const WORLD_SEEDS: [i32; 4] = [0, 1, 42, 1234567];
/// The climate sample grid of the dumper.
const GRID_SIZE: usize = 16;
const GRID_START: f64 = -2048.0;
const GRID_STEP: f64 = 256.0;

/// Loads the real `Mask.json` through the ported loader chain.
fn load_mask(seed: i32) -> (ClimateMaskJsonLoader, PathBuf) {
    let data_folder = default_world_folder();
    let mask_file = data_folder.join("Mask.json");
    let loader =
        ClimateMaskJsonLoader::new(SeedString::new(&seed.to_string()), &data_folder, &mask_file);
    (loader, data_folder)
}

// ---------------------------------------------------------------------
// 1. the climate mask samples
// ---------------------------------------------------------------------

#[test]
fn climate_mask_loader_matches_java_bit_exact() {
    let Some(golden) = load_golden() else {
        return;
    };

    let mut checked = 0usize;

    for seed in WORLD_SEEDS {
        let (loader, _data_folder) = load_mask(seed);
        let noise = loader.load_climate_noise();
        let graph = Rc::new(loader.load_climate_graph());

        let entry = &golden["noise"]["seeds"][seed.to_string()];
        let expected_id = entry["id"].as_array().expect("id");
        let expected_continent = entry["continent"].as_array().expect("continent");
        let expected_temperature = entry["temperature"].as_array().expect("temperature");
        let expected_intensity = entry["intensity"].as_array().expect("intensity");
        let expected_fade = entry["fade"].as_array().expect("fade");
        assert_eq!(expected_id.len(), GRID_SIZE * GRID_SIZE);

        let mut buffer = Buffer::default();
        for j in 0..GRID_SIZE {
            for i in 0..GRID_SIZE {
                let index = j * GRID_SIZE + i;
                let x = GRID_START + i as f64 * GRID_STEP;
                let y = GRID_START + j as f64 * GRID_STEP;

                let id = noise.generate(seed, x, y, &mut buffer, &graph);
                assert_eq!(id, as_i32(&expected_id[index]), "seed={seed} id[{index}]");
                assert_eq!(
                    buffer.continent.to_bits(),
                    as_f64(&expected_continent[index]).to_bits(),
                    "seed={seed} continent[{index}]"
                );
                assert_eq!(
                    buffer.temperature.to_bits(),
                    as_f64(&expected_temperature[index]).to_bits(),
                    "seed={seed} temperature[{index}]"
                );
                assert_eq!(
                    buffer.intensity.to_bits(),
                    as_f64(&expected_intensity[index]).to_bits(),
                    "seed={seed} intensity[{index}]"
                );
                assert_eq!(
                    buffer.fade.to_bits(),
                    as_f64(&expected_fade[index]).to_bits(),
                    "seed={seed} fade[{index}]"
                );

                checked += 1;
            }
        }
    }

    assert_eq!(checked, WORLD_SEEDS.len() * GRID_SIZE * GRID_SIZE);
}

// ---------------------------------------------------------------------
// 2. the climate graph construction (the climate type tree)
// ---------------------------------------------------------------------

/// Compares a loaded climate type against the dumped construction.
fn assert_climate_type(actual: &ClimateType, expected: &Value, path: &str) {
    assert_eq!(
        actual.name,
        expected["name"].as_str().expect("climate name"),
        "{path}: name"
    );

    for (key, color) in [("color", actual.color), ("island", actual.island)] {
        let dumped = &expected[key];
        assert_eq!(color.land, as_i32(&dumped["land"]), "{path}.{key}.land");
        assert_eq!(color.shore, as_i32(&dumped["shore"]), "{path}.{key}.shore");
        assert_eq!(color.ocean, as_i32(&dumped["ocean"]), "{path}.{key}.ocean");
        assert_eq!(
            color.shallow_ocean,
            as_i32(&dumped["shallowOcean"]),
            "{path}.{key}.shallowOcean"
        );
    }

    let points = expected["points"].as_array().expect("points");
    assert_eq!(actual.points.len(), points.len(), "{path}: point count");
    for (i, point) in actual.points.iter().enumerate() {
        let dumped = &points[i];
        assert_eq!(
            point.temperature.to_bits(),
            as_f64(&dumped["temperature"]).to_bits(),
            "{path}.points[{i}].temperature"
        );
        assert_eq!(
            point.intensity.to_bits(),
            as_f64(&dumped["intensity"]).to_bits(),
            "{path}.points[{i}].intensity"
        );
        assert_eq!(
            point.modifier.to_bits(),
            as_f64(&dumped["modifier"]).to_bits(),
            "{path}.points[{i}].modifier"
        );
    }

    let children = expected["children"].as_array().expect("children");
    assert_eq!(actual.children.len(), children.len(), "{path}: child count");
    for (i, child) in actual.children.iter().enumerate() {
        assert_climate_type(child, &children[i], &format!("{path}.children[{i}]"));
    }
}

#[test]
fn climate_graph_construction_matches_the_golden_dump() {
    let Some(golden) = load_golden() else {
        return;
    };

    let (loader, _data_folder) = load_mask(0);
    let graph = loader.load_climate_graph();

    assert_eq!(graph.fade_mode(), FadeMode::Children);
    assert_eq!(graph.fade_radius(), 50.0);
    assert_eq!(graph.fade_distance(), 100.0);

    let dumped = golden["construction"]["graph"]["climates"]
        .as_array()
        .expect("climates");
    let parents = graph.get_parents();
    assert_eq!(parents.len(), dumped.len());
    for (i, parent) in parents.iter().enumerate() {
        assert_climate_type(parent, &dumped[i], &format!("climates[{i}]"));
    }
}

// ---------------------------------------------------------------------
// 3. the grid seed offset
// ---------------------------------------------------------------------
#[test]
fn grid_seed_offset_matches_the_golden_construction() {
    let Some(golden) = load_golden() else {
        return;
    };

    let (loader, _data_folder) = load_mask(0);
    let noise = loader.load_climate_noise();

    let expected = as_i32(&golden["noise"]["seeds"]["0"]["construction"]["grid"]["seedOffset"]);
    assert_eq!(noise.grid.seed_offset, expected);
    // The Default world has no "Grid" section, so the loader falls back to
    // `DEFAULT_GRID` (seed offset 0, scale 1, the direct grid).
    assert_eq!(noise.grid.seed_offset, 0);
    assert_eq!(noise.grid.scale, 1.0);
}

// ---------------------------------------------------------------------
// 3. the "Masks" weighted pick of the world configuration
// ---------------------------------------------------------------------

#[test]
fn masks_pick_resolves_to_the_climate_branch() {
    let data_folder = default_world_folder();
    let config = WorldGenConfig::new(
        data_folder.clone(),
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);

    let world_json = loader.load_world_json(&data_folder.join("World.json"));
    // The Default world points at the procedural mask.
    assert_eq!(world_json["Masks"], serde_json::json!(["Mask.json"]));

    let provider = loader.load_mask_provider(&world_json, (1, 1), (0, 0));
    assert!(
        provider.as_climate().is_some(),
        "the weighted pick must resolve to the climate (Mask.json) branch"
    );
    assert!(provider.as_image().is_none());

    // The climate provider exposes the same climate graph the loader built.
    let climate: &ClimateMaskProvider = provider.as_climate().expect("climate branch");
    assert_eq!(climate.get_graph().get_parents().len(), 3);
}

#[test]
fn world_configuration_is_read() {
    let data_folder = default_world_folder();
    let config = WorldGenConfig::new(
        data_folder.clone(),
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);

    let world_json = loader.load_world_json(&data_folder.join("World.json"));
    assert_eq!(loader.load_world_size(&world_json), (1, 1));
    assert_eq!(loader.load_world_offset(&world_json), (0, 0));
    assert_eq!(
        loader.load_prefab_store(&world_json),
        PrefabStoreRoot::Assets
    );

    // The `World.json` masks entry is a single-element array -> a singleton
    // weighted map -> the pick is "Mask.json" regardless of the random draw.
    let provider: LoadedMaskProvider = loader.load_mask_provider(&world_json, (1, 1), (0, 0));
    assert!(provider.as_climate().is_some());
}

// ---------------------------------------------------------------------
// 4. the loaders are deterministic
// ---------------------------------------------------------------------

#[test]
fn the_loaded_climate_graph_is_stable() {
    let (loader, _data_folder) = load_mask(0);
    let first = loader.load_climate_graph();
    let second = loader.load_climate_graph();

    assert_eq!(first.fade_mode(), second.fade_mode());
    assert_eq!(first.fade_radius(), second.fade_radius());
    assert_eq!(first.fade_distance(), second.fade_distance());
    assert_eq!(first.get_parents().len(), second.get_parents().len());
    for (a, b) in first.get_parents().iter().zip(second.get_parents().iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.color, b.color);
        assert_eq!(a.island, b.island);
        assert_eq!(a.points.len(), b.points.len());
        assert_eq!(a.children.len(), b.children.len());
    }
}

#[test]
fn the_data_folder_is_resolved_relative_to_the_crate() {
    let folder = default_world_folder();
    assert!(folder.join("Mask.json").exists(), "{folder:?}");
    assert!(Path::new(&folder).is_dir());
}
