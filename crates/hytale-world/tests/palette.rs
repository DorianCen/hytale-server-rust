//! Palette type, promotion, and byte-layout tests.

use hytale_protocol::io::{Reader, Writer};
use hytale_world::chunk::palette::{
    ByteKey, KeyCodec, Palette, PaletteType, SetResult, ShortKey, BLOCK_COUNT,
};

fn ids(distinct: usize) -> Vec<i32> {
    (0..BLOCK_COUNT)
        .map(|index| (index % distinct) as i32)
        .collect()
}

fn assert_roundtrip(palette: &Palette, key: &dyn KeyCodec) {
    let mut packet = Writer::new();
    palette.serialize_for_packet(&mut packet);
    assert_eq!(packet.len(), palette.serialized_packet_size());
    let mut reader = Reader::new(packet.as_slice());
    let decoded =
        Palette::deserialize_for_packet(palette.kind(), &mut reader).expect("packet deserialize");
    assert_eq!(decoded.kind(), palette.kind());
    assert_eq!(decoded.entries(), palette.entries());
    for index in 0..BLOCK_COUNT {
        assert_eq!(decoded.get(index), palette.get(index), "index {index}");
    }

    let mut storage = Writer::new();
    palette
        .serialize(key, &mut storage)
        .expect("storage serialize");
    assert_eq!(storage.len(), palette.serialized_size(key));
    let mut reader = Reader::new(storage.as_slice());
    let decoded =
        Palette::deserialize(palette.kind(), key, &mut reader).expect("storage deserialize");
    assert_eq!(decoded.entries(), palette.entries());
    for index in 0..BLOCK_COUNT {
        assert_eq!(decoded.get(index), palette.get(index), "index {index}");
    }
}

#[test]
fn empty_palette_rejects_non_zero_and_serializes_to_nothing() {
    let mut palette = Palette::empty();
    assert_eq!(palette.kind(), PaletteType::Empty);
    assert_eq!(palette.count(), 1);
    assert_eq!(palette.count_of(0), BLOCK_COUNT);
    assert!(palette.is_solid(0));
    assert_eq!(palette.get(12_345), 0);
    assert_eq!(palette.set(12_345, 0), SetResult::Unchanged);
    assert_eq!(palette.set(12_345, 5), SetResult::RequiresPromote);
    assert_eq!(palette.serialized_packet_size(), 0);
    assert_eq!(palette.serialized_size(&ByteKey), 0);
    assert_roundtrip(&palette, &ByteKey);
}

#[test]
fn half_byte_promotes_when_all_sixteen_internal_ids_are_used() {
    let mut palette = Palette::with_kind(PaletteType::HalfByte);
    for index in 0..15 {
        assert_eq!(
            palette.set(index, (index + 1) as i32),
            SetResult::AddedOrRemoved
        );
    }
    assert_eq!(palette.count(), 16);
    assert_eq!(palette.kind(), PaletteType::HalfByte);
    assert_eq!(palette.set(15, 16), SetResult::RequiresPromote);

    palette.promote().expect("promote to byte");
    assert_eq!(palette.kind(), PaletteType::Byte);
    assert_eq!(palette.set(15, 16), SetResult::AddedOrRemoved);
    assert_eq!(palette.get(15), 16);
    assert_eq!(palette.get(14), 15);
    assert_roundtrip(&palette, &ByteKey);
}

#[test]
fn byte_promotes_after_256_distinct_ids() {
    let mut palette = Palette::with_kind(PaletteType::Byte);
    for index in 0..255 {
        assert_eq!(
            palette.set(index, (index + 1) as i32),
            SetResult::AddedOrRemoved
        );
    }
    assert_eq!(palette.count(), 256);
    assert_eq!(palette.set(255, 256), SetResult::RequiresPromote);

    palette.promote().expect("promote to short");
    assert_eq!(palette.kind(), PaletteType::Short);
    assert_eq!(palette.set(255, 256), SetResult::AddedOrRemoved);
    assert_eq!(palette.get(255), 256);
    assert_roundtrip(&palette, &ShortKey);
}

