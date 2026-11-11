//! Regression coverage for interaction ids sent to the release client.
//!
//! Java's `AssetMapWithIndexes.NOT_FOUND` (`i32::MIN`) is the "no interaction"
//! sentinel. Encoding it as `0` makes the client follow interaction 0 instead of
//! stopping; because `*Change_Active_Slot` is id 0 and has no `Next`, that node
//! then points at itself and the client overflows its stack while resolving the
//! chain. The release client dies right after `PrepareWorldMap` with a SIGSEGV
//! when this happens.

use std::path::PathBuf;

use hytale_assets::packets::{ASSET_NOT_FOUND, INTERACTION_NOT_FOUND};
use hytale_assets::{AssetPack, AssetPacket, AssetRegistry};
use hytale_protocol::generated::assets::{
    UpdateInteractions, UpdateRootInteractions, UpdateUnarmedInteractions,
};
use hytale_protocol::generated::common::{Interaction as ProtocolInteraction, InteractionType};
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

fn load_packets() -> (Vec<AssetPacket>, AssetRegistry) {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load phase 2");
    let packets = registry
        .config_assets()
        .expect("encode config assets")
        .packets;
    (packets, registry)
}

fn decode<T: Packet + Wire>(packets: &[AssetPacket]) -> T {
    let packet = packets
        .iter()
        .find(|packet| packet.id == T::ID)
        .unwrap_or_else(|| panic!("missing packet {}", T::ID));
    T::decode(&mut Reader::new(&packet.payload)).expect("decode packet")
}

/// Protocol type id selected by Java's `Interaction.generatePacket()` for each
/// asset `Type`. Keep this aligned with the Java codec registrations; an
/// unknown type fails the audit instead of silently falling back.
fn java_protocol_type_id(type_name: &str) -> u32 {
    match type_name {
        "AddItem"
        | "Bed"
        | "CanBreakRespawnPoint"
        | "CarryBlock"
        | "ChangeFarmingStage"
        | "CycleBlockGroup"
        | "Door"
        | "FertilizeSoil"
        | "HarvestCrop"
        | "LaunchPad"
        | "OpenBenchPage"
        | "OpenContainer"
        | "OpenProcessingBench"
        | "PlaceFluid"
        | "Portal"
        | "PortalReturn"
        | "Seating"
        | "SpawnMinecart"
        | "SpawnNPC"
        | "TeleportConfigInstance"
        | "UseCaptureCrate"
        | "UseCoop"
        | "UseWateringCan" => 0,
        "Simple"
        | "CameraShake"
        | "CarryDroppedBlock"
        | "ChangeStatWithModifier"
        | "ClearCarriedBlock"
        | "ContextualUseNPC"
        | "DestroyBlock"
        | "Explode"
        | "GameFlagCondition"
        | "GrantAbilityCharges"
        | "Interrupt"
        | "LaunchAbilityProjectile"
        | "LaunchProjectile"
        | "OpenCustomUI"
        | "PlacementCountCondition"
        | "PullEntityAbility"
        | "RefillContainer"
        | "RevealMapMarkersInView"
        | "SendBeacon"
        | "SendMessage"
        | "SignalNearbyVolumes"
        | "SpawnAbilityEntity"
        | "SpawnDeployableFromRaycast"
        | "SpawnPrefab"
        | "SpawnTriggerVolume"
        | "SplitAbilityProjectile"
        | "StatsConditionWithModifier"
        | "TeleportInstance"
        | "TriggerAbilityFX"
        | "TriggerSpawnMarkers" => 1,
        "PlaceBlock" | "CarryPlaceBlock" => 2,
        "BreakBlock" => 3,
        "PickBlock" => 4,
        "UseBlock" => 5,
        "UseEntity" => 6,
        "BuilderTool" => 7,
        "ModifyInventory" => 8,
        "Charging" => 9,
        "Wielding" => 10,
        "Chaining" => 11,
        "Condition" => 12,
        "StatsCondition" => 13,
        "BlockCondition" => 14,
        "Replace" => 15,
        "ChangeBlock" => 16,
        "ChangeState" => 17,
        "FirstClick" => 18,
        "Selector" => 20,
        "DamageEntity" => 21,
        "Repeat" => 22,
        "Parallel" => 23,
        "ChangeActiveSlot" => 24,
        "CancelChain" => 39,
        "EffectCondition" => 25,
        "ApplyForce" => 26,
        "ApplyEffect" => 27,
        "ClearEntityEffect" => 28,
        "Serial" => 29,
        "ChangeStat" => 30,
        "MovementCondition" => 31,
        "Projectile" => 32,
        "RemoveEntity" => 33,
        "ResetCooldown" => 34,
        "TriggerCooldown" => 35,
        "CooldownCondition" => 36,
        "ChainFlag" => 37,
        "MemoriesCondition" => 43,
        "DurabilityCondition" => 45,
        "DragPlaceBlock" => 46,
        "ExtrudePlaceBlock" => 47,
        "SurfaceDrawPlaceBlock" => 48,
        "PlaceModeSelect" => 49,
        "DragEraseBlock" => 50,
        "GroundedCondition" => 51,
        other => panic!("unmapped interaction Type `{other}`; extend the Java audit"),
    }
}

