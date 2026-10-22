//! Headless boot gate: load the base-game pack, generate a seed-0 chunk,
//! convert it to the runtime column and save a Java-compatible region blob.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_server::{
    run_chunk_tool, run_headless, run_world_boot, ChunkToolOptions, HeadlessOptions,
    WorldBootOptions,
};
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

    // Load through the runtime world storage, mutate one block, save and
    // verify the reload independently of the generation path.
    let block_id = assets
        .block_types
        .id_of("Rock_Stone")
        .expect("Rock_Stone is in the asset registry");
    let tool = run_chunk_tool(&ChunkToolOptions {
        assets_pack: options.assets_pack.clone(),
        save_dir: save_dir.clone(),
        chunk_x: options.chunk_x,
        chunk_z: options.chunk_z,
        set_block: Some((
            options.chunk_x * 32 + 1,
            250,
            options.chunk_z * 32 + 1,
            block_id,
        )),
    })
    .expect("load, mutate and save the chunk");
    assert!(tool.verified);
    assert_eq!(tool.block_before, Some(0));
    assert_eq!(tool.block_after, Some(block_id));
    assert_eq!(tool.non_air_blocks, non_air + 1);

    // Boot the complete save: load/create config and player, load the stored
    // chunk, edit another block and write all three back.
    let boot = run_world_boot(&WorldBootOptions {
        assets_pack: options.assets_pack.clone(),
        assets_root: options.assets_root.clone(),
        world_dir: options.world_dir.clone(),
        save_dir: save_dir.clone(),
        workers: options.workers,
        view_radius: 0,
        chunks_per_tick: 8,
        seed: i64::from(options.seed),
        display_name: Some("Headless Gate".to_string()),
        world_uuid: None,
        player_uuid: None,
        set_block: Some((
            options.chunk_x * 32 + 2,
            251,
            options.chunk_z * 32 + 2,
            block_id,
        )),
    })
    .expect("boot the complete save");
    assert!(boot.config_created);
    assert!(boot.player_created);
    assert_eq!(boot.stored_chunks, 1);
    assert_eq!(boot.requested_chunks, 1);
    assert_eq!(boot.loaded_chunks, 1);
    assert_eq!(boot.generated_chunks, 1);
    assert_eq!(boot.saved_chunks, 2);
    assert_eq!(
        boot.block_edit,
        Some((
            options.chunk_x * 32 + 2,
            251,
            options.chunk_z * 32 + 2,
            0,
            block_id
        ))
    );
    assert!(save_dir.join("config.json").is_file());
    assert!(save_dir
        .join("players")
        .join(format!("{}.json", boot.player_uuid))
        .is_file());

    std::fs::remove_dir_all(save_dir).ok();
}
