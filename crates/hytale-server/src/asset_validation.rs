//! `--validate-assets`: load the phase-2 registry, encode the config burst and
//! verify that it decodes, is deterministic and only references tags that exist
//! in `ServerTags`.
//!
//! The command intentionally does not read the golden fixtures. It validates the
//! live registry and its encoded packets, so it also works against a mod pack.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use hytale_assets::{
    AssetCollection, AssetError, AssetPack, AssetPacket, AssetPacketError, AssetRegistry,
};
use hytale_protocol::generated::assets::{
    UpdateBlockBreakingDecals, UpdateBlockHitboxes, UpdateBlockParticleSets, UpdateBlockSoundSets,
    UpdateBlockTypes, UpdateEnvironments, UpdateFluidFX, UpdateFluids, UpdateItemCategories,
    UpdateItemPlayerAnimations, UpdateItemQualities, UpdateItemReticles, UpdateItemSoundSets,
    UpdateItems, UpdatePhysicalMaterials, UpdateSoundEvents, UpdateTagPatterns,
};
use hytale_protocol::generated::common::{TagPattern, TagPatternType};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;
use thiserror::Error;

/// Errors that prevent asset validation from running at all.
#[derive(Debug, Error)]
pub enum AssetValidationError {
    /// The asset pack could not be opened.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The config packets could not be encoded.
    #[error(transparent)]
    Packets(#[from] AssetPacketError),
}

/// One loaded store's count and id range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreReport {
    /// Store name.
    pub name: &'static str,
    /// Number of loaded assets.
    pub count: usize,
    /// Lowest assigned id (`None` for default maps).
    pub first_id: Option<i32>,
    /// Highest assigned id (`None` for default maps).
    pub last_id: Option<i32>,
    /// The packet `maxId` equivalent (`max(id) + 1`, or 0).
    pub next_id: i32,
}

/// One encoded config packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketReport {
    /// Packet registry id.
    pub id: u32,
    /// Store that produced it.
    pub store: String,
    /// Uncompressed payload size.
    pub bytes: usize,
}

/// Result of validating one asset pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetValidationReport {
    /// Store counts and id ranges.
    pub stores: Vec<StoreReport>,
    /// Encoded config packets in send order.
    pub packets: Vec<PacketReport>,
    /// Number of entries in `ServerTags`.
    pub tag_count: usize,
    /// Lowest tag index.
    pub tag_first: Option<i32>,
    /// Highest tag index.
    pub tag_last: Option<i32>,
    /// Number of asset/pattern tag references that were checked.
    pub checked_tag_indexes: usize,
    /// Non-fatal validation drift. Empty means the pack is valid.
    pub failures: Vec<String>,
}

/// Load and validate the asset pack at `pack`.
pub fn validate_assets(pack: &Path) -> Result<AssetValidationReport, AssetValidationError> {
    let pack = AssetPack::open(pack)?;
    let registry = AssetRegistry::load_phase2(&pack)?;
    let config = registry.config_assets()?;

    let mut failures = Vec::new();

    // Determinism: encoding the same registry twice must produce the same
    // packets and tag map.
    let repeated = registry.config_assets()?;
    if config != repeated {
        failures.push("config_assets() is not deterministic for repeated calls".to_string());
    }

    let expected = expected_packets();
    if config.packets.len() != expected.len() {
        failures.push(format!(
            "expected {} config packets, got {}",
            expected.len(),
            config.packets.len()
        ));
    }
    for (index, (expected_id, expected_store)) in expected.iter().enumerate() {
        let Some(packet) = config.packets.get(index) else {
            continue;
        };
        if packet.id != *expected_id || packet.store != *expected_store {
            failures.push(format!(
                "packet {index}: expected {} `{}`, got {} `{}`",
                expected_id, expected_store, packet.id, packet.store
            ));
        }
    }

    let mut seen_ids = BTreeSet::new();
    for packet in &config.packets {
        if !seen_ids.insert(packet.id) {
            failures.push(format!("duplicate config packet id {}", packet.id));
        }
    }

    let tag_indexes: BTreeSet<i32> = config.tags.values().copied().collect();
    let mut checked_tag_indexes = 0usize;
    for packet in &config.packets {
        match check_packet(packet, &tag_indexes) {
            Ok(checked) => checked_tag_indexes += checked,
            Err(message) => failures.push(message),
        }
    }

    let stores = store_reports(&registry);
    let packets = config
        .packets
        .iter()
        .map(|packet| PacketReport {
            id: packet.id,
            store: packet.store.to_string(),
            bytes: packet.payload.len(),
        })
        .collect();
    let tag_first = config.tags.values().copied().min();
    let tag_last = config.tags.values().copied().max();

    Ok(AssetValidationReport {
        stores,
        packets,
        tag_count: config.tags.len(),
        tag_first,
        tag_last,
        checked_tag_indexes,
        failures,
    })
}

