//! End-to-end loader tests: the full `ChunkGeneratorJsonLoader::load()` path
//! (world JSON → mask provider → zone pattern provider → file contexts →
//! zones → `ChunkGenerator`).
//!
//! * The **real Default world** walks the whole chain and stops at the
//!   documented Phase-4 cave seam (the first zone's tile biome carries
//!   block-referencing containers too, but every Default zone has a cave
//!   configuration, so the cave seam fires first).
//! * A **synthetic minimal world** (temp dir, real block/fluid/environment
//!   names) loads completely through `ChunkGenerator::new`; the resolved
//!   biome containers are checked against the golden registry ids.

use std::path::Path;

use procedural::json::SeedString;
use worldgen_hytale::container::LayerEntry;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen_config::WorldGenConfig;

/// Whether the base-game asset pack is available (it is not part of the
/// source tree — see `.gitignore`). The biome prefab containers resolve
/// their prefab names through it.
fn assets_available() -> bool {
    std::path::Path::new(&format!(
        "{}/../../Assets/extracted/Server/Prefabs",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
    .is_dir()
}

fn manifest_root() -> String {
    format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    )
}

#[test]
fn default_world_loads_completely() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = manifest_root();
    let config = WorldGenConfig::new(root.into(), "Default".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);

    // The chain runs: world json → climate mask → 28-zone requirement →
    // context scan → zone parsing → the cave configuration of the zone →
    // the biome chain (including the biome prefab containers, whose prefab
    // names resolve through the prefab store).
    //
    // The Phase-4 cave seam and the Phase-5a prefab seams are cleared; the
    // only remaining seam is the *unique* prefab container (Phase 5c), which
    // is skipped with a warning.
    let generator = loader.load();
    assert_eq!(
        generator.zone_pattern_provider().get_zones().len(),
        28,
        "the Default world has 28 zones"
    );
}

/// Builds a minimal synthetic world (single climate type, one zone, one tile
/// biome with all seven containers) and returns the temp root.
fn write_minimal_world() -> String {
    // Unique per test (tests run in parallel threads in one process).
    let root = std::env::temp_dir().join(format!(
        "hytale-loader-test-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "-")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Zones/Zone1")).unwrap();

    std::fs::write(
        root.join("World.json"),
        r#"{
  "Masks": ["Mask.json"],
  "PrefabStore": "WORLD_GEN",
  "Width": 1,
  "Height": 1,
  "OffsetX": 0,
  "OffsetY": 0
}"#,
    )
    .unwrap();

    // Procedural climate mask: constant noises, one climate type without
    // children, no unique zones.
    std::fs::write(
        root.join("Mask.json"),
        r##"{
  "Randomizer": {"Generators": []},
  "Noise": {
    "Thresholds": {"Land": 0.5, "Island": 0.75, "BeachSize": 0.02, "ShallowOceanSize": 0.08},
    "Continent": {"NoiseType": "CONSTANT", "Value": 0.9},
    "Temperature": {"NoiseType": "CONSTANT", "Value": 0.5},
    "Intensity": {"NoiseType": "CONSTANT", "Value": 0.15}
  },
  "Climate": {
    "FadeMode": "CHILDREN",
    "FadeRadius": 50.0,
    "FadeDistance": 100.0,
    "Climates": [
      {
        "Name": "Test Climate",
        "Color": "#78ff27",
        "Shore": "#78ff27",
        "Ocean": "#3013c2",
        "ShallowOcean": "#3013c2",
        "Points": [{"Temperature": 0.5, "Intensity": 0.15, "Modifier": 1.0}],
        "Children": []
      }
    ]
  },
  "UniqueZones": []
}"##,
    )
    .unwrap();

    std::fs::write(
        root.join("Zones.json"),
        r##"{
  "MaskMapping": {
    "#78ff27": ["Zone1"],
    "#3013c2": ["Zone1"]
  }
}"##,
    )
    .unwrap();

    // The zone: no Discovery, no Cave, no UniquePrefabs; the biome generator
    // inherits the tile biomes from the folder scan.
    std::fs::write(root.join("Zones/Zone1/Zone.json"), "{}").unwrap();

    // One tile biome with real block/fluid/environment names. The ids come
    // from `testdata/golden/registries.json` (the Java asset pipeline dump):
    // Rock_Bedrock=445, Rock_Stone=1527, Soil_Grass=1250, Water_Source=2,
    // environment Default=58.
    //
    // `TerrainHeightThreshold` is REQUIRED (Java: an absent key is an NPE
    // wrapped into the biome load error); a basic Positions/Values
    // interpreter pair.
    std::fs::write(
        root.join("Zones/Zone1/Tile.Plains.json"),
        r##"{
  "Weight": 1.0,
  "TerrainHeightThreshold": {
    "Positions": [104, 117, 121, 125],
    "Values": [1, 0.9, 0.5, 0]
  },
  "Layers": {
    "Default": "Rock_Bedrock",
    "Environment": "Default",
    "Static": [
      {
        "Blocks": "Rock_Stone",
        "Min": 10,
        "Max": 20
      }
    ],
    "Dynamic": [
      {
        "Blocks": "Soil_Grass",
        "Offset": 3
      }
    ]
  },
  "Covers": [
    {
      "Type": "Soil_Grass",
      "Weight": [1.0],
      "Offset": [2],
      "Density": 0.5,
      "OnWater": true,
      "Parent": "Rock_Bedrock"
    }
  ],
  "Tint": {
    "Default": "#102030",
    "Entries": [
      {
        "Colors": "#abcdef",
        "Weights": [1.0]
      }
    ]
  },
  "Environment": {
    "Default": "Default",
    "Entries": [
      {
        "Names": "Default",
        "Weights": [1.0]
      }
    ]
  },
  "Water": {
    "Entries": [
      {
        "Fluid": "Water_Source",
        "Min": 10,
        "Max": 20
      }
    ]
  },
  "Fade": {
    "FadeStart": 1,
    "FadeLength": 2,
    "TerrainStart": 3,
    "TerrainLength": 4,
    "FadeHeightmap": 5
  }
}"##,
    )
    .unwrap();

    root.to_string_lossy().to_string()
}

