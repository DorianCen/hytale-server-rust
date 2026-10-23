//! Golden parity for the P0 asset packets whose ids match Java.
//!
//! `testdata/golden/server/assets/*.bin` are real Java `generateInitPacket`
//! payloads (unframed, uncompressed). `PhysicalMaterial` and
//! `BlockBoundingBoxes` have deterministic Java ids, so the Rust encoders must
//! match byte-for-byte.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_protocol::generated::assets::{UpdateBlockHitboxes, UpdatePhysicalMaterials};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn load_registry() -> AssetRegistry {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    AssetRegistry::load_p0(&pack).expect("load P0")
}

fn golden(store: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
        .expect("read the golden payload")
}

fn compare(store: &str, expected: &[u8], actual: &[u8]) {
    if expected == actual {
        return;
    }
    let first = expected.iter().zip(actual).position(|(a, b)| a != b);
    panic!(
        "{store} differs: golden {} bytes, rust {} bytes, first difference {first:?}",
        expected.len(),
        actual.len()
    );
}

#[test]
fn physical_material_packet_matches_java() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_registry();
    let packet = registry
        .physical_material_packet()
        .expect("encode PhysicalMaterial");
    assert_eq!(packet.id, UpdatePhysicalMaterials::ID);
    assert_eq!(packet.store, "PhysicalMaterial");
    compare(
        "PhysicalMaterial",
        &golden("PhysicalMaterial"),
        &packet.payload,
    );

    let decoded = UpdatePhysicalMaterials::decode(&mut Reader::new(&packet.payload))
        .expect("decode the packet");
    let materials = decoded.physical_materials.expect("material map");
    assert_eq!(materials.len(), registry.physical_materials.len());
    assert_eq!(decoded.max_id, 12);
    assert_eq!(
        materials
            .get(&0)
            .and_then(|material| material.id.as_deref()),
        Some("EMPTY")
    );
}

#[test]
fn block_hitbox_packet_matches_java() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_registry();
    let packet = registry
        .block_hitbox_packet()
        .expect("encode BlockBoundingBoxes");
    assert_eq!(packet.id, UpdateBlockHitboxes::ID);
    assert_eq!(packet.store, "BlockBoundingBoxes");
    compare(
        "BlockBoundingBoxes",
        &golden("BlockBoundingBoxes"),
        &packet.payload,
    );

    let decoded =
        UpdateBlockHitboxes::decode(&mut Reader::new(&packet.payload)).expect("decode the packet");
    let hitboxes = decoded.block_base_hitboxes.expect("hitbox map");
    assert_eq!(hitboxes.len(), registry.block_bounding_boxes.len());
    assert_eq!(decoded.max_id, 259);
    // The Java unit box for `Arcade_Machine` is [0,0,0]..[1,1,1].
    let machine = hitboxes
        .values()
        .find(|boxes| boxes.len() == 1)
        .expect("at least one single-box asset");
    assert_eq!(machine[0].min_x, 0.0);
    assert_eq!(machine[0].max_x, 1.0);
}
