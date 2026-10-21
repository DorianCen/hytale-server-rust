//! Wire primitive, framing, and zstd tests.

use hytale_protocol::frame::{
    compress_payload, decode_frame, decompress_payload, encode_frame, MAX_PACKET_SIZE,
};
use hytale_protocol::io::{NullableBitset, ProtocolError, Reader, Uuid, Writer};

fn roundtrip_varint(value: u32, expected: &[u8]) {
    let mut writer = Writer::new();
    writer.write_varint(value).unwrap();
    assert_eq!(writer.as_slice(), expected, "encoding {value}");
    let mut reader = Reader::new(writer.as_slice());
    assert_eq!(reader.read_varint().unwrap(), value);
    assert!(reader.is_empty());
}

#[test]
fn varint_boundary_values() {
    roundtrip_varint(0, &[0x00]);
    roundtrip_varint(127, &[0x7F]);
    roundtrip_varint(128, &[0x80, 0x01]);
    roundtrip_varint(16_383, &[0xFF, 0x7F]);
    roundtrip_varint(16_384, &[0x80, 0x80, 0x01]);
    roundtrip_varint(2_097_151, &[0xFF, 0xFF, 0x7F]);
    roundtrip_varint(2_097_152, &[0x80, 0x80, 0x80, 0x01]);
    roundtrip_varint(268_435_455, &[0xFF, 0xFF, 0xFF, 0x7F]);
    roundtrip_varint(268_435_456, &[0x80, 0x80, 0x80, 0x80, 0x01]);
    roundtrip_varint(i32::MAX as u32, &[0xFF, 0xFF, 0xFF, 0xFF, 0x07]);
}

#[test]
fn varint_rejects_truncation_and_overflow() {
    assert!(matches!(
        Reader::new(&[0x80]).read_varint(),
        Err(ProtocolError::UnexpectedEof { .. })
    ));
    assert!(matches!(
        Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80]).read_varint(),
        Err(ProtocolError::InvalidVarInt(_))
    ));
    assert!(matches!(
        Reader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x08]).read_varint(),
        Err(ProtocolError::InvalidVarInt(_))
    ));
}

#[test]
fn fixed_types_are_little_endian() {
    let mut writer = Writer::new();
    writer.write_u8(0x12);
    writer.write_i8(-2);
    writer.write_u16(0x3456);
    writer.write_i16(-3);
    writer.write_u32(0x789A_BCDE);
    writer.write_i32(-4);
    writer.write_u64(0x0102_0304_0506_0708);
    writer.write_i64(-5);
    writer.write_f32(1.5);
    writer.write_f64(-2.25);
    writer.write_bool(true);
    writer.write_bool(false);
    assert_eq!(
        writer.as_slice(),
        [
            0x12, 0xFE, 0x56, 0x34, 0xFD, 0xFF, 0xDE, 0xBC, 0x9A, 0x78, 0xFC, 0xFF, 0xFF, 0xFF,
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xFB, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0x00, 0x00, 0xC0, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xC0,
            0x01, 0x00
        ]
    );

    let mut reader = Reader::new(writer.as_slice());
    assert_eq!(reader.read_u8().unwrap(), 0x12);
    assert_eq!(reader.read_i8().unwrap(), -2);
    assert_eq!(reader.read_u16().unwrap(), 0x3456);
    assert_eq!(reader.read_i16().unwrap(), -3);
    assert_eq!(reader.read_u32().unwrap(), 0x789A_BCDE);
    assert_eq!(reader.read_i32().unwrap(), -4);
    assert_eq!(reader.read_u64().unwrap(), 0x0102_0304_0506_0708);
    assert_eq!(reader.read_i64().unwrap(), -5);
    assert_eq!(reader.read_f32().unwrap(), 1.5);
    assert_eq!(reader.read_f64().unwrap(), -2.25);
    assert!(reader.read_bool().unwrap());
    assert!(!reader.read_bool().unwrap());
    assert!(reader.is_empty());
}

#[test]
fn uuid_uses_big_endian_long_order() {
    let uuid = Uuid::new(0x0102_0304_0506_0708, 0x1112_1314_1516_1718);
    let mut writer = Writer::new();
    writer.write_uuid(uuid);
    assert_eq!(
        writer.as_slice(),
        [1, 2, 3, 4, 5, 6, 7, 8, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]
    );
    assert_eq!(Reader::new(writer.as_slice()).read_uuid().unwrap(), uuid);
}

#[test]
fn nullable_bits_cover_every_position() {
    let mut bits = NullableBitset::new(3);
    for index in 0..24 {
        bits.set(index, index % 3 == 0).unwrap();
    }
    assert_eq!(bits.as_bytes(), &[0b0100_1001, 0b1001_0010, 0b0010_0100]);
    for index in 0..24 {
        assert_eq!(bits.is_set(index).unwrap(), index % 3 == 0, "index {index}");
    }
    assert!(bits.is_set(24).is_err());
}

#[test]
fn fixed_strings_are_zero_padded_and_bounded() {
    let mut writer = Writer::new();
    writer
        .write_fixed_ascii_string("version", "1.2.3", 8)
        .unwrap();
    assert_eq!(writer.as_slice(), b"1.2.3\0\0\0");
    let mut reader = Reader::new(writer.as_slice());
    assert_eq!(
        reader.read_fixed_ascii_string("version", 8).unwrap(),
        "1.2.3"
    );

    assert!(writer
        .write_fixed_ascii_string("version", "012345678", 8)
        .is_err());
    assert!(writer
        .write_fixed_ascii_string("version", "\u{e9}", 8)
        .is_err());
}

