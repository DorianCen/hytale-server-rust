//! Golden parity for the T36 block/fluid child stores and their init packets.
//!
//! The Java dumper assigned these ids in the same file order the Rust loader
//! models, so the five streamed stores are byte-compared with the committed
//! payloads. `BlockType`, `Fluid` and `Environment` keep their Rust-owned ids,
//! so their child-index fields are compared by asset name/key instead.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use hytale_assets::{AssetPack, AssetRegistry, TagRegistry};
use hytale_protocol::generated::assets::{
    UpdateBlockBreakingDecals, UpdateBlockParticleSets, UpdateBlockSoundSets, UpdateBlockTypes,
    UpdateEnvironments, UpdateFluidFX, UpdateFluids, UpdateSoundEvents,
};
use hytale_protocol::generated::common::{BlockType, Fluid};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn registry() -> &'static AssetRegistry {
    static REGISTRY: OnceLock<AssetRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
        AssetRegistry::load_phase2(&pack).expect("load P0 + child stores")
    })
}

fn index() -> &'static Value {
    static INDEX: OnceLock<Value> = OnceLock::new();
    INDEX.get_or_init(|| {
        let bytes = std::fs::read(repo_root().join("testdata/golden/server/assets/index.json"))
            .expect("read asset index");
        serde_json::from_slice(&bytes).expect("parse asset index")
    })
}

fn golden(store: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
        .unwrap_or_else(|error| panic!("read {store}.bin: {error}"))
}

fn decode<T: Wire>(bytes: &[u8], store: &str) -> T {
    T::decode(&mut Reader::new(bytes)).unwrap_or_else(|error| panic!("decode {store}: {error}"))
}

fn store_entry(name: &str) -> &'static Value {
    index()["stores"]
        .as_array()
        .expect("stores array")
        .iter()
        .find(|store| store["store"] == name)
        .unwrap_or_else(|| panic!("missing store `{name}` in index.json"))
}

fn compare_bytes(store: &str) {
    let expected = golden(store);
    let registry = registry();
    let packet = match store {
        "BlockBreakingDecal" => registry.block_breaking_decal_packet(),
        "BlockParticleSet" => registry.block_particle_set_packet(),
        "BlockSoundSet" => registry.block_sound_set_packet(),
        "FluidFX" => registry.fluid_fx_packet(),
        "SoundEvent" => registry.sound_event_packet(),
        other => panic!("unknown store `{other}`"),
    }
    .unwrap_or_else(|error| panic!("encode {store}: {error}"));

    if expected != packet.payload {
        let first = expected
            .iter()
            .zip(&packet.payload)
            .position(|(left, right)| left != right);
        panic!(
            "{store}: golden {} bytes, rust {} bytes, first difference {first:?}",
            expected.len(),
            packet.payload.len()
        );
    }
}

#[test]
fn child_packets_are_byte_equal_to_the_java_golden() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    for store in [
        "BlockBreakingDecal",
        "BlockParticleSet",
        "BlockSoundSet",
        "FluidFX",
        "SoundEvent",
    ] {
        compare_bytes(store);
    }
}

