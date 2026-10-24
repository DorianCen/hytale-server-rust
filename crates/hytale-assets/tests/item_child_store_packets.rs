//! T37 golden parity for the item-side child stores.
//!
//! `load_phase2` adds `ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`,
//! `ItemCategory` and `ItemPlayerAnimations` to P0. Their ids/order match the
//! Java asset maps, so every payload is byte-compared with the committed
//! `AssetDumper` output. The item packet is compared by key for the fields that
//! used to fall back to protocol zero values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetPacket, AssetRegistry, TagRegistry};
use hytale_protocol::generated::assets::{
    UpdateItemCategories, UpdateItemPlayerAnimations, UpdateItemQualities, UpdateItemReticles,
    UpdateItemSoundSets, UpdateItems,
};
use hytale_protocol::generated::wire::Wire;
use hytale_protocol::io::Reader;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn load() -> (AssetRegistry, Vec<AssetPacket>) {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load P0 + item child stores");
    let tags = TagRegistry::from_assets(&registry);
    let packets = vec![
        registry.item_sound_set_packet().expect("ItemSoundSet"),
        registry.item_quality_packet().expect("ItemQuality"),
        registry.item_reticle_packet().expect("ItemReticleConfig"),
        registry.item_category_packet().expect("ItemCategory"),
        registry
            .item_player_animations_packet()
            .expect("ItemPlayerAnimations"),
        registry.item_packet(&tags).expect("Item"),
    ];
    (registry, packets)
}

fn golden_bytes(store: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
        .unwrap_or_else(|error| panic!("read {store}.bin: {error}"))
}

fn golden<T: Wire>(store: &str) -> T {
    let bytes = golden_bytes(store);
    T::decode(&mut Reader::new(&bytes)).expect("decode golden")
}

fn index_store<'a>(index: &'a Value, name: &str) -> &'a Value {
    index["stores"]
        .as_array()
        .expect("stores array")
        .iter()
        .find(|store| store["store"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("missing index entry for {name}"))
}

fn registry_ids<T>(collection: &hytale_assets::AssetCollection<T>) -> BTreeMap<String, i32> {
    collection
        .iter()
        .filter_map(|record| Some((record.key.clone(), record.id?)))
        .collect()
}

#[test]
fn item_child_store_counts_ids_and_keys_match_the_java_index() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available - the test is skipped");
        return;
    }
    let (registry, _) = load();
    let index: Value = serde_json::from_slice(
        &std::fs::read(repo_root().join("testdata/golden/server/assets/index.json"))
            .expect("read index.json"),
    )
    .expect("parse index.json");

    for (name, actual_ids, actual_count, actual_max) in [
        (
            "ItemSoundSet",
            registry_ids(&registry.item_sound_sets),
            registry.item_sound_sets.len(),
            registry
                .item_sound_sets
                .iter()
                .filter_map(|record| record.id)
                .max()
                .map_or(0, |id| id + 1),
        ),
        (
            "ItemQuality",
            registry_ids(&registry.item_qualities),
            registry.item_qualities.len(),
            registry
                .item_qualities
                .iter()
                .filter_map(|record| record.id)
                .max()
                .map_or(0, |id| id + 1),
        ),
        (
            "ItemReticleConfig",
            registry_ids(&registry.item_reticles),
            registry.item_reticles.len(),
            registry
                .item_reticles
                .iter()
                .filter_map(|record| record.id)
                .max()
                .map_or(0, |id| id + 1),
        ),
    ] {
        let entry = index_store(&index, name);
        assert_eq!(
            actual_count,
            entry["count"].as_u64().unwrap() as usize,
            "{name}: count"
        );
        assert_eq!(
            actual_max,
            entry["maxId"].as_i64().unwrap() as i32,
            "{name}: maxId"
        );
        let expected_ids: BTreeMap<String, i32> = entry["ids"]
            .as_object()
            .expect("ids object")
            .iter()
            .map(|(key, value)| (key.clone(), value.as_i64().unwrap() as i32))
            .collect();
        assert_eq!(actual_ids, expected_ids, "{name}: key -> id");
    }

    for (name, keys) in [
        (
            "ItemCategory",
            registry
                .item_categories
                .iter()
                .map(|record| record.key.clone())
                .collect::<BTreeSet<_>>(),
        ),
        (
            "ItemPlayerAnimations",
            registry
                .item_player_animations
                .iter()
                .map(|record| record.key.clone())
                .collect::<BTreeSet<_>>(),
        ),
    ] {
        let entry = index_store(&index, name);
        assert_eq!(
            keys.len(),
            entry["count"].as_u64().unwrap() as usize,
            "{name}: count"
        );
        let expected: BTreeSet<String> = entry["keys"]
            .as_array()
            .expect("keys array")
            .iter()
            .map(|key| key.as_str().unwrap().to_string())
            .collect();
        assert_eq!(keys, expected, "{name}: keys");
    }
}

