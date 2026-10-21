//! Golden parity test for the **loader layer itself**: the real
//! `testdata/Default/Mask.json` configuration is loaded through the ported
//! `procedural::json` loaders (`SeedString` derivation, `"File"` reference
//! resolution, `NoisePropertyJsonLoader` composition) and the continent noise
//! is sampled on the dumper's 16×16 grid.
//!
//! Reference data: `testdata/golden/climate.json` (`noise.seeds.<seed>` →
//! `continent`), produced by `tools/java-dumper` `Phase2Dumper.dumpClimate`:
//! Java loads `Mask.json` through `ClimateNoiseJsonLoader` and samples
//! `buffer.continent` (= `NoiseProperty.get(seed, x, y)`, see
//! `ClimateNoise.generate`) at `x = start + i*step`, `y = start + j*step`.
//! Comparisons are **bit-exact** (Java `Double.toHexString` strings).
//!
//! The golden `construction` also pins the integer seed offsets the Java
//! loaders derived from the seed strings (e.g. the continent top-level
//! `SINGLE` property's `seedOffset == 203152061` for world seed `0`, i.e.
//! `javaHash("0.NoiseProperty.NoiseProperty.NoiseProperty") * 114512143`) —
//! the loader rebuilds exactly that tree, so any seed-derivation drift fails
//! the sample comparison.

use std::path::Path;
use std::rc::Rc;

use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::seed_string::SeedString;
use procedural::json::JsonLoaderBase;
use procedural::noise::property::NoiseProperty;
use procedural::noise::supplier::IDoubleRange;
use serde_json::Value;

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

const WORLD_SEEDS: [i32; 4] = [0, 1, 42, 1234567];
const GRID_SIZE: usize = 16;
const GRID_START: f64 = -2048.0;
const GRID_STEP: f64 = 256.0;

fn manifest_dir() -> String {
    std::env::var("CARGO_MANIFEST_DIR").unwrap()
}

fn load_mask_json() -> (String, Value) {
    let root = format!("{}/../../testdata/Default", manifest_dir());
    let text = std::fs::read_to_string(format!("{root}/Mask.json"))
        .unwrap_or_else(|e| panic!("cannot read Mask.json: {e} — is testdata/ present?"));
    (root, serde_json::from_str(&text).unwrap())
}

/// Java: `new ClimateNoiseJsonLoader<>(seedString, worldDir, maskJson.get("Noise")).load()`
/// → `loadContinentNoise()` → `NoisePropertyJsonLoader(seed, dataFolder, mustGetObject("Continent"))`.
fn load_continent_noise(data_folder: &Path, mask_json: &Value, seed: i32) -> Rc<dyn NoiseProperty> {
    let noise_holder = JsonLoaderBase::new(
        SeedString::new(&seed.to_string()),
        data_folder,
        mask_json.get("Noise").cloned(),
    );
    let continent = noise_holder.must_get_object("Continent", None);
    NoisePropertyJsonLoader::new(
        SeedString::new(&seed.to_string()),
        data_folder,
        Some(continent),
    )
    .load()
}

#[test]
fn continent_noise_loader_matches_java_bit_exact() {
    let (root, mask_json) = load_mask_json();
    let data_folder = Path::new(&root);

    for seed in WORLD_SEEDS {
        let continent = load_continent_noise(data_folder, &mask_json, seed);

        let golden_path = format!("{}/../../testdata/golden/climate.json", manifest_dir());
        let golden: Value = serde_json::from_str(
            &std::fs::read_to_string(&golden_path)
                .unwrap_or_else(|e| panic!("cannot read {golden_path}: {e}")),
        )
        .unwrap();
        let golden_samples: &Vec<Value> = golden["noise"]["seeds"][seed.to_string()]["continent"]
            .as_array()
            .unwrap();

        let mut checked = 0;
        for j in 0..GRID_SIZE {
            for i in 0..GRID_SIZE {
                let x = GRID_START + i as f64 * GRID_STEP;
                let y = GRID_START + j as f64 * GRID_STEP;
                let expected =
                    parse_java_hex_double(golden_samples[j * GRID_SIZE + i].as_str().unwrap());
                let actual = continent.get(seed, x, y);
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "continent sample mismatch at seed {seed}, ({i},{j}) [{x},{y}]: bits {:x} vs {:x}",
                    actual.to_bits(),
                    expected.to_bits()
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 256);
    }
}

#[test]
fn seed_offsets_match_the_golden_construction() {
    // The dump pins the top-level continent SINGLE property's seedOffset
    // ("0.NoiseProperty.NoiseProperty.NoiseProperty"). We cannot introspect the
    // built property tree (it is opaque), but the sample test above fails on
    // any seed drift; this test pins the SeedString hash convention itself.
    let s = SeedString::new("0").append(".NoiseProperty");
    let s = s.append(".NoiseProperty");
    let s = s.append(".NoiseProperty");
    assert_eq!(s.hash_code(), 203_152_061);
}

// Silence the unused-trait warning if IDoubleRange is not required here.
#[allow(unused)]
fn _unused(_r: &dyn IDoubleRange) {}