#[test]
fn child_packets_decode_with_the_generated_codec() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = registry();

    let decals = registry
        .block_breaking_decal_packet()
        .expect("encode BlockBreakingDecal");
    assert_eq!(decals.id, UpdateBlockBreakingDecals::ID);
    let decals: UpdateBlockBreakingDecals = decode(&decals.payload, "BlockBreakingDecal");
    assert_eq!(
        decals.block_breaking_decals.len(),
        registry.block_breaking_decals.len()
    );

    let particles = registry
        .block_particle_set_packet()
        .expect("encode BlockParticleSet");
    assert_eq!(particles.id, UpdateBlockParticleSets::ID);
    let particles: UpdateBlockParticleSets = decode(&particles.payload, "BlockParticleSet");
    assert_eq!(
        particles.block_particle_sets.as_ref().map(BTreeMap::len),
        Some(registry.block_particle_sets.len())
    );

    let sounds = registry
        .block_sound_set_packet()
        .expect("encode BlockSoundSet");
    assert_eq!(sounds.id, UpdateBlockSoundSets::ID);
    let sounds: UpdateBlockSoundSets = decode(&sounds.payload, "BlockSoundSet");
    assert_eq!(
        sounds.block_sound_sets.as_ref().map(BTreeMap::len),
        Some(registry.block_sound_sets.len())
    );
    assert_eq!(sounds.max_id, registry.block_sound_sets.len() as i32);

    let effects = registry.fluid_fx_packet().expect("encode FluidFX");
    assert_eq!(effects.id, UpdateFluidFX::ID);
    let effects: UpdateFluidFX = decode(&effects.payload, "FluidFX");
    assert_eq!(
        effects.fluid_fx.as_ref().map(BTreeMap::len),
        Some(registry.fluid_fx.len())
    );
    assert_eq!(effects.max_id, registry.fluid_fx.len() as i32);

    let events = registry.sound_event_packet().expect("encode SoundEvent");
    assert_eq!(events.id, UpdateSoundEvents::ID);
    let events: UpdateSoundEvents = decode(&events.payload, "SoundEvent");
    assert_eq!(
        events.sound_events.as_ref().map(BTreeMap::len),
        Some(registry.sound_events.len())
    );
    assert_eq!(events.max_id, registry.sound_events.len() as i32);

    // Spot-check the Java defaults the generated structs cannot express.
    let empty = events
        .sound_events
        .as_ref()
        .and_then(|events| events.get(&0))
        .expect("EMPTY SoundEvent");
    assert_eq!(empty.id.as_deref(), Some("EMPTY"));
    assert_eq!(empty.volume, 1.0);
    assert_eq!(empty.pitch, 1.0);
    assert_eq!(empty.start_attenuation_distance, 2.0);
    assert_eq!(empty.max_distance, 16.0);
    assert_eq!(empty.spatial_blend, 0.6);
    assert_eq!(empty.max_instance, 50);
    assert_eq!(empty.audio_category, 0);
    assert!(empty.layers.is_none());
}

#[test]
fn child_store_counts_keys_and_ids_match_index_json() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = registry();

    let indexed = |collection: &hytale_assets::AssetCollection<Value>| {
        let mut keys = Vec::new();
        let mut ids = BTreeMap::new();
        let mut max = -1;
        for record in collection.iter() {
            keys.push(record.key.clone());
            if let Some(id) = record.id {
                ids.insert(record.key.clone(), id);
                max = max.max(id);
            }
        }
        keys.sort();
        (collection.len(), keys, Some((max + 1, ids)))
    };
    let default = |collection: &hytale_assets::AssetCollection<Value>| {
        let mut keys: Vec<String> = collection.iter().map(|record| record.key.clone()).collect();
        keys.sort();
        let ids: Option<(i32, BTreeMap<String, i32>)> = None;
        (collection.len(), keys, ids)
    };
    struct StoreView {
        name: &'static str,
        count: usize,
        keys: Vec<String>,
        max_id: Option<i32>,
        ids: BTreeMap<String, i32>,
    }
    let indexed_view = |name: &'static str, collection: &hytale_assets::AssetCollection<Value>| {
        let (count, keys, indexed) = indexed(collection);
        let (max_id, ids) = indexed.expect("indexed store");
        StoreView {
            name,
            count,
            keys,
            max_id: Some(max_id),
            ids,
        }
    };
    let default_view = |name: &'static str, collection: &hytale_assets::AssetCollection<Value>| {
        let (count, keys, _) = default(collection);
        StoreView {
            name,
            count,
            keys,
            max_id: None,
            ids: BTreeMap::new(),
        }
    };
    let stores = [
        default_view("BlockBreakingDecal", &registry.block_breaking_decals),
        default_view("BlockParticleSet", &registry.block_particle_sets),
        indexed_view("BlockSoundSet", &registry.block_sound_sets),
        indexed_view("FluidFX", &registry.fluid_fx),
        indexed_view("SoundEvent", &registry.sound_events),
        indexed_view("AudioCategory", &registry.audio_categories),
        indexed_view("AudioState", &registry.audio_states),
    ];

    for StoreView {
        name,
        count,
        keys,
        max_id,
        ids,
    } in stores
    {
        let entry = store_entry(name);
        assert_eq!(
            count,
            entry["count"].as_u64().expect("count") as usize,
            "{name}: count"
        );
        let expected_keys: Vec<String> = entry["keys"]
            .as_array()
            .expect("keys")
            .iter()
            .map(|key| key.as_str().expect("key").to_string())
            .collect();
        assert_eq!(keys, expected_keys, "{name}: keys");

        if let Some(max_id) = max_id {
            assert_eq!(
                max_id,
                entry["maxId"].as_i64().expect("maxId") as i32,
                "{name}: maxId"
            );
            let expected_ids: BTreeMap<String, i32> = entry["ids"]
                .as_object()
                .expect("ids")
                .iter()
                .map(|(key, id)| (key.clone(), id.as_i64().expect("id") as i32))
                .collect();
            assert_eq!(ids, expected_ids, "{name}: key -> id");
        } else {
            assert!(entry.get("maxId").is_none(), "{name}: no maxId expected");
        }
    }
}

