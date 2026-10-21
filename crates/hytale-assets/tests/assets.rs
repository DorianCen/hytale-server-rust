//! Pack VFS, `Parent` resolution, and store ordering tests.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hytale_assets::order::{order_stores, StoreNode};
use hytale_assets::pack::{AssetError, AssetKey, AssetPack};
use hytale_assets::parent::{MergeHook, ParentResolver};
use serde_json::{json, Value};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn provider(assets: &[(&str, Value)]) -> BTreeMap<String, Value> {
    assets
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

#[test]
fn directory_pack_lists_and_reads_json() {
    let pack = AssetPack::open(fixture("pack")).expect("open pack");
    assert!(!pack.is_zip());
    let keys = pack.list_keys().expect("keys");
    let names: Vec<&str> = keys.iter().map(|key| key.source.as_str()).collect();
    assert!(names.contains(&"Base/Stone.json"));
    assert!(names.contains(&"Child.json"));

    let child = AssetKey::from_source("Base/Stone.json");
    assert_eq!(child.key, "Stone");
    assert_eq!(child.effective_key(), "Stone");
    assert_eq!(child.clone().in_container("Rock").effective_key(), "Rock");

    assert!(pack.read("Child.json").is_ok());
    assert!(pack.read("../Child.json").is_err());
    assert!(pack.read("..\\Child.json").is_err());
    assert!(pack.read("/etc/passwd").is_err());
    assert_eq!(
        pack.find_case_insensitive("base/stone.json")
            .unwrap()
            .as_deref(),
        Some("Base/Stone.json")
    );
    assert!(pack.read_case_insensitive("CHILD.json").unwrap().is_some());
}

#[test]
fn zip_and_directory_packs_agree() {
    let directory = fixture("pack");
    let zip_path = temp_path("pack.zip");
    write_zip(&directory, &zip_path);

    let dir_pack = AssetPack::open(&directory).unwrap();
    let zip_pack = AssetPack::open(&zip_path).unwrap();
    assert!(zip_pack.is_zip());
    assert_eq!(dir_pack.list().unwrap(), zip_pack.list().unwrap());
    for path in dir_pack.list().unwrap() {
        assert_eq!(
            dir_pack.read(&path).unwrap(),
            zip_pack.read(&path).unwrap(),
            "{path}"
        );
    }
    std::fs::remove_file(zip_path).ok();
}

#[test]
fn parent_merges_scalars_objects_and_replaces_arrays() {
    let assets = provider(&[
        (
            "Stone",
            json!({
                "$Title": "Stone",
                "$Comment": "metadata",
                "Value": 1,
                "Nested": {"a": 1, "b": 2},
                "List": [1, 2],
                "Keep": "parent"
            }),
        ),
        (
            "MossyStone",
            json!({
                "Parent": "Stone",
                "$Author": "test",
                "Value": 2,
                "Nested": {"b": 3},
                "List": [9]
            }),
        ),
    ]);
    let resolver = ParentResolver::new(&assets);
    let resolved = resolver.resolve("MossyStone").expect("resolves");
    assert_eq!(resolved["Value"], 2);
    assert_eq!(resolved["Nested"], json!({"a": 1, "b": 3}));
    assert_eq!(resolved["List"], json!([9]));
    assert_eq!(resolved["Keep"], "parent");
    assert!(resolved.get("Parent").is_none());
    assert!(resolved.get("$Title").is_none());
    assert!(resolved.get("$Author").is_none());
    assert!(resolved.get("$Comment").is_none());
}

#[test]
fn store_hooks_can_append_arrays_or_merge_custom() {
    let assets = provider(&[
        ("Base", json!({"List": [1, 2]})),
        ("Child", json!({"Parent": "Base", "List": [3]})),
    ]);
    let appended = ParentResolver::with_hook(&assets, MergeHook::AppendArrays)
        .resolve("Child")
        .unwrap();
    assert_eq!(appended["List"], json!([1, 2, 3]));

    fn custom(parent: &mut Value, child: Value) {
        if let (Some(parent), Some(child)) = (parent.as_object_mut(), child.as_object()) {
            for (key, value) in child {
                if key == "List" {
                    let mut combined = parent
                        .get("List")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    combined.extend(value.as_array().cloned().unwrap_or_default());
                    parent.insert(key.clone(), Value::Array(combined));
                } else {
                    parent.insert(key.clone(), value.clone());
                }
            }
        }
    }
    let custom = ParentResolver::with_hook(&assets, MergeHook::Custom(custom))
        .resolve("Child")
        .unwrap();
    assert_eq!(custom["List"], json!([1, 2, 3]));
}

#[test]
fn missing_parent_and_cycle_report_the_chain() {
    let missing = provider(&[("Orphan", json!({"Parent": "Missing"}))]);
    let error = ParentResolver::new(&missing).resolve("Orphan").unwrap_err();
    assert!(matches!(error, AssetError::MissingParent { .. }));
    assert!(error.to_string().contains("Orphan"));
    assert!(error.to_string().contains("Missing"));

    let cycle = provider(&[("A", json!({"Parent": "B"})), ("B", json!({"Parent": "A"}))]);
    let error = ParentResolver::new(&cycle).resolve("A").unwrap_err();
    assert!(matches!(error, AssetError::ParentCycle { .. }));
    let message = error.to_string();
    assert!(message.contains("A -> B -> A"), "{message}");
}

#[test]
fn store_order_respects_edges_and_is_stable() {
    let stores = vec![
        StoreNode::new("Item", vec!["BlockGroup"], vec!["BlockType"]),
        StoreNode::new("BlockType", Vec::<&str>::new(), Vec::<&str>::new()),
        StoreNode::new("BlockGroup", Vec::<&str>::new(), vec!["Item"]),
        StoreNode::new("Unrelated", Vec::<&str>::new(), Vec::<&str>::new()),
        StoreNode::new("External", vec!["NotAStore"], Vec::<&str>::new()),
    ];
    let order = order_stores(&stores).expect("orders");
    assert_eq!(
        order,
        vec!["BlockType", "Item", "BlockGroup", "Unrelated", "External"]
    );
    assert_eq!(order, order_stores(&stores).unwrap(), "stable across runs");
}

#[test]
fn store_order_cycle_is_an_error() {
    let stores = vec![
        StoreNode::new("A", vec!["B"], Vec::<&str>::new()),
        StoreNode::new("B", vec!["A"], Vec::<&str>::new()),
    ];
    let error = order_stores(&stores).unwrap_err();
    assert!(matches!(error, AssetError::StoreCycle { .. }));
}

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-assets-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn write_zip(directory: &Path, destination: &Path) {
    let file = std::fs::File::create(destination).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    fn walk(
        root: &Path,
        directory: &Path,
        writer: &mut zip::ZipWriter<std::fs::File>,
        options: zip::write::SimpleFileOptions,
    ) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, writer, options);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                writer.start_file(name, options).unwrap();
                writer.write_all(&std::fs::read(path).unwrap()).unwrap();
            }
        }
    }
    walk(directory, directory, &mut writer, options);
    writer.finish().unwrap();
}
