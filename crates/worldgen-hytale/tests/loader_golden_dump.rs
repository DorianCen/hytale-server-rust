//! Golden parity test for the **world-loader chain** (Subtask 7d).
//!
//! Reference data: `testdata/golden/loader.json`, produced by
//! `tools/java-dumper` (`LoaderDumper.dumpLoader`) — the **real** Java
//! loader chain (`ChunkGeneratorJsonLoader` → `ZonePatternProviderJsonLoader`
//! → `FileContextLoader` → `ZoneJsonLoader` → `BiomePatternGeneratorJsonLoader`
//! → the biome/container loaders) run over the **real** Default world
//! (`testdata/Default`) for the world seeds 0 / 1 / 42 / 1234567.
//!
//! The dump contains
//!
//! * the weighted `Masks` pick of `World.json` (name, kind, random seed),
//! * the zone requirement (sorted),
//! * the file context (zone ids / names / order and the biome file ids),
//! * the `MaskMapping` colour → zone mapping,
//! * the per-zone summary (discovery config, tile-biome weights / size
//!   modifiers / map colours, the weighted picks, the custom-biome order),
//! * the cave `EntryPoints` grid seed offsets (the phase-4 seam: the cave
//!   *generator* is not built yet, only its seed chain is pinned),
//! * the per-seed zone-pattern / biome-pattern grid seed offsets, the biome
//!   pattern extents and the sampled biomes (index / name / direct name),
//! * the full seed-0 biome containers (layers, covers, tint, environment,
//!   water, fade, interpolation).
//!
//! Every comparison is **bit-exact** (Java `Double.toHexString` /
//! `Float.toHexString` strings are parsed back into `f64` / `f32`).
//!
//! ## Documented seams
//!
//! * **phase 5 (prefabs)** — the biome `Prefabs` containers are not loaded:
//!   the Java dumper stubs `loadPrefabContainer()` to `null` and the Rust
//!   side passes `None` as well. The dump records whether the biome JSON
//!   *has* a `Prefabs` key; the extents are therefore `0` on both sides.
//! * **phase 4 (caves)** — `LoaderDumper` pins the cave `EntryPoints` seed
//!   chain; the full cave node-graph/geometry parity is verified by the
//!   chunk-level `tests/chunks_golden.rs` (which is now down to 150 cave-cover
//!   voxels, see NEXT_STEPS.md).
//!
//! The noise-derived samples are **bit-exact now** (0 deviations): the
//! `OLDSCHOOL` first-octave offset and the `DensityPointEvaluator`
//! `0x9afc33ae` int-literal sign extension are fixed, and the
//! `BiomePatternGeneratorJsonLoader` resolves the `GridGenerator` child
//! exactly like Java. The pinned-column tolerance machinery is kept as a
//! regression counter (the expected count is `0`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::json::point_generator_json_loader::PointGeneratorJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::IHeightThresholdInterpreter;
use procedural::noise::support::NoiseProperty;
use serde_json::Value;
use worldgen_model::registry::Registries;

use worldgen_hytale::biome::{
    Biome, BiomeInterpolation, BiomePatternGenerator, CustomBiome, TileBiome,
};
use worldgen_hytale::chunk::MAX_RADIUS2;
use worldgen_hytale::container::{
    CoverContainer, EnvironmentContainer, FadeContainer, LayerContainer, LayerEntry, TintContainer,
    WaterContainer,
};
use worldgen_hytale::loader::biome::biome_json_loader::BiomeJsonLoader;
use worldgen_hytale::loader::biome::biome_pattern_generator_json_loader::BiomePatternGeneratorJsonLoader;
use worldgen_hytale::loader::biome::custom_biome_json_loader::CustomBiomeJsonLoader;
use worldgen_hytale::loader::biome::tile_biome_json_loader::TileBiomeJsonLoader;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::loader::color_util::ColorUtil;
use worldgen_hytale::loader::context::file_context_loader::FileContextLoader;
use worldgen_hytale::loader::seed_string_resource::SeedStringResource;
use worldgen_hytale::loader::zone::zone_color_mapping_json_loader::ZoneColorMappingJsonLoader;
use worldgen_hytale::loader::zone::zone_json_loader::ZoneJsonLoader;
use worldgen_hytale::loader::zone::zone_requirement_json_loader::ZoneRequirementJsonLoader;
use worldgen_hytale::util::constant_noise_property::default_zero;
use worldgen_hytale::weighted_map::{IWeightedMap, WeightedMap};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_hytale::zone::discovery_config::ZoneDiscoveryConfig;
use worldgen_hytale::zone::zone::Zone;

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

    let mut m: f64 = 0.0;
    for c in int_part.chars() {
        m = m * 16.0 + c.to_digit(16).unwrap() as f64;
    }
    let mut scale = 1.0f64;
    for c in frac_part.chars() {
        scale *= 16.0;
        m = m * 16.0 + c.to_digit(16).unwrap() as f64;
    }

    let v = m / scale * (2.0f64).powi(exp);
    sign * v
}

/// Bit-exact comparison against a Java hex double string.
fn assert_hex(actual: f64, expected: &Value, path: &str) {
    let expected = expected
        .as_str()
        .unwrap_or_else(|| panic!("{path}: expected a hex string, got {expected}"));
    let parsed = parse_java_hex_double(expected);
    assert_eq!(
        actual.to_bits(),
        parsed.to_bits(),
        "{path}: {actual} != {expected} (bits)"
    );
}

/// Bit-exact comparison of an `f32` against a Java `Float.toHexString`.
fn assert_hex_f32(actual: f32, expected: &Value, path: &str) {
    let expected = expected
        .as_str()
        .unwrap_or_else(|| panic!("{path}: expected a hex string, got {expected}"));
    let parsed = parse_java_hex_double(expected) as f32;
    assert_eq!(
        actual.to_bits(),
        parsed.to_bits(),
        "{path}: {actual} != {expected} (bits)"
    );
}

fn as_i32(v: &Value) -> i32 {
    match v.as_i64() {
        Some(i) => i as i32,
        None => panic!("not an integer: {v}"),
    }
}

// ---------------------------------------------------------------------
// Constants (must stay in sync with `LoaderDumper`)
// ---------------------------------------------------------------------

/// The world seeds of the dumper (`LoaderDumper.WORLD_SEEDS`).
const WORLD_SEEDS: [i32; 4] = [0, 1, 42, 1234567];
/// The biome sample coordinates of the dumper.
const BIOME_SAMPLE_COORDS: [(i32, i32); 8] = [
    (0, 0),
    (1, 0),
    (-1, 1),
    (7, 3),
    (-248, -616),
    (1664, 1024),
    (12345, -54321),
    (-100000, 99999),
];
/// The container sample coordinates of the dumper.
const CONTAINER_SAMPLE_COORDS: [(i32, i32); 3] = [(0, 0), (7, 3), (-5, 9)];
/// The weighted pick values of the dumper.
const PICK_VALUES: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
/// The runtime seed of the container evaluations.
const CONTAINER_SAMPLE_SEED: i32 = 0;
/// The seed of the cover part picks.
const COVER_PICK_SEED: i64 = 0;

/// Historical pinned columns of the pre-fix noise deviations. The port is
/// bit-exact now, so these are only kept as an escape hatch for the
/// comparison helpers; the expected deviation count is `0`.
const KNOWN_NOISE_DEVIATION_COLUMNS: &[(i32, i32)] = &[(7, 3), (-5, 9)];

/// The tolerated difference (in height steps) at the pinned columns.
///
/// The tolerance covers the *range scaled* comparisons as well: the water
/// container maps the noise value through a 105 block range, so the known
/// noise deviation of the pinned column shows up as five height steps
/// there (the two biomes with a water container that uses a `MIN` / `MAX`
/// fractal noise).
const KNOWN_NOISE_DEVIATION_TOLERANCE: f64 = 5.0;

/// The tolerated difference of the repetition counts at the pinned columns.
const KNOWN_NOISE_REPETITION_TOLERANCE: i32 = 10;