#[test]
fn block_fluid_and_environment_child_indexes_match_the_java_golden() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = registry();
    let tags = TagRegistry::from_assets(registry);

    // BlockType: compare `blockSoundSetIndex` by block name (block ids are
    // Rust-owned).
    let packet = registry.block_type_packet(&tags).expect("encode BlockType");
    let actual: UpdateBlockTypes = decode(&packet.payload, "BlockType");
    let actual_blocks = actual.block_types.expect("block map");
    let golden_packet: UpdateBlockTypes = decode(&golden("BlockType"), "BlockType");
    let golden_blocks: BTreeMap<String, BlockType> = golden_packet
        .block_types
        .expect("golden block map")
        .into_values()
        .filter_map(|block| Some((block.name.clone()?, block)))
        .collect();

    let mut checked = 0usize;
    for record in registry.block_types.iter() {
        let Some(block) = actual_blocks.get(&record.id.expect("block id")) else {
            continue;
        };
        let name = record.key.as_str();
        let expected = record
            .value
            .json
            .get("BlockSoundSetId")
            .and_then(Value::as_str)
            .filter(|id| !id.eq_ignore_ascii_case("EMPTY"))
            .and_then(|id| registry.block_sound_sets.id_of(id))
            .unwrap_or(0);
        assert_eq!(
            block.block_sound_set_index, expected,
            "{name}: blockSoundSetIndex from JSON"
        );
        if let Some(golden_block) = golden_blocks.get(name) {
            assert_eq!(
                block.block_sound_set_index, golden_block.block_sound_set_index,
                "{name}: blockSoundSetIndex vs golden"
            );
            checked += 1;
        }
    }
    assert!(checked > 6000, "checked {checked} blocks");

    // Fluid: fluid ids match Java, so compare the indexes by id.
    let packet = registry.fluid_packet(&tags).expect("encode Fluid");
    let actual: UpdateFluids = decode(&packet.payload, "Fluid");
    let actual_fluids = actual.fluids.expect("fluid map");
    let golden_fluids: BTreeMap<i32, Fluid> = decode::<UpdateFluids>(&golden("Fluid"), "Fluid")
        .fluids
        .expect("golden fluids");
    assert_eq!(actual_fluids.len(), golden_fluids.len());
    for (id, golden_fluid) in &golden_fluids {
        let fluid = actual_fluids.get(id).expect("matching fluid id");
        assert_eq!(
            fluid.fluid_fx_index,
            golden_fluid.fluid_fx_index,
            "{}: fluidFxIndex",
            golden_fluid.id.as_deref().unwrap_or_default()
        );
        assert_eq!(
            fluid.block_sound_set_index,
            golden_fluid.block_sound_set_index,
            "{}: blockSoundSetIndex",
            golden_fluid.id.as_deref().unwrap_or_default()
        );
    }

    // Environment: the phase-2 pack has no `FluidParticles`, but the field must
    // stay byte-compatible with Java when it appears.
    let packet = registry
        .environment_packet(&tags)
        .expect("encode Environment");
    let actual: UpdateEnvironments = decode(&packet.payload, "Environment");
    let actual_environments = actual.environments.expect("environment map");
    let golden_environments = decode::<UpdateEnvironments>(&golden("Environment"), "Environment")
        .environments
        .expect("golden environments");
    for (id, golden_environment) in &golden_environments {
        let environment = actual_environments
            .get(id)
            .expect("matching environment id");
        assert_eq!(
            environment.fluid_particles,
            golden_environment.fluid_particles,
            "{}: fluidParticles",
            golden_environment.id.as_deref().unwrap_or_default()
        );
    }
}

