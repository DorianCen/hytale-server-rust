//! Golden parity for the `TagPattern` store and `UpdateTagPatterns` (84).
//!
//! Java's global tag indexes are owned by the Rust [`TagRegistry`], so the
//! payload is not byte-compared: the same op tree is emitted, but `Equals`
//! indexes follow the deterministic Rust tag order. The test instead compares
//! the full id -> op-tree structure and proves every index resolves in
//! `ServerTags`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry, TagPatternNode, TagRegistry};
use hytale_protocol::generated::assets::{
    UpdateBlockTypes, UpdateEnvironments, UpdateFluids, UpdateItems, UpdateTagPatterns,
};
use hytale_protocol::generated::common::{TagPattern, TagPatternType};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn load_phase2() -> AssetRegistry {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    AssetRegistry::load_phase2(&pack).expect("load phase 2")
}

fn golden_payload(store: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
        .expect("read the golden payload")
}

fn golden_index() -> Value {
    serde_json::from_slice(
        &std::fs::read(repo_root().join("testdata/golden/server/assets/index.json"))
            .expect("read the asset index golden"),
    )
    .expect("parse the asset index golden")
}

fn packet(packets: &[hytale_assets::AssetPacket], id: u32) -> &hytale_assets::AssetPacket {
    packets
        .iter()
        .find(|packet| packet.id == id)
        .unwrap_or_else(|| panic!("missing config packet {id}"))
}

/// Structural view of a pattern, ignoring the Rust-owned `tagIndex`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Equals,
    And(Vec<Shape>),
    Or(Vec<Shape>),
    Not(Box<Shape>),
}

fn shape(pattern: &TagPattern) -> Shape {
    match pattern.r#type {
        TagPatternType::Equals => Shape::Equals,
        TagPatternType::And => Shape::And(
            pattern
                .operands
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(shape)
                .collect(),
        ),
        TagPatternType::Or => Shape::Or(
            pattern
                .operands
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(shape)
                .collect(),
        ),
        TagPatternType::Not => Shape::Not(Box::new(shape(
            pattern.not.as_deref().expect("Not pattern payload"),
        ))),
    }
}

fn collect_pattern_indexes(pattern: &TagPattern, out: &mut BTreeSet<i32>) {
    if pattern.r#type == TagPatternType::Equals {
        out.insert(pattern.tag_index);
    }
    for operand in pattern.operands.as_deref().unwrap_or_default() {
        collect_pattern_indexes(operand, out);
    }
    if let Some(nested) = pattern.not.as_deref() {
        collect_pattern_indexes(nested, out);
    }
}

fn collect_equals_indexes(pattern: &TagPattern, out: &mut Vec<i32>) {
    if pattern.r#type == TagPatternType::Equals {
        out.push(pattern.tag_index);
    }
    for operand in pattern.operands.as_deref().unwrap_or_default() {
        collect_equals_indexes(operand, out);
    }
    if let Some(nested) = pattern.not.as_deref() {
        collect_equals_indexes(nested, out);
    }
}

fn collect_equals_tags(pattern: &TagPatternNode, out: &mut Vec<String>) {
    match pattern {
        TagPatternNode::Equals { tag } => out.push(tag.clone()),
        TagPatternNode::And { operands } | TagPatternNode::Or { operands } => {
            for operand in operands {
                collect_equals_tags(operand, out);
            }
        }
        TagPatternNode::Not { pattern } => collect_equals_tags(pattern, out),
    }
}

#[test]
fn tag_pattern_store_matches_the_java_index() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_phase2();
    let index = golden_index();
    let store = index["stores"]
        .as_array()
        .expect("stores array")
        .iter()
        .find(|store| store["store"] == "TagPattern")
        .expect("TagPattern store entry");

    let expected: BTreeMap<String, i32> = store["ids"]
        .as_object()
        .expect("TagPattern ids")
        .iter()
        .map(|(key, id)| (key.clone(), id.as_i64().expect("numeric id") as i32))
        .collect();
    let actual: BTreeMap<String, i32> = registry
        .tag_patterns
        .iter()
        .map(|record| (record.key.clone(), record.id.expect("TagPattern id")))
        .collect();

    assert_eq!(registry.tag_patterns.len(), 196);
    assert_eq!(
        actual, expected,
        "TagPattern key/id mapping differs from Java"
    );

    let file_assets = expected.keys().filter(|key| !key.starts_with('*')).count();
    let contained = expected.len() - file_assets;
    assert_eq!(file_assets, 11, "file-backed TagPattern count");
    assert_eq!(contained, 185, "contained TagPattern count");

    let config = registry.config_assets().expect("encode config assets");
    let decoded = UpdateTagPatterns::decode(&mut Reader::new(
        &packet(&config.packets, UpdateTagPatterns::ID).payload,
    ))
    .expect("decode UpdateTagPatterns");
    assert_eq!(decoded.max_id, 196, "TagPattern maxId");
    assert_eq!(
        decoded.patterns.expect("patterns map").len(),
        expected.len(),
        "TagPattern packet count"
    );
}

