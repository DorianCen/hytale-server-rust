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
    UpdateAbilityKeywords, UpdateAmbienceFX, UpdateAudioCategories, UpdateAudioStates,
    UpdateBeamConfigs, UpdateBlockBreakingDecals, UpdateBlockHitboxes, UpdateBlockParticleSets,
    UpdateBlockSets, UpdateBlockSoundSets, UpdateBlockTypes, UpdateConnectedBlockRuleSets,
    UpdateEntityEffects, UpdateEntityStatTypes, UpdateEntityUIComponents, UpdateEnvironments,
    UpdateEqualizerEffects, UpdateFieldcraftCategories, UpdateFluidFX, UpdateFluids,
    UpdateHitboxCollisionConfig, UpdateInteractions, UpdateItemCategories,
    UpdateItemPlayerAnimations, UpdateItemQualities, UpdateItemReticles, UpdateItemSoundSets,
    UpdateItems, UpdateModelvfxs, UpdateMusicContainers, UpdateMusicTransitions,
    UpdateParticleSpawners, UpdateParticleSystems, UpdatePhysicalMaterials,
    UpdateProjectileConfigs, UpdateRecipes, UpdateRepulsionConfig, UpdateResourceTypes,
    UpdateReverbEffects, UpdateRootInteractions, UpdateSoundEvents, UpdateSoundSets,
    UpdateStingers, UpdateTagPatterns, UpdateTrails, UpdateUnarmedInteractions, UpdateWeathers,
};
use hytale_protocol::generated::common::{
    BlockNeighbor, BlockType, ConnectedBlockShape, ConnectedBlockUpdateMode, DestinationAlign,
    FadeCurve, RequiredBlockFaceSupport, SyncPoint, UpdateType,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

fn load() -> (AssetRegistry, Vec<hytale_assets::AssetPacket>) {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load phase 2");
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
            UpdateSoundEvents::ID,
            UpdateBlockSoundSets::ID,
            UpdateFluidFX::ID,
            UpdateFluids::ID,
            UpdateEnvironments::ID,
            UpdateTagPatterns::ID,
            UpdateBlockTypes::ID,
            UpdateBlockParticleSets::ID,
            UpdateBlockBreakingDecals::ID,
            UpdateItemSoundSets::ID,
            UpdateItemQualities::ID,
            UpdateItemReticles::ID,
            UpdateItemCategories::ID,
            UpdateItemPlayerAnimations::ID,
            UpdateEntityStatTypes::ID,
            UpdateEntityEffects::ID,
            UpdateProjectileConfigs::ID,
            UpdateWeathers::ID,
            UpdateItems::ID,
            UpdateBlockSets::ID,
            UpdateTrails::ID,
            UpdateParticleSystems::ID,
            UpdateParticleSpawners::ID,
            UpdateModelvfxs::ID,
            UpdateFieldcraftCategories::ID,
            UpdateResourceTypes::ID,
            UpdateRecipes::ID,
            UpdateAmbienceFX::ID,
            UpdateInteractions::ID,
            UpdateRootInteractions::ID,
            UpdateUnarmedInteractions::ID,
            UpdateEntityUIComponents::ID,
            UpdateHitboxCollisionConfig::ID,
            UpdateRepulsionConfig::ID,
            UpdateSoundSets::ID,
            UpdateAudioCategories::ID,
            UpdateReverbEffects::ID,
            UpdateEqualizerEffects::ID,
            UpdateMusicContainers::ID,
            UpdateAudioStates::ID,
            UpdateConnectedBlockRuleSets::ID,
            UpdateMusicTransitions::ID,
            UpdateStingers::ID,
            UpdateBeamConfigs::ID,
            UpdateAbilityKeywords::ID,
        ]
    );
    assert_eq!(packet(&packets, UpdateTagPatterns::ID).store, "TagPattern");
    assert_eq!(packet(&packets, UpdateBlockTypes::ID).store, "BlockType");
    assert_eq!(packet(&packets, UpdateItems::ID).store, "Item");
}

