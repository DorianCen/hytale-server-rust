//! Golden parity tests for the Phase-2 world mask (`zoom`), the procedural
//! climate mask (`climate`) of the Default world and the block / fluid /
//! environment registries of the base-game asset pack (`registries`).
//!
//! Reference data: `testdata/golden/zoom.json`, `testdata/golden/climate.json`
//! and `testdata/golden/registries.json`, produced by `tools/java-dumper` (see
//! `Phase2Dumper.java` and `RegistryDumper.java`). The Java side loads
//! the real `testdata/Default/Mask.json` configuration through the real loaders
//! (`ClimateNoiseJsonLoader`, `ClimateGraphJsonLoader`,
//! `CoordinateRandomizerJsonLoader`) and dumps
//!
//! * the **resolved construction** of every noise property (including the integer
//!   seed offsets the loaders derived from the seed strings) so this test can
//!   rebuild the exact same graph, and
//! * the sampled results of the ported code paths.
//!
//! The registry dump comes from the real CoreServer asset pipeline (the vanilla
//! `AssetRegistryLoader`) running over `Assets/Assets.zip`, so the block, fluid
//! and environment ids it contains are the ids the Java world generator uses.
//!
//! All floating-point comparisons are **bit-exact** (`to_bits()`), matching the
//! Java `Double.toHexString` dump format.

use std::rc::Rc;

use procedural::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
use procedural::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
use procedural::noise::constant_noise::ConstantNoise;
use procedural::noise::function::NoiseFunction;
use procedural::noise::grid_noise::GridNoise;
use procedural::noise::point::PointGenerator;
use procedural::noise::point_noise::PointNoise;
use procedural::noise::property::blend_noise_property::BlendNoiseProperty;
use procedural::noise::property::distorted_noise_property::DistortedNoiseProperty;
use procedural::noise::property::fractal_noise_property::{FractalMode, FractalNoiseProperty};
use procedural::noise::property::normalize_noise_property::NormalizeNoiseProperty;
use procedural::noise::property::offset_noise_property::OffsetNoiseProperty;
use procedural::noise::property::scale_noise_property::ScaleNoiseProperty;
use procedural::noise::property::single_noise_property::SingleNoiseProperty;
use procedural::noise::property::sum_noise_property::{SumNoiseProperty, SumNoisePropertyEntry};
use procedural::noise::property::NoiseProperty;
use procedural::noise::random::{
    AmplitudeNoiseProperty, CoordinateRandomizer, ICoordinateRandomizer,
};
use procedural::noise::simplex_noise::SimplexNoise;
use serde_json::Value;

use worldgen_hytale::biome::biome_pattern_generator::BiomePatternGenerator;
use worldgen_hytale::biome::tile_biome::TileBiome;
use worldgen_hytale::climate::direct_grid::direct_grid;
use worldgen_hytale::climate::{
    Buffer, ClimateColor, ClimateGraph, ClimateNoise, ClimatePoint, ClimateType, FadeMode, Grid,
    Thresholds,
};
use worldgen_hytale::weighted_map::{IWeightedMap, WeightedMap};
use worldgen_hytale::zone::discovery_config::ZoneDiscoveryConfig;
use worldgen_hytale::zone::unique::UniqueEntry;
use worldgen_hytale::zone::zone::Zone;
use worldgen_hytale::zoom::{FuzzyZoom, PixelProvider};

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
// Golden data
// ---------------------------------------------------------------------

/// Loads a golden file; `None` when the file is missing (the test then skips
/// with a message instead of failing — the dumps are regenerated by
/// `tools/java-dumper/run.sh`).
fn load_golden(name: &str) -> Option<Value> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/../../testdata/golden/{name}");
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(serde_json::from_str(&text).expect("valid JSON")),
        Err(e) => {
            eprintln!("skipping: cannot read {path} ({e}) — run tools/java-dumper/run.sh first");
            None
        }
    }
}

// ---------------------------------------------------------------------
// Construction builders (the dumped trees mirror the Java objects 1:1)
// ---------------------------------------------------------------------

