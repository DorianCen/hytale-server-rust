//! Full-protocol registry and round-trip gate.
//!
//! `registry.rs` is generated from `protocol.json`; these tests make sure the
//! table is complete, ids are unique, every family can round-trip a default
//! value, and the frame header checks reject mismatched traffic.

use hytale_protocol::generated::connection::Connect;
use hytale_protocol::generated::registry::{
    check_all_samples, check_family_samples, packet_info, settings, validate_packet, PACKETS,
};
use hytale_protocol::generated::wire::{NetworkChannel, Packet, PacketDirection};
use hytale_protocol::io::ProtocolError;

#[test]
fn registry_has_every_packet_with_unique_ascending_ids() {
    assert_eq!(PACKETS.len(), 389);
    assert_eq!(PACKETS.len(), settings::PACKET_COUNT);

    let mut previous = None;
    for info in PACKETS {
        if let Some(previous) = previous {
            assert!(
                info.id > previous,
                "packet ids must be sorted and unique: {} then {}",
                previous,
                info.id
            );
        }
        previous = Some(info.id);
        assert!(!info.name.is_empty());
    }

    let families = [
        "asseteditor",
        "assets",
        "auth",
        "buildertools",
        "camera",
        "connection",
        "entities",
        "interaction",
        "interface_",
        "inventory",
        "machinima",
        "player",
        "serveraccess",
        "setup",
        "stream",
        "voice",
        "window",
        "world",
        "worldmap",
    ];
    let seen: std::collections::BTreeSet<&str> = PACKETS.iter().map(|info| info.family).collect();
    assert_eq!(seen.len(), families.len());
    for family in families {
        assert!(seen.contains(family), "missing packet family {family}");
    }
}

#[test]
fn one_default_packet_round_trips_per_family() {
    check_family_samples().expect("every packet family must round-trip");
}

#[test]
fn every_packet_round_trips_with_its_default_value() {
    check_all_samples().expect("every generated packet must round-trip");
}

#[test]
fn protocol_settings_match_the_java_constants() {
    assert_eq!(settings::CRC, -386_561_135);
    assert_eq!(settings::VERSION, 3);
    assert_eq!(settings::BUILD_NUMBER, 258);
    assert_eq!(settings::PACKET_COUNT, 389);
    assert_eq!(settings::STRUCT_COUNT, 445);
    assert_eq!(settings::ENUM_COUNT, 177);
    assert_eq!(settings::MAX_PACKET_SIZE, 1_677_721_600);
}

#[test]
fn packet_lookup_and_header_validation() {
    let info = packet_info(Connect::ID).expect("Connect is registered");
    assert_eq!(info.name, "Connect");
    assert_eq!(info.direction, PacketDirection::ToServer);
    assert_eq!(info.channel, NetworkChannel::Default);
    assert!(!info.compressed);

    assert!(matches!(
        validate_packet(9_999, PacketDirection::ToServer, NetworkChannel::Default, 0),
        Err(ProtocolError::UnknownPacket { packet_id: 9_999 })
    ));
    assert!(matches!(
        validate_packet(
            Connect::ID,
            PacketDirection::ToClient,
            NetworkChannel::Default,
            32
        ),
        Err(ProtocolError::WrongDirection { .. })
    ));
    assert!(matches!(
        validate_packet(
            Connect::ID,
            PacketDirection::ToServer,
            NetworkChannel::Chunks,
            32
        ),
        Err(ProtocolError::WrongChannel { .. })
    ));
    assert!(matches!(
        validate_packet(
            Connect::ID,
            PacketDirection::ToServer,
            NetworkChannel::Default,
            usize::MAX
        ),
        Err(ProtocolError::PayloadTooLarge { .. })
    ));
    assert!(validate_packet(
        Connect::ID,
        PacketDirection::ToServer,
        NetworkChannel::Default,
        32
    )
    .is_ok());
}
