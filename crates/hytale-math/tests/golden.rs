//! Golden parity tests for `hytale-math` vs the Java reference dumps in
//! `testdata/golden/` (produced by `tools/java-dumper/`).
//!
//! All floating-point comparisons are **bit-exact** (`to_bits()`), matching
//! the Java `Double.toHexString` / `Float.toHexString` dump format.

use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use hytale_math::math_util;

// ---------------------------------------------------------------------
// Java hex float parsing
// ---------------------------------------------------------------------

fn parse_hex_f64(s: &str) -> f64 {
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
        .strip_prefix("0X").or_else(|| rest.strip_prefix("0x"))
        .expect("hex prefix");
    let (mantissa_hex, exp_str) = rest.split_once('p').or_else(|| rest.split_once('P')).expect("p exponent");
    let exp: i32 = exp_str.parse().expect("exponent");
    let (int_part, frac_part) = match mantissa_hex.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa_hex, ""),
    };
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
    sign * (m / scale * (2.0f64).powi(exp))
}

fn parse_hex_f32(s: &str) -> f32 {
    parse_hex_f64(s) as f32 // Float.toHexString values are exactly representable in f64 too
}

/// JSON value → f64: hex strings parse, integer numbers convert.
fn as_f64(v: &serde_json::Value) -> f64 {
    if let Some(s) = v.as_str() {
        parse_hex_f64(s)
    } else if let Some(i) = v.as_i64() {
        i as f64
    } else {
        v.as_f64().expect("numeric")
    }
}

/// JSON value → f32 (Java floats are dumped as hex strings).
fn as_f32(v: &serde_json::Value) -> f32 {
    if let Some(s) = v.as_str() {
        parse_hex_f32(s)
    } else if let Some(i) = v.as_i64() {
        i as f32
    } else {
        v.as_f64().expect("numeric") as f32
    }
}

fn as_i32(v: &serde_json::Value) -> i32 {
    v.as_i64().expect("int") as i32
}
fn as_i64(v: &serde_json::Value) -> i64 {
    v.as_i64().expect("int")
}

fn load(name: &str) -> serde_json::Value {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/../../testdata/golden/{name}");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e} — run tools/java-dumper/run.sh first"));
    serde_json::from_str(&text).expect("valid JSON")
}

// ---------------------------------------------------------------------
// FastRandom
// ---------------------------------------------------------------------