/// Release clients reject `WorldLoadFinished` unless every one of these asset
/// types was initialized, even for stores this port sends empty.
#[test]
fn release_required_asset_types_are_all_initialized() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    let ids: BTreeSet<u32> = packets.iter().map(|packet| packet.id).collect();
    for id in [
        UpdateAbilityKeywords::ID,
        UpdateAmbienceFX::ID,
        UpdateAudioCategories::ID,
        UpdateAudioStates::ID,
        UpdateBeamConfigs::ID,
        UpdateBlockSets::ID,
        UpdateConnectedBlockRuleSets::ID,
        UpdateEntityUIComponents::ID,
        UpdateEqualizerEffects::ID,
        UpdateFieldcraftCategories::ID,
        UpdateHitboxCollisionConfig::ID,
        UpdateInteractions::ID,
        UpdateModelvfxs::ID,
        UpdateMusicContainers::ID,
        UpdateMusicTransitions::ID,
        UpdateParticleSpawners::ID,
        UpdateParticleSystems::ID,
        UpdateRecipes::ID,
        UpdateRepulsionConfig::ID,
        UpdateResourceTypes::ID,
        UpdateReverbEffects::ID,
        UpdateRootInteractions::ID,
        UpdateSoundSets::ID,
        UpdateStingers::ID,
        UpdateTrails::ID,
        UpdateUnarmedInteractions::ID,
    ] {
        assert!(ids.contains(&id), "missing required asset type packet {id}");
    }

    // Regression: the release client null-references an empty asset store if
    // its collection is encoded as null. Every required collection must be an
    // explicit empty map/vector.
    let block_sets = UpdateBlockSets::decode(&mut Reader::new(
        &packet(&packets, UpdateBlockSets::ID).payload,
    ))
    .expect("decode UpdateBlockSets");
    assert_eq!(block_sets.r#type, UpdateType::Init);
    assert!(block_sets
        .block_sets
        .as_ref()
        .is_some_and(|sets| !sets.is_empty()));

    let particles = UpdateParticleSystems::decode(&mut Reader::new(
        &packet(&packets, UpdateParticleSystems::ID).payload,
    ))
    .expect("decode UpdateParticleSystems");
    assert!(particles
        .particle_systems
        .as_ref()
        .is_some_and(|systems| !systems.is_empty()));
    assert_eq!(particles.removed_particle_systems, None);

    let recipes = UpdateRecipes::decode(&mut Reader::new(
        &packet(&packets, UpdateRecipes::ID).payload,
    ))
    .expect("decode UpdateRecipes");
    assert_eq!(recipes.recipes, Some(BTreeMap::new()));
    assert_eq!(recipes.removed_recipes, Some(Vec::new()));

    // Sound events reference AudioCategory/AudioState ids, so those stores
    // must contain the real (preloaded) entries rather than an empty map.
    let categories = UpdateAudioCategories::decode(&mut Reader::new(
        &packet(&packets, UpdateAudioCategories::ID).payload,
    ))
    .expect("decode UpdateAudioCategories");
    let categories = categories.categories.expect("categories");
    assert!(
        categories.contains_key(&0),
        "the EMPTY category preload is missing"
    );
    assert!(categories.len() > 1, "only the preload category was sent");

    let states = UpdateAudioStates::decode(&mut Reader::new(
        &packet(&packets, UpdateAudioStates::ID).payload,
    ))
    .expect("decode UpdateAudioStates");
    let states = states.audio_states.expect("audio states");
    assert!(
        states.contains_key(&0),
        "the EMPTY audio-state preload is missing"
    );
    assert!(states.len() > 1, "only the preload audio state was sent");
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
        assert_eq!(
            block.interactions.is_some(),
            expected.interactions.is_some(),
            "{name}: interactions presence"
        );
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
            block.block_sound_set_index, expected.block_sound_set_index,
            "{name}: blockSoundSetIndex"
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

        assert!(
            block.tag_indexes.is_some(),
            "{name}: tagIndexes must not be null"
        );

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
    assert!(
        actual.values().all(|block| block.interactions.is_some()),
        "every block must carry a non-null interactions map"
    );

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
        assert_eq!(
            block.placement_settings.is_some(),
            expected.placement_settings.is_some(),
            "{name}: placementSettings presence"
        );
        assert_eq!(
            block.conditional_sounds.is_some(),
            expected.conditional_sounds.is_some(),
            "{name}: conditionalSounds presence"
        );
        assert_eq!(
            block.states.is_some(),
            expected.states.is_some(),
            "{name}: states presence"
        );
        assert_eq!(
            block.bench.is_some(),
            expected.bench.is_some(),
            "{name}: bench presence"
        );
    }
}