#[test]
fn block_sound_set_and_fluid_fx_indexes_resolve_without_zero_sentinels() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = registry();

    // Every authored SoundEvents entry must resolve to a known SoundEvent id.
    let packet = registry
        .block_sound_set_packet()
        .expect("encode BlockSoundSet");
    let sets: UpdateBlockSoundSets = decode(&packet.payload, "BlockSoundSet");
    let sets = sets.block_sound_sets.expect("sound set map");
    for set in sets.values() {
        for (event, index) in set.sound_event_indices.as_ref().unwrap_or(&BTreeMap::new()) {
            assert!(
                registry.sound_events.get_by_id(*index).is_some(),
                "{:?}: sound event {event:?} index {index} is unresolved",
                set.id
            );
        }
    }

    // Every FluidFX id in the golden is emitted and every reaction references it.
    let packet = registry.fluid_fx_packet().expect("encode FluidFX");
    let effects: UpdateFluidFX = decode(&packet.payload, "FluidFX");
    let effects = effects.fluid_fx.expect("fluid fx map");
    assert_eq!(effects.len(), 5);
    for (id, effect) in &effects {
        assert!(registry.fluid_fx.get_by_id(*id).is_some());
        assert!(effect.id.is_some());
    }
}

#[test]
fn sound_event_audio_categories_and_state_bindings_resolve() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let registry = registry();
    let packet = registry.sound_event_packet().expect("encode SoundEvent");
    let packet: UpdateSoundEvents = decode(&packet.payload, "SoundEvent");
    let events = packet.sound_events.expect("sound event map");

    for event in events.values() {
        if event.audio_category >= 0 {
            assert!(
                registry
                    .audio_categories
                    .get_by_id(event.audio_category)
                    .is_some(),
                "{:?}: audio category {} is unresolved",
                event.id,
                event.audio_category
            );
        }
        for binding in event.state_bindings.as_deref().unwrap_or_default() {
            assert!(
                binding.audio_state_index < 0
                    || registry
                        .audio_states
                        .get_by_id(binding.audio_state_index)
                        .is_some(),
                "{:?}: audio state {} is unresolved",
                event.id,
                binding.audio_state_index
            );
            let values = registry
                .audio_states
                .get_by_id(binding.audio_state_index)
                .and_then(|record| record.value.get("Values"))
                .and_then(Value::as_array);
            for delta in binding.deltas.as_deref().unwrap_or_default() {
                assert!(
                    delta.value_index < 0
                        || values
                            .is_some_and(|values| { (delta.value_index as usize) < values.len() }),
                    "value index {} is outside the audio state's values",
                    delta.value_index
                );
            }
        }
    }

    // The three `SFX_Gobliterator_Saw_Charge_*` events bind the Saw Charge
    // pitch axis and resolve all four authored value names.
    let saw = events
        .values()
        .find(|event| event.id.as_deref() == Some("SFX_Gobliterator_Saw_Charge_End"))
        .expect("Gobliterator saw event");
    let binding = saw
        .state_bindings
        .as_deref()
        .and_then(|bindings| bindings.first())
        .expect("state binding");
    let values = registry
        .audio_states
        .get_by_id(binding.audio_state_index)
        .and_then(|record| record.value.get("Values"))
        .and_then(Value::as_array)
        .expect("the saw charge axis has values");
    let resolved: Vec<i32> = binding
        .deltas
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|delta| delta.value_index)
        .collect();
    assert_eq!(resolved.len(), 4);
    for index in resolved {
        assert!((0..values.len() as i32).contains(&index));
    }
}
