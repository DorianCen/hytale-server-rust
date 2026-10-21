//! Shared client-bot logic: Connect construction, framing and packet logging.
//!
//! The binary is intentionally small: it sends one `Connect`, then logs framed
//! packets until the server disconnects or the timeout expires. Packet handlers
//! and the full join flow land in later tasks.

use hytale_protocol::frame::{decode_frame, encode_frame, MAX_PACKET_SIZE};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, NetworkTick, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::{ProtocolError, Reader, Writer};

/// Java `ProtocolSettings.PROTOCOL_CRC`.
pub const DEFAULT_PROTOCOL_CRC: i32 = -386_561_135;
/// Java `ProtocolSettings.PROTOCOL_BUILD_NUMBER`.
pub const DEFAULT_PROTOCOL_BUILD: i32 = 258;
/// Java `ProtocolSettings.PROTOCOL_VERSION`.
pub const DEFAULT_PROTOCOL_VERSION: i32 = 3;

/// Build the initial client `Connect` packet.
pub fn build_connect(
    protocol_crc: i32,
    protocol_build: i32,
    client_version: &str,
    language: &str,
    identity_token: Option<String>,
) -> Connect {
    Connect {
        protocol_crc,
        protocol_build_number: protocol_build,
        client_version: client_version.to_string(),
        client_type: ClientType::Game,
        identity_token,
        language: language.to_string(),
        referral_data: None,
        referral_source: None,
    }
}

/// Encode one packet payload.
pub fn encode_payload<T: Wire>(packet: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = Writer::new();
    packet.encode(&mut writer)?;
    Ok(writer.into_inner())
}

/// Encode any generated packet as a complete frame (`[len][id][payload]`).
pub fn build_frame<T: Packet>(packet: &T) -> Result<Vec<u8>, ProtocolError> {
    let payload = encode_payload(packet)?;
    let mut frame = Vec::new();
    encode_frame(T::ID, &payload, &mut frame)?;
    Ok(frame)
}

/// Encode `Connect` as a complete frame (`[len][id][payload]`).
pub fn build_connect_frame(connect: &Connect) -> Result<Vec<u8>, ProtocolError> {
    build_frame(connect)
}

/// A fully owned frame copied out of a stream buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRecord {
    /// Packet id.
    pub packet_id: u32,
    /// Payload bytes.
    pub payload: Vec<u8>,
}

impl FrameRecord {
    /// Name of a packet in the connection family, or `None` for other families.
    pub fn name(&self) -> Option<&'static str> {
        connection_packet_name(self.packet_id)
    }

    /// Decode a known connection-family packet into a readable line.
    pub fn describe(&self) -> Option<String> {
        decode_connection_packet(self.packet_id, &self.payload).map(|value| match value {
            Ok(description) => description,
            Err(error) => format!("decode error: {error}"),
        })
    }
}

/// Drain every complete frame from `buffer`, leaving an incomplete tail.
pub fn drain_frames(buffer: &mut Vec<u8>) -> Result<Vec<FrameRecord>, ProtocolError> {
    let mut frames = Vec::new();
    while let Some((frame, consumed)) = decode_frame(buffer, MAX_PACKET_SIZE)? {
        frames.push(FrameRecord {
            packet_id: frame.packet_id,
            payload: frame.payload.to_vec(),
        });
        buffer.drain(..consumed);
    }
    Ok(frames)
}

/// Name of a connection-family packet.
pub fn connection_packet_name(packet_id: u32) -> Option<&'static str> {
    Some(match packet_id {
        0 => "Connect",
        1 => "ClientDisconnect",
        2 => "ServerDisconnect",
        3 => "Ping",
        4 => "Pong",
        5 => "NetworkTick",
        363 => "InsecurePlayerOptions",
        364 => "RequestInsecurePlayerOptions",
        _ => return None,
    })
}

/// Decode a connection-family packet payload.
pub fn decode_connection_packet(
    packet_id: u32,
    payload: &[u8],
) -> Option<Result<String, ProtocolError>> {
    let mut reader = Reader::new(payload);
    Some(match packet_id {
        0 => Connect::decode(&mut reader).map(|value| format!("{value:?}")),
        1 => ClientDisconnect::decode(&mut reader).map(|value| format!("{value:?}")),
        2 => ServerDisconnect::decode(&mut reader).map(|value| format!("{value:?}")),
        3 => Ping::decode(&mut reader).map(|value| format!("{value:?}")),
        4 => Pong::decode(&mut reader).map(|value| format!("{value:?}")),
        5 => NetworkTick::decode(&mut reader).map(|value| format!("{value:?}")),
        363 => InsecurePlayerOptions::decode(&mut reader).map(|value| format!("{value:?}")),
        364 => RequestInsecurePlayerOptions::decode(&mut reader).map(|_| "{}".to_string()),
        _ => return None,
    })
}

/// Hex helper for `--dry-run` output.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_frame_round_trips_through_the_framer() {
        let connect = build_connect(
            DEFAULT_PROTOCOL_CRC,
            DEFAULT_PROTOCOL_BUILD,
            "2026.01.01",
            "en-US",
            None,
        );
        let frame = build_connect_frame(&connect).expect("frame");
        let mut buffer = frame;
        let records = drain_frames(&mut buffer).expect("drain");
        assert_eq!(records.len(), 1);
        assert!(buffer.is_empty());
        assert_eq!(records[0].packet_id, 0);
        assert_eq!(records[0].name(), Some("Connect"));
        let mut reader = Reader::new(&records[0].payload);
        let decoded = Connect::decode(&mut reader).expect("decode");
        assert_eq!(decoded, connect);
    }

    #[test]
    fn drain_frames_keeps_an_incomplete_tail() {
        let connect = build_connect(1, 2, "v", "en-US", None);
        let first = build_connect_frame(&connect).expect("first frame");
        let mut second = build_connect_frame(&connect).expect("second frame");
        let tail = second.split_off(second.len() - 2);
        let mut buffer = first;
        buffer.extend_from_slice(&second);

        let records = drain_frames(&mut buffer).expect("partial drain");
        assert_eq!(records.len(), 1);
        assert!(!buffer.is_empty());

        buffer.extend_from_slice(&tail);
        let records = drain_frames(&mut buffer).expect("complete drain");
        assert_eq!(records.len(), 1);
        assert!(buffer.is_empty());
    }
}