/// The number of noise-derived samples that deviate at the pinned columns.
static NOISE_DEVIATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The number of noise-derived samples that deviate from the Java dump.
///
/// The port is bit-exact: zero. (The `OLDSCHOOL` fractal octave offset and the
/// `DensityPointEvaluator` `0x9afc33ae` int-literal sign extension were fixed
/// in this session; before that the count was 2590.)
const EXPECTED_NOISE_DEVIATIONS: usize = 0;

/// Compares a noise-derived integer: bit-exact, except at the pinned columns.
fn assert_noise_int(expected: i32, actual: i32, known: bool, path: &str) {
    if expected == actual {
        return;
    }
    assert!(
        known,
        "{path}: {expected} vs {actual} (outside the pinned columns)"
    );
    assert!(
        (expected as f64 - actual as f64).abs() <= KNOWN_NOISE_DEVIATION_TOLERANCE,
        "{path}: {expected} vs {actual} (larger than the pinned tolerance)"
    );
    NOISE_DEVIATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Compares a noise-derived double: bit-exact, except at the pinned columns.
fn assert_noise_value(expected: &Value, actual: f64, known: bool, path: &str) {
    let parsed = parse_java_hex_double(
        expected
            .as_str()
            .unwrap_or_else(|| panic!("{path}: expected a hex string, got {expected}")),
    );
    if parsed.to_bits() == actual.to_bits() {
        return;
    }
    assert!(
        known,
        "{path}: {parsed} vs {actual} (outside the pinned columns)"
    );
    assert!(
        (parsed - actual).abs() <= KNOWN_NOISE_DEVIATION_TOLERANCE,
        "{path}: {parsed} vs {actual} (larger than the pinned tolerance)"
    );
    NOISE_DEVIATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Compares a noise-derived repetition count: bit-exact, except at the pinned
/// columns.
fn assert_noise_repetitions(expected: i32, actual: i32, known: bool, path: &str) {
    if expected == actual {
        return;
    }
    assert!(
        known,
        "{path}: {expected} vs {actual} (outside the pinned columns)"
    );
    assert!(
        (expected - actual).abs() <= KNOWN_NOISE_REPETITION_TOLERANCE,
        "{path}: {expected} vs {actual} (larger than the pinned tolerance)"
    );
    NOISE_DEVIATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Compares a value that is derived from a noise sample (a block id, a
/// colour, an environment id): the Rust procedural noise port may resolve a
/// different sample; every deviation is counted (the total is asserted at
/// the end of the test), never ignored.
fn assert_noise_derived(expected: i32, actual: i32, _known: bool, _path: &str) {
    if expected == actual {
        return;
    }
    NOISE_DEVIATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

// ---------------------------------------------------------------------
// Golden data
// ---------------------------------------------------------------------

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn default_world_folder() -> PathBuf {
    manifest_dir().join("../../testdata/Default")
}

/// Loads a JSON object the way Java's lenient gson does.
///
/// The real Default-world files carry `//` line comments (Java's gson runs in
/// lenient mode, the Rust `serde_json` parser does not). The loader package
/// itself is owned by another slice, so the comment stripping happens here in
/// the test.
fn load_json_lenient(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to load file {}: {e}", path.display()));
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: Value = serde_json::from_str(&strip_json_comments(text))
        .unwrap_or_else(|e| panic!("Failed to load file {}: {e}", path.display()));
    if !value.is_object() {
        panic!("Failed to load file {}: not a JSON object", path.display());
    }
    value
}

/// Resolves a `{"File": "<dotted.path>"}` redirect (Java
/// `JsonLoader.loadFile`).
fn resolve_file_reference(value: &Value, folder: &Path) -> Value {
    if let Some(file) = value
        .as_object()
        .and_then(|object| object.get("File"))
        .and_then(|file| file.as_str())
    {
        let path = folder.join(format!("{}.json", file.replace('.', "/")));
        return load_json_lenient(&path);
    }
    value.clone()
}

/// Removes the `//` and `/* */` comments of a JSON document (the string
/// literals are preserved).
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut previous = '\0';
                    for c2 in chars.by_ref() {
                        if previous == '*' && c2 == '/' {
                            break;
                        }
                        previous = c2;
                    }
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out
}

fn load_golden() -> Value {
    let path = manifest_dir().join("../../testdata/golden/loader.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {} ({e}) — run tools/java-dumper/run.sh first",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("valid JSON")
}

// ---------------------------------------------------------------------
// The Rust loader chain
// ---------------------------------------------------------------------

/// One loaded zone of a seed run.
struct ZoneRun {
    id: i32,
    name: String,
    /// The zone seed chain (`<seed>.Zones.Zone-<name>`).
    seed_chain: String,
    discovery: Rc<ZoneDiscoveryConfig>,
    tile_biomes: Vec<Rc<TileBiome>>,
    custom_biomes: Vec<Rc<CustomBiome>>,
    tile_map: Rc<dyn IWeightedMap<Rc<TileBiome>>>,
    generator: Rc<BiomePatternGenerator>,
    biome_grid_seed_offset: i32,
    biome_grid_seed_key: Option<String>,
    /// The raw biome JSONs (the `Prefabs` / `HeightmapNoise` presence).
    biome_json: HashMap<i32, Value>,
    /// The zone folder.
    path: PathBuf,
    /// Whether the containers of a biome could be loaded (the Rust port has
    /// no `MIN` / `MAX` fractal-mode mapping yet, see [`load_biome_parts`]).
    biome_containers_complete: HashMap<i32, bool>,
    /// The tile biome ids / names in the file-context order.
    tile_biome_files: Vec<(i32, String)>,
    /// The custom biome ids / names in the file-context order (the loaded
    /// custom biomes are sorted by their generator priority instead).
    custom_biome_files: Vec<(i32, String)>,
}

/// One run of the loader chain for a single world seed.
struct SeedRun {
    seed: i32,
    base: SeedString,
    mask_name: String,
    mask_kind: &'static str,
    mask_random_seed: i32,
    mask_map_size: usize,
    zone_requirement: Vec<String>,
    zone_pattern_grid_seed_offset: i32,
    zone_pattern_grid_seed_key: Option<String>,
    zones: Vec<ZoneRun>,
}

/// Builds the `Masks` weighted map exactly like
/// `ChunkGeneratorJsonLoader.loadMaskProvider`.
fn build_mask_map(world_json: &Value) -> WeightedMap<String> {
    let mut builder = WeightedMap::<String>::builder();
    match world_json.get("Masks") {
        None => {
            builder.put("Mask.png".to_string(), 1.0);
        }
        Some(Value::Null) => {}
        Some(Value::Array(items)) => {
            if items.is_empty() {
                builder.put("Mask.png".to_string(), 1.0);
            } else {
                for item in items {
                    builder.put(JsonLoaderBase::get_as_string(item), 1.0);
                }
            }
        }
        Some(Value::Object(object)) => {
            if object.is_empty() {
                builder.put("Mask.png".to_string(), 1.0);
            } else {
                for (key, value) in object {
                    builder.put(key.clone(), JsonLoaderBase::get_as_double(value));
                }
            }
        }
        Some(primitive) => {
            builder.put(JsonLoaderBase::get_as_string(primitive), 1.0);
        }
    }
    builder.build()
}

/// Runs the loader chain for one world seed.
fn run_chain(seed: i32, folder: &Path, registries: &Rc<Registries>, full: bool) -> SeedRun {
    let base = SeedString::new(&seed.to_string());
    let config = WorldGenConfig::new(
        folder.to_path_buf(),
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let loader = ChunkGeneratorJsonLoader::new(base.clone(), config);

    // --- the weighted mask pick (ChunkGeneratorJsonLoader.loadMaskProvider) ---
    let world_json = loader.load_world_json(&folder.join("World.json"));
    let mask_map = build_mask_map(&world_json);
    let mut random = FastRandom::new(base.hash_code() as i64);
    let mask_name = mask_map
        .get_random(&mut random)
        .cloned()
        .expect("the mask map must not be empty");
    let mask_kind = if mask_name.ends_with("Mask.json") {
        "Climate"
    } else {
        "Image"
    };
    let mask_random_seed = base.hash_code();
    let mask_map_size = mask_map.size();

    // --- the zone requirement (ZonePatternProviderJsonLoader.loadZoneRequirement) ---
    let zones_json = load_json_lenient(&folder.join("Zones.json"));
    let requirement =
        ZoneRequirementJsonLoader::new(base.clone(), folder, Some(zones_json.clone())).load();
    let mut zone_requirement: Vec<String> = requirement.iter().cloned().collect();
    zone_requirement.sort();

    // --- the zone pattern grid (SeedString(<seed>).ZonePatternGenerator) ---
    let zones_base = JsonLoaderBase::new(base.clone(), folder, Some(zones_json.clone()));
    let zone_grid = zones_base.get("GridGenerator");
    let zone_pattern_grid_seed_offset = PointGeneratorJsonLoader::new(
        base.append(".ZonePatternGenerator"),
        folder,
        zone_grid.clone(),
    )
    .load_seed();
    let zone_pattern_grid_seed_key = zone_grid
        .as_ref()
        .and_then(|g| g.get("Seed"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    // --- the file context (FileContextLoader.load) ---
    let context = FileContextLoader::new("Default", folder, requirement.clone()).load();
    let zone_contexts: Vec<_> = context
        .get_zones()
        .borrow()
        .iter()
        .map(|(_, zone)| Rc::clone(zone))
        .collect();

    let mut zones = Vec::new();
    for zone_context in zone_contexts {
        let zone_name = zone_context.base.get_name().to_string();
        let zone_id = zone_context.base.get_id();
        let zone_seed = base.append(".Zones").append(&format!(".Zone-{zone_name}"));
        let zone_json = load_json_lenient(&zone_context.base.get_path().join("Zone.json"));

        // The discovery config (ZoneJsonLoader.loadDiscoveryConfig).
        let zone_loader = ZoneJsonLoader::new(
            base.append(".Zones"),
            folder,
            Some(zone_json.clone()),
            Rc::clone(&zone_context),
            Rc::clone(registries),
            Rc::new(SeedStringResource::new()),
        );
        let discovery = zone_loader.load_discovery_config();

        // The tile biomes (ZoneBiomesJsonLoader + TileBiomeJsonLoader).
        let tile_contexts: Vec<_> = zone_context
            .get_tile_biomes()
            .borrow()
            .iter()
            .map(|(_, biome)| Rc::clone(biome))
            .collect();
        let mut tile_biomes = Vec::new();
        let mut tile_biome_files: Vec<(i32, String)> = Vec::new();
        let mut custom_biome_files: Vec<(i32, String)> = Vec::new();
        let mut biome_json = HashMap::new();
        let mut biome_containers_complete: HashMap<i32, bool> = HashMap::new();
        for biome_context in tile_contexts {
            let json = load_json_lenient(biome_context.base.get_path());
            biome_json.insert(biome_context.base.get_id(), json.clone());
            let biome_loader = TileBiomeJsonLoader::new(
                zone_seed.clone(),
                folder,
                Some(json),
                Rc::clone(&biome_context),
                Rc::clone(registries),
            );
            tile_biome_files.push((
                biome_context.base.get_id(),
                biome_context.base.get_name().to_string(),
            ));
            let parts = load_biome_parts(&biome_loader.base, full);
            biome_containers_complete.insert(biome_context.base.get_id(), parts.complete);
            tile_biomes.push(Rc::new(TileBiome::new(
                biome_context.base.get_id(),
                biome_context.base.get_name().to_string(),
                parts.interpolation,
                parts.threshold,
                parts.cover,
                parts.layer,
                // The prefab container is the phase-5 seam: the Java dumper
                // stubs it to `null`, the Rust side passes `None`.
                None,
                parts.tint,
                parts.environment,
                parts.water,
                parts.fade,
                parts.heightmap_noise,
                biome_loader.load_weight(),
                biome_loader.load_size_modifier(),
                biome_loader.base.load_color(),
            )));
        }

        // The custom biomes (ZoneCustomBiomesJsonLoader + CustomBiomeJsonLoader).
        let custom_contexts: Vec<_> = zone_context
            .get_custom_biomes()
            .borrow()
            .iter()
            .map(|(_, biome)| Rc::clone(biome))
            .collect();
        let mut custom_biomes = Vec::new();
        for biome_context in custom_contexts {
            let json = load_json_lenient(biome_context.base.get_path());
            biome_json.insert(biome_context.base.get_id(), json.clone());
            let biome_loader = CustomBiomeJsonLoader::new(
                zone_seed.clone(),
                folder,
                Some(json),
                Rc::clone(&biome_context),
                tile_biomes.clone(),
                Rc::clone(registries),
            );
            let generator = biome_loader
                .load_custom_biome_generator()
                .unwrap_or_else(|| {
                    panic!(
                        "no custom biome generator for {}",
                        biome_context.base.get_path().display()
                    )
                });
            custom_biome_files.push((
                biome_context.base.get_id(),
                biome_context.base.get_name().to_string(),
            ));
            let parts = load_biome_parts(&biome_loader.base, full);
            biome_containers_complete.insert(biome_context.base.get_id(), parts.complete);
            custom_biomes.push(Rc::new(CustomBiome::new(
                biome_context.base.get_id(),
                biome_context.base.get_name().to_string(),
                parts.interpolation,
                generator,
                parts.threshold,
                parts.cover,
                parts.layer,
                None,
                parts.tint,
                parts.environment,
                parts.water,
                parts.fade,
                parts.heightmap_noise,
                biome_loader.base.load_color(),
            )));
        }
        // Java `Arrays.sort` (stable) by descending generator priority.
        custom_biomes.sort_by(|a, b| {
            b.get_custom_biome_generator()
                .get_priority()
                .cmp(&a.get_custom_biome_generator().get_priority())
        });

        // The weighted tile-biome map.
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        for biome in &tile_biomes {
            builder.put(Rc::clone(biome), biome.get_weight());
        }
        let tile_map: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());

        // The biome pattern generator. Java builds the point generator from
        // the `GridGenerator` child of the `BiomeGenerator` element; the
        // loader resolves that child itself (the same input Java uses).
        let biome_generator = zones_base.get("BiomeGenerator").or_else(|| {
            JsonLoaderBase::new(zone_seed.clone(), folder, Some(zone_json.clone()))
                .get("BiomeGenerator")
        });
        let grid = biome_generator.as_ref().and_then(|generator| {
            JsonLoaderBase::new(zone_seed.clone(), folder, Some(generator.clone()))
                .get("GridGenerator")
        });
        let biome_grid_seed_offset = PointGeneratorJsonLoader::new(
            zone_seed.append(".BiomePatternGenerator"),
            folder,
            grid.clone(),
        )
        .load_seed();
        let biome_grid_seed_key = grid
            .as_ref()
            .and_then(|g| g.get("Seed"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let generator = BiomePatternGeneratorJsonLoader::new(
            zone_seed.clone(),
            folder,
            biome_generator,
            Rc::clone(&tile_map),
            custom_biomes.clone(),
        )
        .load();

        zones.push(ZoneRun {
            id: zone_id,
            name: zone_name,
            seed_chain: zone_seed.seed().to_string(),
            discovery,
            tile_biomes,
            custom_biomes,
            tile_map,
            generator,
            biome_grid_seed_offset,
            biome_grid_seed_key,
            biome_json,
            path: zone_context.base.get_path().to_path_buf(),
            biome_containers_complete,
            tile_biome_files,
            custom_biome_files,
        });
    }

    SeedRun {
        seed,
        base,
        mask_name,
        mask_kind,
        mask_random_seed,
        mask_map_size,
        zone_requirement,
        zone_pattern_grid_seed_offset,
        zone_pattern_grid_seed_key,
        zones,
    }
}

/// The container parts of a biome.
struct BiomeParts {
    interpolation: Rc<BiomeInterpolation>,
    threshold: Rc<dyn IHeightThresholdInterpreter>,
    cover: Rc<CoverContainer>,
    layer: Rc<LayerContainer>,
    tint: Rc<TintContainer>,
    environment: Rc<EnvironmentContainer>,
    water: Rc<WaterContainer>,
    fade: Rc<FadeContainer>,
    heightmap_noise: Rc<dyn NoiseProperty>,
    /// `false` when a container could not be loaded.
    complete: bool,
}

/// Runs a loader step, falling back to a dummy value when it panics.
///
/// Two custom biomes of the Default world reference noise files with the
/// `FractalMode` values `MIN` / `MAX`; the Rust
/// `NoisePropertyJsonLoader::fractal_mode_value_of` (in `crates/procedural`,
/// owned by another slice) does not map those two enum constants yet, so
/// their containers cannot be built on the Rust side. The test asserts the
/// exact set of such biomes instead of skipping them silently.
fn catch_or<T>(f: impl FnOnce() -> T, fallback: T) -> (T, bool) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(value) => (value, true),
        Err(_) => (fallback, false),
    }
}

/// Loads the container parts of a biome.
fn load_biome_parts(base: &BiomeJsonLoader, full: bool) -> BiomeParts {
    let interpolation = base.load_interpolation();
    let threshold = base.load_terrain_height_threshold();
    let (heightmap_noise, noise_ok) = catch_or(|| base.load_heightmap_noise(), default_zero());
    if !full {
        // The "light" pass: the containers do not influence the weights, the
        // seed chains or the biome pattern.
        return BiomeParts {
            interpolation,
            threshold,
            cover: dummy_cover_container(),
            layer: dummy_layer_container(),
            tint: dummy_tint_container(),
            environment: dummy_environment_container(),
            water: dummy_water_container(),
            fade: dummy_fade_container(),
            heightmap_noise,
            complete: false,
        };
    }

    let mut complete = true;
    let (cover, ok) = catch_or(|| base.load_cover_container(), dummy_cover_container());
    complete &= ok;
    let (layer, ok) = catch_or(|| base.load_layer_containers(), dummy_layer_container());
    complete &= ok;
    let (tint, ok) = catch_or(|| base.load_tint_container(), dummy_tint_container());
    complete &= ok;
    let (environment, ok) = catch_or(
        || base.load_environment_container(),
        dummy_environment_container(),
    );
    complete &= ok;
    let (water, ok) = catch_or(|| base.load_water_container(), dummy_water_container());
    complete &= ok;
    let (fade, ok) = catch_or(|| base.load_fade_container(), dummy_fade_container());
    complete &= ok;
    complete &= noise_ok;

    BiomeParts {
        interpolation,
        threshold,
        cover,
        layer,
        tint,
        environment,
        water,
        fade,
        heightmap_noise,
        complete,
    }
}

/// The dummy containers of the "light" pass (the containers do not influence
/// the biome pattern, the weights or the seed chains).
fn dummy_cover_container() -> Rc<worldgen_hytale::container::CoverContainer> {
    Rc::new(worldgen_hytale::container::CoverContainer::new(Vec::new()))
}

fn dummy_layer_container() -> Rc<worldgen_hytale::container::LayerContainer> {
    Rc::new(worldgen_hytale::container::LayerContainer::new(
        0,
        0,
        Vec::new(),
        Vec::new(),
    ))
}

fn dummy_tint_container() -> Rc<worldgen_hytale::container::TintContainer> {
    let mut builder = WeightedMap::<i32>::builder();
    let color_mapping: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
    Rc::new(worldgen_hytale::container::TintContainer::new(
        worldgen_hytale::container::DefaultTintContainerEntry::new(color_mapping, default_zero()),
        Vec::new(),
    ))
}

fn dummy_environment_container() -> Rc<worldgen_hytale::container::EnvironmentContainer> {
    let mut builder = WeightedMap::<i32>::builder();
    let environment_mapping: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
    Rc::new(worldgen_hytale::container::EnvironmentContainer::new(
        worldgen_hytale::container::DefaultEnvironmentContainerEntry::new(
            environment_mapping,
            default_zero(),
        ),
        Vec::new(),
    ))
}

fn dummy_water_container() -> Rc<worldgen_hytale::container::WaterContainer> {
    Rc::new(worldgen_hytale::container::WaterContainer::new(Vec::new()))
}

fn dummy_fade_container() -> Rc<worldgen_hytale::container::FadeContainer> {
    Rc::new(worldgen_hytale::container::FadeContainer::new(
        0.0, 0.0, 0.0, 0.0, 0.0,
    ))
}

// ---------------------------------------------------------------------
// The test
// ---------------------------------------------------------------------

#[test]
fn the_loader_chain_matches_the_java_golden_dump() {
    // The two biomes with an unsupported `FractalMode` panic inside the
    // loader; their message is filtered out of the test output (the panic is
    // caught and the biome is listed explicitly below).
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        if !message.contains("FractalMode") {
            let location = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            eprintln!("panicked at {location}: {message}");
        }
    }));

    let golden = load_golden();
    let folder = default_world_folder();
    let registries = Rc::new(Registries::load());

    // The meta section pins the shared constants.
    let meta = &golden["meta"];
    assert_eq!(meta["worldDir"], serde_json::json!("Default"));
    assert_eq!(
        meta["seeds"],
        serde_json::json!(WORLD_SEEDS.to_vec()),
        "the world seeds must stay in sync"
    );
    assert_eq!(
        meta["biomeSampleCoords"],
        serde_json::json!(BIOME_SAMPLE_COORDS
            .iter()
            .map(|(x, z)| serde_json::json!([x.to_string(), z.to_string()]))
            .collect::<Vec<_>>()),
        "the biome sample coordinates must stay in sync"
    );
    assert_eq!(
        meta["containerSampleCoords"],
        serde_json::json!(CONTAINER_SAMPLE_COORDS
            .iter()
            .map(|(x, z)| serde_json::json!([x.to_string(), z.to_string()]))
            .collect::<Vec<_>>()),
        "the container sample coordinates must stay in sync"
    );

    // --- the mask pick (per seed) ---
    for seed in WORLD_SEEDS {
        let run = run_chain(seed, &folder, &registries, false);
        let expected = &golden["maskPick"][seed.to_string()];
        assert_eq!(run.mask_name, expected["maskName"].as_str().unwrap());
        assert_eq!(run.mask_kind, expected["kind"].as_str().unwrap());
        assert_eq!(run.mask_random_seed, as_i32(&expected["randomSeed"]));
        assert_eq!(
            run.mask_map_size,
            expected["mapSize"].as_u64().unwrap() as usize
        );

        // --- the zone requirement ---
        let expected_requirement = golden["zoneRequirement"]
            .as_array()
            .expect("zoneRequirement")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(run.zone_requirement, expected_requirement, "seed {seed}");
    }

    // --- the seed-0 run carries the seed-independent sections ---
    let seed_zero = run_chain(0, &folder, &registries, true);

    // The file context (zone ids, names, order, biome file ids).
    let file_context = &golden["fileContext"];
    assert_eq!(file_context["name"], serde_json::json!("Default"));
    let expected_zones = file_context["zones"].as_array().expect("zones");
    assert_eq!(
        expected_zones.len(),
        seed_zero.zones.len(),
        "the zone count must match"
    );
    for (expected, zone) in expected_zones.iter().zip(seed_zero.zones.iter()) {
        assert_eq!(as_i32(&expected["id"]), zone.id, "zone id");
        assert_eq!(expected["name"].as_str().unwrap(), zone.name, "zone name");
        assert_eq!(
            expected["zoneSeedChain"].as_str().unwrap(),
            zone.seed_chain,
            "zone {} seed chain",
            zone.name
        );

        let expected_tile = expected["tileBiomes"].as_array().expect("tileBiomes");
        assert_eq!(
            expected_tile.len(),
            zone.tile_biome_files.len(),
            "tile biome count"
        );
        for (expected_biome, (id, name)) in expected_tile.iter().zip(zone.tile_biome_files.iter()) {
            assert_eq!(as_i32(&expected_biome["id"]), *id);
            assert_eq!(expected_biome["name"].as_str().unwrap(), name);
        }

        let expected_custom = expected["customBiomes"].as_array().expect("customBiomes");
        assert_eq!(
            expected_custom.len(),
            zone.custom_biome_files.len(),
            "custom biome count"
        );
        for (expected_biome, (id, name)) in
            expected_custom.iter().zip(zone.custom_biome_files.iter())
        {
            assert_eq!(as_i32(&expected_biome["id"]), *id);
            assert_eq!(expected_biome["name"].as_str().unwrap(), name);
        }
    }

    // --- the colour mapping ---
    let zones_json = load_json_lenient(&folder.join("Zones.json"));
    let mask_mapping = JsonLoaderBase::new(SeedString::new("0"), &folder, Some(zones_json.clone()))
        .get("MaskMapping")
        .expect("MaskMapping");
    let mut zone_lookup: HashMap<String, Rc<Zone>> = HashMap::new();
    for zone in &seed_zero.zones {
        zone_lookup.insert(
            zone.name.clone(),
            Rc::new(Zone {
                id: zone.id,
                name: zone.name.clone(),
                discovery_config: zone.discovery.clone(),
                cave_generator: None,
                unique_prefab_container: Rc::new(
                    worldgen_hytale::container::unique_prefab_container::UniquePrefabContainer::new(
                        0,
                        Vec::new(),
                    ),
                ),
                biome_pattern_generator: zone.generator.clone(),
            }),
        );
    }
    let mapping = ZoneColorMappingJsonLoader::new(
        SeedString::new("0"),
        &folder,
        Some(mask_mapping),
        Rc::new(zone_lookup),
    )
    .load();
    for entry in golden["colorMapping"].as_array().expect("colorMapping") {
        let key = entry["key"].as_str().expect("key");
        let rgb = ColorUtil::hex_string(key);
        let mapped = mapping
            .get(rgb)
            .unwrap_or_else(|| panic!("no colour mapping for {key} (#{:x})", rgb));
        let names: Vec<&str> = mapped.iter().map(|zone| zone.name.as_str()).collect();
        let expected = entry["zones"]
            .as_array()
            .expect("zones")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, expected, "colour mapping {key}");
    }

    // --- the per-zone summary (discovery, weights, picks, custom order) ---
    for zone in &seed_zero.zones {
        let expected = &golden["zones"][&zone.name];
        assert_eq!(as_i32(&expected["id"]), zone.id, "zone {}", zone.name);

        // The discovery config.
        let discovery = &expected["discovery"];
        let actual = &zone.discovery;
        assert_eq!(
            actual.display,
            discovery["display"].as_bool().unwrap(),
            "{}: discovery.display",
            zone.name
        );
        assert_eq!(
            actual.zone,
            discovery["zone"].as_str().unwrap(),
            "{}: discovery.zone",
            zone.name
        );
        assert_eq!(
            actual.sound_event_id.as_deref(),
            discovery["soundEventId"].as_str(),
            "{}: discovery.soundEventId",
            zone.name
        );
        assert_eq!(
            actual.icon.as_deref(),
            discovery["icon"].as_str(),
            "{}: discovery.icon",
            zone.name
        );
        assert_eq!(
            actual.major,
            discovery["major"].as_bool().unwrap(),
            "{}: discovery.major",
            zone.name
        );
        assert_hex_f32(
            actual.duration,
            &discovery["duration"],
            &format!("{}: discovery.duration", zone.name),
        );
        assert_hex_f32(
            actual.fade_in_duration,
            &discovery["fadeInDuration"],
            &format!("{}: discovery.fadeInDuration", zone.name),
        );
        assert_hex_f32(
            actual.fade_out_duration,
            &discovery["fadeOutDuration"],
            &format!("{}: discovery.fadeOutDuration", zone.name),
        );

        // The tile biomes (weight, size modifier, map colour).
        let expected_tile = expected["tileBiomes"].as_array().expect("tileBiomes");
        assert_eq!(expected_tile.len(), zone.tile_biomes.len());
        for (expected_biome, biome) in expected_tile.iter().zip(zone.tile_biomes.iter()) {
            assert_eq!(as_i32(&expected_biome["id"]), biome.get_id());
            assert_eq!(expected_biome["name"].as_str().unwrap(), biome.get_name());
            assert_hex(
                biome.get_weight(),
                &expected_biome["weight"],
                &format!("{}: {} weight", zone.name, biome.get_name()),
            );
            assert_hex(
                biome.get_size_modifier(),
                &expected_biome["sizeModifier"],
                &format!("{}: {} sizeModifier", zone.name, biome.get_name()),
            );
            assert_eq!(
                as_i32(&expected_biome["mapColor"]),
                biome.get_map_color(),
                "{}: {} mapColor",
                zone.name,
                biome.get_name()
            );
        }

        // The weighted picks.
        let expected_picks = expected["tileBiomePicks"]
            .as_array()
            .expect("tileBiomePicks");
        assert_eq!(expected_picks.len(), PICK_VALUES.len());
        for (pick, expected_pick) in PICK_VALUES.iter().zip(expected_picks.iter()) {
            let actual = zone
                .tile_map
                .get(*pick)
                .map(|biome| biome.get_name().to_string());
            assert_eq!(
                actual.as_deref(),
                expected_pick.as_str(),
                "{}: pick {pick}",
                zone.name
            );
        }

        // The custom biome order (descending priority).
        let expected_custom = expected["customBiomes"].as_array().expect("customBiomes");
        let actual_custom: Vec<&str> = zone
            .custom_biomes
            .iter()
            .map(|biome| biome.get_name())
            .collect();
        let expected_custom: Vec<&str> = expected_custom
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            actual_custom, expected_custom,
            "{}: custom biomes",
            zone.name
        );
    }

    // --- the caves (the phase-4 seam: the seed chain of the entry points) ---
    for zone in &seed_zero.zones {
        let expected = &golden["caves"][&zone.name];
        let caves_json = zone.path.join("Cave").join("Caves.json");
        if !caves_json.exists() {
            assert!(
                !expected["present"].as_bool().unwrap(),
                "{}: caves",
                zone.name
            );
            continue;
        }
        assert!(expected["present"].as_bool().unwrap(), "{}", zone.name);
        let caves = load_json_lenient(&caves_json);
        let types = caves
            .get("Types")
            .and_then(|t| t.as_array())
            .expect("cave types");
        let expected_types = expected["types"].as_array().expect("cave types");
        assert_eq!(
            types.len(),
            expected_types.len(),
            "{}: cave types",
            zone.name
        );
        for (type_json, expected_type) in types.iter().zip(expected_types.iter()) {
            // The cave type entries may be `{"File": ...}` redirects.
            let type_json = resolve_file_reference(type_json, &folder);
            let name = type_json
                .get("Name")
                .and_then(|n| n.as_str())
                .expect("name");
            assert_eq!(expected_type["name"].as_str().unwrap(), name);
            let entry_points = type_json
                .get("EntryPoints")
                .map(|e| resolve_file_reference(e, &folder));
            let seed_key = entry_points
                .as_ref()
                .and_then(|e| e.get("Seed"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            assert_eq!(
                expected_type["entryPoints"]["seedKey"].as_str(),
                seed_key.as_deref(),
                "{}: cave {name} seed key",
                zone.name
            );

            // The seed offset of every world seed.
            for seed in WORLD_SEEDS {
                let chain = SeedString::new(&seed.to_string())
                    .append(&format!(".Zones.Zone-{}", zone.name))
                    .append(".CaveGenerator")
                    .append(&format!("-{name}"))
                    .append(".CaveType");
                let offset =
                    PointGeneratorJsonLoader::new(chain, &folder, entry_points.clone()).load_seed();
                assert_eq!(
                    as_i32(&expected_type["entryPoints"]["seedOffsets"][seed.to_string()]),
                    offset,
                    "{}: cave {name} seed offset (seed {seed})",
                    zone.name
                );
            }
        }
    }

    // --- the per-seed sections ---
    for seed in WORLD_SEEDS {
        let run = if seed == 0 {
            // The seed-0 run was loaded with the full containers.
            SeedRun {
                seed: seed_zero.seed,
                base: seed_zero.base.clone(),
                mask_name: seed_zero.mask_name.clone(),
                mask_kind: seed_zero.mask_kind,
                mask_random_seed: seed_zero.mask_random_seed,
                mask_map_size: seed_zero.mask_map_size,
                zone_requirement: seed_zero.zone_requirement.clone(),
                zone_pattern_grid_seed_offset: seed_zero.zone_pattern_grid_seed_offset,
                zone_pattern_grid_seed_key: seed_zero.zone_pattern_grid_seed_key.clone(),
                zones: Vec::new(),
            }
        } else {
            run_chain(seed, &folder, &registries, false)
        };
        let expected = &golden["seeds"][seed.to_string()];
        assert_eq!(expected["seedString"].as_str().unwrap(), run.base.seed());
        assert_eq!(as_i32(&expected["seedHash"]), run.base.hash_code());
        assert_eq!(
            as_i32(&expected["zonePatternGrid"]["seedOffset"]),
            run.zone_pattern_grid_seed_offset,
            "seed {seed}: zone pattern grid"
        );
        assert_eq!(
            expected["zonePatternGrid"]["seedKey"].as_str(),
            run.zone_pattern_grid_seed_key.as_deref(),
            "seed {seed}: zone pattern grid key"
        );
        assert_eq!(expected["maskPick"].as_str().unwrap(), run.mask_name);
        assert_eq!(expected["maskKind"].as_str().unwrap(), run.mask_kind);

        // The per-zone biome pattern grid and the biome samples.
        let zones = if seed == 0 {
            &seed_zero.zones
        } else {
            &run.zones
        };
        let expected_zones = expected["zones"].as_object().expect("zones");
        assert_eq!(expected_zones.len(), zones.len(), "seed {seed}");
        for zone in zones {
            let expected_zone = &expected_zones[&zone.name];
            assert_eq!(
                as_i32(&expected_zone["biomePatternGrid"]["seedOffset"]),
                zone.biome_grid_seed_offset,
                "seed {seed}: {} biome grid",
                zone.name
            );
            assert_eq!(
                expected_zone["biomePatternGrid"]["seedKey"].as_str(),
                zone.biome_grid_seed_key.as_deref(),
                "seed {seed}: {} biome grid key",
                zone.name
            );
            assert_eq!(
                as_i32(&expected_zone["biomePatternGrid"]["extents"]),
                zone.generator.get_extents(),
                "seed {seed}: {} extents",
                zone.name
            );

            let custom: Vec<&str> = zone
                .custom_biomes
                .iter()
                .map(|biome| biome.get_name())
                .collect();
            let expected_custom: Vec<&str> = expected_zone["customBiomes"]
                .as_array()
                .expect("customBiomes")
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(
                custom, expected_custom,
                "seed {seed}: {} custom biomes",
                zone.name
            );

            // The sampled biomes.
            let indices = expected_zone["biomeIndex"].as_array().expect("biomeIndex");
            let biomes = expected_zone["biome"].as_array().expect("biome");
            let directs = expected_zone["biomeDirect"]
                .as_array()
                .expect("biomeDirect");
            assert_eq!(indices.len(), BIOME_SAMPLE_COORDS.len());
            for i in 0..BIOME_SAMPLE_COORDS.len() {
                let (x, z) = BIOME_SAMPLE_COORDS[i];
                assert_eq!(as_i32(&indices[i]["x"]), x);
                assert_eq!(as_i32(&indices[i]["z"]), z);
                assert_hex(
                    zone.generator.get_biome_index(seed, x, z),
                    &indices[i]["v"],
                    &format!("seed {seed}: {} biome index ({x},{z})", zone.name),
                );
                assert_eq!(
                    zone.generator
                        .get_biome(seed, x, z)
                        .map(|biome| biome.get_name().to_string()),
                    biomes[i]["v"].as_str().map(|s| s.to_string()),
                    "seed {seed}: {} biome ({x},{z})",
                    zone.name
                );
                assert_eq!(
                    zone.generator
                        .get_biome_direct(seed, x as f64, z as f64)
                        .map(|biome| biome.get_name().to_string()),
                    directs[i]["v"].as_str().map(|s| s.to_string()),
                    "seed {seed}: {} biome direct ({x},{z})",
                    zone.name
                );
            }
        }
    }

    // --- the biome containers (the seed-0 run) ---
    let expected_biomes = golden["biomes"].as_object().expect("biomes");
    let mut checked = 0usize;
    let mut skipped_containers: Vec<String> = Vec::new();
    for zone in &seed_zero.zones {
        let mut all: Vec<(Rc<dyn Biome>, bool, f64)> = Vec::new();
        for biome in &zone.tile_biomes {
            all.push((Rc::clone(biome) as Rc<dyn Biome>, true, biome.get_weight()));
        }
        for biome in &zone.custom_biomes {
            all.push((Rc::clone(biome) as Rc<dyn Biome>, false, 0.0));
        }

        for (biome, is_tile, weight) in all {
            let id = biome.get_id();
            let expected = expected_biomes
                .get(&id.to_string())
                .unwrap_or_else(|| panic!("no golden biome {id}"));
            assert_eq!(as_i32(&expected["id"]), id);
            assert_eq!(expected["name"].as_str().unwrap(), biome.get_name());
            assert_eq!(expected["zone"].as_str().unwrap(), zone.name);
            let expected_type = expected["type"].as_str().unwrap();
            assert_eq!(
                expected_type,
                if is_tile { "Tile" } else { "Custom" },
                "biome {id}: tile/custom type"
            );

            let complete = zone
                .biome_containers_complete
                .get(&id)
                .copied()
                .unwrap_or(false);

            let json = &zone.biome_json[&id];
            assert_eq!(
                expected["prefabContainer"]["present"].as_bool().unwrap(),
                json.get("Prefabs").is_some(),
                "biome {id}: prefab container presence"
            );
            assert_eq!(
                expected["heightmapNoisePresent"].as_bool().unwrap(),
                json.get("HeightmapNoise").is_some(),
                "biome {id}: heightmap noise presence"
            );

            if is_tile {
                assert_hex(weight, &expected["weight"], &format!("biome {id}: weight"));
            } else {
                assert!(
                    expected.get("weight").is_none(),
                    "biome {id}: a custom biome has no weight"
                );
            }

            // The interpolation.
            let interpolation = biome.get_interpolation();
            assert_eq!(
                as_i32(&expected["interpolation"]["radius"]),
                interpolation.get_radius(),
                "biome {id}: interpolation radius"
            );
            let radii = expected["interpolation"]["biomeRadii2"]
                .as_array()
                .expect("biomeRadii2");
            for entry in radii {
                let biome_id = as_i32(&entry[0]);
                assert_eq!(
                    as_i32(&entry[1]),
                    interpolation.get_biome_radius2(biome_id),
                    "biome {id}: interpolation radius of {biome_id}"
                );
            }
            if radii.is_empty() {
                assert_eq!(
                    interpolation.get_default_return(),
                    MAX_RADIUS2,
                    "biome {id}: the empty radius map must keep the Java default"
                );
            }

            // The container assertions (skipped for the biomes whose
            // containers the Rust port cannot build yet).
            if !complete {
                skipped_containers.push(biome.get_name().to_string());
            } else {
                // The layer container.
                let layers = biome.get_layer_container();
                let filling = layers.get_filling();
                assert_eq!(
                    as_i32(&expected["layers"]["filling"]["blockId"]),
                    filling.block_id,
                    "biome {id}: filling block"
                );
                assert_eq!(
                    as_i32(&expected["layers"]["filling"]["rotation"]),
                    filling.rotation,
                    "biome {id}: filling rotation"
                );
                assert_eq!(
                    as_i32(&expected["layers"]["filling"]["fluidId"]),
                    filling.fluid_id,
                    "biome {id}: filling fluid"
                );
                assert_eq!(
                    as_i32(&expected["layers"]["fillingEnvironment"]),
                    layers.get_filling_environment(),
                    "biome {id}: filling environment"
                );

                let top_blocks = expected["layers"]["topBlockAt"]
                    .as_array()
                    .expect("topBlockAt");
                for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                    let block = layers.get_top_block_at(CONTAINER_SAMPLE_SEED, *x, *z);
                    assert_eq!(as_i32(&top_blocks[i]["x"]), *x);
                    assert_eq!(as_i32(&top_blocks[i]["z"]), *z);
                    assert_noise_derived(
                        as_i32(&top_blocks[i]["blockId"]),
                        block.block_id,
                        KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                        &format!("biome {id}: top block at ({x},{z})"),
                    );
                    assert_eq!(
                        as_i32(&top_blocks[i]["rotation"]),
                        block.rotation,
                        "biome {id}: top block rotation at ({x},{z})"
                    );
                    assert_eq!(
                        as_i32(&top_blocks[i]["fluidId"]),
                        block.fluid_id,
                        "biome {id}: top block fluid at ({x},{z})"
                    );
                }

                let static_layers = expected["layers"]["staticLayers"]
                    .as_array()
                    .expect("staticLayers");
                assert_eq!(
                    static_layers.len(),
                    layers.get_static_layers().len(),
                    "biome {id}: static layer count"
                );
                for (expected_layer, layer) in
                    static_layers.iter().zip(layers.get_static_layers().iter())
                {
                    assert_eq!(
                        as_i32(&expected_layer["environmentId"]),
                        layer.get_environment_id(),
                        "biome {id}: static layer environment"
                    );
                    let expected_entries = expected_layer["entries"].as_array().expect("entries");
                    assert_eq!(
                        expected_entries.len(),
                        layer.get_entries().len(),
                        "biome {id}: static layer entry count"
                    );
                    for (expected_entry, entry) in
                        expected_entries.iter().zip(layer.get_entries().iter())
                    {
                        let block_array = entry.get_block_array();
                        let expected_repetitions = expected_entry["repetitions"]
                            .as_array()
                            .expect("repetitions");
                        assert_eq!(
                            expected_repetitions.len(),
                            block_array.get_entries().len(),
                            "biome {id}: repetition count"
                        );
                        for (expected_values, repetition) in expected_repetitions
                            .iter()
                            .zip(block_array.get_entries().iter())
                        {
                            for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                                assert_noise_repetitions(
                                    as_i32(&expected_values[i]),
                                    repetition.get_repetitions(
                                        CONTAINER_SAMPLE_SEED,
                                        *x as f64,
                                        *z as f64,
                                    ),
                                    KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                                    &format!("biome {id}: repetitions at ({x},{z})"),
                                );
                            }
                        }
                        for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                            let known = KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z));
                            assert_noise_int(
                                as_i32(&expected_entry["minInt"][i]),
                                entry.get_min_int(CONTAINER_SAMPLE_SEED, *x, *z),
                                known,
                                &format!("biome {id}: min int at ({x},{z})"),
                            );
                            assert_noise_int(
                                as_i32(&expected_entry["maxInt"][i]),
                                entry.get_max_int(CONTAINER_SAMPLE_SEED, *x, *z),
                                known,
                                &format!("biome {id}: max int at ({x},{z})"),
                            );
                            assert_noise_value(
                                &expected_entry["minValue"][i],
                                entry.get_min_value(CONTAINER_SAMPLE_SEED, *x, *z),
                                known,
                                &format!("biome {id}: min value at ({x},{z})"),
                            );
                            assert_noise_value(
                                &expected_entry["maxValue"][i],
                                entry.get_max_value(CONTAINER_SAMPLE_SEED, *x, *z),
                                known,
                                &format!("biome {id}: max value at ({x},{z})"),
                            );
                        }
                    }
                }

                let dynamic_layers = expected["layers"]["dynamicLayers"]
                    .as_array()
                    .expect("dynamicLayers");
                assert_eq!(
                    dynamic_layers.len(),
                    layers.get_dynamic_layers().len(),
                    "biome {id}: dynamic layer count"
                );
                for (expected_layer, layer) in dynamic_layers
                    .iter()
                    .zip(layers.get_dynamic_layers().iter())
                {
                    assert_eq!(
                        as_i32(&expected_layer["environmentId"]),
                        layer.get_environment_id(),
                        "biome {id}: dynamic layer environment"
                    );
                    let expected_entries = expected_layer["entries"].as_array().expect("entries");
                    assert_eq!(
                        expected_entries.len(),
                        layer.get_entries().len(),
                        "biome {id}: dynamic layer entry count"
                    );
                    for (expected_entry, entry) in
                        expected_entries.iter().zip(layer.get_entries().iter())
                    {
                        let block_array = entry.get_block_array();
                        let expected_repetitions = expected_entry["repetitions"]
                            .as_array()
                            .expect("repetitions");
                        assert_eq!(
                            expected_repetitions.len(),
                            block_array.get_entries().len(),
                            "biome {id}: dynamic repetition count"
                        );
                        for (expected_values, repetition) in expected_repetitions
                            .iter()
                            .zip(block_array.get_entries().iter())
                        {
                            for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                                assert_noise_repetitions(
                                    as_i32(&expected_values[i]),
                                    repetition.get_repetitions(
                                        CONTAINER_SAMPLE_SEED,
                                        *x as f64,
                                        *z as f64,
                                    ),
                                    KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                                    &format!("biome {id}: dynamic repetitions at ({x},{z})"),
                                );
                            }
                        }
                    }
                    for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                        assert_noise_int(
                            as_i32(&expected_layer["offset"][i]),
                            layer.get_offset(CONTAINER_SAMPLE_SEED, *x, *z),
                            KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                            &format!("biome {id}: dynamic offset at ({x},{z})"),
                        );
                    }
                }

                // The cover container.
                let covers = biome.get_cover_container();
                let expected_covers = expected["covers"].as_array().expect("covers");
                assert_eq!(
                    expected_covers.len(),
                    covers.get_entries().len(),
                    "biome {id}: cover count"
                );
                for (expected_cover, cover) in
                    expected_covers.iter().zip(covers.get_entries().iter())
                {
                    assert_hex(
                        cover.get_cover_density(),
                        &expected_cover["density"],
                        &format!("biome {id}: cover density"),
                    );
                    assert_eq!(
                        cover.is_on_water(),
                        expected_cover["onWater"].as_bool().unwrap(),
                        "biome {id}: cover onWater"
                    );

                    let mut parts: Vec<(i32, i32, i32, i32, f64)> = Vec::new();
                    cover.get_entries().for_each_entry(&mut |part, weight| {
                        let entry = part.get_entry();
                        parts.push((
                            entry.block_id,
                            entry.rotation,
                            entry.fluid_id,
                            part.get_offset(),
                            weight,
                        ));
                    });
                    let expected_parts = expected_cover["parts"].as_array().expect("parts");
                    assert_eq!(
                        expected_parts.len(),
                        parts.len(),
                        "biome {id}: cover part count"
                    );
                    for (expected_part, (block_id, rotation, fluid_id, offset, weight)) in
                        expected_parts.iter().zip(parts.iter())
                    {
                        assert_hex(
                            *weight,
                            &expected_part["weight"],
                            &format!("biome {id}: cover part weight"),
                        );
                        assert_eq!(
                            as_i32(&expected_part["blockId"]),
                            *block_id,
                            "biome {id}: cover part block"
                        );
                        assert_eq!(
                            as_i32(&expected_part["rotation"]),
                            *rotation,
                            "biome {id}: cover part rotation"
                        );
                        assert_eq!(
                            as_i32(&expected_part["fluidId"]),
                            *fluid_id,
                            "biome {id}: cover part fluid"
                        );
                        assert_eq!(
                            as_i32(&expected_part["offset"]),
                            *offset,
                            "biome {id}: cover part offset"
                        );
                    }

                    // The picked part (a fresh FastRandom per cover entry).
                    let mut random = FastRandom::new(COVER_PICK_SEED);
                    let picked = cover.get(&mut random);
                    let expected_picked = &expected_cover["picked"];
                    if let Some(picked) = picked {
                        assert_eq!(
                            as_i32(&expected_picked["blockId"]),
                            picked.get_entry().block_id,
                            "biome {id}: picked cover block"
                        );
                        assert_eq!(
                            as_i32(&expected_picked["rotation"]),
                            picked.get_entry().rotation,
                            "biome {id}: picked cover rotation"
                        );
                        assert_eq!(
                            as_i32(&expected_picked["fluidId"]),
                            picked.get_entry().fluid_id,
                            "biome {id}: picked cover fluid"
                        );
                        assert_eq!(
                            as_i32(&expected_picked["offset"]),
                            picked.get_offset(),
                            "biome {id}: picked cover offset"
                        );
                    } else {
                        assert!(
                            expected_picked.is_null(),
                            "biome {id}: the cover pick must be null"
                        );
                    }
                }

                // The tint container.
                let tint = biome.get_tint_container();
                let mut colors: Vec<(i32, f64)> = Vec::new();
                tint.get_default_entry()
                    .get_color_mapping()
                    .for_each_entry(&mut |color, weight| colors.push((*color, weight)));
                let expected_colors = expected["tint"]["default"]["colors"]
                    .as_array()
                    .expect("colors");
                assert_eq!(
                    expected_colors.len(),
                    colors.len(),
                    "biome {id}: tint colour count"
                );
                for (expected_color, (color, weight)) in expected_colors.iter().zip(colors.iter()) {
                    assert_eq!(as_i32(&expected_color["c"]), *color);
                    assert_hex(
                        *weight,
                        &expected_color["w"],
                        &format!("biome {id}: tint colour weight"),
                    );
                }
                assert_eq!(
                    expected["tint"]["entryCount"].as_u64().unwrap() as usize,
                    tint.get_entries().len(),
                    "biome {id}: tint entry count"
                );
                let tint_at = expected["tint"]["tintColorAt"]
                    .as_array()
                    .expect("tintColorAt");
                for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                    assert_noise_derived(
                        as_i32(&tint_at[i]["v"]),
                        tint.get_tint_color_at(CONTAINER_SAMPLE_SEED, *x, *z),
                        KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                        &format!("biome {id}: tint colour at ({x},{z})"),
                    );
                }

                // The environment container.
                let environment = biome.get_environment_container();
                let mut environments: Vec<(i32, f64)> = Vec::new();
                environment
                    .get_default_entry()
                    .get_environment_mapping()
                    .for_each_entry(&mut |id, weight| environments.push((*id, weight)));
                let expected_environments = expected["environment"]["default"]["environments"]
                    .as_array()
                    .expect("environments");
                assert_eq!(
                    expected_environments.len(),
                    environments.len(),
                    "biome {id}: environment count"
                );
                for (expected_environment, (id, weight)) in
                    expected_environments.iter().zip(environments.iter())
                {
                    assert_eq!(as_i32(&expected_environment["c"]), *id);
                    assert_hex(
                        *weight,
                        &expected_environment["w"],
                        &format!("biome {id}: environment weight"),
                    );
                }
                assert_eq!(
                    expected["environment"]["entryCount"].as_u64().unwrap() as usize,
                    environment.get_entries().len(),
                    "biome {id}: environment entry count"
                );
                let environment_at = expected["environment"]["environmentAt"]
                    .as_array()
                    .expect("environmentAt");
                for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                    assert_noise_derived(
                        as_i32(&environment_at[i]["v"]),
                        environment.get_environment_at(CONTAINER_SAMPLE_SEED, *x, *z),
                        KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                        &format!("biome {id}: environment at ({x},{z})"),
                    );
                }

                // The water container.
                let water = biome.get_water_container();
                assert_eq!(
                    expected["water"]["hasEntries"].as_bool().unwrap(),
                    water.has_entries(),
                    "biome {id}: water hasEntries"
                );
                let expected_entries = expected["water"]["entries"].as_array().expect("entries");
                assert_eq!(
                    expected_entries.len(),
                    water.get_entries().len(),
                    "biome {id}: water entry count"
                );
                for (expected_entry, entry) in
                    expected_entries.iter().zip(water.get_entries().iter())
                {
                    assert_eq!(
                        as_i32(&expected_entry["blockId"]),
                        entry.get_block(),
                        "biome {id}: water block"
                    );
                    assert_eq!(
                        as_i32(&expected_entry["fluidId"]),
                        entry.get_fluid(),
                        "biome {id}: water fluid"
                    );
                    for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                        assert_noise_int(
                            as_i32(&expected_entry["min"][i]),
                            entry.get_min(CONTAINER_SAMPLE_SEED, *x, *z),
                            KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                            &format!("biome {id}: water min at ({x},{z})"),
                        );
                        assert_noise_int(
                            as_i32(&expected_entry["max"][i]),
                            entry.get_max(CONTAINER_SAMPLE_SEED, *x, *z),
                            KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                            &format!("biome {id}: water max at ({x},{z})"),
                        );
                    }
                }
                let max_heights = expected["water"]["maxHeight"]
                    .as_array()
                    .expect("maxHeight");
                for (i, (x, z)) in CONTAINER_SAMPLE_COORDS.iter().enumerate() {
                    assert_noise_derived(
                        as_i32(&max_heights[i]["v"]),
                        water.get_max_height(CONTAINER_SAMPLE_SEED, *x, *z),
                        KNOWN_NOISE_DEVIATION_COLUMNS.contains(&(*x, *z)),
                        &format!("biome {id}: water max height at ({x},{z})"),
                    );
                }

                // The fade container.
                let fade = biome.get_fade_container();
                assert_hex(
                    fade.get_mask_fade_start(),
                    &expected["fade"]["maskFadeStart"],
                    &format!("biome {id}: mask fade start"),
                );
                assert_hex(
                    fade.get_mask_fade_length(),
                    &expected["fade"]["maskFadeLength"],
                    &format!("biome {id}: mask fade length"),
                );
                assert_hex(
                    fade.get_mask_fade_sum(),
                    &expected["fade"]["maskFadeSum"],
                    &format!("biome {id}: mask fade sum"),
                );
                assert_hex(
                    fade.get_height_fade_start(),
                    &expected["fade"]["heightFadeStart"],
                    &format!("biome {id}: height fade start"),
                );
                assert_hex(
                    fade.get_height_fade_length(),
                    &expected["fade"]["heightFadeLength"],
                    &format!("biome {id}: height fade length"),
                );
                assert_hex(
                    fade.get_height_fade_sum(),
                    &expected["fade"]["heightFadeSum"],
                    &format!("biome {id}: height fade sum"),
                );
                assert_hex(
                    fade.get_fade_heightmap(),
                    &expected["fade"]["fadeHeightmap"],
                    &format!("biome {id}: fade heightmap"),
                );
            }

            checked += 1;
        }
    }
    assert_eq!(
        checked,
        expected_biomes.len(),
        "every dumped biome must be checked"
    );

    // The pinned noise deviations are counted, never ignored.
    assert_eq!(
        NOISE_DEVIATIONS.load(std::sync::atomic::Ordering::Relaxed),
        EXPECTED_NOISE_DEVIATIONS,
        "the number of noise-derived samples that deviate at the pinned columns"
    );

    // Restore a plain panic hook for the rest of the test binary.
    std::panic::set_hook(Box::new(|info| eprintln!("panicked: {info}")));

    // The two custom biomes whose cover noise uses the `MIN` / `MAX`
    // fractal modes: the Rust `NoisePropertyJsonLoader` could not map those
    // enum constants yet (`crates/procedural`), so their containers were not
    // comparable. The mapping is implemented now, so every container of
    // every biome is compared. The list is asserted, never skipped
    // silently.
    let mut skipped_sorted = skipped_containers.clone();
    skipped_sorted.sort();
    assert_eq!(
        skipped_sorted,
        Vec::<String>::new(),
        "the biomes whose containers the Rust port cannot build yet"
    );
}
