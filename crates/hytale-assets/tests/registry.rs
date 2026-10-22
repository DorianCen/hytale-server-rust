//! P0 asset registry tests.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_protocol::generated::common::Opacity;

fn fixture_pack() -> AssetPack {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
    AssetPack::open(path).expect("open the fixture pack")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn opacity_name(opacity: Opacity) -> &'static str {
    match opacity {
        Opacity::Solid => "Solid",
        Opacity::Semitransparent => "Semitransparent",
        Opacity::Cutout => "Cutout",
        Opacity::Transparent => "Transparent",
    }
}

#[test]
fn loads_the_p0_stores_and_assigns_ids_once() {
    let registry = AssetRegistry::load_p0(&fixture_pack()).expect("load the fixture registry");

    let fluids: Vec<(&str, i32)> = registry
        .fluids
        .iter()
        .map(|asset| (asset.key.as_str(), asset.id.expect("fluid id")))
        .collect();
    assert_eq!(
        fluids,
        vec![
            ("Empty", 0),
            ("Unknown", 1),
            ("Lava_Source", 2),
            ("Fire", 3),
            ("Lava", 4),
        ]
    );
    assert_eq!(
        registry
            .fluid("lava")
            .expect("case-insensitive fluid lookup")
            .max_fluid_level,
        4
    );
    assert_eq!(
        registry
            .fluid("Fire")
            .expect("Fire is loaded")
            .max_fluid_level,
        3
    );

    let environments: Vec<(&str, i32)> = registry
        .environments
        .iter()
        .map(|asset| (asset.key.as_str(), asset.id.expect("environment id")))
        .collect();
    assert_eq!(environments, vec![("Unknown", 0), ("Default", 1)]);
    assert_eq!(
        registry
            .environments
            .get("Default")
            .expect("Default environment")
            .value["SpawnDensity"],
        serde_json::json!(0.5)
    );

    assert_eq!(registry.physical_materials.id_of("EMPTY"), Some(0));
    assert_eq!(registry.physical_materials.id_of("Stone"), Some(1));
    assert_eq!(registry.block_bounding_boxes.id_of("Full"), Some(0));
    assert_eq!(registry.block_bounding_boxes.id_of("Box"), Some(1));
}

#[test]
fn preloads_and_standalone_blocks_keep_the_java_id_prefix() {
    let registry = AssetRegistry::load_p0(&fixture_pack()).expect("load the fixture registry");

    let prefix: Vec<&str> = (0..=4)
        .map(|id| {
            registry
                .block_types
                .get_by_id(id)
                .unwrap_or_else(|| panic!("missing block id {id}"))
                .key
                .as_str()
        })
        .collect();
    assert_eq!(
        prefix,
        vec![
            "Empty",
            "Unknown",
            "Debug_Cube",
            "Debug_Model",
            "Debug_Test_Block"
        ]
    );
}

#[test]
fn item_block_types_and_nested_states_are_extracted() {
    let registry = AssetRegistry::load_p0(&fixture_pack()).expect("load the fixture registry");

    for key in [
        "Rock_Stone",
        "Plant",
        "Mug",
        "*Mug_State_Filled",
        "Bench",
        "*Bench_State_Definitions_Open",
    ] {
        assert!(
            registry.block_type(key).is_some(),
            "missing extracted block type `{key}`"
        );
    }
    // A string definition is a reference, not a new contained asset.
    assert!(registry
        .block_type("*Bench_State_Definitions_Alias")
        .is_none());

    let plant = registry.block_type("Plant").expect("Plant");
    let support = plant.base_support().expect("plants require support");
    assert!(support.down);
    assert!(!support.up);

    let mug = registry.block_type("Mug").expect("Mug");
    let mug_state = registry
        .block_type("*Mug_State_Filled")
        .expect("Mug state block");
    assert_eq!(mug_state.draw_type, mug.draw_type);
    let mug_support = mug_state.base_support().expect("inherited support");
    assert!(mug_support.down);
    assert!(!mug_support.up);

    let bench_open = registry
        .block_type("*Bench_State_Definitions_Open")
        .expect("Bench open state");
    let bench_support = bench_open.base_support().expect("explicit support");
    assert!(!bench_support.down);
    assert!(bench_support.up);
}