fn build_function(v: &Value) -> Rc<dyn NoiseFunction> {
    match v["kind"].as_str().expect("function kind") {
        "SIMPLEX" => Rc::new(SimplexNoise::new()),
        "CONSTANT" => Rc::new(ConstantNoise::new(as_f64(&v["value"]))),
        "POINT" => Rc::new(PointNoise::from_squares(
            as_f64(&v["x"]),
            as_f64(&v["y"]),
            as_f64(&v["z"]),
            as_f64(&v["innerRadius2"]),
            as_f64(&v["outerRadius2"]),
        )),
        "GRID" => Rc::new(GridNoise::new(
            as_f64(&v["thicknessX"]),
            as_f64(&v["thicknessY"]),
            as_f64(&v["thicknessZ"]),
        )),
        other => panic!("unsupported noise function: {other}"),
    }
}

fn build_generators(v: &Value) -> Vec<AmplitudeNoiseProperty> {
    v.as_array()
        .expect("generators array")
        .iter()
        .map(|g| {
            AmplitudeNoiseProperty::new(build_property(&g["property"]), as_f64(&g["amplitude"]))
        })
        .collect()
}

fn build_randomizer(v: &Value) -> Rc<dyn ICoordinateRandomizer> {
    if v["kind"].as_str() != Some("COORDINATE_RANDOMIZER") {
        return procedural::noise::random::empty_randomizer();
    }
    Rc::new(CoordinateRandomizer::new(
        build_generators(&v["x"]),
        build_generators(&v["y"]),
        build_generators(&v["z"]),
    ))
}

fn build_property(v: &Value) -> Rc<dyn NoiseProperty> {
    match v["kind"].as_str().expect("property kind") {
        "SINGLE" => Rc::new(SingleNoiseProperty::new(
            as_i32(&v["seedOffset"]),
            build_function(&v["function"]),
        )),
        "FRACTAL" => {
            let mode = match v["mode"].as_str().expect("fractal mode") {
                "FBM" => FractalMode::Fbm,
                "BILLOW" => FractalMode::Billow,
                "MULTI_RIGID" => FractalMode::MultiRigid,
                "OLDSCHOOL" => FractalMode::Oldschool,
                "MIN" => FractalMode::Min,
                "MAX" => FractalMode::Max,
                other => panic!("unsupported fractal mode: {other}"),
            };
            Rc::new(FractalNoiseProperty::new(
                as_i32(&v["seedOffset"]),
                build_function(&v["function"]),
                mode,
                as_i32(&v["octaves"]),
                as_f64(&v["lacunarity"]),
                as_f64(&v["persistence"]),
            ))
        }
        "NORMALIZE" => Rc::new(NormalizeNoiseProperty::new(
            build_property(&v["noise"]),
            as_f64(&v["min"]),
            as_f64(&v["range"]),
        )),
        "SCALE" => Rc::new(ScaleNoiseProperty::new(
            build_property(&v["noise"]),
            as_f64(&v["scaleX"]),
            as_f64(&v["scaleY"]),
            as_f64(&v["scaleZ"]),
        )),
        "OFFSET" => Rc::new(OffsetNoiseProperty::new(
            build_property(&v["noise"]),
            as_f64(&v["offsetX"]),
            as_f64(&v["offsetY"]),
            as_f64(&v["offsetZ"]),
        )),
        "DISTORTED" => Rc::new(DistortedNoiseProperty::new(
            build_property(&v["noise"]),
            build_randomizer(&v["randomizer"]),
        )),
        "BLEND" => {
            let alpha = build_property(&v["alpha"]);
            let noises: Vec<Rc<dyn NoiseProperty>> = v["noises"]
                .as_array()
                .expect("blend noises")
                .iter()
                .map(build_property)
                .collect();
            let thresholds: Vec<f64> = v["thresholds"]
                .as_array()
                .expect("blend thresholds")
                .iter()
                .map(as_f64)
                .collect();
            Rc::new(BlendNoiseProperty::new(alpha, noises, thresholds))
        }
        "SUM" => {
            let entries: Vec<SumNoisePropertyEntry> = v["entries"]
                .as_array()
                .expect("sum entries")
                .iter()
                .map(|e| {
                    SumNoisePropertyEntry::new(build_property(&e["noise"]), as_f64(&e["factor"]))
                })
                .collect();
            Rc::new(SumNoiseProperty::new(entries))
        }
        other => panic!("unsupported noise property: {other}"),
    }
}

