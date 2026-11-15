//! T76 release-client placement trace seam.

use std::path::PathBuf;

use hytale_server::{PlaceBlockOptions, WorldThread, WorldThreadOptions};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-placement-trace-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn client_placements_write_the_rotation_trace_record() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("save");
    let trace = temp_dir("trace").with_extension("jsonl");
    std::env::set_var("HYTALE_PLACEMENT_TRACE", &trace);
    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, start) = WorldThread::spawn(options).expect("spawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    let block = thread
        .block_type_id("Deco_Bone_Pile")
        .expect("resolve bone pile")
        .expect("bone pile block");
    let (x, y, z) = (
        start.player_chunk.0 * 32 + 8,
        200,
        start.player_chunk.1 * 32 + 8,
    );
    let mut placement = PlaceBlockOptions::new(3);
    placement.client_rotation = true;
    assert!(thread
        .place_block_with_options(x, y, z, block, placement)
        .expect("client-authored placement")
        .is_some());
    thread.shutdown().expect("shutdown");
    std::env::remove_var("HYTALE_PLACEMENT_TRACE");

    let line = std::fs::read_to_string(&trace)
        .expect("trace file")
        .lines()
        .next()
        .expect("one trace record")
        .to_string();
    let record: serde_json::Value = serde_json::from_str(&line).expect("trace JSON");
    assert_eq!(record["block_key"], "Deco_Bone_Pile");
    assert_eq!(record["client_rotation"], true);
    assert_eq!(record["requested_rotation"], 3);
    assert_eq!(record["final_rotation"], 3);
    assert_eq!(record["random_rotation"], "YawStep1");
    assert_eq!(record["x"], x);
    assert_eq!(record["y"], y);
    assert_eq!(record["z"], z);

    std::fs::remove_file(trace).ok();
    std::fs::remove_dir_all(save_dir).ok();
}
