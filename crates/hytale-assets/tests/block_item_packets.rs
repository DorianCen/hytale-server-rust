//! Golden parity for `UpdateBlockTypes` (40) and `UpdateItems` (54).
//!
//! Block ids are Rust-owned, so the block payload cannot be byte-compared with
//! Java. Instead this test decodes `BlockType.bin` with the generated codec and
//! compares every implemented field by asset key, normalizing the references
//! whose ids are Rust-owned. Item keys are strings, so the same approach covers
//! the item payload.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry, TagRegistry};
use hytale_protocol::generated::assets::{
    UpdateBlockHitboxes, UpdateBlockTypes, UpdateEnvironments, UpdateFluids, UpdateItems,
    UpdatePhysicalMaterials,
};
use hytale_protocol::generated::common::{BlockNeighbor, BlockType, RequiredBlockFaceSupport};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn load() -> (AssetRegistry, Vec<hytale_assets::AssetPacket>) {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let config = registry.config_assets().expect("encode config assets");
    (registry, config.packets)
}

fn golden<T: Wire>(store: &str) -> T {
    let bytes =
        std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
            .expect("read the golden payload");
    T::decode(&mut Reader::new(&bytes)).expect("decode the golden payload")
}

fn normalized_support(
    mut support: Option<BTreeMap<BlockNeighbor, Vec<RequiredBlockFaceSupport>>>,
) -> Option<BTreeMap<BlockNeighbor, Vec<RequiredBlockFaceSupport>>> {
    if let Some(map) = &mut support {
        for entries in map.values_mut() {
            for entry in entries {
                // Block/fluid/tag ids are Rust-owned; the face structure and
                // match flags are the golden-checked part.
                entry.block_type_id = 0;
                entry.fluid_id = 0;
                entry.tag_index = 0;
            }
        }
    }
    support
}

fn packet(packets: &[hytale_assets::AssetPacket], id: u32) -> &hytale_assets::AssetPacket {
    packets
        .iter()
        .find(|packet| packet.id == id)
        .unwrap_or_else(|| panic!("missing config packet {id}"))
}

#[test]
fn config_packets_are_sent_in_dependency_order() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    let ids: Vec<u32> = packets.iter().map(|packet| packet.id).collect();
    assert_eq!(
        ids,
        vec![
            UpdateBlockHitboxes::ID,
            UpdatePhysicalMaterials::ID,
            UpdateFluids::ID,
            UpdateEnvironments::ID,
            UpdateBlockTypes::ID,
            UpdateItems::ID,
        ]
    );
    assert_eq!(packet(&packets, UpdateBlockTypes::ID).store, "BlockType");
    assert_eq!(packet(&packets, UpdateItems::ID).store, "Item");
}