fn build_color(v: &Value) -> ClimateColor {
    ClimateColor::new(
        as_i32(&v["land"]),
        as_i32(&v["shore"]),
        as_i32(&v["ocean"]),
        as_i32(&v["shallowOcean"]),
    )
}

fn build_climate_type(v: &Value) -> Rc<ClimateType> {
    let points: Vec<ClimatePoint> = v["points"]
        .as_array()
        .expect("climate points")
        .iter()
        .map(|p| {
            ClimatePoint::with_modifier(
                as_f64(&p["temperature"]),
                as_f64(&p["intensity"]),
                as_f64(&p["modifier"]),
            )
        })
        .collect();
    let children: Vec<Rc<ClimateType>> = v["children"]
        .as_array()
        .expect("climate children")
        .iter()
        .map(build_climate_type)
        .collect();
    Rc::new(ClimateType::new(
        v["name"].as_str().expect("climate name"),
        build_color(&v["color"]),
        build_color(&v["island"]),
        points,
        children,
    ))
}

/// A zone with an empty biome pattern generator. The unique-zone scan only
/// uses the entry's radius, padding and parent colours — the zone handle
/// itself is never dereferenced (the Java dumper passes `null` there).
fn stub_zone() -> Rc<Zone> {
    let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(WeightedMap::builder().build());
    Rc::new(Zone {
        id: 1,
        name: "zone-1".to_string(),
        discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
        cave_generator: None,
        unique_prefab_container: Rc::new(
            worldgen_hytale::container::unique_prefab_container::UniquePrefabContainer::new(
                0,
                Vec::new(),
            ),
        ),
        biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
            Rc::new(PointGenerator::new(
                0,
                GridCellDistanceFunction::distance_function(),
                NormalPointEvaluator::euclidean(),
            )),
            tile_biomes,
            Vec::new(),
        )),
    })
}

// ---------------------------------------------------------------------
// Zoom (world mask) parity
// ---------------------------------------------------------------------

/// `maxPositions` of the `generateUniqueZoneCandidates` calls (must match the
/// Java dumper).
const UNIQUE_MAX_POSITIONS: usize = 64;

