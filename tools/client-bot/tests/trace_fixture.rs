//! Fast validation of the committed v1 lifecycle trace fixture.
//!
//! This does not need the asset pack or networking: it checks that the fixture
//! is internally consistent with the generated protocol registry and still
//! contains the real-client-required milestones. The live comparison is
//! `tests/trace_gate.rs` (ignored because it generates a world).

use std::collections::BTreeSet;
use std::path::PathBuf;

use hytale_net::{read_trace, TraceDirection, TraceStream};
use hytale_protocol::generated::registry::packet_info;
use hytale_protocol::generated::wire::NetworkChannel;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden/server/trace/v1-lifecycle.jsonl")
}

#[test]
fn the_committed_trace_fixture_is_registry_consistent() {
    let trace = read_trace(fixture_path()).expect("read the committed trace");
    assert_eq!(trace.header.format, hytale_net::TRACE_FORMAT_VERSION);
    assert_eq!(trace.header.protocol_crc, -386_561_135);
    assert_eq!(trace.header.protocol_build, 258);
    assert!(!trace.records.is_empty());

    for record in &trace.records {
        let info = packet_info(record.packet_id)
            .unwrap_or_else(|| panic!("unknown packet id {}", record.packet_id));
        assert_eq!(
            record.packet_name.as_deref(),
            Some(info.name),
            "packet name drift for id {}",
            record.packet_id
        );
        let expected_channel = match info.channel {
            NetworkChannel::Default => "Default",
            NetworkChannel::Chunks => "Chunks",
            NetworkChannel::WorldMap => "WorldMap",
            NetworkChannel::Voice => "Voice",
        };
        assert_eq!(
            record.channel.as_deref(),
            Some(expected_channel),
            "channel drift for {}",
            info.name
        );
        match record.stream {
            TraceStream::Setup => assert_eq!(info.channel, NetworkChannel::Default),
            TraceStream::Chunks => assert_eq!(info.channel, NetworkChannel::Chunks),
        }
        assert!(
            record.payload_sha256.is_empty() || record.payload_sha256.len() == 64,
            "malformed payload hash for {}",
            info.name
        );
    }
}

#[test]
fn the_fixture_covers_the_v1_lifecycle_and_real_client_constraints() {
    let trace = read_trace(fixture_path()).expect("read the committed trace");
    let sessions: BTreeSet<u32> = trace.records.iter().map(|record| record.session).collect();
    assert_eq!(sessions, BTreeSet::from([1, 2, 3]));

    // The real client log classified ViewRadius (id 32) as a Chunks-channel
    // packet after `ClientReady`; a Default-channel write disconnects it during
    // loading.
    let view_radius = trace
        .records
        .iter()
        .find(|record| record.packet_name.as_deref() == Some("ViewRadius"))
        .expect("ViewRadius in the trace");
    assert_eq!(view_radius.packet_id, 32);
    assert_eq!(view_radius.stream, TraceStream::Chunks);
    assert_eq!(view_radius.channel.as_deref(), Some("Chunks"));

    // Config assets arrive in the Java dependency order before JoinWorld.
    let config_ids: Vec<u32> = trace
        .records
        .iter()
        .filter(|record| {
            record.session == 1
                && record.direction == TraceDirection::Outbound
                && record.stream == TraceStream::Setup
        })
        .map(|record| record.packet_id)
        .filter(|id| {
            matches!(
                *id,
                41 | 87 | 65 | 42 | 63 | 83 | 61 | 84 | 40 | 44 | 45 | 43 | 55 | 57 | 56 | 52 | 54
            )
        })
        .collect();
    assert_eq!(
        config_ids,
        vec![41, 87, 65, 42, 63, 83, 61, 84, 40, 44, 45, 43, 55, 57, 56, 52, 54]
    );

    for milestone in [
        "JoinWorld",
        "ClientTeleport",
        "UpdatePlayerInventory",
        "SetActiveSlot",
        "AddToServerPlayerList",
        "RemoveFromServerPlayerList",
        "EntityUpdates",
        "ServerSetBlock",
        "SetColumn",
        "SetChunk",
        "SetChunkEnvironments",
    ] {
        assert!(
            trace
                .records
                .iter()
                .any(|record| record.packet_name.as_deref() == Some(milestone)),
            "missing lifecycle milestone {milestone}"
        );
    }

    let block_updates: Vec<_> = trace
        .records
        .iter()
        .filter(|record| record.packet_name.as_deref() == Some("ServerSetBlock"))
        .collect();
    assert!(
        block_updates.len() >= 2,
        "place and reconnect-break expected"
    );
    assert!(block_updates
        .iter()
        .all(|record| record.stream == TraceStream::Chunks));
}