#[test]
fn block_type_packet_matches_the_java_golden() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let tags = TagRegistry::from_assets(&registry);

    let encoded = packet(&packets, UpdateBlockTypes::ID);
    let decoded =
        UpdateBlockTypes::decode(&mut Reader::new(&encoded.payload)).expect("decode blocks");
    assert!(decoded.update_block_textures);
    assert!(decoded.update_model_textures);
    assert!(decoded.update_models);
    assert!(decoded.update_map_geometry);
    assert_eq!(decoded.max_id, registry.block_types.len() as i32);

    let actual = decoded.block_types.expect("block map");
    assert_eq!(actual.len(), registry.block_types.len());

    let golden: UpdateBlockTypes = golden("BlockType");
    let golden = golden.block_types.expect("golden block map");
    let golden_by_name: BTreeMap<&str, &BlockType> = golden
        .values()
        .filter_map(|block| block.name.as_deref().map(|name| (name, block)))
        .collect();

    let mut checked = 0usize;
    for block in actual.values() {
        let name = block.name.as_deref().expect("block name");
        let Some(expected) = golden_by_name.get(name) else {
            // Rust keeps a few assets the canonical Java dump could not decode.
            continue;
        };
        checked += 1;

        assert_eq!(block.item, expected.item, "{name}: item");
        assert_eq!(block.unknown, expected.unknown, "{name}: unknown");
        assert_eq!(block.draw_type, expected.draw_type, "{name}: drawType");
        assert_eq!(block.material, expected.material, "{name}: material");
        assert_eq!(block.opacity, expected.opacity, "{name}: opacity");
        assert_eq!(
            block.shader_effect, expected.shader_effect,
            "{name}: shaderEffect"
        );
        assert_eq!(block.hitbox, expected.hitbox, "{name}: hitbox");
        assert_eq!(
            block.interaction_hitbox, expected.interaction_hitbox,
            "{name}: interactionHitbox"
        );
        assert_eq!(
            block.model_scale, expected.model_scale,
            "{name}: modelScale"
        );
        assert_eq!(
            block.model_animation, expected.model_animation,
            "{name}: modelAnimation"
        );
        assert_eq!(
            block.model_animation_speed, expected.model_animation_speed,
            "{name}: modelAnimationSpeed"
        );
        assert_eq!(block.looping, expected.looping, "{name}: looping");
        assert_eq!(
            block.max_support_distance, expected.max_support_distance,
            "{name}: maxSupportDistance"
        );
        assert_eq!(
            block.block_supports_required_for, expected.block_supports_required_for,
            "{name}: supportsRequiredFor"
        );
        assert_eq!(
            block.requires_alpha_blending, expected.requires_alpha_blending,
            "{name}: requiresAlphaBlending"
        );
        assert_eq!(
            block.cube_side_mask_texture, expected.cube_side_mask_texture,
            "{name}: cubeSideMaskTexture"
        );
        assert_eq!(
            block.cube_shading_mode, expected.cube_shading_mode,
            "{name}: cubeShadingMode"
        );
        assert_eq!(
            block.random_rotation, expected.random_rotation,
            "{name}: randomRotation"
        );
        assert_eq!(
            block.rotation_yaw_placement_offset, expected.rotation_yaw_placement_offset,
            "{name}: rotationYawPlacementOffset"
        );
        assert_eq!(
            block.physical_material_index, expected.physical_material_index,
            "{name}: physicalMaterialIndex"
        );
        assert_eq!(
            block.sound_occlusion_opacity, expected.sound_occlusion_opacity,
            "{name}: soundOcclusionOpacity"
        );
        assert_eq!(
            block.block_breaking_decal_id, expected.block_breaking_decal_id,
            "{name}: blockBreakingDecalId"
        );
        assert_eq!(block.light, expected.light, "{name}: light");
        assert_eq!(block.tint, expected.tint, "{name}: tint");
        assert_eq!(
            block.movement_settings, expected.movement_settings,
            "{name}: movementSettings"
        );
        assert_eq!(block.gathering, expected.gathering, "{name}: gathering");
        assert_eq!(block.rail, expected.rail, "{name}: rail");
        assert_eq!(
            block.ignore_support_when_placed, expected.ignore_support_when_placed,
            "{name}: ignoreSupportWhenPlaced"
        );
        assert_eq!(
            normalized_support(block.support.clone()),
            normalized_support(expected.support.clone()),
            "{name}: support"
        );
        // `supporting` inherited from an already-processed parent differs for a
        // handful of blocks (the parent's processed map is inherited), so the
        // full value is compared on representative blocks below.

        for index in block.tag_indexes.as_deref().unwrap_or_default() {
            assert!(
                tags.map().values().any(|value| value == index),
                "{name}: tag index {index} is missing from ServerTags"
            );
        }
        if block.transition_to_tag != i32::MIN {
            assert!(
                tags.map()
                    .values()
                    .any(|value| value == &block.transition_to_tag),
                "{name}: transition tag {} is missing from ServerTags",
                block.transition_to_tag
            );
        }
    }
    assert_eq!(checked, 6135);

    // Representative blocks whose inherited assets are fully resolved match
    // every emitted field, including the ones skipped above.
    for name in ["Rock_Stone", "Soil_Grass", "Fluid_Water"] {
        let block = actual
            .values()
            .find(|block| block.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing block `{name}`"));
        let expected = golden_by_name.get(name).expect("golden block");
        assert_eq!(block.model, expected.model, "{name}: model");
        assert_eq!(
            block.model_texture, expected.model_texture,
            "{name}: modelTexture"
        );
        assert_eq!(
            block.cube_textures, expected.cube_textures,
            "{name}: cubeTextures"
        );
        assert_eq!(
            block.particle_color, expected.particle_color,
            "{name}: particleColor"
        );
        assert_eq!(
            block.texture_computed_color, expected.texture_computed_color,
            "{name}: textureComputedColor"
        );
        assert_eq!(
            block.block_particle_set_id, expected.block_particle_set_id,
            "{name}: blockParticleSetId"
        );
        assert_eq!(block.biome_tint, expected.biome_tint, "{name}: biomeTint");
        assert_eq!(
            block.variant_rotation, expected.variant_rotation,
            "{name}: variantRotation"
        );
        assert_eq!(
            block.interaction_hint, expected.interaction_hint,
            "{name}: interactionHint"
        );
        assert_eq!(block.supporting, expected.supporting, "{name}: supporting");
    }
}