#[test]
fn zoom_golden_parity() {
    let Some(root) = load_golden("zoom.json") else {
        return;
    };

    let mask = &root["construction"]["mask"];
    let width = mask["imageWidth"].as_u64().expect("image width") as usize;
    let height = mask["imageHeight"].as_u64().expect("image height") as usize;
    let zoom_x = as_f64(&mask["blocksPerPixel"]);
    let zoom_y = zoom_x;
    let offset_x = as_i32(&mask["offsetX"]);
    let offset_y = as_i32(&mask["offsetY"]);

    let coordinates: Vec<(f64, f64)> = root["grid"]["coordinates"]
        .as_array()
        .expect("coordinates")
        .iter()
        .map(|c| (as_f64(&c[0]), as_f64(&c[1])))
        .collect();

    let mut checked = 0usize;
    for (seed_str, entry) in root["seeds"].as_object().expect("seeds object") {
        let seed: i32 = seed_str.parse().expect("seed");

        // The rasterised continent mask (Java PixelProvider array order).
        let pixels: Vec<i32> = mask["pixels"][seed_str]
            .as_array()
            .expect("pixels")
            .iter()
            .map(as_i32)
            .collect();
        let provider = PixelProvider::from_pixels(width, height, pixels);

        // The real Mask.json randomizer (SIMPLEX, scale 0.01, amplitude 16.0).
        let randomizer =
            build_randomizer(&root["construction"]["randomizerConstruction"][seed_str]);
        let zoom = FuzzyZoom::new(randomizer, provider, zoom_x, zoom_y, offset_x, offset_y);

        let expected_get_x = entry["getX"].as_array().expect("getX");
        let expected_get_y = entry["getY"].as_array().expect("getY");
        let expected_mask = entry["mask"].as_array().expect("mask");
        let expected_distance = entry["distance"].as_array().expect("distance");
        let expected_distance_raw = entry["distanceRaw"].as_array().expect("distanceRaw");
        let expected_in_bounds = entry["inBounds"].as_array().expect("inBounds");

        assert_eq!(expected_get_x.len(), coordinates.len());
        for (i, (x, z)) in coordinates.iter().enumerate() {
            // Java FuzzyZoom.getX/getY -- the fuzzy domain warp.
            let rx = zoom.get_x(seed, *x, *z);
            let rz = zoom.get_y(seed, *x, *z);
            assert_eq!(
                rx.to_bits(),
                as_f64(&expected_get_x[i]).to_bits(),
                "seed={seed} idx={i} getX: expected {}, got {}",
                expected_get_x[i],
                rx
            );
            assert_eq!(
                rz.to_bits(),
                as_f64(&expected_get_y[i]).to_bits(),
                "seed={seed} idx={i} getY: expected {}, got {}",
                expected_get_y[i],
                rz
            );

            // The mask is sampled at the warped coordinates (ZonePatternGenerator).
            let pixel = zoom.generate(rx, rz) & 0xFFFFFF;
            assert_eq!(pixel, as_i32(&expected_mask[i]), "seed={seed} idx={i} mask");

            let in_bounds = zoom.in_bounds(rx, rz);
            assert_eq!(
                in_bounds,
                expected_in_bounds[i].as_bool().expect("inBounds"),
                "seed={seed} idx={i} inBounds"
            );

            let raw = zoom.distance(rx, rz);
            assert_eq!(
                raw.to_bits(),
                as_f64(&expected_distance_raw[i]).to_bits(),
                "seed={seed} idx={i} distanceRaw: expected {}, got {}",
                expected_distance_raw[i],
                raw
            );

            // ZonePatternGenerator only asks for the distance when the warped
            // coordinate is inside the mask.
            let distance = if in_bounds { raw } else { f64::INFINITY };
            assert_eq!(
                distance.to_bits(),
                as_f64(&expected_distance[i]).to_bits(),
                "seed={seed} idx={i} distance: expected {}, got {}",
                expected_distance[i],
                distance
            );

            checked += 1;
        }

        // Java ExactZoom.generateUniqueZoneCandidates — the unique-zone scan
        // over the rasterised mask.
        let expected_candidates = entry["uniqueZoneCandidates"]
            .as_array()
            .expect("uniqueZoneCandidates");
        let zone = stub_zone();
        let unique_entries: Vec<UniqueEntry> = expected_candidates
            .iter()
            .map(|c| {
                UniqueEntry::new(
                    zone.clone(),
                    as_i32(&c["color"]),
                    c["parent"]
                        .as_array()
                        .expect("parent colours")
                        .iter()
                        .map(as_i32)
                        .collect(),
                    as_i32(&c["radius"]),
                    as_i32(&c["padding"]),
                )
            })
            .collect();
        let candidates =
            zoom.generate_unique_zone_candidates(&unique_entries, UNIQUE_MAX_POSITIONS);
        assert_eq!(candidates.len(), expected_candidates.len());
        for (i, candidate) in candidates.iter().enumerate() {
            let expected_positions = expected_candidates[i]["positions"]
                .as_array()
                .expect("positions");
            assert_eq!(
                candidate.positions.len(),
                expected_positions.len(),
                "seed={seed} candidate {i} position count"
            );
            for (j, position) in candidate.positions.iter().enumerate() {
                assert_eq!(
                    position.x,
                    as_i32(&expected_positions[j][0]),
                    "seed={seed} candidate {i} position {j} x"
                );
                assert_eq!(
                    position.y,
                    as_i32(&expected_positions[j][1]),
                    "seed={seed} candidate {i} position {j} y"
                );
            }
            checked += candidate.positions.len();
        }
    }

    assert!(checked > 1_000, "suspiciously few checks: {checked}");
}

// ---------------------------------------------------------------------
// Climate parity
// ---------------------------------------------------------------------