#[test]
fn ids_and_keys_match_the_java_p0_goldens() {
    let root = repository_root();
    let zip = root.join("Assets/Assets.zip");
    if !zip.is_file() {
        eprintln!("skipping registry golden test: {} is absent", zip.display());
        return;
    }

    let pack = AssetPack::open(&zip).expect("open Assets.zip");
    let registry = AssetRegistry::load_p0(&pack).expect("load the P0 registry from the pack");

    let index: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("testdata/golden/server/assets/index.json"))
            .expect("read the asset index golden"),
    )
    .expect("parse the asset index golden");

    for (store_name, actual) in [
        (
            "Fluid",
            registry
                .fluids
                .iter()
                .map(|asset| (asset.key.clone(), asset.id.expect("fluid id")))
                .collect::<HashMap<_, _>>(),
        ),
        (
            "Environment",
            registry
                .environments
                .iter()
                .map(|asset| (asset.key.clone(), asset.id.expect("environment id")))
                .collect::<HashMap<_, _>>(),
        ),
        (
            "PhysicalMaterial",
            registry
                .physical_materials
                .iter()
                .map(|asset| (asset.key.clone(), asset.id.expect("material id")))
                .collect::<HashMap<_, _>>(),
        ),
        (
            "BlockBoundingBoxes",
            registry
                .block_bounding_boxes
                .iter()
                .map(|asset| (asset.key.clone(), asset.id.expect("hitbox id")))
                .collect::<HashMap<_, _>>(),
        ),
    ] {
        let expected: HashMap<String, i32> = index["stores"]
            .as_array()
            .expect("index stores")
            .iter()
            .find(|store| store["store"] == store_name)
            .unwrap_or_else(|| panic!("missing {store_name} in the index"))["ids"]
            .as_object()
            .expect("ids object")
            .iter()
            .map(|(key, id)| (key.clone(), id.as_i64().expect("numeric id") as i32))
            .collect();
        assert_eq!(actual, expected, "{store_name} ids differ from Java");
    }

    let prefix: Vec<&str> = (0..=4)
        .map(|id| {
            registry
                .block_types
                .get_by_id(id)
                .unwrap_or_else(|| panic!("missing block id {id}"))
                .key
                .as_str()
        })
        .collect();
    assert_eq!(
        prefix,
        vec![
            "Empty",
            "Unknown",
            "Debug_Cube",
            "Debug_Model",
            "Debug_Test_Block"
        ]
    );
}

#[test]
fn block_type_semantics_match_the_java_registry_dump() {
    let root = repository_root();
    let zip = root.join("Assets/Assets.zip");
    if !zip.is_file() {
        eprintln!(
            "skipping registry semantic test: {} is absent",
            zip.display()
        );
        return;
    }

    let pack = AssetPack::open(&zip).expect("open Assets.zip");
    let registry = AssetRegistry::load_p0(&pack).expect("load the P0 registry from the pack");
    let golden: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("testdata/golden/registries.json"))
            .expect("read the registry golden"),
    )
    .expect("parse the registry golden");

    let mut golden_keys = HashSet::new();
    let mut checked = 0usize;
    for asset in golden["blocks"]["assets"]
        .as_array()
        .expect("golden block assets")
    {
        let name = asset["name"].as_str().expect("block name");
        golden_keys.insert(name.to_string());
        let our = registry
            .block_type(name)
            .unwrap_or_else(|| panic!("missing block type `{name}`"));

        assert_eq!(
            our.draw_type.value(),
            asset["drawTypeId"].as_i64().expect("draw type id"),
            "{name}: draw type"
        );
        assert_eq!(
            opacity_name(our.opacity),
            asset["opacity"].as_str().expect("opacity"),
            "{name}: opacity"
        );

        match asset.get("support").and_then(|support| support.as_array()) {
            Some(support) => {
                for (rotation, expected) in support.iter().enumerate() {
                    let actual = our
                        .support_at(rotation as u8)
                        .unwrap_or_else(|| panic!("{name}: missing support at {rotation}"));
                    assert_eq!(
                        actual.down,
                        expected["down"].as_bool().expect("support.down"),
                        "{name}: support down at rotation {rotation}"
                    );
                    assert_eq!(
                        actual.up,
                        expected["up"].as_bool().expect("support.up"),
                        "{name}: support up at rotation {rotation}"
                    );
                }
                assert_eq!(
                    our.base_support(),
                    our.support_at(0),
                    "{name}: base support is rotation 0"
                );
            }
            None => assert!(
                our.support.is_none(),
                "{name}: Java has no support map but Rust does"
            ),
        }
        checked += 1;
    }

    assert_eq!(checked, golden_keys.len());
    let our_keys: HashSet<String> = registry
        .block_types
        .iter()
        .map(|asset| asset.key.clone())
        .collect();
    let missing: Vec<&String> = golden_keys.difference(&our_keys).collect();
    assert!(missing.is_empty(), "missing block keys: {missing:?}");

    // The only extras are assets Java's canonical dump failed to decode or
    // generates through the item loader in a different order; no golden key is
    // missing.
    let extra: HashSet<&String> = our_keys.difference(&golden_keys).collect();
    let expected_extra: HashSet<&str> = [
        "ExamplePrefabPlacementBlock_Tree",
        "PortalKey_Goblin_Breach",
        "PortalKey_The_Deepworks",
    ]
    .into_iter()
    .collect();
    let extra: HashSet<&str> = extra.into_iter().map(String::as_str).collect();
    assert_eq!(extra, expected_extra);
}
