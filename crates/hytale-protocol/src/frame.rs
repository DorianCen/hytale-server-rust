//! Packet framing and zstd payload handling.
//!
//! Wire format: `[u32 LE payload length][u32 LE packet id][payload]`. The
//! length covers the payload only, not the packet id. Compressed packets carry
//! a zstd frame whose header declares the uncompressed size.

use crate::io::{ProtocolError, Result};

/// Protocol-wide maximum payload size (`ProtocolSettings.MAX_PACKET_SIZE`).
pub const MAX_PACKET_SIZE: u32 = 1_677_721_600;

/// Bytes before the payload: length plus packet id.
pub const PACKET_HEADER_SIZE: usize = 8;

/// A decoded frame borrowing its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    /// Packet id.
    pub packet_id: u32,
    /// Packet payload (compressed when the registry says so).
    pub payload: &'a [u8],
}

/// Try to decode one frame.
///
/// Returns `Ok(None)` when the input does not yet contain a complete frame, so
/// a streaming decoder can wait for more bytes. Invalid lengths are rejected
/// as soon as they are visible.
pub fn decode_frame(input: &[u8], max_packet_payload: u32) -> Result<Option<(Frame<'_>, usize)>> {
    if input.len() < PACKET_HEADER_SIZE {
        return Ok(None);
    }
    let length = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
    let packet_id = u32::from_le_bytes([input[4], input[5], input[6], input[7]]);

    // Java reads an int; values with the sign bit set are negative lengths.
    if length & 0x8000_0000 != 0 || length > MAX_PACKET_SIZE {
        return Err(ProtocolError::InvalidFrameLength {
            length,
            max: MAX_PACKET_SIZE,
        });
    }
    if length > max_packet_payload {
        return Err(ProtocolError::PayloadTooLarge {
            packet_id,
            length,
            max: max_packet_payload,
        });
    }

    let total = PACKET_HEADER_SIZE + length as usize;
    if input.len() < total {
        return Ok(None);
    }
    Ok(Some((
        Frame {
            packet_id,
            payload: &input[PACKET_HEADER_SIZE..total],
        },
        total,
    )))
}

/// Encode one frame into `out`.
pub fn encode_frame(packet_id: u32, payload: &[u8], out: &mut Vec<u8>) -> Result<()> {
    if payload.len() as u64 > u64::from(MAX_PACKET_SIZE) {
        return Err(ProtocolError::PayloadTooLarge {
            packet_id,
            length: payload.len() as u32,
            max: MAX_PACKET_SIZE,
        });
    }
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&packet_id.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

/// Compress a packet payload with zstd.
pub fn compress_payload(payload: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::bulk::compress(payload, level)
        .map_err(|error| ProtocolError::Compression(error.to_string()))
}

/// Decompress a zstd packet payload.
///
/// The frame must declare its uncompressed size, and that size must not exceed
/// `max_size`. The declared size is cross-checked against the actual output.
pub fn decompress_payload(payload: &[u8], max_size: usize) -> Result<Vec<u8>> {
    let content_size = zstd::zstd_safe::get_frame_content_size(payload).map_err(|error| {
        ProtocolError::Decompression(format!("invalid frame header: {error:?}"))
    })?;
    let Some(content_size) = content_size else {
        return Err(ProtocolError::MissingContentSize);
    };
    if content_size > max_size as u64 {
        return Err(ProtocolError::DecompressedTooLarge {
            length: content_size,
            max: max_size,
        });
    }
    let output = zstd::bulk::decompress(payload, content_size as usize)
        .map_err(|error| ProtocolError::Decompression(error.to_string()))?;
    if output.len() as u64 != content_size {
        return Err(ProtocolError::Decompression(format!(
            "declared {content_size} bytes but decoded {}",
            output.len()
        )));
    }
    Ok(output)
}
