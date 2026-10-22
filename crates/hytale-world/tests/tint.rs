//! Column tint palette layout.

use hytale_world::chunk::tint::TintPalette;

#[test]
fn a_new_palette_serializes_to_one_zero_key() {
    let palette = TintPalette::new();
    let bytes = palette.serialize();
    // u16 key count, one i32 key, i32 packed length, then 1280 packed bytes.
    assert_eq!(bytes.len(), 2 + 4 + 4 + 1280);
    assert_eq!(&bytes[..2], &1u16.to_le_bytes());
    assert_eq!(&bytes[2..6], &0i32.to_le_bytes());
    assert_eq!(&bytes[6..10], &1280i32.to_le_bytes());
    assert!(bytes[10..].iter().all(|byte| *byte == 0));
}

#[test]
fn keys_keep_first_insertion_order_and_round_trip() {
    let mut palette = TintPalette::new();
    palette.set(3, 7, 0x1122_3344);
    palette.set(0, 0, 0x5566_7788);
    palette.set(31, 31, 0x1122_3344);

    assert_eq!(palette.get(3, 7), 0x1122_3344);
    assert_eq!(palette.get(0, 0), 0x5566_7788);
    assert_eq!(palette.get(31, 31), 0x1122_3344);

    let bytes = palette.serialize();
    let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
    assert_eq!(count, 3);
    assert_eq!(&bytes[2..6], &0i32.to_le_bytes());
    let key = |offset: usize| {
        i32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    assert_eq!(key(2), 0);
    assert_eq!(key(6), 0x1122_3344);
    assert_eq!(key(10), 0x5566_7788);
    assert_eq!(TintPalette::deserialize(&bytes), Some(palette));
}
