//! Golden parity for the config asset packets whose ids match Java.
//!
//! `testdata/golden/server/assets/*.bin` are real Java `generateInitPacket`
//! payloads (unframed, uncompressed). `PhysicalMaterial` and
//! `BlockBoundingBoxes` have deterministic Java ids, so the Rust encoders must
//! match byte-for-byte. The load also carries the child stores so `Fluid` and
//! `Environment` can resolve their indexes.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_protocol::generated::assets::{
    UpdateBlockHitboxes, UpdateEnvironments, UpdateFluids, UpdatePhysicalMaterials,
};
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
    AssetRegistry::load_phase2(&pack).expect("load the phase-2 registry")
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

#[test]
fn environment_and_fluid_packets_decode_and_agree_with_server_tags() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = load_registry();
    let config = registry.config_assets().expect("encode config assets");
    let environment = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateEnvironments::ID)
        .expect("environment packet");
    let fluid = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateFluids::ID)
        .expect("fluid packet");

    let environments =
        UpdateEnvironments::decode(&mut Reader::new(&environment.payload)).expect("decode env");
    let environments = environments.environments.expect("environment map");
    assert_eq!(environments.len(), registry.environments.len());
    assert_eq!(environments.len(), 135);

    let golden_environments = UpdateEnvironments::decode(&mut Reader::new(&golden("Environment")))
        .expect("decode golden env")
        .environments
        .expect("golden env map");
    for (id, expected) in golden_environments {
        let actual = environments.get(&id).expect("matching environment id");
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.water_tint, expected.water_tint);
        assert_eq!(actual.fluid_particles, expected.fluid_particles);
        for index in actual.tag_indexes.as_deref().unwrap_or_default() {
            assert!(
                config.tags.values().any(|value| value == index),
                "environment tag index {index} is missing from ServerTags"
            );
        }
    }

    let fluids = UpdateFluids::decode(&mut Reader::new(&fluid.payload))
        .expect("decode fluids")
        .fluids
        .expect("fluid map");
    assert_eq!(fluids.len(), registry.fluids.len());
    assert_eq!(fluids.len(), 19);

    let golden_fluids = UpdateFluids::decode(&mut Reader::new(&golden("Fluid")))
        .expect("decode golden fluids")
        .fluids
        .expect("golden fluid map");
    for (id, expected) in golden_fluids {
        let actual = fluids.get(&id).expect("matching fluid id");
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.max_fluid_level, expected.max_fluid_level);
        assert_eq!(actual.cube_textures, expected.cube_textures);
        assert_eq!(
            actual.requires_alpha_blending,
            expected.requires_alpha_blending
        );
        assert_eq!(actual.opacity, expected.opacity);
        assert_eq!(actual.shader_effect, expected.shader_effect);
        assert_eq!(actual.light, expected.light);
        assert_eq!(actual.particles, expected.particles);
        assert_eq!(actual.draw_type, expected.draw_type);
        assert_eq!(actual.block_particle_set_id, expected.block_particle_set_id);
        assert_eq!(actual.fluid_fx_index, expected.fluid_fx_index);
        assert_eq!(actual.block_sound_set_index, expected.block_sound_set_index);
        assert_eq!(actual.particle_color, expected.particle_color);
        // Tag indexes are Rust-owned.
        for index in actual.tag_indexes.as_deref().unwrap_or_default() {
            assert!(
                config.tags.values().any(|value| value == index),
                "fluid tag index {index} is missing from ServerTags"
            );
        }
    }
}