#[test]
fn from_ids_selects_the_java_thresholds() {
    assert_eq!(
        Palette::from_ids(&ids(1)).unwrap().kind(),
        PaletteType::Empty
    );
    assert_eq!(
        Palette::from_ids(&ids(15)).unwrap().kind(),
        PaletteType::HalfByte
    );
    assert_eq!(
        Palette::from_ids(&ids(16)).unwrap().kind(),
        PaletteType::Byte
    );
    assert_eq!(
        Palette::from_ids(&ids(255)).unwrap().kind(),
        PaletteType::Byte
    );
    assert_eq!(
        Palette::from_ids(&ids(256)).unwrap().kind(),
        PaletteType::Short
    );
    assert!(Palette::from_ids(&[0; 10]).is_err());
}

#[test]
fn generated_palettes_roundtrip_both_layouts() {
    assert_roundtrip(&Palette::from_generated_ids(&ids(1)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_generated_ids(&ids(3)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_generated_ids(&ids(20)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_generated_ids(&ids(300)).unwrap(), &ShortKey);
}

#[test]
fn set_order_palette_uses_the_recorded_first_use_order() {
    let mut values = vec![0; BLOCK_COUNT];
    values[10] = 9;
    values[11] = 5;
    values[12] = 9;
    let palette = Palette::from_ids_with_order(&values, &[5, 9]).unwrap();
    let entries = palette.entries();
    assert_eq!(entries[0].external, 0);
    assert_eq!(entries[1].external, 5);
    assert_eq!(entries[2].external, 9);
    for (index, value) in values.iter().enumerate() {
        assert_eq!(palette.get(index), *value);
    }
}

#[test]
fn all_types_roundtrip_both_layouts() {
    assert_roundtrip(&Palette::from_ids(&ids(1)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_ids(&ids(3)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_ids(&ids(20)).unwrap(), &ByteKey);
    assert_roundtrip(&Palette::from_ids(&ids(300)).unwrap(), &ShortKey);

    // A full section of one id, and alternating ids.
    assert_roundtrip(&Palette::from_ids(&vec![7; BLOCK_COUNT]).unwrap(), &ByteKey);
    let alternating: Vec<i32> = (0..BLOCK_COUNT)
        .map(|index| if index % 2 == 0 { 0 } else { 9 })
        .collect();
    assert_roundtrip(&Palette::from_ids(&alternating).unwrap(), &ByteKey);
}

#[test]
fn palette_table_order_is_deterministic() {
    let mut palette = Palette::from_ids(&ids(1)).unwrap();
    palette.promote().unwrap();
    palette.set(0, 7);
    palette.set(1, 5);
    palette.set(2, 7);
    palette.set(3, 9);
    let entries = palette.entries();
    assert_eq!(entries[0].external, 0);
    assert_eq!(entries[1].external, 7);
    assert_eq!(entries[2].external, 5);
    assert_eq!(entries[3].external, 9);
    assert_eq!((entries[1].internal, entries[2].internal), (1, 2));
}

#[test]
fn promote_preserves_block_data_and_demote_compacts_internal_ids() {
    let alternating: Vec<i32> = (0..BLOCK_COUNT)
        .map(|index| match index % 3 {
            0 => 0,
            1 => 5,
            _ => 11,
        })
        .collect();
    let mut palette = Palette::from_ids(&alternating).unwrap();
    assert_eq!(palette.kind(), PaletteType::HalfByte);
    palette.promote().unwrap();
    for (index, expected) in alternating.iter().enumerate() {
        assert_eq!(palette.get(index), *expected);
    }

    // Byte with 20 distinct ids cannot demote, then dropping to 14 can.
    let mut byte = Palette::from_ids(&ids(20)).unwrap();
    assert_eq!(byte.kind(), PaletteType::Byte);
    assert!(byte.demote().is_err());
    for index in 0..BLOCK_COUNT {
        let id = byte.get(index);
        if id >= 14 {
            byte.set(index, 0);
        }
    }
    assert!(byte.should_demote());
    byte.demote().expect("demote to half byte");
    assert_eq!(byte.kind(), PaletteType::HalfByte);
    for index in 0..BLOCK_COUNT {
        let expected = if (index % 20) >= 14 {
            0
        } else {
            (index % 20) as i32
        };
        assert_eq!(byte.get(index), expected, "index {index}");
    }
    assert_roundtrip(&byte, &ByteKey);
}

#[test]
fn byte_key_rejects_ids_that_do_not_fit() {
    let palette = Palette::from_ids(&ids(300)).unwrap();
    let mut writer = Writer::new();
    assert!(palette.serialize(&ByteKey, &mut writer).is_err());
}
