//! Regression coverage for interaction ids sent to the release client.
//!
//! Java's `AssetMapWithIndexes.NOT_FOUND` (`i32::MIN`) is the "no interaction"
//! sentinel. Encoding it as `0` makes the client follow interaction 0 instead of
//! stopping; because `*Change_Active_Slot` is id 0 and has no `Next`, that node
//! then points at itself and the client overflows its stack while resolving the
//! chain. The release client dies right after `PrepareWorldMap` with a SIGSEGV
//! when this happens.

use std::path::PathBuf;

use hytale_assets::packets::INTERACTION_NOT_FOUND;
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

fn load_packets() -> Vec<AssetPacket> {
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load phase 2");
    registry
        .config_assets()
        .expect("encode config assets")
        .packets
}

fn decode<T: Packet + Wire>(packets: &[AssetPacket]) -> T {
    let packet = packets
        .iter()
        .find(|packet| packet.id == T::ID)
        .unwrap_or_else(|| panic!("missing packet {}", T::ID));
    T::decode(&mut Reader::new(&packet.payload)).expect("decode packet")
}

#[test]
fn absent_interaction_references_use_not_found() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let packets = load_packets();
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
        for reference in &root.interactions {
            assert!(
                *reference == INTERACTION_NOT_FOUND || interactions.contains_key(reference),
                "root interaction {id} references missing interaction slot {reference}"
            );
        }
    }

    let unarmed: UpdateUnarmedInteractions = decode(&packets);
    let swap = unarmed
        .interactions
        .expect("unarmed map")
        .get(&InteractionType::SwapFrom)
        .copied()
        .expect("SwapFrom binding");
    assert!(
        swap == INTERACTION_NOT_FOUND || roots.contains_key(&swap),
        "unarmed SwapFrom references missing root interaction slot {swap}"
    );
}
