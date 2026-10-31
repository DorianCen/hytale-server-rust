//! Deterministic protocol fuzzing smoke test.
//!
//! This is the in-tree fuzz target used by `tools/security.sh`: it feeds random
//! bytes to framing/decompression and to the client-to-server decoders that the
//! session loop reaches. The budget is controlled by
//! `HYTALE_FUZZ_ITERATIONS` (default 10,000 for the fast test tier).
//!
//! A panic is a test failure; malformed input must always return `Err`.

use hytale_protocol::frame::{decode_frame, decompress_payload};
use hytale_protocol::generated::asseteditor::AssetEditorUpdateJsonAsset;
use hytale_protocol::generated::buildertools::BuilderToolArgUpdate;
use hytale_protocol::generated::connection::{
    ClientDisconnect, Connect, InsecurePlayerOptions, Ping,
};
use hytale_protocol::generated::inventory::{DropItemStack, MoveItemStack, SetActiveSlot};
use hytale_protocol::generated::machinima::RequestMachinimaActorModel;
use hytale_protocol::generated::player::{ClientMovement, ClientReady, MouseInteraction};
use hytale_protocol::generated::setup::{RequestAssets, SetupFinalize, ViewRadius};
use hytale_protocol::generated::voice::VoiceData;
use hytale_protocol::generated::window::{CloseWindow, SendWindowAction};
use hytale_protocol::generated::{Packet, Wire};
use hytale_protocol::io::{Reader, Result};

fn iterations() -> usize {
    std::env::var("HYTALE_FUZZ_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000)
        .clamp(1, 1_000_000)
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*, deterministic and dependency-free.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(len);
        while bytes.len() < len {
            bytes.extend_from_slice(&self.next().to_le_bytes());
        }
        bytes.truncate(len);
        bytes
    }
}

fn decode_known(packet_id: u32, bytes: &[u8]) -> Option<Result<()>> {
    let mut reader = Reader::new(bytes);
    Some(match packet_id {
        id if id == Connect::ID => Connect::decode(&mut reader).map(|_| ()),
        id if id == ClientDisconnect::ID => ClientDisconnect::decode(&mut reader).map(|_| ()),
        id if id == Ping::ID => Ping::decode(&mut reader).map(|_| ()),
        id if id == InsecurePlayerOptions::ID => {
            InsecurePlayerOptions::decode(&mut reader).map(|_| ())
        }
        id if id == ClientMovement::ID => ClientMovement::decode(&mut reader).map(|_| ()),
        id if id == ClientReady::ID => ClientReady::decode(&mut reader).map(|_| ()),
        id if id == MouseInteraction::ID => MouseInteraction::decode(&mut reader).map(|_| ()),
        id if id == MoveItemStack::ID => MoveItemStack::decode(&mut reader).map(|_| ()),
        id if id == DropItemStack::ID => DropItemStack::decode(&mut reader).map(|_| ()),
        id if id == SetActiveSlot::ID => SetActiveSlot::decode(&mut reader).map(|_| ()),
        id if id == SendWindowAction::ID => SendWindowAction::decode(&mut reader).map(|_| ()),
        id if id == CloseWindow::ID => CloseWindow::decode(&mut reader).map(|_| ()),
        id if id == RequestAssets::ID => RequestAssets::decode(&mut reader).map(|_| ()),
        id if id == SetupFinalize::ID => SetupFinalize::decode(&mut reader).map(|_| ()),
        id if id == ViewRadius::ID => ViewRadius::decode(&mut reader).map(|_| ()),
        id if id == RequestMachinimaActorModel::ID => {
            RequestMachinimaActorModel::decode(&mut reader).map(|_| ())
        }
        id if id == BuilderToolArgUpdate::ID => {
            BuilderToolArgUpdate::decode(&mut reader).map(|_| ())
        }
        id if id == VoiceData::ID => VoiceData::decode(&mut reader).map(|_| ()),
        id if id == AssetEditorUpdateJsonAsset::ID => {
            AssetEditorUpdateJsonAsset::decode(&mut reader).map(|_| ())
        }
        _ => return None,
    })
}

#[test]
fn random_frames_and_client_packets_never_panic() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let budget = iterations();
    for iteration in 0..budget {
        let len = (rng.next() as usize) % 768;
        let bytes = rng.bytes(len);

        let _ = decode_frame(&bytes, 4096);
        let _ = decompress_payload(&bytes, 4096);
        for packet_id in [
            Connect::ID,
            ClientDisconnect::ID,
            Ping::ID,
            InsecurePlayerOptions::ID,
            ClientMovement::ID,
            ClientReady::ID,
            MouseInteraction::ID,
            MoveItemStack::ID,
            DropItemStack::ID,
            SetActiveSlot::ID,
            SendWindowAction::ID,
            CloseWindow::ID,
            RequestAssets::ID,
            SetupFinalize::ID,
            ViewRadius::ID,
            RequestMachinimaActorModel::ID,
            BuilderToolArgUpdate::ID,
            VoiceData::ID,
            AssetEditorUpdateJsonAsset::ID,
        ] {
            let _ = decode_known(packet_id, &bytes);
        }

        // Also exercise the first bytes of a valid frame's length prefix.
        if iteration.is_multiple_of(97) {
            let mut framed = bytes;
            framed.extend_from_slice(&[0xff; 8]);
            let _ = decode_frame(&framed, 4096);
        }
    }
}
