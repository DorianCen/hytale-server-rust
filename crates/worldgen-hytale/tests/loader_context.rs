//! Structural tests for the loader context tree (`FileContextLoader`) against
//! the real Default world assets (`testdata/Default/Zones/`).
//!
//! These verify the id-assignment ORDER determinism (sorted zone folders;
//! per-zone `Tile.` before `Custom.` biome files, each alphabetically). The
//! definitive bit-exact verification of the ids happens with the Java dumper
//! extension (see `PORTING_HISTORY.md`) — the ids flow into seed derivation, so
//! they must match the Java scan exactly.

use std::collections::HashSet;

use procedural::json::SeedString;
use worldgen_hytale::loader::context::file_context_loader::FileContextLoader;
use worldgen_hytale::loader::zone::zone_requirement_json_loader::ZoneRequirementJsonLoader;

fn manifest_root() -> String {
    format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    )
}

#[test]
fn zone_requirement_collects_the_mask_mapping_zones() {
    let root = manifest_root();
    let zones_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{root}/Zones.json")).unwrap())
            .unwrap();

    let loader =
        ZoneRequirementJsonLoader::new(SeedString::new("0"), root.as_ref(), Some(zones_json));
    let requirement = loader.load();

    // Every zone name referenced by the Default world's MaskMapping.
    for name in [
        "Oceans",
        "Zone1_Shallow_Ocean",
        "Zone1_Shore",
        "Zone1_Spawn",
        "Zone1_Temple",
        "Zone1_Tier1",
        "Zone1_Tier2",
        "Zone1_Tier3",
        "Zone2_Tier3",
        "Zone3_Tier3",
        "Zone4_Tier4",
        "Zone4_Tier5",
    ] {
        assert!(requirement.contains(name), "missing {name}");
    }
    assert_eq!(requirement.len(), 28);
}

#[test]
fn context_loader_assigns_zone_ids_in_sorted_order() {
    let root = manifest_root();
    let zones_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{root}/Zones.json")).unwrap())
            .unwrap();

    let requirement =
        ZoneRequirementJsonLoader::new(SeedString::new("0"), root.as_ref(), Some(zones_json))
            .load();

    let loader = FileContextLoader::new("Default", root.as_ref(), requirement);
    let context = loader.load();

    let zones = context.get_zones();
    let registry = zones.borrow();
    assert_eq!(registry.size(), 28);

    // Java: folders sorted by AssetPath.COMPARATOR (component-wise string
    // compare), ids assigned in registration order starting at 0.
    let expected_order = {
        let mut names: Vec<String> = registry.iter().map(|(n, _)| n.to_string()).collect();
        names.sort_by(|a, b| {
            // java string compare == byte compare for ASCII names
            a.cmp(b)
        });
        names
    };
    for (index, (name, zone)) in registry.iter().enumerate() {
        assert_eq!(&expected_order[index], name);
        assert_eq!(zone.base.id, index as i32, "zone {name}");
    }

    // The helper folders (Layers/Masks/Noise/PrefabPatterns_Shared) must not
    // be registered as zones (no Zone.json inside).
    let names: HashSet<String> = registry.iter().map(|(n, _)| n.to_string()).collect();
    for helper in ["Layers", "Masks", "Noise", "PrefabPatterns_Shared"] {
        assert!(!names.contains(helper), "{helper} must not be a zone");
    }

    // Spot-check the biome scan of Zone1_Spawn: one Tile + one Custom biome,
    // Tile sorted before Custom (Java enum ordinal order), ids continuing the
    // global biome counter.
    let spawn = registry.get("Zone1_Spawn");
    let tile_biomes = spawn.get_tile_biomes().borrow();
    let custom_biomes = spawn.get_custom_biomes().borrow();
    assert_eq!(tile_biomes.size(), 1);
    assert_eq!(custom_biomes.size(), 1);
    assert_eq!(tile_biomes.iter().next().unwrap().0, "Plains_Spawn");
    assert_eq!(custom_biomes.iter().next().unwrap().0, "River_Plains_Spawn");
    assert!(
        tile_biomes.iter().next().unwrap().1.base.id
            < custom_biomes.iter().next().unwrap().1.base.id
    );
}