#[test]
fn update_tag_patterns_matches_the_java_op_trees() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_phase2();
    let config = registry.config_assets().expect("encode config assets");
    let rust_packet = packet(&config.packets, UpdateTagPatterns::ID);
    let golden_bytes = golden_payload("TagPattern");
    let golden = UpdateTagPatterns::decode(&mut Reader::new(&golden_bytes))
        .expect("decode the golden TagPattern payload");
    let rust = UpdateTagPatterns::decode(&mut Reader::new(&rust_packet.payload))
        .expect("decode the Rust TagPattern payload");

    assert_eq!(rust.r#type, golden.r#type);
    assert_eq!(rust.max_id, golden.max_id);
    // The payloads have the same op trees and i32-width indexes; only the
    // Rust-owned tag index values differ, so the sizes must agree.
    assert_eq!(rust_packet.payload.len(), golden_bytes.len());

    let golden_patterns = golden.patterns.expect("golden patterns");
    let rust_patterns = rust.patterns.expect("Rust patterns");
    assert_eq!(
        rust_patterns.keys().collect::<Vec<_>>(),
        golden_patterns.keys().collect::<Vec<_>>(),
        "pattern ids differ"
    );

    let known_indexes: BTreeSet<i32> = config.tags.values().copied().collect();
    for (id, golden_pattern) in &golden_patterns {
        let rust_pattern = rust_patterns.get(id).expect("matching Rust pattern id");
        assert_eq!(
            shape(rust_pattern),
            shape(golden_pattern),
            "pattern {id} op tree differs"
        );
        let mut indexes = BTreeSet::new();
        collect_pattern_indexes(rust_pattern, &mut indexes);
        for index in indexes {
            assert!(
                known_indexes.contains(&index),
                "pattern {id}: tag index {index} is missing from ServerTags"
            );
        }
    }

    // Each `Equals` index must resolve back to the exact tag name in the source
    // document, not merely to some registered tag.
    let tags_by_index: BTreeMap<i32, &str> = config
        .tags
        .iter()
        .map(|(name, index)| (*index, name.as_str()))
        .collect();
    for record in registry.tag_patterns.iter() {
        let id = record.id.expect("TagPattern id");
        let rust_pattern = rust_patterns.get(&id).expect("Rust pattern");
        let mut indexes = Vec::new();
        collect_equals_indexes(rust_pattern, &mut indexes);
        let mut expected = Vec::new();
        collect_equals_tags(&record.value.pattern, &mut expected);
        let resolved: Vec<&str> = indexes
            .iter()
            .map(|index| {
                *tags_by_index
                    .get(index)
                    .unwrap_or_else(|| panic!("pattern {id}: unresolved tag index {index}"))
            })
            .collect();
        assert_eq!(
            resolved, expected,
            "pattern {id}: Equals tag mapping differs"
        );
    }
}

#[test]
fn every_config_packet_tag_index_resolves_in_server_tags() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_phase2();
    let config = registry.config_assets().expect("encode config assets");
    let known: BTreeSet<i32> = config.tags.values().copied().collect();

    let mut emitted = BTreeSet::new();
    let mut note = |index: i32| {
        if index >= 0 {
            emitted.insert(index);
        }
    };

    let environments = UpdateEnvironments::decode(&mut Reader::new(
        &packet(&config.packets, UpdateEnvironments::ID).payload,
    ))
    .expect("decode environments")
    .environments
    .expect("environment map");
    for environment in environments.values() {
        for index in environment.tag_indexes.as_deref().unwrap_or_default() {
            note(*index);
        }
    }

    let fluids = UpdateFluids::decode(&mut Reader::new(
        &packet(&config.packets, UpdateFluids::ID).payload,
    ))
    .expect("decode fluids")
    .fluids
    .expect("fluid map");
    for fluid in fluids.values() {
        for index in fluid.tag_indexes.as_deref().unwrap_or_default() {
            note(*index);
        }
    }

    let patterns = UpdateTagPatterns::decode(&mut Reader::new(
        &packet(&config.packets, UpdateTagPatterns::ID).payload,
    ))
    .expect("decode patterns")
    .patterns
    .expect("pattern map");
    for pattern in patterns.values() {
        let mut indexes = BTreeSet::new();
        collect_pattern_indexes(pattern, &mut indexes);
        for index in indexes {
            note(index);
        }
    }

    let blocks = UpdateBlockTypes::decode(&mut Reader::new(
        &packet(&config.packets, UpdateBlockTypes::ID).payload,
    ))
    .expect("decode blocks")
    .block_types
    .expect("block map");
    for block in blocks.values() {
        for index in block.tag_indexes.as_deref().unwrap_or_default() {
            note(*index);
        }
        note(block.transition_to_tag);
        for entries in block
            .support
            .as_ref()
            .into_iter()
            .flat_map(|map| map.values())
        {
            for entry in entries {
                note(entry.tag_index);
            }
        }
    }

    let items = UpdateItems::decode(&mut Reader::new(
        &packet(&config.packets, UpdateItems::ID).payload,
    ))
    .expect("decode items")
    .items
    .expect("item map");
    for item in items.values() {
        for index in item.tag_indexes.as_deref().unwrap_or_default() {
            note(*index);
        }
    }

    let missing: Vec<i32> = emitted.difference(&known).copied().collect();
    assert!(
        missing.is_empty(),
        "tag indexes emitted but missing from ServerTags: {missing:?}"
    );
}

#[test]
fn repeated_config_assets_are_stable() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_phase2();
    let first = registry.config_assets().expect("first config assets");
    let second = registry.config_assets().expect("second config assets");

    assert_eq!(first.tags, second.tags, "ServerTags drifted");
    assert_eq!(
        first
            .packets
            .iter()
            .map(|packet| (packet.store, packet.id, packet.payload.clone()))
            .collect::<Vec<_>>(),
        second
            .packets
            .iter()
            .map(|packet| (packet.store, packet.id, packet.payload.clone()))
            .collect::<Vec<_>>(),
        "config packet payloads drifted"
    );

    let tags_first = TagRegistry::from_assets(&registry).map();
    let tags_second = TagRegistry::from_assets(&registry).map();
    assert_eq!(tags_first, tags_second, "TagRegistry drifted");
}

#[test]
fn load_p0_does_not_load_tag_patterns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    assert!(registry.tag_patterns.is_empty());
}