/// The expected config packet order. Kept here so the CLI reports order drift.
fn expected_packets() -> [(u32, &'static str); 17] {
    [
        (UpdateBlockHitboxes::ID, "BlockBoundingBoxes"),
        (UpdatePhysicalMaterials::ID, "PhysicalMaterial"),
        (UpdateSoundEvents::ID, "SoundEvent"),
        (UpdateBlockSoundSets::ID, "BlockSoundSet"),
        (UpdateFluidFX::ID, "FluidFX"),
        (UpdateFluids::ID, "Fluid"),
        (UpdateEnvironments::ID, "Environment"),
        (UpdateTagPatterns::ID, "TagPattern"),
        (UpdateBlockTypes::ID, "BlockType"),
        (UpdateBlockParticleSets::ID, "BlockParticleSet"),
        (UpdateBlockBreakingDecals::ID, "BlockBreakingDecal"),
        (UpdateItemSoundSets::ID, "ItemSoundSet"),
        (UpdateItemQualities::ID, "ItemQuality"),
        (UpdateItemReticles::ID, "ItemReticleConfig"),
        (UpdateItemCategories::ID, "ItemCategory"),
        (UpdateItemPlayerAnimations::ID, "ItemPlayerAnimations"),
        (UpdateItems::ID, "Item"),
    ]
}

fn store_reports(registry: &AssetRegistry) -> Vec<StoreReport> {
    macro_rules! store {
        ($name:literal, $field:ident) => {
            store_report($name, &registry.$field)
        };
    }
    vec![
        store!("BlockBoundingBoxes", block_bounding_boxes),
        store!("PhysicalMaterial", physical_materials),
        store!("SoundEvent", sound_events),
        store!("AudioCategory", audio_categories),
        store!("AudioState", audio_states),
        store!("BlockSoundSet", block_sound_sets),
        store!("FluidFX", fluid_fx),
        store!("Fluid", fluids),
        store!("Environment", environments),
        store!("TagPattern", tag_patterns),
        store!("BlockType", block_types),
        store!("BlockParticleSet", block_particle_sets),
        store!("BlockBreakingDecal", block_breaking_decals),
        store!("ItemSoundSet", item_sound_sets),
        store!("ItemQuality", item_qualities),
        store!("ItemReticleConfig", item_reticles),
        store!("ItemCategory", item_categories),
        store!("ItemPlayerAnimations", item_player_animations),
        store!("Item", items),
        store!("BlockTypeListAsset", block_type_lists),
        store!("PrefabListAsset", prefab_lists),
    ]
}

fn store_report<T>(name: &'static str, collection: &AssetCollection<T>) -> StoreReport {
    let mut first_id = None;
    let mut last_id = None;
    let mut next_id = 0;
    for record in collection.iter() {
        let Some(id) = record.id else { continue };
        first_id.get_or_insert(id);
        last_id = Some(last_id.map_or(id, |last: i32| last.max(id)));
        next_id = next_id.max(id + 1);
    }
    StoreReport {
        name,
        count: collection.len(),
        first_id,
        last_id,
        next_id,
    }
}

fn check_packet(packet: &AssetPacket, tags: &BTreeSet<i32>) -> Result<usize, String> {
    match packet.id {
        UpdateBlockHitboxes::ID => {
            let _: UpdateBlockHitboxes = decode(packet)?;
            Ok(0)
        }
        UpdatePhysicalMaterials::ID => {
            let _: UpdatePhysicalMaterials = decode(packet)?;
            Ok(0)
        }
        UpdateSoundEvents::ID => {
            let _: UpdateSoundEvents = decode(packet)?;
            Ok(0)
        }
        UpdateBlockSoundSets::ID => {
            let _: UpdateBlockSoundSets = decode(packet)?;
            Ok(0)
        }
        UpdateFluidFX::ID => {
            let _: UpdateFluidFX = decode(packet)?;
            Ok(0)
        }
        UpdateFluids::ID => {
            let packet: UpdateFluids = decode(packet)?;
            let mut checked = 0;
            let mut failures = Vec::new();
            for (id, fluid) in packet.fluids.unwrap_or_default() {
                check_tag_list(
                    fluid.tag_indexes.as_deref(),
                    tags,
                    &format!("Fluid {id}"),
                    &mut checked,
                    &mut failures,
                );
            }
            if failures.is_empty() {
                Ok(checked)
            } else {
                Err(failures.join("; "))
            }
        }
        UpdateEnvironments::ID => {
            let packet: UpdateEnvironments = decode(packet)?;
            let mut checked = 0;
            let mut failures = Vec::new();
            for (id, environment) in packet.environments.unwrap_or_default() {
                check_tag_list(
                    environment.tag_indexes.as_deref(),
                    tags,
                    &format!("Environment {id}"),
                    &mut checked,
                    &mut failures,
                );
            }
            if failures.is_empty() {
                Ok(checked)
            } else {
                Err(failures.join("; "))
            }
        }
        UpdateTagPatterns::ID => {
            let packet: UpdateTagPatterns = decode(packet)?;
            let mut checked = 0;
            let mut failures = Vec::new();
            for (id, pattern) in packet.patterns.unwrap_or_default() {
                walk_pattern_tags(
                    &pattern,
                    tags,
                    &format!("TagPattern {id}"),
                    &mut checked,
                    &mut failures,
                );
            }
            if failures.is_empty() {
                Ok(checked)
            } else {
                Err(failures.join("; "))
            }
        }
        UpdateBlockTypes::ID => {
            let packet: UpdateBlockTypes = decode(packet)?;
            let mut checked = 0;
            let mut failures = Vec::new();
            for (id, block) in packet.block_types.unwrap_or_default() {
                let name = block.name.clone().unwrap_or_else(|| format!("id {id}"));
                check_tag_list(
                    block.tag_indexes.as_deref(),
                    tags,
                    &format!("BlockType {name}"),
                    &mut checked,
                    &mut failures,
                );
                if block.transition_to_tag != i32::MIN {
                    check_tag(
                        block.transition_to_tag,
                        tags,
                        &format!("BlockType {name} transitionToTag"),
                        &mut checked,
                        &mut failures,
                    );
                }
            }
            if failures.is_empty() {
                Ok(checked)
            } else {
                Err(failures.join("; "))
            }
        }
        UpdateBlockParticleSets::ID => {
            let _: UpdateBlockParticleSets = decode(packet)?;
            Ok(0)
        }
        UpdateBlockBreakingDecals::ID => {
            let _: UpdateBlockBreakingDecals = decode(packet)?;
            Ok(0)
        }
        UpdateItemSoundSets::ID => {
            let _: UpdateItemSoundSets = decode(packet)?;
            Ok(0)
        }
        UpdateItemQualities::ID => {
            let _: UpdateItemQualities = decode(packet)?;
            Ok(0)
        }
        UpdateItemReticles::ID => {
            let _: UpdateItemReticles = decode(packet)?;
            Ok(0)
        }
        UpdateItemCategories::ID => {
            let _: UpdateItemCategories = decode(packet)?;
            Ok(0)
        }
        UpdateItemPlayerAnimations::ID => {
            let _: UpdateItemPlayerAnimations = decode(packet)?;
            Ok(0)
        }
        UpdateItems::ID => {
            let packet: UpdateItems = decode(packet)?;
            let mut checked = 0;
            let mut failures = Vec::new();
            for (key, item) in packet.items.unwrap_or_default() {
                check_tag_list(
                    item.tag_indexes.as_deref(),
                    tags,
                    &format!("Item {key}"),
                    &mut checked,
                    &mut failures,
                );
            }
            if failures.is_empty() {
                Ok(checked)
            } else {
                Err(failures.join("; "))
            }
        }
        other => Err(format!("unexpected config packet id {other}")),
    }
}

fn decode<T: Wire>(packet: &AssetPacket) -> Result<T, String> {
    T::decode(&mut Reader::new(&packet.payload))
        .map_err(|error| format!("{} ({}) does not decode: {error}", packet.store, packet.id))
}

fn check_tag_list(
    indexes: Option<&[i32]>,
    tags: &BTreeSet<i32>,
    context: &str,
    checked: &mut usize,
    failures: &mut Vec<String>,
) {
    for index in indexes.unwrap_or_default() {
        check_tag(*index, tags, context, checked, failures);
    }
}

fn check_tag(
    index: i32,
    tags: &BTreeSet<i32>,
    context: &str,
    checked: &mut usize,
    failures: &mut Vec<String>,
) {
    *checked += 1;
    if !tags.contains(&index) {
        failures.push(format!(
            "{context}: tag index {index} is missing from ServerTags"
        ));
    }
}

fn walk_pattern_tags(
    pattern: &TagPattern,
    tags: &BTreeSet<i32>,
    context: &str,
    checked: &mut usize,
    failures: &mut Vec<String>,
) {
    if pattern.r#type == TagPatternType::Equals {
        check_tag(pattern.tag_index, tags, context, checked, failures);
    }
    for operand in pattern.operands.as_deref().unwrap_or_default() {
        walk_pattern_tags(operand, tags, context, checked, failures);
    }
    if let Some(nested) = pattern.not.as_deref() {
        walk_pattern_tags(nested, tags, context, checked, failures);
    }
}