#[test]
fn interactions_use_the_java_protocol_variant() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");

    let mut checked = 0usize;
    for record in registry.interactions.iter() {
        let Some(id) = record.id else {
            continue;
        };
        let type_name = record
            .value
            .get("Type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Simple");
        let actual = interactions
            .get(&id)
            .unwrap_or_else(|| panic!("missing encoded interaction `{}`", record.key));
        assert_eq!(
            actual.type_id(),
            java_protocol_type_id(type_name),
            "`{}` (`{type_name}`) uses the wrong protocol variant",
            record.key
        );
        checked += 1;
    }
    assert!(checked > 1_600, "only checked {checked} interactions");
}

#[test]
fn chaining_and_block_fallbacks_resolve_their_children() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");

    let id_of = |key: &str| {
        registry
            .interactions
            .id_of(key)
            .unwrap_or_else(|| panic!("missing interaction `{key}`"))
    };

    // `Chaining.Next` arrays and `Flags` maps contain inline children. Java
    // registers them as `*<parent>_Next_<index>` / `*<parent>_Flags_<flag>`.
    match interactions
        .get(&id_of("Debug_Combo_Primary"))
        .expect("chaining interaction")
    {
        ProtocolInteraction::ChainingInteraction(node) => {
            assert_eq!(node.chain_id.as_deref(), Some("Debug_Combo"));
            assert_eq!(node.chaining_allowance, 0.8);
            assert_eq!(node.chaining_next.len(), 3);
            for next in &node.chaining_next {
                assert!(
                    interactions.contains_key(next),
                    "chaining Next references missing interaction {next}"
                );
            }
            let flags = node.flags.as_ref().expect("chaining flags");
            assert!(flags.contains_key("Special_Second"));
            for flag in flags.values() {
                assert!(
                    interactions.contains_key(flag),
                    "chaining flag references missing interaction {flag}"
                );
            }
        }
        other => panic!("expected ChainingInteraction, got {other:?}"),
    }

    // Block-derived interactions without their own packet body use
    // `SimpleBlockInteraction`, including `UseLatestTarget`.
    match interactions
        .get(&id_of("Watering_Can_Use"))
        .expect("watering can interaction")
    {
        ProtocolInteraction::SimpleBlockInteraction(node) => {
            assert!(node.use_latest_target);
        }
        other => panic!("expected SimpleBlockInteraction, got {other:?}"),
    }
    assert!(matches!(
        interactions.get(&id_of("Door")),
        Some(ProtocolInteraction::SimpleBlockInteraction(_))
    ));

    // `CarryPlaceBlockInteraction` reuses `PlaceBlockInteraction` with a `-1`
    // block id and no held-item removal.
    match interactions
        .get(&id_of("Carry_Place_Block"))
        .expect("carry place interaction")
    {
        ProtocolInteraction::PlaceBlockInteraction(node) => {
            assert_eq!(node.block_id, -1);
            assert!(!node.remove_item_in_hand);
        }
        other => panic!("expected PlaceBlockInteraction, got {other:?}"),
    }

    // `DragEraseBlockInteraction` extends `DragPlaceBlockInteraction` in Java
    // but has its own protocol type id.
    assert!(matches!(
        interactions.get(&id_of("Creative_Eraser_Drag")),
        Some(ProtocolInteraction::DragEraseBlockInteraction(_))
    ));
}