#[test]
fn climate_golden_parity() {
    let Some(root) = load_golden("climate.json") else {
        return;
    };

    // --- climate graph (table + fade map) -----------------------------
    let graph_meta = &root["construction"]["graph"];
    let parents: Vec<Rc<ClimateType>> = graph_meta["climates"]
        .as_array()
        .expect("climates")
        .iter()
        .map(build_climate_type)
        .collect();
    let fade_mode = match graph_meta["fadeMode"].as_str().expect("fade mode") {
        "PARENTS" => FadeMode::Parents,
        "CHILDREN" => FadeMode::Children,
        other => panic!("unsupported fade mode: {other}"),
    };
    let graph = ClimateGraph::new(
        as_i32(&graph_meta["resolution"]),
        parents,
        fade_mode,
        as_f64(&graph_meta["fadeRadius"]),
        as_f64(&graph_meta["fadeDistance"]),
    );

    let table = graph.get_table();
    let expected_table = root["graph"]["table"].as_array().expect("table");
    assert_eq!(expected_table.len(), (table.width * table.height) as usize);
    for (index, expected) in expected_table.iter().enumerate() {
        let x = (index as i32) % table.width;
        let y = (index as i32) / table.width;
        assert_eq!(
            table.at(x, y),
            as_i32(expected),
            "climate table index={index} (x={x}, y={y})"
        );
    }

    let fade = graph.get_fade_map();
    let expected_fade = root["graph"]["fade"].as_array().expect("fade");
    assert_eq!(expected_fade.len(), (fade.width * fade.height) as usize);
    for (index, expected) in expected_fade.iter().enumerate() {
        let x = (index as i32) % fade.width;
        let y = (index as i32) / fade.width;
        let got = fade.at(x, y);
        assert_eq!(
            got.to_bits(),
            as_f64(expected).to_bits(),
            "climate fade index={index} (x={x}, y={y}): expected {expected}, got {got}"
        );
    }

    // --- climate noise -------------------------------------------------
    let grid_meta = &root["noise"]["grid"];
    let size = grid_meta["size"].as_u64().expect("grid size") as usize;
    let start = as_f64(&grid_meta["start"]);
    let step = as_f64(&grid_meta["step"]);

    let mut checked = 0usize;
    for (seed_str, entry) in root["noise"]["seeds"].as_object().expect("seeds object") {
        let seed: i32 = seed_str.parse().expect("seed");
        let construction = &entry["construction"];

        let grid = Grid::new(
            as_i32(&construction["grid"]["seedOffset"]),
            as_f64(&construction["grid"]["scale"]),
            direct_grid(),
            NormalPointEvaluator::euclidean(),
        );
        let thresholds = Thresholds::new(
            as_f64(&construction["thresholds"]["land"]),
            as_f64(&construction["thresholds"]["island"]),
            as_f64(&construction["thresholds"]["beachSize"]),
            as_f64(&construction["thresholds"]["shallowOceanSize"]),
        );
        let noise = ClimateNoise::new(
            grid,
            build_property(&construction["continent"]),
            build_property(&construction["temperature"]),
            build_property(&construction["intensity"]),
            thresholds,
        );

        let expected_id = entry["id"].as_array().expect("id");
        let expected_continent = entry["continent"].as_array().expect("continent");
        let expected_temperature = entry["temperature"].as_array().expect("temperature");
        let expected_intensity = entry["intensity"].as_array().expect("intensity");
        let expected_fade = entry["fade"].as_array().expect("fade");
        assert_eq!(expected_id.len(), size * size);

        let mut buffer = Buffer::default();
        for j in 0..size {
            for i in 0..size {
                let index = j * size + i;
                let x = start + i as f64 * step;
                let y = start + j as f64 * step;

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

    assert!(checked > 500, "suspiciously few checks: {checked}");
}

// ---------------------------------------------------------------------
// Registry parity (blocks / fluids / environments)
// ---------------------------------------------------------------------

/// The registries of the base-game asset pack.
///
/// The reference data is `testdata/golden/registries.json`, produced by
/// `tools/java-dumper` (`RegistryDumper.java`). The Java side boots the real
/// CoreServer asset pipeline (the vanilla `AssetRegistryLoader`) over the
/// base-game asset pack, so the ids in the dump are the ids the Java world
/// generator uses — the Rust side never re-derives them.
#[test]
fn registries_golden_test() {
    let Some(root) = load_golden("registries.json") else {
        return;
    };

    // --- the loader ----------------------------------------------------
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/../../testdata/golden/registries.json");
    let registries = worldgen_model::registry::Registries::from_golden(&path)
        .expect("the golden registry dump must load");

    // --- blocks --------------------------------------------------------
    let block_assets = root["blocks"]["assets"].as_array().expect("blocks.assets");
    assert!(
        !block_assets.is_empty(),
        "the dump must contain block types"
    );
    assert_eq!(
        registries.blocks.len(),
        root["blocks"]["count"].as_u64().expect("blocks.count") as usize,
        "the loaded block registry must have one slot per dumped block"
    );

    // The four pre-loaded block types keep their hard-coded ids.
    for (name, id) in [
        ("Empty", 0_i64),
        ("Unknown", 1),
        ("Debug_Cube", 2),
        ("Debug_Model", 3),
    ] {
        let expected = block_assets
            .iter()
            .find(|asset| asset["name"] == name)
            .unwrap_or_else(|| panic!("the dump must contain the block '{name}'"));
        assert_eq!(expected["index"].as_i64(), Some(id), "the id of '{name}'");
    }

    // The four pre-loaded block types are at the front of the registry and
    // carry the draw types of the Java constants.
    assert_eq!(
        registries.blocks.get(0).map(|block| block.draw_type),
        Some(worldgen_model::registry::DrawType::Empty)
    );
    assert_eq!(
        registries.blocks.get(2).map(|block| block.draw_type),
        Some(worldgen_model::registry::DrawType::Cube)
    );
    assert_eq!(
        registries.blocks.get(3).map(|block| block.draw_type),
        Some(worldgen_model::registry::DrawType::Model)
    );

    // The blocks the world generator resolves by name must be there with a
    // real index and a real draw type.
    for name in ["Soil_Dirt", "Rock_Stone"] {
        let asset = block_assets
            .iter()
            .find(|asset| asset["name"] == name)
            .unwrap_or_else(|| panic!("the dump must contain the block '{name}'"));
        let index = asset["index"].as_i64().expect("block index") as i32;
        assert!(
            index >= 4,
            "the block '{name}' must have a real id: {index}"
        );

        let draw_type = asset["drawType"].as_str().expect("drawType");
        let expected = worldgen_model::registry::DrawType::from_name(draw_type)
            .unwrap_or_else(|| panic!("unknown draw type '{draw_type}'"));
        assert_eq!(
            registries.blocks.get(index).map(|block| block.draw_type),
            Some(expected),
            "the draw type of '{name}' (id {index})"
        );
    }

    // --- fluids --------------------------------------------------------
    let fluid_assets = root["fluids"]["assets"].as_array().expect("fluids.assets");
    assert!(!fluid_assets.is_empty(), "the dump must contain fluids");
    assert_eq!(
        registries.fluids.len(),
        root["fluids"]["count"].as_u64().expect("fluids.count") as usize,
        "the loaded fluid registry must have one slot per dumped fluid"
    );

    let water = fluid_assets
        .iter()
        .find(|asset| asset["name"] == "Fluid_Water" || asset["name"] == "Water")
        .expect("the dump must contain the water fluid");
    let water_index = water["index"].as_i64().expect("fluid index") as i32;
    let water_level = water["maxFluidLevel"].as_i64().expect("maxFluidLevel") as i8;
    assert!(
        (0..=8).contains(&water_level),
        "the water fluid must have a sane max fluid level: {water_level}"
    );
    assert_eq!(
        registries.fluids.get_max_fluid_level(water_index),
        water_level,
        "the max fluid level of the water fluid (id {water_index})"
    );

    // --- environments --------------------------------------------------
    let environment_assets = root["environments"]["assets"]
        .as_array()
        .expect("environments.assets");
    assert!(
        !environment_assets.is_empty(),
        "the dump must contain environments"
    );
    for asset in environment_assets {
        let index = asset["index"].as_i64().expect("environment index") as i32;
        let name = asset["name"].as_str().expect("environment name");
        assert_eq!(
            registries.environments.name(index),
            Some(name),
            "the name of the environment at id {index}"
        );
    }
    assert!(registries.environments.index_of("Default").is_some());
}