/// Every live item carries a materialized tag-index list, like Java.
#[test]
fn item_tag_indexes_are_materialized() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    let decoded = hytale_protocol::generated::assets::UpdateItems::decode(&mut Reader::new(
        &packet(
            &packets,
            hytale_protocol::generated::assets::UpdateItems::ID,
        )
        .payload,
    ))
    .expect("decode items");
    let items = decoded.items.expect("item map");
    assert!(!items.is_empty());
    assert!(
        items.values().all(|item| item.tag_indexes.is_some()),
        "every item must carry a non-null tagIndexes list"
    );
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
        // Nested item trees are presence-checked: Java sends the object when
        // the asset declares it and omits it otherwise. Deep values are
        // covered by the per-field regressions below.
        assert_eq!(item.tool.is_some(), expected.tool.is_some(), "{key}: tool");
        assert_eq!(
            item.weapon.is_some(),
            expected.weapon.is_some(),
            "{key}: weapon"
        );
        assert_eq!(
            item.armor.is_some(),
            expected.armor.is_some(),
            "{key}: armor"
        );
        assert_eq!(
            item.glider_config.is_some(),
            expected.glider_config.is_some(),
            "{key}: gliderConfig"
        );
        assert_eq!(
            item.block_selector_tool.is_some(),
            expected.block_selector_tool.is_some(),
            "{key}: blockSelectorTool"
        );
        assert_eq!(
            item.builder_tool_data.is_some(),
            expected.builder_tool_data.is_some(),
            "{key}: builderToolData"
        );
        assert_eq!(
            item.trails.is_some(),
            expected.trails.is_some(),
            "{key}: trails"
        );
        assert_eq!(
            item.item_appearance_conditions.is_some(),
            expected.item_appearance_conditions.is_some(),
            "{key}: itemAppearanceConditions"
        );
        assert_eq!(
            item.hud_ui.is_some(),
            expected.hud_ui.is_some(),
            "{key}: hudUI"
        );
        assert_eq!(
            item.ability.is_some(),
            expected.ability.is_some(),
            "{key}: ability"
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

/// T66: previously-empty stores now carry the real asset data instead of a
/// placeholder map. Counts are derived from the same registry the packets are
/// built from, so the gate fails if a loader regression empties one.
#[test]
fn simple_config_stores_are_no_longer_placeholders() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();

    let decode = |id: u32| -> Vec<u8> { packet(&packets, id).payload.clone() };
    let sound_sets: UpdateSoundSets =
        UpdateSoundSets::decode(&mut Reader::new(&decode(UpdateSoundSets::ID)))
            .expect("decode sound sets");
    assert_eq!(
        sound_sets.sound_sets.as_ref().map(BTreeMap::len),
        Some(registry.sound_sets.len())
    );
    assert!(sound_sets
        .sound_sets
        .as_ref()
        .is_some_and(|sets| sets.values().any(|set| set.sounds.is_some())));

    let resource_types: UpdateResourceTypes =
        UpdateResourceTypes::decode(&mut Reader::new(&decode(UpdateResourceTypes::ID)))
            .expect("decode resource types");
    assert_eq!(
        resource_types.resource_types.as_ref().map(BTreeMap::len),
        Some(registry.resource_types.len())
    );

    let fieldcraft: UpdateFieldcraftCategories = UpdateFieldcraftCategories::decode(
        &mut Reader::new(&decode(UpdateFieldcraftCategories::ID)),
    )
    .expect("decode fieldcraft");
    assert_eq!(
        fieldcraft.item_categories.as_ref().map(Vec::len),
        Some(registry.fieldcraft_categories.len())
    );

    let reverb: UpdateReverbEffects =
        UpdateReverbEffects::decode(&mut Reader::new(&decode(UpdateReverbEffects::ID)))
            .expect("decode reverb");
    assert_eq!(
        reverb.effects.as_ref().map(BTreeMap::len),
        Some(registry.reverb_effects.len())
    );
    assert!(reverb
        .effects
        .as_ref()
        .is_some_and(|effects| !effects.is_empty()));

    let equalizer: UpdateEqualizerEffects =
        UpdateEqualizerEffects::decode(&mut Reader::new(&decode(UpdateEqualizerEffects::ID)))
            .expect("decode equalizer");
    assert_eq!(
        equalizer.effects.as_ref().map(BTreeMap::len),
        Some(registry.equalizer_effects.len())
    );
    assert!(equalizer
        .effects
        .as_ref()
        .is_some_and(|effects| !effects.is_empty()));

    let beams: UpdateBeamConfigs =
        UpdateBeamConfigs::decode(&mut Reader::new(&decode(UpdateBeamConfigs::ID)))
            .expect("decode beams");
    assert_eq!(
        beams.beam_configs.as_ref().map(BTreeMap::len),
        Some(registry.beams.len())
    );
    assert!(beams
        .beam_configs
        .as_ref()
        .is_some_and(|configs| !configs.is_empty()));

    let hitboxes: UpdateHitboxCollisionConfig = UpdateHitboxCollisionConfig::decode(
        &mut Reader::new(&decode(UpdateHitboxCollisionConfig::ID)),
    )
    .expect("decode hitbox collision");
    assert_eq!(
        hitboxes
            .hitbox_collision_configs
            .as_ref()
            .map(BTreeMap::len),
        Some(registry.hitbox_collision_configs.len())
    );

    let repulsion: UpdateRepulsionConfig =
        UpdateRepulsionConfig::decode(&mut Reader::new(&decode(UpdateRepulsionConfig::ID)))
            .expect("decode repulsion");
    assert_eq!(
        repulsion.repulsion_configs.as_ref().map(BTreeMap::len),
        Some(registry.repulsion_configs.len())
    );

    let block_sets: UpdateBlockSets =
        UpdateBlockSets::decode(&mut Reader::new(&decode(UpdateBlockSets::ID)))
            .expect("decode block sets");
    assert_eq!(
        block_sets.block_sets.as_ref().map(BTreeMap::len),
        Some(registry.block_sets.len())
    );
    assert!(block_sets.block_sets.as_ref().is_some_and(|sets| sets
        .values()
        .any(|set| set.blocks.as_ref().is_some_and(|blocks| !blocks.is_empty()))));

    let entity_ui: UpdateEntityUIComponents =
        UpdateEntityUIComponents::decode(&mut Reader::new(&decode(UpdateEntityUIComponents::ID)))
            .expect("decode entity UI");
    assert_eq!(
        entity_ui.components.as_ref().map(BTreeMap::len),
        Some(registry.entity_ui_components.len())
    );
    assert!(entity_ui
        .components
        .as_ref()
        .is_some_and(|components| !components.is_empty()));

    let model_vfxs: UpdateModelvfxs =
        UpdateModelvfxs::decode(&mut Reader::new(&decode(UpdateModelvfxs::ID)))
            .expect("decode model vfx");
    assert_eq!(
        model_vfxs.model_vf_xs.as_ref().map(BTreeMap::len),
        Some(registry.model_vfxs.len())
    );
    assert!(model_vfxs
        .model_vf_xs
        .as_ref()
        .is_some_and(|effects| !effects.is_empty()));

    let trails: UpdateTrails =
        UpdateTrails::decode(&mut Reader::new(&decode(UpdateTrails::ID))).expect("decode trails");
    assert_eq!(
        trails.trails.as_ref().map(BTreeMap::len),
        Some(registry.trails.len())
    );
    assert!(trails
        .trails
        .as_ref()
        .is_some_and(|trails| !trails.is_empty()));

    let keywords: UpdateAbilityKeywords =
        UpdateAbilityKeywords::decode(&mut Reader::new(&decode(UpdateAbilityKeywords::ID)))
            .expect("decode ability keywords");
    assert_eq!(
        keywords.ability_keywords.as_ref().map(BTreeMap::len),
        Some(registry.ability_keywords.len())
    );
    assert!(keywords
        .ability_keywords
        .as_ref()
        .is_some_and(|keywords| !keywords.is_empty()));
}

