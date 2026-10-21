//! Connection-family round-trip and malformed-input tests.

use std::collections::BTreeMap;

use hytale_protocol::generated::common::{
    BoolParamValue, FormattedMessage, FormattedMessageImage, HostAddress, InstantData,
    IntParamValue, ParamValue, PlayerSkin, StringParamValue,
};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientDisconnectReason, ClientType, Connect, DisconnectType, NetworkTick,
    Ping, Pong, PongType, RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::{Reader, Writer};

fn roundtrip<T>(value: &T) -> T
where
    T: Wire + PartialEq + std::fmt::Debug,
{
    let mut writer = Writer::new();
    value.encode(&mut writer).expect("encode");
    assert_eq!(
        writer.len(),
        value.compute_size().expect("compute size"),
        "computed size must match encoded size"
    );

    let mut reader = Reader::new(writer.as_slice());
    let decoded = T::decode(&mut reader).expect("decode");
    assert!(reader.is_empty(), "decode must consume exactly one value");
    assert_eq!(&decoded, value);
    decoded
}

fn sample_connect() -> Connect {
    Connect {
        protocol_crc: 0x1234_5678,
        protocol_build_number: 258,
        client_version: "2026.01.01".to_string(),
        client_type: ClientType::Game,
        identity_token: Some("identity-token".to_string()),
        language: "en-US".to_string(),
        referral_data: Some(vec![1, 2, 3, 4]),
        referral_source: Some(HostAddress {
            host: "example.com".to_string(),
            port: 5520,
        }),
    }
}

#[test]
fn connect_round_trip_with_all_nullable_fields() {
    let decoded = roundtrip(&sample_connect());
    assert_eq!(decoded.protocol_crc, 0x1234_5678);
    assert_eq!(decoded.client_type, ClientType::Game);
    assert_eq!(decoded.referral_source.expect("source").port, 5520);
    assert_eq!(Connect::ID, 0);
}

#[test]
fn connect_round_trip_without_nullable_fields() {
    let value = Connect {
        identity_token: None,
        referral_data: None,
        referral_source: None,
        ..sample_connect()
    };
    let decoded = roundtrip(&value);
    assert!(decoded.identity_token.is_none());
    assert!(decoded.referral_data.is_none());
    assert!(decoded.referral_source.is_none());

    // The absent optional fields have -1 offset slots. The fixed block is 30
    // bytes plus four 4-byte slots, then the non-nullable language string.
    let mut writer = Writer::new();
    value.encode(&mut writer).expect("encode");
    assert_eq!(&writer.as_slice()[30..34], &(-1i32).to_le_bytes());
    assert_eq!(&writer.as_slice()[38..42], &(-1i32).to_le_bytes());
    assert_eq!(&writer.as_slice()[42..46], &(-1i32).to_le_bytes());
}

#[test]
fn connect_empty_and_maximum_strings_and_arrays() {
    let empty = Connect {
        client_version: String::new(),
        identity_token: Some(String::new()),
        language: String::new(),
        referral_data: Some(Vec::new()),
        ..sample_connect()
    };
    roundtrip(&empty);

    let max = Connect {
        client_version: "v".repeat(20),
        identity_token: Some("i".repeat(8192)),
        language: "l".repeat(100),
        referral_data: Some(vec![7; 4096]),
        ..sample_connect()
    };
    roundtrip(&max);

    let too_long = Connect {
        identity_token: Some("i".repeat(8193)),
        ..sample_connect()
    };
    let mut writer = Writer::new();
    assert!(too_long.encode(&mut writer).is_err());
}

#[test]
fn ping_pong_and_network_tick_round_trip() {
    let instant = InstantData {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    };
    roundtrip(&Ping {
        id: 42,
        time: instant.clone(),
        last_ping_value_raw: 10,
        last_ping_value_direct: 11,
        last_ping_value_tick: 12,
    });
    roundtrip(&Pong {
        id: 42,
        time: instant,
        r#type: PongType::Tick,
        packet_queue_size: 7,
    });
    roundtrip(&NetworkTick {
        tick_duration_micros: 50_000,
        server_tick: 99,
    });
}

