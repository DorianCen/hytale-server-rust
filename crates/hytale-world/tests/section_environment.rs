//! EnvironmentSection storage/packet round-trips and run override behavior.

use hytale_world::chunk::section::EnvironmentSection;

#[test]
fn uniform_storage_and_packet_round_trip() {
    let section = EnvironmentSection::uniform(17);
    let decoded = EnvironmentSection::deserialize(&section.serialize().expect("serialize"))
        .expect("deserialize");
    assert_eq!(decoded, section);
    let packet = section.serialize_for_packet().expect("packet");
    let packet_decoded =
        EnvironmentSection::deserialize_for_packet(&packet).expect("packet decode");
    assert_eq!(packet_decoded, section);
}

#[test]
fn column_overrides_survive_storage_and_packet() {
    let mut section = EnvironmentSection::uniform(3);
    assert!(!section.set(1, 5, 2, 3));
    assert!(section.set(1, 5, 2, 9));
    assert!(section.set_range(1, 2, 10, 20, 11).expect("range"));
    assert!(section.set_column(4, 7, 23));
    for y in 0..32 {
        assert_eq!(
            section.get(1, y, 2),
            if (10..=20).contains(&y) {
                11
            } else if y == 5 {
                9
            } else {
                3
            }
        );
    }
    assert_eq!(section.get(4, 31, 7), 23);

    let storage = section.serialize().expect("serialize");
    let decoded = EnvironmentSection::deserialize(&storage).expect("deserialize");
    assert_eq!(decoded, section);
    let packet = section.serialize_for_packet().expect("packet");
    let packet_decoded =
        EnvironmentSection::deserialize_for_packet(&packet).expect("packet decode");
    assert_eq!(packet_decoded, section);
}

#[test]
fn uniform_section_detects_contains_and_runs() {
    let section = EnvironmentSection::uniform(8);
    assert!(section.contains(8));
    assert!(!section.contains(9));
    assert_eq!(section.column_runs(0, 0), vec![(31, 8)]);

    let mut columns = EnvironmentSection::new();
    assert!(columns.set(0, 3, 0, 5));
    assert_eq!(columns.column_runs(0, 0), vec![(2, 0), (3, 5), (31, 0)]);
}
