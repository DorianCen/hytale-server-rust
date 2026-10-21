//! BlockSection storage/packet round-trips, ticking and palette promotion.

use hytale_world::chunk::palette::BLOCK_COUNT;
use hytale_world::chunk::section::{BlockSection, LightData};

fn sample() -> BlockSection {
    let mut section = BlockSection::new();
    section.set_block_migration_asset_count(77);
    for index in 0..600 {
        section
            .set(
                index,
                (index % 20) as i32,
                (index % 5) as i32,
                (index % 9) as i32,
            )
            .expect("set block");
    }
    section.set(17, 300, 7, 42).expect("wide block id");
    section.set_ticking(5, true);
    section.set_ticking(600, true);
    section.set_ticking_wait_adjacent(7, true);
    section.merge_ticking_blocks();
    section.set_sky_lit_stamp(0x0102_0304_0506_0708);
    let mut local = LightData::empty(section.local_change_counter());
    local.set(0, 0x1234);
    local.set(17, 0xABCD);
    let mut global = LightData::empty(section.global_change_counter());
    global.set(2, 0x0F0F);
    section.set_local_light(local);
    section.set_global_light(global);
    section
}

#[test]
fn storage_round_trip_preserves_blocks_ticking_light_and_counters() {
    let section = sample();
    let bytes = section.serialize().expect("serialize");
    let decoded = BlockSection::deserialize(&bytes).expect("deserialize");
    assert_eq!(decoded, section);
    assert_eq!(decoded.block_migration_asset_count(), 77);
    assert_eq!(decoded.get(17), 300);
    assert_eq!(decoded.get_filler(17), 42);
    assert_eq!(decoded.get_rotation_index(17), 7);
    assert!(decoded.is_ticking(5));
    assert!(decoded.is_ticking(600));
    assert_eq!(decoded.local_light().get(17), 0xABCD);
    assert_eq!(decoded.global_light().get(2), 0x0F0F);
    assert_eq!(decoded.sky_lit_stamp(), 0x0102_0304_0506_0708);
}

#[test]
fn packet_round_trip_preserves_block_layers() {
    let section = sample();
    let data = section.serialize_for_packet();
    let decoded = BlockSection::deserialize_for_packet(&data).expect("packet decode");
    for index in 0..1000 {
        assert_eq!(decoded.get(index), section.get(index), "block {index}");
        assert_eq!(
            decoded.get_filler(index),
            section.get_filler(index),
            "filler {index}"
        );
        assert_eq!(
            decoded.get_rotation_index(index),
            section.get_rotation_index(index),
            "rotation {index}"
        );
    }
    let packet = section.to_packet().expect("packet");
    assert_eq!(packet.data, data);
    assert!(packet.local_light.is_some());
    assert!(packet.global_light.is_some());
}

#[test]
fn promotion_preserves_mixed_ids() {
    let mut section = BlockSection::new();
    for index in 0..BLOCK_COUNT {
        let id = (index % 300) as i32;
        section
            .set(index, id, (index % 6) as i32, (index % 13) as i32)
            .expect("set");
    }
    let decoded =
        BlockSection::deserialize(&section.serialize().expect("serialize")).expect("decode");
    for index in 0..BLOCK_COUNT {
        assert_eq!(decoded.get(index), (index % 300) as i32);
        assert_eq!(decoded.get_filler(index), (index % 13) as i32);
        assert_eq!(decoded.get_rotation_index(index), (index % 6) as i32);
    }
}

#[test]
fn empty_section_serializes_without_ticking_or_light() {
    let section = BlockSection::new();
    let decoded =
        BlockSection::deserialize(&section.serialize().expect("serialize")).expect("decode");
    assert_eq!(decoded, section);
    assert!(decoded.local_light().is_empty());
    assert_eq!(decoded.ticking_blocks_count(), 0);
}