#[test]
fn disconnect_packets_round_trip() {
    roundtrip(&ClientDisconnect {
        reason: ClientDisconnectReason::UserLeave,
        r#type: DisconnectType::Crash,
    });

    let mut params = BTreeMap::new();
    params.insert(
        "name".to_string(),
        ParamValue::StringParamValue(StringParamValue {
            value: Some("Steve".to_string()),
        }),
    );
    params.insert(
        "count".to_string(),
        ParamValue::IntParamValue(IntParamValue { value: 3 }),
    );
    params.insert(
        "enabled".to_string(),
        ParamValue::BoolParamValue(BoolParamValue { value: true }),
    );

    let mut message_params = BTreeMap::new();
    message_params.insert(
        "nested".to_string(),
        FormattedMessage {
            raw_text: Some("nested".to_string()),
            ..Default::default()
        },
    );

    let reason = FormattedMessage {
        raw_text: Some("goodbye".to_string()),
        message_id: Some("server.disconnect".to_string()),
        children: Some(vec![FormattedMessage {
            raw_text: Some("child".to_string()),
            ..Default::default()
        }]),
        params: Some(params),
        message_params: Some(message_params),
        color: Some("#ff0000".to_string()),
        bold: Some(true),
        italic: None,
        monospace: Some(false),
        underlined: None,
        strikethrough: Some(false),
        link: Some("https://example.com".to_string()),
        markup_enabled: true,
        image: Some(FormattedMessageImage {
            file_path: "Images/icon.png".to_string(),
            width: 16,
            height: 32,
        }),
    };

    let decoded = roundtrip(&ServerDisconnect {
        reason: Some(reason),
        r#type: DisconnectType::Disconnect,
    });
    assert!(decoded.reason.is_some());

    roundtrip(&ServerDisconnect {
        reason: None,
        r#type: DisconnectType::Disconnect,
    });
}

#[test]
fn insecure_player_options_round_trip_with_player_skin() {
    let skin = PlayerSkin {
        body_characteristic: Some("default".to_string()),
        underwear: Some("u".to_string()),
        face: Some("f".to_string()),
        eyes: Some("e".to_string()),
        ears: Some("ears".to_string()),
        mouth: Some("m".to_string()),
        facial_hair: None,
        haircut: Some("h".to_string()),
        eyebrows: Some("b".to_string()),
        pants: Some("p".to_string()),
        overpants: None,
        undertop: Some("ut".to_string()),
        overtop: Some("ot".to_string()),
        shoes: Some("s".to_string()),
        head_accessory: None,
        face_accessory: Some("fa".to_string()),
        ear_accessory: None,
        skin_feature: Some("sf".to_string()),
        gloves: None,
        cape: Some("c".to_string()),
    };
    roundtrip(
        &hytale_protocol::generated::connection::InsecurePlayerOptions {
            uuid: hytale_protocol::io::Uuid::new(0x0102_0304_0506_0708, 0x1112_1314_1516_1718),
            username: "Steve".to_string(),
            skin: Some(skin),
        },
    );

    roundtrip(&RequestInsecurePlayerOptions {});
}

#[test]
fn malformed_input_returns_errors() {
    let bad_enum = [0xFFu8];
    let mut reader = Reader::new(&bad_enum);
    assert!(ClientDisconnect::decode(&mut reader).is_err());

    // Truncated fixed block.
    let mut reader = Reader::new(&[0u8; 4]);
    assert!(Connect::decode(&mut reader).is_err());

    // Bad varint in a variable field.
    let mut writer = Writer::new();
    sample_connect().encode(&mut writer).expect("encode");
    let mut bytes = writer.into_inner();
    bytes[46] = 0xFF;
    bytes[47] = 0xFF;
    let mut reader = Reader::new(&bytes);
    assert!(Connect::decode(&mut reader).is_err());

    // Non-canonical offset slot.
    let mut writer = Writer::new();
    sample_connect().encode(&mut writer).expect("encode");
    let mut bytes = writer.into_inner();
    bytes[30..34].copy_from_slice(&999i32.to_le_bytes());
    let mut reader = Reader::new(&bytes);
    assert!(Connect::decode(&mut reader).is_err());
}