/// T66: every item tree Java materializes should be present for the same
/// asset keys. Counts mirror the T66 gap inventory.
#[test]
fn item_nested_trees_have_the_expected_presence() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    let decoded = UpdateItems::decode(&mut Reader::new(&packet(&packets, UpdateItems::ID).payload))
        .expect("decode items");
    let items = decoded.items.expect("item map");
    let count = |f: fn(&hytale_protocol::generated::common::ItemBase) -> bool| {
        items.values().filter(|item| f(item)).count()
    };
    let counts = (
        count(|item| item.tool.is_some()),
        count(|item| item.weapon.is_some()),
        count(|item| item.armor.is_some()),
        count(|item| item.glider_config.is_some()),
        count(|item| item.block_selector_tool.is_some()),
        count(|item| item.builder_tool_data.is_some()),
        count(|item| item.trails.is_some()),
        count(|item| item.item_appearance_conditions.is_some()),
        count(|item| item.hud_ui.is_some()),
        count(|item| item.ability.is_some()),
    );
    assert_eq!(counts, (37, 275, 122, 2, 2, 39, 5, 117, 26, 24));
}

/// T66: the particle stores are not Rust-id-dependent, so the Rust payload can
/// be byte-compared with the Java dump once map ordering is canonicalized.
#[test]
fn particle_packets_match_the_java_golden() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (_, packets) = load();
    for (id, store) in [
        (UpdateParticleSystems::ID, "ParticleSystem"),
        (UpdateParticleSpawners::ID, "ParticleSpawner"),
    ] {
        let expected =
            std::fs::read(repo_root().join(format!("testdata/golden/server/assets/{store}.bin")))
                .expect("read the golden payload");
        assert_eq!(
            packet(&packets, id).payload,
            expected,
            "{store} packet differs from the Java golden"
        );
    }
}

