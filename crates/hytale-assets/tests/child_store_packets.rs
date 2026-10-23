//! Decode/checksum gate for the T35 child-store payloads.
//!
//! The Java dumper owns the bytes; this test proves that every committed
//! payload decodes with the generated Rust codec and that `index.json` matches
//! the files on disk.

use std::path::PathBuf;

use hytale_protocol::generated::assets::{
    UpdateAudioCategories, UpdateAudioStates, UpdateBlockBreakingDecals, UpdateBlockParticleSets,
    UpdateBlockSoundSets, UpdateFluidFX, UpdateItemCategories, UpdateItemPlayerAnimations,
    UpdateItemQualities, UpdateItemReticles, UpdateItemSoundSets, UpdateParticleSpawners,
    UpdateParticleSystems, UpdateSoundEvents, UpdateTagPatterns,
};
use hytale_protocol::generated::wire::Wire;
use hytale_protocol::io::Reader;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_dir() -> PathBuf {
    root().join("testdata/golden/server/assets")
}

fn payload(store: &str) -> Vec<u8> {
    std::fs::read(assets_dir().join(format!("{store}.bin")))
        .unwrap_or_else(|error| panic!("read {store}.bin: {error}"))
}

fn decode<T: Wire>(store: &str) -> T {
    let bytes = payload(store);
    T::decode(&mut Reader::new(&bytes))
        .unwrap_or_else(|error| panic!("decode {store}.bin: {error}"))
}

#[test]
fn child_store_payloads_decode_with_generated_codecs() {
    let _: UpdateBlockBreakingDecals = decode("BlockBreakingDecal");
    let _: UpdateBlockParticleSets = decode("BlockParticleSet");
    let _: UpdateBlockSoundSets = decode("BlockSoundSet");
    let _: UpdateItemCategories = decode("ItemCategory");
    let _: UpdateItemQualities = decode("ItemQuality");
    let _: UpdateAudioCategories = decode("AudioCategory");
    let _: UpdateAudioStates = decode("AudioState");
    let _: UpdateFluidFX = decode("FluidFX");
    let _: UpdateItemSoundSets = decode("ItemSoundSet");
    let _: UpdateParticleSpawners = decode("ParticleSpawner");
    let _: UpdateParticleSystems = decode("ParticleSystem");
    let _: UpdateSoundEvents = decode("SoundEvent");
    let _: UpdateTagPatterns = decode("TagPattern");
    let _: UpdateItemPlayerAnimations = decode("ItemPlayerAnimations");
    let _: UpdateItemReticles = decode("ItemReticleConfig");
}

#[test]
fn index_metadata_matches_committed_payloads() {
    let index: Value = serde_json::from_slice(
        &std::fs::read(assets_dir().join("index.json")).expect("read asset index"),
    )
    .expect("parse asset index");

    let stores = index["stores"].as_array().expect("stores array");
    assert_eq!(stores.len(), 26, "phase-2 store count");

    for store in stores {
        let name = store["store"].as_str().expect("store name");
        let bin = store["bin"].as_str().expect("bin field");
        if bin.is_empty() {
            assert_eq!(store["packetSize"], -1, "{name}: no packet but a size");
            continue;
        }
        let bytes = std::fs::read(assets_dir().join(bin))
            .unwrap_or_else(|error| panic!("{name}: read {bin}: {error}"));
        assert_eq!(
            bytes.len(),
            store["packetSize"].as_u64().expect("packetSize") as usize,
            "{name}: payload size"
        );
        let digest = Sha256::digest(&bytes);
        let actual: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            actual,
            store["sha256"].as_str().expect("sha256"),
            "{name}: payload hash"
        );
    }
}

#[test]
fn p0_payloads_are_unchanged_in_size() {
    // The phase-2 re-baseline changed four P0 hashes but not their sizes.
    for (store, size) in [
        ("BlockBoundingBoxes", 12_079usize),
        ("BlockType", 5_872_115),
        ("Environment", 7_514),
        ("Fluid", 5_580),
        ("Item", 2_628_479),
        ("PhysicalMaterial", 325),
    ] {
        assert_eq!(payload(store).len(), size, "{store}: P0 payload size");
    }
}