#[test]
fn charging_wielding_memories_and_active_slot_children_resolve() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");
    let id_of = |key: &str| {
        registry
            .interactions
            .id_of(key)
            .unwrap_or_else(|| panic!("missing interaction `{key}`"))
    };

    // `Charging.Next` is a float-keyed inline map. Every branch must resolve to
    // a real interaction id, never NOT_FOUND.
    match interactions
        .get(&id_of("Consume_Charge_Food_T1_Inner"))
        .expect("charging consumable")
    {
        ProtocolInteraction::ChargingInteraction(node) => {
            let next = node.charged_next.as_ref().expect("charged next map");
            assert!(!next.is_empty());
            assert_eq!(next[0].0, 2.5);
            for (_, child) in next {
                assert!(
                    interactions.contains_key(child),
                    "charging branch {child} is missing"
                );
            }
        }
        other => panic!("expected ChargingInteraction, got {other:?}"),
    }

    // `Wielding.Next` is a single child encoded at charge 0.0.
    match interactions
        .get(&id_of("Common_Guard_Wield"))
        .expect("wielding guard")
    {
        ProtocolInteraction::WieldingInteraction(node) => {
            let next = node.charged_next.as_ref().expect("wielding next");
            assert_eq!(next.len(), 1);
            assert_eq!(next[0].0, 0.0);
            assert!(interactions.contains_key(&next[0].1));
        }
        other => panic!("expected WieldingInteraction, got {other:?}"),
    }

    // `MemoriesCondition.Next` is an int-keyed map; `Teleporter_Place` has all
    // six levels plus a failed branch.
    match interactions
        .get(&id_of("Teleporter_Place"))
        .expect("memories teleporter")
    {
        ProtocolInteraction::MemoriesConditionInteraction(node) => {
            assert_eq!(node.memories_next.len(), 6);
            assert!(node.memories_next.contains_key(&0));
            assert!(node.memories_next.contains_key(&5));
            for child in node.memories_next.values() {
                assert!(
                    interactions.contains_key(child),
                    "memories branch {child} is missing"
                );
            }
            assert_ne!(node.failed, INTERACTION_NOT_FOUND);
            assert!(interactions.contains_key(&node.failed));
        }
        other => panic!("expected MemoriesConditionInteraction, got {other:?}"),
    }

    // An explicit ChangeActiveSlot target is preserved; the synthetic default
    // interaction uses Java's UNSET sentinel when no slot is authored.
    match interactions
        .get(&id_of("Skeleton_Burnt_Praetorian_EquipSecondSword"))
        .expect("equip next change slot")
    {
        ProtocolInteraction::SimpleInteraction(node) => {
            let ProtocolInteraction::SerialInteraction(serial) =
                interactions.get(&node.next).expect("equip serial")
            else {
                panic!("equip next is not Serial");
            };
            let slot = serial
                .serial_interactions
                .iter()
                .find_map(|child| match interactions.get(child) {
                    Some(ProtocolInteraction::ChangeActiveSlotInteraction(slot)) => Some(slot),
                    _ => None,
                })
                .expect("change active slot in serial");
            assert_eq!(slot.target_slot, 1);
        }
        other => panic!("expected SimpleInteraction, got {other:?}"),
    }
}