#[test]
fn golden_fastrandom() {
    let root = load("fastrandom.json");
    for (seed_str, entry) in root["seeds"].as_object().expect("seeds") {
        let seed = seed_str.parse::<i64>().expect("seed");
        let mut rng = FastRandom::new(seed);
        for expected in entry["nextInt_256"].as_array().unwrap() {
            assert_eq!(rng.next_int(), as_i32(expected), "nextInt seed={seed}");
        }
        for expected in entry["nextInt_bound100_128"].as_array().unwrap() {
            assert_eq!(rng.next_int_bound(100), as_i32(expected), "nextInt(100) seed={seed}");
        }
        for expected in entry["nextLong_128"].as_array().unwrap() {
            assert_eq!(rng.next_long(), as_i64(expected), "nextLong seed={seed}");
        }
        for expected in entry["nextDouble_128"].as_array().unwrap() {
            assert_eq!(
                rng.next_double().to_bits(),
                as_f64(expected).to_bits(),
                "nextDouble seed={seed}"
            );
        }
        for expected in entry["nextFloat_128"].as_array().unwrap() {
            assert_eq!(
                rng.next_float().to_bits(),
                as_f32(expected).to_bits(),
                "nextFloat seed={seed}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// HashUtil
// ---------------------------------------------------------------------

#[test]
fn golden_hashutil() {
    let root = load("hashutil.json");
    for arity in 1..=4 {
        let read_ins = |row: &serde_json::Value| -> Vec<i64> {
            match row["in"].as_array() {
                Some(arr) => arr.iter().map(as_i64).collect(),
                None => vec![as_i64(&row["in"])],
            }
        };
        for row in root[&format!("hash_{arity}")].as_array().unwrap() {
            let ins = read_ins(row);
            let expected = as_i64(&row["out"]);
            let got = match arity {
                1 => hash_util::hash(ins[0]),
                2 => hash_util::hash2(ins[0], ins[1]),
                3 => hash_util::hash3(ins[0], ins[1], ins[2]),
                _ => hash_util::hash4(ins[0], ins[1], ins[2], ins[3]),
            };
            assert_eq!(got, expected, "hash_{arity} {ins:?}");
        }
        for row in root[&format!("rehash_{arity}")].as_array().unwrap() {
            let ins = read_ins(row);
            let expected = as_i64(&row["out"]);
            let got = match arity {
                1 => hash_util::rehash(ins[0]),
                2 => hash_util::rehash2(ins[0], ins[1]),
                3 => hash_util::rehash3(ins[0], ins[1], ins[2]),
                _ => hash_util::rehash4(ins[0], ins[1], ins[2], ins[3]),
            };
            assert_eq!(got, expected, "rehash_{arity} {ins:?}");
        }
        for row in root[&format!("random_{arity}")].as_array().unwrap() {
            let ins = read_ins(row);
            let expected = as_f64(&row["out"]);
            let got = match arity {
                1 => hash_util::random(ins[0]),
                2 => hash_util::random2(ins[0], ins[1]),
                3 => hash_util::random3(ins[0], ins[1], ins[2]),
                _ => hash_util::random4(ins[0], ins[1], ins[2], ins[3]),
            };
            assert_eq!(got.to_bits(), expected.to_bits(), "random_{arity} {ins:?}");
        }
    }

    for row in root["randomInt"].as_array().unwrap() {
        let got = hash_util::random_int(
            as_i64(&row["l1"]),
            as_i64(&row["l2"]),
            as_i64(&row["l3"]),
            as_i32(&row["bound"]),
        );
        assert_eq!(got, as_i32(&row["out"]), "randomInt {row}");
    }

    // UUID strings → (most, least) significant bits, Java UUID layout.
    for row in root["hashUuid"].as_array().unwrap() {
        let uuid = row["uuid"].as_str().unwrap().replace('-', "");
        let most = u64::from_str_radix(&uuid[..16], 16).unwrap() as i64;
        let least = u64::from_str_radix(&uuid[16..], 16).unwrap() as i64;
        let got = hash_util::hash_uuid(least, most);
        assert_eq!(got, as_i64(&row["out"]), "hashUuid {row}");
    }
}

// ---------------------------------------------------------------------
// MathUtil
// ---------------------------------------------------------------------

/// `out` for f64-returning functions: bit-exact against the hex dump.
fn check_f64_rows(root: &serde_json::Value, key: &str, f: impl Fn(&serde_json::Value) -> f64) {
    for row in root[key].as_array().unwrap() {
        let expected = as_f64(&row["out"]);
        let got = f(row);
        assert_eq!(
            got.to_bits(),
            expected.to_bits(),
            "{key} row {row}: expected {expected}, got {got}"
        );
    }
}

fn check_f32_rows(root: &serde_json::Value, key: &str, f: impl Fn(&serde_json::Value) -> f32) {
    for row in root[key].as_array().unwrap() {
        let expected = as_f32(&row["out"]);
        let got = f(row);
        assert_eq!(
            got.to_bits(),
            expected.to_bits(),
            "{key} row {row}: expected {expected}, got {got}"
        );
    }
}

#[test]
fn golden_mathutil() {
    let root = load("mathutil.json");

    // int in / int out
    check_f64_rows(&root, "abs_int", |r| math_util::abs(as_i32(&r["in"])) as f64);
    for row in root["abs_int"].as_array().unwrap() {
        assert_eq!(math_util::abs(as_i32(&row["in"])), as_i32(&row["out"]), "abs_int {row}");
    }
    for row in root["byteCount"].as_array().unwrap() {
        assert_eq!(math_util::byte_count(as_i32(&row["in"])), as_i32(&row["out"]), "byteCount {row}");
    }

    // f64 -> i32/i64
    for row in root["floor_double"].as_array().unwrap() {
        assert_eq!(math_util::floor(as_f64(&row["in"])), as_i32(&row["out"]), "floor {row}");
    }
    for row in root["ceil_double"].as_array().unwrap() {
        assert_eq!(math_util::ceil(as_f64(&row["in"])), as_i32(&row["out"]), "ceil {row}");
    }
    for row in root["fastFloor_double"].as_array().unwrap() {
        assert_eq!(math_util::fast_floor_f64(as_f64(&row["in"])), as_i64(&row["out"]), "fastFloor f64 {row}");
    }
    for row in root["fastFloor_float"].as_array().unwrap() {
        assert_eq!(math_util::fast_floor_f32(as_f32(&row["in"])), as_i32(&row["out"]), "fastFloor f32 {row}");
    }
    for row in root["fastCeil_double"].as_array().unwrap() {
        assert_eq!(math_util::fast_ceil_f64(as_f64(&row["in"])), as_i64(&row["out"]), "fastCeil f64 {row}");
    }
    for row in root["fastCeil_float"].as_array().unwrap() {
        assert_eq!(math_util::fast_ceil_f32(as_f32(&row["in"])), as_i32(&row["out"]), "fastCeil f32 {row}");
    }
    for row in root["fastRound_double"].as_array().unwrap() {
        assert_eq!(math_util::fast_round_f64(as_f64(&row["in"])), as_i64(&row["out"]), "fastRound f64 {row}");
    }
    for row in root["fastRound_float"].as_array().unwrap() {
        assert_eq!(math_util::fast_round_f32(as_f32(&row["in"])), as_i32(&row["out"]), "fastRound f32 {row}");
    }

    // clamps
    for row in root["clamp_double"].as_array().unwrap() {
        let got = math_util::clamp_f64(as_f64(&row["v"]), as_f64(&row["min"]), as_f64(&row["max"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "clamp f64 {row}");
    }
    for row in root["clamp_float"].as_array().unwrap() {
        let got = math_util::clamp_f32(as_f32(&row["v"]), as_f32(&row["min"]), as_f32(&row["max"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "clamp f32 {row}");
    }
    for row in root["clamp_int"].as_array().unwrap() {
        assert_eq!(
            math_util::clamp_i32(as_i32(&row["v"]), as_i32(&row["min"]), as_i32(&row["max"])),
            as_i32(&row["out"]),
            "clamp i32 {row}"
        );
    }
    for row in root["clamp_long"].as_array().unwrap() {
        assert_eq!(
            math_util::clamp_i64(as_i64(&row["v"]), as_i64(&row["min"]), as_i64(&row["max"])),
            as_i64(&row["out"]),
            "clamp i64 {row}"
        );
    }

    // lerp family
    for row in root["lerp_double"].as_array().unwrap() {
        let got = math_util::lerp_f64(as_f64(&row["a"]), as_f64(&row["b"]), as_f64(&row["t"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "lerp f64 {row}");
    }
    for row in root["lerpUnclamped_double"].as_array().unwrap() {
        let got = math_util::lerp_unclamped_f64(as_f64(&row["a"]), as_f64(&row["b"]), as_f64(&row["t"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "lerpUnclamped f64 {row}");
    }
    for row in root["lerp_float"].as_array().unwrap() {
        let got = math_util::lerp_f32(as_f32(&row["a"]), as_f32(&row["b"]), as_f32(&row["t"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "lerp f32 {row}");
    }
    for row in root["lerpUnclamped_float"].as_array().unwrap() {
        let got = math_util::lerp_unclamped_f32(as_f32(&row["a"]), as_f32(&row["b"]), as_f32(&row["t"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "lerpUnclamped f32 {row}");
    }
    for row in root["seededLerp_float"].as_array().unwrap() {
        let got = math_util::seeded_lerp(as_f32(&row["a"]), as_f32(&row["b"]), as_i64(&row["seed"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "seededLerp {row}");
    }
    for row in root["floorMod_double"].as_array().unwrap() {
        let got = math_util::floor_mod_f64(as_f64(&row["x"]), as_f64(&row["y"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "floorMod {row}");
    }
    for row in root["compareAngle_double"].as_array().unwrap() {
        let got = math_util::compare_angle(as_f64(&row["a"]), as_f64(&row["b"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "compareAngle {row}");
    }
    for row in root["shortAngleDistance_float"].as_array().unwrap() {
        let got = math_util::short_angle_distance(as_f32(&row["a"]), as_f32(&row["b"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "shortAngleDistance {row}");
    }
    for row in root["lerpAngle_float"].as_array().unwrap() {
        let got = math_util::lerp_angle(as_f32(&row["a"]), as_f32(&row["b"]), as_f32(&row["t"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "lerpAngle {row}");
    }
    for row in root["wrapAngle_float"].as_array().unwrap() {
        let got = math_util::wrap_angle(as_f32(&row["angle"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "wrapAngle {row}");
    }

    // half floats / packing
    for row in root["halfFloatToFloat"].as_array().unwrap() {
        let got = math_util::half_float_to_float(as_i32(&row["in"]));
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "halfFloatToFloat {row}");
    }
    for row in root["halfFloatFromFloat"].as_array().unwrap() {
        assert_eq!(
            math_util::half_float_from_float(as_f32(&row["in"])),
            as_i32(&row["out"]),
            "halfFloatFromFloat {row}"
        );
    }
    for row in root["packInt"].as_array().unwrap() {
        let packed = math_util::pack_int(as_i32(&row["x"]), as_i32(&row["z"]));
        assert_eq!(packed, as_i32(&row["packed"]), "packInt {row}");
        assert_eq!(math_util::unpack_left_int(packed), as_i32(&row["unpackLeft"]), "packInt unpackLeft {row}");
        assert_eq!(math_util::unpack_right_int(packed), as_i32(&row["unpackRight"]), "packInt unpackRight {row}");
    }
    for row in root["packLong"].as_array().unwrap() {
        let packed = math_util::pack_long(as_i32(&row["left"]), as_i32(&row["right"]));
        assert_eq!(packed, as_i64(&row["packed"]), "packLong {row}");
        assert_eq!(math_util::unpack_left_long(packed), as_i32(&row["unpackLeft"]), "packLong unpackLeft {row}");
        assert_eq!(math_util::unpack_right_long(packed), as_i32(&row["unpackRight"]), "packLong unpackRight {row}");
    }

    // lengths
    for row in root["lengthSquared_2d"].as_array().unwrap() {
        let got = math_util::length_squared2_f64(as_f64(&row["x"]), as_f64(&row["y"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "lengthSquared2 {row}");
    }
    for row in root["length_2d"].as_array().unwrap() {
        let got = math_util::length2_f64(as_f64(&row["x"]), as_f64(&row["y"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "length2 {row}");
    }
    for row in root["lengthSquared_3d"].as_array().unwrap() {
        let got = math_util::length_squared3_f64(as_f64(&row["x"]), as_f64(&row["y"]), as_f64(&row["z"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "lengthSquared3 {row}");
    }
    for row in root["length_3d"].as_array().unwrap() {
        let got = math_util::length3_f64(as_f64(&row["x"]), as_f64(&row["y"]), as_f64(&row["z"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "length3 {row}");
    }

    // misc
    for row in root["round_double"].as_array().unwrap() {
        let got = math_util::round(as_f64(&row["d"]), as_i32(&row["p"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "round {row}");
    }
    for row in root["within_double"].as_array().unwrap() {
        assert_eq!(
            math_util::within(as_f64(&row["val"]), as_f64(&row["min"]), as_f64(&row["max"])),
            row["out"].as_bool().unwrap(),
            "within {row}"
        );
    }
    for row in root["clipToZero_double"].as_array().unwrap() {
        let got = math_util::clip_to_zero_f64(as_f64(&row["v"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "clipToZero {row}");
    }
    for row in root["closeToZero_double"].as_array().unwrap() {
        assert_eq!(
            math_util::close_to_zero_f64(as_f64(&row["v"])),
            row["out"].as_bool().unwrap(),
            "closeToZero {row}"
        );
    }
    for row in root["getPercentageOf"].as_array().unwrap() {
        assert_eq!(
            math_util::get_percentage_of(as_i32(&row["index"]), as_i32(&row["max"])),
            as_i32(&row["out"]),
            "getPercentageOf {row}"
        );
    }
    for row in root["percent"].as_array().unwrap() {
        let got = math_util::percent(as_i32(&row["v"]), as_i32(&row["total"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "percent {row}");
    }
    for row in root["saturatedMultiply"].as_array().unwrap() {
        assert_eq!(
            math_util::saturated_multiply(as_i64(&row["a"]), as_i64(&row["b"])),
            as_i64(&row["out"]),
            "saturatedMultiply {row}"
        );
    }

    // line distances
    for row in root["distanceToLineSq"].as_array().unwrap() {
        let got = math_util::distance_to_line_sq(
            as_f64(&row["x"]),
            as_f64(&row["y"]),
            as_f64(&row["ax"]),
            as_f64(&row["ay"]),
            as_f64(&row["bx"]),
            as_f64(&row["by"]),
        );
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "distanceToLineSq {row}");
    }
    for row in root["distanceToInfLineSq"].as_array().unwrap() {
        let got = math_util::distance_to_inf_line_sq(
            as_f64(&row["x"]),
            as_f64(&row["y"]),
            as_f64(&row["ax"]),
            as_f64(&row["ay"]),
            as_f64(&row["bx"]),
            as_f64(&row["by"]),
        );
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "distanceToInfLineSq {row}");
    }
    for row in root["sideOfLine"].as_array().unwrap() {
        assert_eq!(
            math_util::side_of_line(
                as_f64(&row["x"]),
                as_f64(&row["y"]),
                as_f64(&row["ax"]),
                as_f64(&row["ay"]),
                as_f64(&row["bx"]),
                as_f64(&row["by"]),
            ),
            as_i32(&row["out"]),
            "sideOfLine {row}"
        );
    }
    for row in root["mapToRange_float"].as_array().unwrap() {
        let got = math_util::map_to_range(
            as_f32(&row["value"]),
            as_f32(&row["valueMin"]),
            as_f32(&row["valueMax"]),
            as_f32(&row["rangeMin"]),
            as_f32(&row["rangeMax"]),
        );
        assert_eq!(got.to_bits(), as_f32(&row["out"]).to_bits(), "mapToRange {row}");
    }
    for row in root["percentile"].as_array().unwrap() {
        let data: Vec<i64> = row["sortedData"].as_array().unwrap().iter().map(|v| as_i64(v)).collect();
        let got = math_util::percentile(&data, as_f64(&row["percentile"]));
        assert_eq!(got.to_bits(), as_f64(&row["out"]).to_bits(), "percentile {row}");
    }
}