#[test]
fn item_child_packets_match_the_java_golden_bytes() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available - the test is skipped");
        return;
    }
    let (_, packets) = load();
    for (store, packet) in [
        ("ItemSoundSet", &packets[0]),
        ("ItemQuality", &packets[1]),
        ("ItemReticleConfig", &packets[2]),
        ("ItemCategory", &packets[3]),
        ("ItemPlayerAnimations", &packets[4]),
    ] {
        assert_eq!(
            packet.payload,
            golden_bytes(store),
            "{store}: payload differs from Java"
        );
        assert_eq!(packet.store, store, "{store}: packet store");
    }
}

#[test]
fn item_child_packets_round_trip_through_the_generated_codecs() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available - the test is skipped");
        return;
    }
    let (_, packets) = load();
    let sound: UpdateItemSoundSets =
        UpdateItemSoundSets::decode(&mut Reader::new(&packets[0].payload)).expect("ItemSoundSet");
    assert_eq!(sound.item_sound_sets.as_ref().unwrap().len(), 36);
    assert_eq!(sound.max_id, 36);

    let quality: UpdateItemQualities =
        UpdateItemQualities::decode(&mut Reader::new(&packets[1].payload)).expect("ItemQuality");
    assert_eq!(quality.item_qualities.as_ref().unwrap().len(), 12);
    assert_eq!(quality.max_id, 12);

    let reticles: UpdateItemReticles =
        UpdateItemReticles::decode(&mut Reader::new(&packets[2].payload))
            .expect("ItemReticleConfig");
    assert_eq!(reticles.item_reticle_configs.as_ref().unwrap().len(), 3);
    assert_eq!(reticles.max_id, 3);

    let categories: UpdateItemCategories =
        UpdateItemCategories::decode(&mut Reader::new(&packets[3].payload)).expect("ItemCategory");
    assert_eq!(categories.item_categories.as_ref().unwrap().len(), 4);

    let animations: UpdateItemPlayerAnimations =
        UpdateItemPlayerAnimations::decode(&mut Reader::new(&packets[4].payload))
            .expect("ItemPlayerAnimations");
    assert_eq!(
        animations.item_player_animations.as_ref().unwrap().len(),
        181
    );
}

#[test]
fn item_packet_closes_quality_sound_reticle_animation_and_entity_fields() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available - the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let encoded = packets
        .iter()
        .find(|packet| packet.store == "Item")
        .expect("item packet");
    let decoded = UpdateItems::decode(&mut Reader::new(&encoded.payload)).expect("decode items");
    let actual = decoded.items.expect("item map");
    let golden_items = golden::<UpdateItems>("Item")
        .items
        .expect("golden item map");

    assert_eq!(actual.len(), registry.items.len());
    assert_eq!(golden_items.len(), 3971);

    // Residual differences are collected instead of skipped: the assertion
    // below requires this list to be empty. Any future Java/port gap must be
    // documented by making the expected residual set explicit here.
    let mut residuals: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for (key, expected) in &golden_items {
        let item = actual
            .get(key)
            .unwrap_or_else(|| panic!("missing Java item `{key}`"));
        if item.quality_index != expected.quality_index {
            residuals
                .entry("qualityIndex")
                .or_default()
                .push(format!("{key}: {}", item.quality_index));
        }
        if item.item_sound_set_index != expected.item_sound_set_index {
            residuals
                .entry("itemSoundSetIndex")
                .or_default()
                .push(format!("{key}: {}", item.item_sound_set_index));
        }
        if item.reticle_index != expected.reticle_index {
            residuals
                .entry("reticleIndex")
                .or_default()
                .push(format!("{key}: {}", item.reticle_index));
        }
        if item.player_animations_id != expected.player_animations_id {
            residuals
                .entry("playerAnimationsId")
                .or_default()
                .push(format!("{key}: {:?}", item.player_animations_id));
        }
        if item.item_entity != expected.item_entity {
            residuals
                .entry("itemEntity")
                .or_default()
                .push(format!("{key}: {:?}", item.item_entity));
        }
    }
    assert!(
        residuals.is_empty(),
        "item fields still divergent from Java: {residuals:?}"
    );
}

#[test]
fn load_p0_keeps_the_extended_stores_empty() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available - the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    assert!(registry.item_sound_sets.is_empty());
    assert!(registry.item_qualities.is_empty());
    assert!(registry.item_reticles.is_empty());
    assert!(registry.item_categories.is_empty());
    assert!(registry.item_player_animations.is_empty());
    // The P0 item packet still encodes (child indexes fall back to 0).
    let tags = TagRegistry::from_assets(&registry);
    let packet = registry.item_packet(&tags).expect("P0 item packet");
    UpdateItems::decode(&mut Reader::new(&packet.payload)).expect("decode P0 items");
}
