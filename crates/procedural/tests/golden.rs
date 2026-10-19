//! Golden parity tests: Rust noise implementations vs Java reference dumps.
//!
//! Reference data: `testdata/golden/noise.json`, produced by
//! `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/GoldenDumper.java`
//! (values are Java `Double.toHexString` strings — bit-exact f64 representations).
//!
//! Java sampled `noise.get(seed, offsetSeed = seed, x, y[, z])` on
//! 32x32 (2d) / 8x8x8 (3d) grids; we rebuild the same instances from the
//! `config` blocks and compare **bit patterns**.

use std::rc::Rc;

use procedural::noise::branch_noise::BranchNoise;
use procedural::noise::cell::cell_distance_function::CellDistanceFunction;
use procedural::noise::cell::evaluator::branch_evaluator::{BranchDirection, BranchEvaluator};
use procedural::noise::cell::evaluator::density_point_evaluator::DensityPointEvaluator;
use procedural::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
use procedural::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
use procedural::noise::cell::grid_cell_distance_function::GridCellPointFunction;
use procedural::noise::cell::jitter::CellJitter;
use procedural::noise::cell_noise::{CellMode, CellNoise};
use procedural::noise::constant_noise::ConstantNoise;
use procedural::noise::distance_noise::{Distance2Mode, DistanceNoise};
use procedural::noise::function::{NoiseFunction2d, NoiseFunction3d};
use procedural::noise::general_noise::InterpolationMode;
use procedural::noise::grid_noise::GridNoise;
use procedural::noise::mesh_noise::MeshNoise;
use procedural::noise::old_simplex_noise::OldSimplexNoise;
use procedural::noise::perlin_noise::PerlinNoise;
use procedural::noise::point_noise::PointNoise;
use procedural::noise::simplex_noise::SimplexNoise;
use procedural::noise::support::{AlwaysTrueIntCondition, IDoubleRange, NoiseFormula};
use procedural::noise::value_noise::ValueNoise;

// ---------------------------------------------------------------------
// Java hex-double parsing ("0x1.8p-2", "-0x1.0p0", "0x0.0p0", "Infinity")
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

    // mantissa = int_part.frac_part in hex → integer m and scale 16^-frac_len,
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
    let _ = frac_part.len();

    // value = sign * m / scale * 2^exp — computed exactly for normal doubles:
    // m < 2^53 (exact), 1/scale is a power of 4 (exact), 2^exp exact.
    let v = m / scale * (2.0f64).powi(exp);
    sign * v
}

// ---------------------------------------------------------------------
// Test-only stand-ins for not-yet-ported supplier/property classes
// ---------------------------------------------------------------------

/// Java `supplier.DoubleRange.Constant`.
struct ConstantDoubleRange(f64);
impl IDoubleRange for ConstantDoubleRange {
    fn get_value(&self, _v: f64) -> f64 {
        self.0
    }
}

/// Java `property.NoiseFormulaProperty.NoiseFormula.SQRT`.
struct SqrtFormula;
impl NoiseFormula for SqrtFormula {
    fn eval(&self, noise: f64) -> f64 {
        f64::sqrt(noise)
    }
}

// ---------------------------------------------------------------------
// Noise construction (mirrors GoldenDumper.createNoises)
// ---------------------------------------------------------------------

struct NoisePair {
    noise_2d: Box<dyn NoiseFunction2d>,
    /// `None` where Java throws UnsupportedOperationException (MESH, BRANCH).
    noise_3d: Option<Box<dyn NoiseFunction3d>>,
}

fn grid_distance_function() -> Rc<dyn CellDistanceFunction> {
    GridCellDistanceFunction::distance_function()
}

