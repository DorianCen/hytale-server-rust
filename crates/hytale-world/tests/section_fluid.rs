//! FluidSection storage/packet round-trips and nibble-level packing.

use hytale_world::chunk::palette::BLOCK_COUNT;
use hytale_world::chunk::section::FluidSection;

#[test]
fn fluid_storage_and_packet_round_trip() {
    let mut section = FluidSection::new();
    assert!(section.is_empty());
    for index in 0..BLOCK_COUNT {
        let fluid = (index % 17) as i32 + 1;
        let level = (index % 16) as i8;
        section.set_fluid(index, fluid, level).expect("set");
    }
    let storage = section.serialize().expect("serialize");
    let decoded = FluidSection::deserialize(&storage).expect("deserialize");
    let packet = section.serialize_for_packet();
    let packet_decoded = FluidSection::deserialize_for_packet(&packet).expect("packet decode");

    for index in 0..BLOCK_COUNT {
        let expected_level = if (index % 16) == 0 {
            0
        } else {
            (index % 16) as i8
        };
        let expected_fluid = if expected_level == 0 {
            0
        } else {
            (index % 17) as i32 + 1
        };
        assert_eq!(decoded.get_fluid_id(index), expected_fluid, "id {index}");
        assert_eq!(
            decoded.get_fluid_level(index),
            expected_level,
            "level {index}"
        );
        assert_eq!(
            packet_decoded.get_fluid_id(index),
            expected_fluid,
            "packet id {index}"
        );
        assert_eq!(
            packet_decoded.get_fluid_level(index),
            expected_level,
            "packet level {index}"
        );
    }
}

#[test]
fn zero_levels_clear_the_level_buffer() {
    let mut section = FluidSection::new();
    section.set_fluid(0, 5, 3).expect("set");
    assert!(!section.serialize().expect("serialize").is_empty());
    section.set_fluid(0, 5, 0).expect("clear level");
    assert!(section.is_empty());
    let storage = section.serialize().expect("serialize");
    let decoded = FluidSection::deserialize(&storage).expect("deserialize");
    assert_eq!(decoded.get_fluid_id(0), 0);
    assert_eq!(decoded.get_fluid_level(0), 0);
}

#[test]
fn all_levels_survive_and_empty_fluid_forces_zero() {
    let mut section = FluidSection::new();
    for level in 0..=15i8 {
        section.set_fluid(level as usize, 9, level).expect("set");
    }
    for level in 0..=15i8 {
        let expected = if level == 0 { 0 } else { level };
        assert_eq!(section.get_fluid_level(level as usize), expected);
    }
    section.set_fluid(6, 0, 15).expect("empty fluid");
    assert_eq!(section.get_fluid_id(6), 0);
    assert_eq!(section.get_fluid_level(6), 0);
}
