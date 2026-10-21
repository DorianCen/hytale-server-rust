//! ChunkColumn lazy sections and GeneratedChunk conversion.

use hytale_math::chunk_util;
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::palette::PaletteType;
use worldgen_model::chunk::GeneratedChunk;

#[test]
fn generated_chunk_converts_lazily_and_losslessly() {
    let mut generated = GeneratedChunk::with_coordinates(0x1234, -3, 7);
    generated.set_block(1, 5, 2, 42, 3, 7);
    generated.set_block(31, 319, 31, 99, 4, 8);
    generated.set_fluid(4, 70, 5, 6, 9);
    generated.set_tint(3, 7, 0x1122_3344);
    generated.set_environment_column(2, 3, 17);
    generated.set_environment(2, 40, 3, 23);

    let column = ChunkColumn::from_generated_chunk(&generated).expect("convert");
    assert_eq!(column.x(), -3);
    assert_eq!(column.z(), 7);
    assert_eq!(column.index(), 0x1234);
    assert!(column.flags().newly_generated);

    assert_eq!(column.get_block(1, 5, 2), 42);
    assert_eq!(column.get_rotation(1, 5, 2), 3);
    assert_eq!(column.get_filler(1, 5, 2), 7);
    assert_eq!(column.get_block(31, 319, 31), 99);
    assert_eq!(column.get_fluid_id(4, 70, 5), 6);
    assert_eq!(column.get_fluid_level(4, 70, 5), 9);
    assert_eq!(column.tint(3, 7), 0x1122_3344);
    assert_eq!(column.environment_column(2, 3), 17);
    assert_eq!(column.get_environment(2, 0, 3), 17);
    assert_eq!(column.get_environment(2, 40, 3), 23);
    assert_eq!(column.get_environment(2, 41, 3), 17);
}

#[test]
fn only_sections_with_data_are_allocated() {
    let mut generated = GeneratedChunk::new();
    generated.set_block(0, 5, 0, 12, 0, 0);
    let column = ChunkColumn::from_generated_chunk(&generated).expect("convert");
    assert!(column.has_section(5));
    for y in (0..320).step_by(32) {
        if y != 0 {
            assert!(!column.has_section(y), "section at {y} should be empty");
        }
    }
}

#[test]
fn mixed_ids_promote_and_preserve_every_block() {
    let mut generated = GeneratedChunk::new();
    for index in 0..32 * 32 * 32 {
        let x = index & 31;
        let z = (index >> 5) & 31;
        let y = index >> 10;
        generated.set_block(x, y, z, index % 300 + 1, index % 5, index % 9);
    }
    let column = ChunkColumn::from_generated_chunk(&generated).expect("convert");
    let section = column.section(0).expect("section");
    assert_eq!(section.block.blocks().kind(), PaletteType::Short);
    for index in 0..32 * 32 * 32 {
        let x = index & 31;
        let z = (index >> 5) & 31;
        let y = index >> 10;
        assert_eq!(column.get_block(x, y, z), index % 300 + 1);
        assert_eq!(column.get_rotation(x, y, z), index % 5);
        assert_eq!(column.get_filler(x, y, z), index % 9);
    }
}

#[test]
fn index_math_masks_negative_and_local_coordinates() {
    let mut column = ChunkColumn::new(0, 0);
    column.set_block(-1, 5, -1, 33, 1, 2).expect("set");

    let expected = chunk_util::index_block(-1, 5, -1);
    assert_eq!(column.get_block(-1, 5, -1), 33);
    assert_eq!(column.get_block(31, 5, 31), 33);
    assert_eq!(expected, chunk_util::index_block(31, 5, 31));
    assert_eq!(column.get_rotation(-1, 5, -1), 1);
    assert_eq!(column.get_filler(-1, 5, -1), 2);

    assert!(!column.has_section(320));
    assert_eq!(column.get_block(0, 320, 0), 0);
}