fn build_noise(key: &str) -> NoisePair {
    let df = || grid_distance_function();
    let make_2d = || -> Box<dyn NoiseFunction2d> {
        match key {
            "SIMPLEX" => Box::new(SimplexNoise::INSTANCE),
            "OLD_SIMPLEX" => Box::new(OldSimplexNoise::INSTANCE),
            "VALUE" => Box::new(ValueNoise::new(InterpolationMode::Quintic)),
            "PERLIN" => Box::new(PerlinNoise::new(InterpolationMode::Quintic)),
            "CONSTANT" => Box::new(ConstantNoise::new(0.5)),
            "GRID" => Box::new(GridNoise::new(0.1, 0.1, 0.1)),
            "CELL" => Box::new(CellNoise::new(
                df(),
                NormalPointEvaluator::euclidean(),
                Rc::new(CellMode::CellValue),
                None,
            )),
            "DISTANCE" => Box::new(DistanceNoise::new(
                df(),
                NormalPointEvaluator::euclidean(),
                Rc::new(Distance2Mode::Sub),
            )),
            "MESH" => Box::new(MeshNoise::new(
                DensityPointEvaluator::get_density_condition(None),
                0.1,
                1.0,
                1.0,
            )),
            "POINT" => Box::new(PointNoise::new(1.5, -2.0, 0.5, 1.0, 8.0)),
            "BRANCH" => Box::new(build_branch_noise()),
            other => panic!("unknown noise key {other}"),
        }
    };
    let noise_2d = make_2d();

    let noise_3d = match key {
        // Java: dump3d skips MESH and BRANCH (3d get() throws).
        "MESH" | "BRANCH" => None,
        _ => {
            let _ = make_2d; // rebuild fresh instance for 3d use
            Some(match key {
                "SIMPLEX" => Box::new(SimplexNoise::INSTANCE) as Box<dyn NoiseFunction3d>,
                "OLD_SIMPLEX" => Box::new(OldSimplexNoise::INSTANCE) as Box<dyn NoiseFunction3d>,
                "VALUE" => Box::new(ValueNoise::new(InterpolationMode::Quintic)) as Box<dyn NoiseFunction3d>,
                "PERLIN" => Box::new(PerlinNoise::new(InterpolationMode::Quintic)) as Box<dyn NoiseFunction3d>,
                "CONSTANT" => Box::new(ConstantNoise::new(0.5)) as Box<dyn NoiseFunction3d>,
                "GRID" => Box::new(GridNoise::new(0.1, 0.1, 0.1)) as Box<dyn NoiseFunction3d>,
                "CELL" => Box::new(CellNoise::new(
                    df(),
                    NormalPointEvaluator::euclidean(),
                    Rc::new(CellMode::CellValue),
                    None,
                )) as Box<dyn NoiseFunction3d>,
                "DISTANCE" => Box::new(DistanceNoise::new(
                    df(),
                    NormalPointEvaluator::euclidean(),
                    Rc::new(Distance2Mode::Sub),
                )) as Box<dyn NoiseFunction3d>,
                "POINT" => Box::new(PointNoise::new(1.5, -2.0, 0.5, 1.0, 8.0)) as Box<dyn NoiseFunction3d>,
                other => panic!("unexpected 3d noise key {other}"),
            })
        }
    };

    NoisePair { noise_2d, noise_3d }
}

fn build_branch_noise() -> BranchNoise {
    // Java GoldenDumper.createBranchNoise():
    //   parentFunction = GridCellDistanceFunction.DISTANCE_FUNCTION
    //   parentEvaluator = PointEvaluator.of(EUCLIDEAN, null, null, jitter 1.0)
    //     → plain NormalPointEvaluator (density/distanceMod null; jitter 1.0 == DEFAULT_ONE)
    //   parentValue = 0.0
    //   parentFade = DoubleRange.Constant(1.0)
    //   parentDensity = ConstantIntCondition.DEFAULT_TRUE
    //   distance2Function = DistanceNoise.Distance2Mode.DIV
    //   noiseFormula = NoiseFormulaProperty.NoiseFormula.SQRT
    //   lineFunction = GridCellDistanceFunction.DISTANCE_FUNCTION
    //   lineEvaluator = new BranchEvaluator(parentFunction,
    //        GridCellDistanceFunction.POINT_FUNCTION, OUTWARD, jitter 1.0, 0.1)
    //   lineScale = 0.1
    //   lineThickness = DoubleRange.Constant(0.1)
    let parent_function = grid_distance_function();
    let line_evaluator = BranchEvaluator::new(
        parent_function.clone(),
        GridCellPointFunction::point_function(),
        BranchDirection::Outward,
        CellJitter::of(1.0, 1.0, 1.0),
        0.1,
    );
    BranchNoise::new(
        parent_function.clone(),
        NormalPointEvaluator::euclidean(),
        0.0,
        Rc::new(ConstantDoubleRange(1.0)),
        Rc::new(AlwaysTrueIntCondition),
        Rc::new(Distance2Mode::Div),
        Rc::new(SqrtFormula),
        parent_function,
        Rc::new(line_evaluator),
        0.1,
        Rc::new(ConstantDoubleRange(0.1)),
    )
}