#[test]
fn minimal_world_loads_end_to_end() {
    let root = write_minimal_world();

    let config = WorldGenConfig::new(root.clone().into(), "Test".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("42"), config);

    let generator = loader.load();

    // The generator holds the fully resolved provider: the zone is reachable
    // through the color mapping (the climate land color) and the tile biome
    // list carries the scanned biome file with id 0.
    let zones = generator.zone_pattern_provider().get_zones();
    assert_eq!(zones.len(), 1);
    assert_eq!(zones[0].name, "Zone1");
    assert_eq!(zones[0].id, 0);

    // The biome containers resolved real names through the golden registries.
    let biomes = zones[0].biome_pattern_generator.get_biomes();
    assert_eq!(biomes.len(), 1);
    let biome = &biomes[0];

    let layers = biome.get_layer_container();
    assert_eq!(layers.get_filling().block_id, 445, "Rock_Bedrock");
    assert_eq!(layers.get_filling_environment(), 58, "environment Default");

    assert_eq!(layers.get_static_layers().len(), 1);
    let static_layer = &layers.get_static_layers()[0];
    assert_eq!(static_layer.get_environment_id(), i32::MIN);
    let static_entry = &static_layer.get_entries()[0];
    assert_eq!(
        static_entry
            .get_block_array()
            .get_top_block_at(0, 0.0, 0.0)
            .block_id,
        1527,
        "Rock_Stone"
    );
    assert_eq!(static_entry.get_min_int(0, 0, 0), 10);
    assert_eq!(static_entry.get_max_int(0, 0, 0), 20);

    assert_eq!(layers.get_dynamic_layers().len(), 1);
    let dynamic_layer = &layers.get_dynamic_layers()[0];
    assert_eq!(dynamic_layer.get_offset(0, 0, 0), 3);
    assert_eq!(
        dynamic_layer.get_entries()[0]
            .get_block_array()
            .get_top_block_at(0, 0.0, 0.0)
            .block_id,
        1250,
        "Soil_Grass"
    );

    let covers = biome.get_cover_container();
    assert_eq!(covers.get_entries().len(), 1);
    let cover = &covers.get_entries()[0];
    assert_eq!(cover.get_cover_density(), 0.5);
    assert!(cover.is_on_water());
    let part = cover.get_entries().get(0.0).expect("one cover part");
    assert_eq!(part.get_entry().block_id, 1250, "Soil_Grass");
    assert_eq!(part.get_offset(), 2);
    assert!(
        cover.get_parent_condition().eval(445, 0),
        "Rock_Bedrock parent"
    );

    let tint = biome.get_tint_container();
    assert_eq!(tint.get_default_entry().get_color_mapping().size(), 1);
    assert_eq!(
        tint.get_default_entry()
            .get_color_mapping()
            .get(0.0)
            .copied(),
        Some(0x102030)
    );
    assert_eq!(tint.get_entries().len(), 1);
    assert_eq!(tint.get_tint_color_at(0, 0, 0), 0xabcdef);

    let environment = biome.get_environment_container();
    assert_eq!(
        environment
            .get_default_entry()
            .get_environment_mapping()
            .get(0.0)
            .copied(),
        Some(58)
    );
    assert_eq!(environment.get_entries().len(), 1);
    assert_eq!(environment.get_environment_at(0, 0, 0), 58);

    let water = biome.get_water_container();
    assert_eq!(water.get_entries().len(), 1);
    let water_entry = &water.get_entries()[0];
    assert_eq!(water_entry.get_block(), 0);
    assert_eq!(water_entry.get_fluid(), 2, "Water_Source");
    assert_eq!(water_entry.get_min(0, 0, 0), 10);
    assert_eq!(water_entry.get_max(0, 0, 0), 20);

    let fade = biome.get_fade_container();
    assert_eq!(fade.get_mask_fade_start(), 1.0);
    assert_eq!(fade.get_mask_fade_length(), 2.0);
    assert_eq!(fade.get_height_fade_start(), 3.0);
    assert_eq!(fade.get_height_fade_length(), 4.0);
    assert_eq!(fade.get_fade_heightmap(), 5.0);

    assert!(biome.get_prefab_container().is_none());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn minimal_world_requires_the_mask_mapping_zone_to_exist() {
    let root = write_minimal_world();
    // Remove the zone folder: the requirement validation must fail.
    std::fs::remove_dir_all(Path::new(&root).join("Zones/Zone1")).unwrap();

    let config = WorldGenConfig::new(root.clone().into(), "Test".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("42"), config);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader.load()));
    let err = result.err().expect("missing zone must fail validation");
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| "non-string panic".to_string());
    assert!(
        msg.contains("Failed to validate zones") || msg.contains("Missing Zone entry"),
        "unexpected error: {msg}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
