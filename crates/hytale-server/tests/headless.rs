//! Headless boot gate: load the base-game pack, generate a seed-0 chunk,
//! convert it to the runtime column and save a Java-compatible region blob.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_server::{run_headless, HeadlessOptions};
use hytale_world::chunk::column::SECTION_COUNT;
use hytale_world::chunk::storage::{chunk_blob_index, ChunkCodec, RegionFile};
use hytale_world::chunk::NO_HEIGHT;

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn assets_available() -> bool {
    manifest_root().join("../../Assets/Assets.zip").is_file()
        && manifest_root()
            .join("../../Assets/extracted/Server/Prefabs")
            .is_dir()
}

#[test]
fn headless_boot_saves_a_reloadable_region_blob() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let repo = manifest_root().join("../..");
    let save_dir = std::env::temp_dir().join(format!(
        "hytale-headless-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let options = HeadlessOptions {
        chunk_x: 4,
        chunk_z: -5,
        ..HeadlessOptions::for_repo(&repo, &save_dir)
    };

    let report = run_headless(&options).expect("headless boot");
    assert!(report.verified);
    assert!(report.non_air_blocks > 0);
    assert!(report.document_bytes > 0);
    assert_eq!(
        report.blob_index,
        chunk_blob_index(options.chunk_x, options.chunk_z)
    );
    assert!(report.region_file.is_file());

    // Decode the committed-format file with a fresh registry, independently of
    // the in-memory column the boot verified against.
    let pack = AssetPack::open(&options.assets_pack).expect("open the pack");
    let assets = AssetRegistry::load_p0(&pack).expect("load the P0 registry");
    let codec = ChunkCodec::new(&assets);
    let mut region = RegionFile::open_read_only(&report.region_file).expect("open the region");
    let document = region
        .read_chunk(options.chunk_x, options.chunk_z)
        .expect("read the chunk")
        .expect("the chunk is present");
    let column = codec
        .decode_at(&document, options.chunk_x, options.chunk_z)
        .expect("decode the chunk");
    assert_eq!(column.x(), options.chunk_x);
    assert_eq!(column.z(), options.chunk_z);
    assert!(column
        .heightmap()
        .serialize_to_ints()
        .expect("legacy heightmap")
        .iter()
        .any(|height| *height != NO_HEIGHT));
    let non_air = (0..SECTION_COUNT as i32)
        .flat_map(|section| (0..32).map(move |y| section * 32 + y))
        .map(|y| {
            (0..32)
                .flat_map(|z| (0..32).map(move |x| (x, z)))
                .filter(|(x, z)| column.get_block(*x, y, *z) != 0)
                .count()
        })
        .sum::<usize>();
    assert!(non_air > 0);

    std::fs::remove_dir_all(save_dir).ok();
}