// ---------------------------------------------------------------------
// Parity tests
// ---------------------------------------------------------------------

fn golden_root() -> serde_json::Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/../../testdata/golden/noise.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e} — run tools/java-dumper/run.sh first"));
    serde_json::from_str(&text).expect("valid JSON")
}

fn parse_grid(meta: &serde_json::Value) -> (usize, f64, f64) {
    let size = meta["size"].as_u64().expect("grid size") as usize;
    let start = parse_java_hex_double(meta["start"].as_str().expect("grid start"));
    let step = parse_java_hex_double(meta["step"].as_str().expect("grid step"));
    (size, start, step)
}

#[test]
fn golden_noise_2d() {
    let root = golden_root();
    let (size, start, step) = parse_grid(&root["grid2d"]);
    let noise2d = &root["noise2d"];

    let mut checked = 0usize;
    for (key, entry) in noise2d.as_object().expect("noise2d object") {
        let pair = build_noise(key);
        let seeds = entry["seeds"].as_object().expect("seeds object");
        for (seed_str, rows) in seeds {
            let seed: i32 = seed_str.parse().expect("seed");
            assert_eq!(rows.as_array().unwrap().len(), size, "{key} row count");
            for (yi, row) in rows.as_array().unwrap().iter().enumerate() {
                let y = start + yi as f64 * step;
                for (xi, v) in row.as_array().unwrap().iter().enumerate() {
                    let x = start + xi as f64 * step;
                    let expected_hex = v.as_str().unwrap_or_else(|| panic!("{key} value is string"));
                    let expected = parse_java_hex_double(expected_hex);
                    let got = pair.noise_2d.get(seed, seed, x, y);
                    assert_eq!(
                        got.to_bits(),
                        expected.to_bits(),
                        "{key} 2d seed={seed} xi={xi} yi={yi} x={x} y={y}: expected {expected_hex} ({}), got {} ({:#x})",
                        expected,
                        got,
                        got.to_bits()
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 10_000, "suspiciously few checks: {checked}");
}

#[test]
fn golden_noise_3d() {
    let root = golden_root();
    let (_size, start, step) = parse_grid(&root["grid3d"]);
    let noise3d = match root.get("noise3d") {
        Some(n) => n,
        None => return,
    };

    for (key, entry) in noise3d.as_object().expect("noise3d object") {
        let pair = build_noise(key);
        let noise_3d = match &pair.noise_3d {
            Some(n) => n,
            None => continue,
        };
        let seeds = entry["seeds"].as_object().expect("seeds object");
        for (seed_str, z_rows) in seeds {
            let seed: i32 = seed_str.parse().expect("seed");
            for (zi, y_rows) in z_rows.as_array().unwrap().iter().enumerate() {
                let z = start + zi as f64 * step;
                for (yi, x_rows) in y_rows.as_array().unwrap().iter().enumerate() {
                    let y = start + yi as f64 * step;
                    for (xi, v) in x_rows.as_array().unwrap().iter().enumerate() {
                        let x = start + xi as f64 * step;
                        let expected_hex = v.as_str().unwrap();
                        let expected = parse_java_hex_double(expected_hex);
                        let got = noise_3d.get(seed, seed, x, y, z);
                        assert_eq!(
                            got.to_bits(),
                            expected.to_bits(),
                            "{key} 3d seed={seed} xi={xi} yi={yi} zi={zi} x={x} y={y} z={z}: expected {expected_hex} ({}), got {}",
                            expected,
                            got
                        );
                    }
                }
            }
        }
    }
}