#[test]
fn block_swing_forks_and_replace_defaults_use_root_ids() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");
    let roots: UpdateRootInteractions = decode(&packets);
    let roots = roots.interactions.expect("root interaction map");

    let id_of = |key: &str| {
        registry
            .interactions
            .id_of(key)
            .unwrap_or_else(|| panic!("missing interaction `{key}`"))
    };

    // `Block_Swing_Left` is `Simple -> Parallel`. Java's `Parallel.Interactions`
    // is `RootInteraction[]`; sending interaction ids here made the client fork
    // the wrong root and loop.
    let ProtocolInteraction::SimpleInteraction(swing) =
        interactions.get(&id_of("Block_Swing_Left")).expect("swing")
    else {
        panic!("Block_Swing_Left is not Simple");
    };
    let ProtocolInteraction::ParallelInteraction(parallel) =
        interactions.get(&swing.next).expect("parallel")
    else {
        panic!("Block_Swing_Left.Next is not Parallel");
    };
    assert_eq!(parallel.next.len(), 3);
    assert!(parallel
        .next
        .iter()
        .all(|root| *root != INTERACTION_NOT_FOUND && roots.contains_key(root)));

    // `Block_Break` -> `BreakBlock` -> `Serial` contains a `Replace` whose
    // `DefaultValue` is a root interaction.
    let ProtocolInteraction::UseBlockInteraction(block_break) = interactions
        .get(&id_of("Block_Break"))
        .expect("block break")
    else {
        panic!("Block_Break is not UseBlock");
    };
    let ProtocolInteraction::BreakBlockInteraction(break_block) =
        interactions.get(&block_break.failed).expect("break block")
    else {
        panic!("Block_Break.Failed is not BreakBlock");
    };
    let ProtocolInteraction::SerialInteraction(serial) =
        interactions.get(&break_block.next).expect("break serial")
    else {
        panic!("BreakBlock.Next is not Serial");
    };
    let mut replaces = 0;
    for child in &serial.serial_interactions {
        let Some(ProtocolInteraction::ReplaceInteraction(replace)) = interactions.get(child) else {
            continue;
        };
        replaces += 1;
        assert_ne!(
            replace.default_value, INTERACTION_NOT_FOUND,
            "Replace `{:?}` has no default root",
            replace.variable
        );
        assert!(
            roots.contains_key(&replace.default_value),
            "Replace `{:?}` default {} is not a root interaction",
            replace.variable,
            replace.default_value
        );
    }
    assert!(replaces > 0);
}

#[test]
fn block_swing_roots_resolve_every_child() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");
    let roots: UpdateRootInteractions = decode(&packets);
    let roots = roots.interactions.expect("root map");
    let id_of = |key: &str| {
        registry
            .interactions
            .id_of(key)
            .unwrap_or_else(|| panic!("missing `{key}`"))
    };
    let ProtocolInteraction::SimpleInteraction(swing) =
        interactions.get(&id_of("Block_Swing_Left")).expect("swing")
    else {
        panic!()
    };
    let ProtocolInteraction::ParallelInteraction(parallel) =
        interactions.get(&swing.next).expect("parallel")
    else {
        panic!()
    };

    let mut pending: Vec<i32> = parallel.next.clone();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(root_id) = pending.pop() {
        if !seen.insert(root_id) {
            continue;
        }
        let root = roots
            .get(&root_id)
            .unwrap_or_else(|| panic!("missing root {root_id}"));
        assert!(!root.interactions.is_empty());
        for child in &root.interactions {
            assert_ne!(
                *child, INTERACTION_NOT_FOUND,
                "root `{:?}` contains a NOT_FOUND child",
                root.id
            );
            let node = interactions.get(child).unwrap_or_else(|| {
                panic!(
                    "root `{:?}` references missing interaction {child}",
                    root.id
                )
            });
            match node {
                ProtocolInteraction::SelectInteraction(select) => {
                    assert!(
                        select.hit_entity == INTERACTION_NOT_FOUND
                            || roots.contains_key(&select.hit_entity),
                        "selector {child} hit entity {} is not a root",
                        select.hit_entity
                    );
                    if select.hit_entity != INTERACTION_NOT_FOUND {
                        pending.push(select.hit_entity);
                    }
                }
                ProtocolInteraction::ReplaceInteraction(replace) => {
                    assert!(
                        replace.default_value == INTERACTION_NOT_FOUND
                            || roots.contains_key(&replace.default_value),
                        "replace {child} default {} is not a root",
                        replace.default_value
                    );
                    if replace.default_value != INTERACTION_NOT_FOUND {
                        pending.push(replace.default_value);
                    }
                }
                _ => {}
            }
        }
    }
}

