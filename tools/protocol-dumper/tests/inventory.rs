//! Inventory parser tests: tiny generated-shaped fixtures plus a malformed file.

use std::path::{Path, PathBuf};

use protocol_dumper::inventory;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn tiny_fixture_classifies_every_kind() {
    let inventory = inventory::build(&fixture("inventory")).expect("fixture parses");
    assert_eq!(inventory.meta.counts.total, 3);
    assert_eq!(inventory.meta.counts.packets, 1);
    assert_eq!(inventory.meta.counts.structs, 1);
    assert_eq!(inventory.meta.counts.enums, 1);
    assert_eq!(inventory.meta.counts.other, 0);

    let packet = inventory
        .classes
        .iter()
        .find(|class| class.name == "TinyPacket")
        .expect("packet present");
    assert_eq!(packet.kind, "packet");
    assert_eq!(packet.package, "com.example");
    let metadata = packet.packet.as_ref().expect("packet metadata");
    assert_eq!(metadata.id, 42);
    assert_eq!(metadata.direction, "ToServer");
    assert_eq!(metadata.channel, "Chunks");
    assert!(metadata.compressed);
    assert_eq!(metadata.fixed_block_size, 5);
    assert_eq!(metadata.variable_field_count, 1);
    assert_eq!(metadata.variable_block_start, 9);

    let structure = inventory
        .classes
        .iter()
        .find(|class| class.name == "TinyStruct")
        .expect("struct present");
    assert_eq!(structure.kind, "struct");

    let enumeration = inventory
        .classes
        .iter()
        .find(|class| class.name == "TinyEnum")
        .expect("enum present");
    assert_eq!(enumeration.kind, "enum");

    // Output is sorted by package and name for stable diffs.
    let names: Vec<&str> = inventory
        .classes
        .iter()
        .map(|class| class.name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn malformed_file_is_an_error_not_a_skip() {
    let error = inventory::build(&fixture("malformed")).expect_err("must fail");
    let message = error.to_string();
    assert!(message.contains("Broken.java"), "{message}");
}