#[test]
fn variable_strings_and_arrays_are_bounded() {
    let mut writer = Writer::new();
    writer.write_var_string("token", "héllo", 32).unwrap();
    writer
        .write_var_ascii_string("language", "en-US", 16)
        .unwrap();
    writer
        .write_array("bytes", &[1u8, 2, 3], 4, |writer, value| {
            writer.write_u8(*value);
            Ok(())
        })
        .unwrap();

    let mut reader = Reader::new(writer.as_slice());
    assert_eq!(reader.read_var_string("token", 32).unwrap(), "héllo");
    assert_eq!(
        reader.read_var_ascii_string("language", 16).unwrap(),
        "en-US"
    );
    let values = reader
        .read_array("bytes", 4, |reader| reader.read_u8())
        .unwrap();
    assert_eq!(values, vec![1, 2, 3]);
    assert!(reader.is_empty());

    // Declared lengths above the bound are rejected before allocation.
    let too_long = [0xFF, 0x7F, b'x', b'x'];
    assert!(matches!(
        Reader::new(&too_long).read_var_string("token", 16),
        Err(ProtocolError::LengthTooLarge { .. })
    ));
    let too_many = [0x05, 1, 2, 3, 4, 5];
    assert!(matches!(
        Reader::new(&too_many).read_array("bytes", 4, |reader| reader.read_u8()),
        Err(ProtocolError::LengthTooLarge { .. })
    ));

    // Invalid UTF-8 and non-ASCII in an ASCII field are errors, not replacements.
    let bad_utf8 = [0x02, 0xC3, 0x28];
    assert!(matches!(
        Reader::new(&bad_utf8).read_var_string("token", 16),
        Err(ProtocolError::InvalidUtf8 { .. })
    ));
    let bad_ascii = [0x02, 0xC3, 0xA9];
    assert!(matches!(
        Reader::new(&bad_ascii).read_var_ascii_string("language", 16),
        Err(ProtocolError::InvalidAscii { .. })
    ));
}

#[test]
fn truncated_reads_are_errors() {
    let mut reader = Reader::new(&[1, 2]);
    assert!(matches!(
        reader.read_u32(),
        Err(ProtocolError::UnexpectedEof {
            needed: 4,
            remaining: 2
        })
    ));
}

#[test]
fn frame_roundtrip_and_partial_input() {
    let mut bytes = Vec::new();
    encode_frame(42, b"hello", &mut bytes).unwrap();
    assert_eq!(&bytes[..4], &[5, 0, 0, 0]);
    assert_eq!(&bytes[4..8], &[42, 0, 0, 0]);

    for split in 0..bytes.len() {
        assert!(
            decode_frame(&bytes[..split], 64).unwrap().is_none(),
            "split {split} should be incomplete"
        );
    }
    let (frame, consumed) = decode_frame(&bytes, 64).unwrap().expect("complete");
    assert_eq!(consumed, bytes.len());
    assert_eq!(frame.packet_id, 42);
    assert_eq!(frame.payload, b"hello");

    // A zero-length payload is valid; trailing bytes are left for the next frame.
    let mut zero = Vec::new();
    encode_frame(7, &[], &mut zero).unwrap();
    let mut with_trailing = zero.clone();
    with_trailing.push(0xAA);
    let (frame, consumed) = decode_frame(&with_trailing, 0).unwrap().expect("complete");
    assert_eq!(consumed, 8);
    assert!(frame.payload.is_empty());
    assert_eq!(with_trailing[consumed], 0xAA);
}

#[test]
fn frame_rejects_invalid_lengths() {
    let mut negative = Vec::new();
    negative.extend_from_slice(&0x8000_0001u32.to_le_bytes());
    negative.extend_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        decode_frame(&negative, MAX_PACKET_SIZE),
        Err(ProtocolError::InvalidFrameLength { .. })
    ));

    let mut oversized = Vec::new();
    oversized.extend_from_slice(&(MAX_PACKET_SIZE + 1).to_le_bytes());
    oversized.extend_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        decode_frame(&oversized, MAX_PACKET_SIZE),
        Err(ProtocolError::InvalidFrameLength { .. })
    ));

    let mut over_packet_max = Vec::new();
    over_packet_max.extend_from_slice(&100u32.to_le_bytes());
    over_packet_max.extend_from_slice(&9u32.to_le_bytes());
    assert!(matches!(
        decode_frame(&over_packet_max, 16),
        Err(ProtocolError::PayloadTooLarge {
            packet_id: 9,
            length: 100,
            max: 16
        })
    ));
}

#[test]
fn zstd_roundtrip_and_bounds() {
    let payload = b"hytale".repeat(1_000);
    let compressed = compress_payload(&payload, 3).unwrap();
    assert!(compressed.len() < payload.len());
    let decompressed = decompress_payload(&compressed, payload.len()).unwrap();
    assert_eq!(decompressed, payload);

    assert!(matches!(
        decompress_payload(&compressed, payload.len() - 1),
        Err(ProtocolError::DecompressedTooLarge { .. })
    ));

    // A zstd frame always declares its content size, including the empty case.
    let compressed = compress_payload(&[], 3).unwrap();
    assert_eq!(
        decompress_payload(&compressed, 0).unwrap(),
        Vec::<u8>::new()
    );
    assert!(matches!(
        decompress_payload(b"not a zstd frame", 16),
        Err(ProtocolError::Decompression(_))
    ));
}