/// Print a human-readable report. Returns true when validation passed.
pub fn print_report(pack: &Path, report: &AssetValidationReport) -> bool {
    println!("asset pack: {}", pack.display());
    println!("stores ({}):", report.stores.len());
    for store in &report.stores {
        let ids = match (store.first_id, store.last_id) {
            (Some(first), Some(last)) => format!("ids {first}..{last} (next {})", store.next_id),
            _ => "no ids".to_string(),
        };
        println!("  {:<22} count={:<6} {ids}", store.name, store.count);
    }
    println!("config packets ({}):", report.packets.len());
    for packet in &report.packets {
        println!(
            "  {:<3} {:<22} {} bytes",
            packet.id, packet.store, packet.bytes
        );
    }
    let range = match (report.tag_first, report.tag_last) {
        (Some(first), Some(last)) => format!("index {first}..{last}"),
        _ => "no indexes".to_string(),
    };
    println!(
        "tags: {} ({range}); checked {} asset/pattern references",
        report.tag_count, report.checked_tag_indexes
    );
    println!("failures: {}", report.failures.len());
    if report.failures.is_empty() {
        println!("validation: ok");
        true
    } else {
        println!("validation: FAILED ({} issue(s))", report.failures.len());
        for failure in &report.failures {
            println!("  - {failure}");
        }
        false
    }
}

/// The default pack path used by the CLI.
pub fn default_pack_path() -> PathBuf {
    PathBuf::from("Assets/Assets.zip")
}