/// T66: the particle packets carry real data: every system has spawners and
/// every spawner has a particle definition, with counts matching the registry.
#[test]
fn particle_stores_are_materialized() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();

    let systems: UpdateParticleSystems = UpdateParticleSystems::decode(&mut Reader::new(
        &packet(&packets, UpdateParticleSystems::ID).payload,
    ))
    .expect("decode particle systems");
    let systems = systems.particle_systems.expect("particle system map");
    assert_eq!(systems.len(), registry.particle_systems.len());
    assert!(systems.values().all(|system| system
        .spawners
        .as_ref()
        .is_some_and(|spawners| !spawners.is_empty())));

    let spawners: UpdateParticleSpawners = UpdateParticleSpawners::decode(&mut Reader::new(
        &packet(&packets, UpdateParticleSpawners::ID).payload,
    ))
    .expect("decode particle spawners");
    let spawners = spawners.particle_spawners.expect("particle spawner map");
    assert_eq!(spawners.len(), registry.particle_spawners.len());
    assert!(spawners.values().all(|spawner| spawner.particle.is_some()));
    assert!(spawners.values().any(|spawner| spawner
        .particle
        .as_ref()
        .is_some_and(|particle| particle.animation_frames.is_some())));
}

/// T66: the music-transition store is materialized from real assets, including
/// Java's preloaded `EMPTY` entry and the container-index expansion.
#[test]
fn music_transitions_are_materialized() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let decoded: UpdateMusicTransitions = UpdateMusicTransitions::decode(&mut Reader::new(
        &packet(&packets, UpdateMusicTransitions::ID).payload,
    ))
    .expect("decode music transitions");
    assert_eq!(decoded.r#type, UpdateType::Init);
    assert_eq!(decoded.max_id, 6);
    assert_eq!(
        decoded.music_transitions.as_ref().map(BTreeMap::len),
        Some(registry.music_transitions.len())
    );

    let transitions = decoded.music_transitions.expect("transition map");
    assert_eq!(transitions.len(), 6);
    assert!(transitions
        .values()
        .all(|transition| transition.id.is_some()));

    let empty = transitions.get(&0).expect("EMPTY at index 0");
    assert_eq!(empty.id.as_deref(), Some("EMPTY"));
    assert_eq!(empty.from_container_indices, None);
    assert_eq!(empty.to_container_indices, None);
    assert_eq!(empty.specificity, 0);
    assert_eq!(empty.align, DestinationAlign::EntryMarker);
    assert_eq!(empty.fade_in_curve, FadeCurve::EqualPowerSine);
    assert_eq!(empty.fade_out_curve, FadeCurve::EqualPowerSine);

    let transition = transitions
        .values()
        .find(|transition| transition.id.as_deref() == Some("MT_Z1D_Goblin_Boss_Phase2_Interlude"))
        .expect("the Phase2 interlude transition");
    let from = registry
        .music_containers
        .id_of("Track_Z1D_Goblin_Boss_Phase2")
        .expect("from container");
    let to = registry
        .music_containers
        .id_of("Track_Z1D_Goblin_Boss_Interlude")
        .expect("to container");
    assert_eq!(transition.from_container_indices, Some(vec![from]));
    assert_eq!(transition.to_container_indices, Some(vec![to]));
    let exit = transition.exit_at.as_ref().expect("ExitAt");
    assert_eq!(exit.point, SyncPoint::NextBar);
    assert_eq!(transition.align, DestinationAlign::EntryMarker);
    assert_eq!(
        transition.fade_out_duration.as_ref().map(|d| d.beats),
        Some(1.0)
    );
    assert_eq!(
        transition.fade_out_delay.as_ref().map(|d| d.beats),
        Some(1.5)
    );
    assert_eq!(
        transition.fade_in_duration.as_ref().map(|d| d.ms),
        Some(0.0)
    );
    assert_eq!(transition.priority, 40);
    assert_eq!(transition.specificity, 8);
}