#[test]
fn the_missing_id_sentinel_is_never_zero() {
    assert_eq!(INTERACTION_NOT_FOUND, ASSET_NOT_FOUND);
    assert_eq!(ASSET_NOT_FOUND, i32::MIN);
    assert_ne!(INTERACTION_NOT_FOUND, 0);
}

#[test]
fn absent_interaction_references_use_not_found() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let (packets, registry) = load_packets();
    let interactions: UpdateInteractions = decode(&packets);
    let interactions = interactions.interactions.expect("interaction map");

    // `*Change_Active_Slot` is preloaded first, so it is interaction 0 and has no
    // `Next`/`Failed`. Both fields must be NOT_FOUND, never a self-reference.
    match interactions.get(&0).expect("interaction 0") {
        ProtocolInteraction::SimpleInteraction(node) => {
            assert_eq!(node.next, INTERACTION_NOT_FOUND);
            assert_eq!(node.failed, INTERACTION_NOT_FOUND);
        }
        other => panic!("expected interaction 0 to be SimpleInteraction, got {other:?}"),
    }

    // Every typed `SimpleInteraction` reference must either stop or point at a
    // populated slot; a dangling id would make the client skip or loop.
    for (id, interaction) in &interactions {
        let ProtocolInteraction::SimpleInteraction(node) = interaction else {
            continue;
        };
        assert_ne!(node.next, *id, "interaction {id} points Next at itself");
        assert_ne!(node.failed, *id, "interaction {id} points Failed at itself");
        for (field, reference) in [("Next", node.next), ("Failed", node.failed)] {
            assert!(
                reference == INTERACTION_NOT_FOUND || interactions.contains_key(&reference),
                "interaction {id} {field} references missing slot {reference}"
            );
        }
    }

    let roots: UpdateRootInteractions = decode(&packets);
    let roots = roots.interactions.expect("root interaction map");
    for (id, root) in &roots {
        assert!(
            !root.interactions.is_empty(),
            "root interaction {id} has no children"
        );
        for reference in &root.interactions {
            // Java itself emits `NOT_FOUND` for placeholder interactions such
            // as the `Effect` variable in consumable roots; the client skips
            // those. The block-break chain has its own stricter audit below.
            assert!(
                *reference == INTERACTION_NOT_FOUND || interactions.contains_key(reference),
                "root interaction {id} references missing interaction slot {reference}"
            );
        }
    }

    for key in ["Unarmed_Attack", "UseBlock", "BreakBlock", "UseEntity"] {
        assert!(
            registry.interactions.id_of(key).is_some(),
            "empty-hand chain references missing interaction `{key}`"
        );
    }

    let unarmed: UpdateUnarmedInteractions = decode(&packets);
    let unarmed = unarmed.interactions.expect("unarmed map");
    for (interaction_type, root) in &unarmed {
        assert!(
            *root == INTERACTION_NOT_FOUND || roots.contains_key(root),
            "unarmed {interaction_type:?} references missing root interaction slot {root}"
        );
    }
    // Java's `UnarmedInteractions.Empty` defines the empty-hand primary/use/pick
    // chains; without them the client never sends block-break or pick
    // interactions. `SwapFrom` is the codec-added default.
    for interaction_type in [
        InteractionType::Primary,
        InteractionType::Use,
        InteractionType::Pick,
        InteractionType::Ability3,
        InteractionType::SwapFrom,
    ] {
        let root = unarmed
            .get(&interaction_type)
            .unwrap_or_else(|| panic!("unarmed map is missing {interaction_type:?}"));
        assert_ne!(
            *root, INTERACTION_NOT_FOUND,
            "unarmed {interaction_type:?} did not resolve to a root interaction"
        );
    }
    assert_eq!(unarmed.get(&InteractionType::SwapFrom), Some(&0));
}