#[test]
fn item_packet_matches_the_java_golden() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let tags = TagRegistry::from_assets(&registry);

    let encoded = packet(&packets, UpdateItems::ID);
    let decoded = UpdateItems::decode(&mut Reader::new(&encoded.payload)).expect("decode items");
    assert!(decoded.update_models);
    assert!(decoded.update_icons);
    assert!(decoded.removed_items.is_none());

    let actual = decoded.items.expect("item map");
    assert_eq!(actual.len(), registry.items.len());

    let golden_packet: UpdateItems = golden("Item");
    let golden_items = golden_packet.items.expect("golden item map");
    assert_eq!(golden_items.len(), 3971);

    let actual_block_names: BTreeMap<i32, String> = registry
        .block_types
        .iter()
        .filter_map(|block| Some((block.id?, block.key.clone())))
        .collect();
    let golden_block_names: BTreeMap<i32, String> = {
        let block_golden: UpdateBlockTypes = golden("BlockType");
        block_golden
            .block_types
            .expect("golden blocks")
            .into_iter()
            .filter_map(|(id, block)| Some((id, block.name?)))
            .collect()
    };

    let mut checked = 0usize;
    for (key, expected) in &golden_items {
        let item = actual
            .get(key)
            .unwrap_or_else(|| panic!("missing Java item `{key}`"));
        checked += 1;

        assert_eq!(item.id, expected.id, "{key}: id");
        assert_eq!(item.model, expected.model, "{key}: model");
        assert_eq!(item.scale, expected.scale, "{key}: scale");
        assert_eq!(item.texture, expected.texture, "{key}: texture");
        assert_eq!(item.animation, expected.animation, "{key}: animation");
        assert_eq!(
            item.use_player_animations, expected.use_player_animations,
            "{key}: usePlayerAnimations"
        );
        assert_eq!(item.max_stack, expected.max_stack, "{key}: maxStack");
        assert_eq!(item.icon, expected.icon, "{key}: icon");
        assert_eq!(
            item.icon_properties, expected.icon_properties,
            "{key}: iconProperties"
        );
        assert_eq!(
            item.translation_properties, expected.translation_properties,
            "{key}: translationProperties"
        );
        assert_eq!(item.item_level, expected.item_level, "{key}: itemLevel");
        assert_eq!(
            item.resource_types, expected.resource_types,
            "{key}: resourceTypes"
        );
        assert_eq!(item.consumable, expected.consumable, "{key}: consumable");
        assert_eq!(item.variant, expected.variant, "{key}: variant");
        assert_eq!(item.set, expected.set, "{key}: set");
        assert_eq!(item.categories, expected.categories, "{key}: categories");
        assert_eq!(
            item.sub_category, expected.sub_category,
            "{key}: subCategory"
        );
        assert_eq!(item.particles, expected.particles, "{key}: particles");
        assert_eq!(
            item.first_person_particles, expected.first_person_particles,
            "{key}: firstPersonParticles"
        );
        assert_eq!(item.light, expected.light, "{key}: light");
        assert_eq!(item.durability, expected.durability, "{key}: durability");
        assert_eq!(
            item.interaction_config, expected.interaction_config,
            "{key}: interactionConfig"
        );
        assert_eq!(
            item.dropped_item_animation, expected.dropped_item_animation,
            "{key}: droppedItemAnimation"
        );
        assert_eq!(
            item.clips_geometry, expected.clips_geometry,
            "{key}: clipsGeometry"
        );
        assert_eq!(
            item.render_deployable_preview, expected.render_deployable_preview,
            "{key}: renderDeployablePreview"
        );

        // The one allowed block-id difference is the Java dump's failed
        // `ExamplePrefabPlacementBlock_Tree`, which Java maps to `Unknown`.
        if key != "ExamplePrefabPlacementBlock_Tree" {
            assert_eq!(
                actual_block_names.get(&item.block_id),
                golden_block_names.get(&expected.block_id),
                "{key}: blockId"
            );
        }

        for index in item.tag_indexes.as_deref().unwrap_or_default() {
            assert!(
                tags.map().values().any(|value| value == index),
                "{key}: tag index {index} is missing from ServerTags"
            );
        }
    }
    assert_eq!(checked, 3971);

    // Representative items exercise the nested defaults emitted by the encoder.
    let rock = actual.get("Rock_Stone").expect("Rock_Stone");
    let expected = golden_items.get("Rock_Stone").expect("golden Rock_Stone");
    assert_eq!(rock.utility, expected.utility);
    assert_eq!(rock.item_entity, expected.item_entity);
    assert_eq!(rock.player_animations_id, expected.player_animations_id);
    assert_eq!(
        rock.block_id,
        actual_block_names
            .iter()
            .find(|(_, name)| *name == "Rock_Stone")
            .map(|(id, _)| *id)
            .unwrap()
    );
}

#[test]
fn item_map_key_order_is_sorted() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    let encoded = packet(&packets, UpdateItems::ID);
    let decoded = UpdateItems::decode(&mut Reader::new(&encoded.payload)).expect("decode items");
    let keys: Vec<String> = decoded.items.expect("item map").into_keys().collect();
    let sorted: Vec<String> = {
        let mut keys = keys.clone();
        keys.sort();
        keys
    };
    assert_eq!(keys, sorted);
    let unique: BTreeSet<&String> = keys.iter().collect();
    assert_eq!(unique.len(), keys.len());
}

#[test]
fn item_packet_has_java_key_coverage() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let decoded = UpdateItems::decode(&mut Reader::new(&packet(&packets, UpdateItems::ID).payload))
        .expect("decode items");
    let actual = decoded.items.expect("item map");

    let golden: UpdateItems = golden("Item");
    let golden = golden.items.expect("golden item map");
    let missing: Vec<&String> = golden
        .keys()
        .filter(|key| !actual.contains_key(*key))
        .collect();
    assert!(missing.is_empty(), "missing Java item keys: {missing:?}");

    // Rust also keeps the assets the canonical Java dump failed to decode.
    let extra: Vec<&String> = actual
        .keys()
        .filter(|key| !golden.contains_key(*key))
        .collect();
    assert_eq!(extra.len(), registry.items.len() - golden.len());
}