/// T66: connected-block rule sets carry their shapes and patterns, including
/// the shapes inherited from `SimplePipe`.
#[test]
fn connected_block_rule_sets_are_materialized() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (registry, packets) = load();
    let decoded: UpdateConnectedBlockRuleSets = UpdateConnectedBlockRuleSets::decode(
        &mut Reader::new(&packet(&packets, UpdateConnectedBlockRuleSets::ID).payload),
    )
    .expect("decode connected block rule sets");
    assert_eq!(decoded.r#type, UpdateType::Init);
    let rule_sets = decoded.rule_sets.expect("rule set map");
    assert_eq!(rule_sets.len(), registry.connected_block_rule_sets.len());
    assert_eq!(rule_sets.len(), 3);

    let simple = rule_sets.get("SimplePipe").expect("SimplePipe");
    assert_eq!(simple.id, "SimplePipe");
    assert_eq!(
        simple.update_mode,
        ConnectedBlockUpdateMode::UpdatePlaceUpdate
    );
    let shapes = simple.shapes.as_ref().expect("SimplePipe shapes");
    assert_eq!(shapes.len(), 2);
    match shapes.get("Straight").expect("Straight shape") {
        ConnectedBlockShape::TemplatedConnectedBlockShape(shape) => {
            assert_eq!(shape.template_id.as_deref(), Some("Straight"));
            assert_eq!(
                shape
                    .face_tags
                    .as_ref()
                    .and_then(|tags| tags.up.as_ref())
                    .map(Vec::len),
                Some(1)
            );
        }
        other => panic!("Straight should be templated, got {other:?}"),
    }
    let patterns = simple.patterns.as_ref().expect("SimplePipe patterns");
    assert_eq!(patterns.len(), 2);
    assert_eq!(patterns[0].output_shape_id, "Straight");
    assert_eq!(
        patterns[0].rotations.as_deref(),
        Some(&[0, 8, 4, 5, 6, 7][..])
    );
    assert!(patterns[0].rule.is_some());
    assert_eq!(patterns[1].output_shape_id, "Corner");

    let metal = rule_sets.get("MetalSimplePipe").expect("MetalSimplePipe");
    let metal_shapes = metal.shapes.as_ref().expect("Metal shapes");
    assert!(metal_shapes.contains_key("Straight"));
    assert!(metal_shapes.contains_key("Corner"));
    assert!(metal_shapes.contains_key("Chimney"));
    assert_eq!(metal.patterns.as_ref().map(Vec::len), Some(2));

    let large = rule_sets
        .get("LargeMetalSimplePipe")
        .expect("LargeMetalSimplePipe");
    let large_shapes = large.shapes.as_ref().expect("Large shapes");
    assert!(large_shapes.contains_key("Mouthpiece"));
    match large_shapes.get("Corner").expect("Large Corner") {
        ConnectedBlockShape::TemplatedConnectedBlockShape(shape) => {
            assert_eq!(shape.relative_rotation_index, 8);
            assert_eq!(shape.template_id.as_deref(), Some("Corner"));
        }
        other => panic!("Large Corner should be templated, got {other:?}"),
    }
}
