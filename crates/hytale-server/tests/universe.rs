//! Universe runtime: two concurrent worlds, edits persisted on shutdown.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_server::{GeneratorOptions, UniverseRuntime};
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::universe::{Universe, WorldProviderKind};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-universe-runtime-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn generator() -> GeneratorOptions {
    GeneratorOptions {
        assets_pack: repo_root().join("Assets/Assets.zip"),
        assets_root: repo_root().join("Assets/extracted"),
        world_dir: repo_root().join("testdata/Default"),
        workers: 1,
    }
}

fn tick_until_loaded(runtime: &UniverseRuntime, name: &str) -> usize {
    let thread = runtime.world_thread(name).expect("live thread");
    let mut generated = 0;
    loop {
        let tick = thread.tick().expect("tick");
        generated += tick.generated;
        if tick.remaining == 0 {
            return generated;
        }
    }
}

#[test]
fn two_worlds_edit_and_reload_with_clean_shutdown() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let root = temp_root("two-worlds");
    let mut runtime = UniverseRuntime::open(&root, generator()).expect("open universe");
    let alpha = runtime
        .create_and_start("Alpha", 11, WorldProviderKind::Default)
        .expect("start Alpha");
    let beta = runtime
        .create_and_start("Beta", 22, WorldProviderKind::Default)
        .expect("start Beta");
    assert_eq!(runtime.running_worlds().len(), 2);

    let mut edits = Vec::new();
    for (name, start) in [("Alpha", &alpha), ("Beta", &beta)] {
        assert_eq!(tick_until_loaded(&runtime, name), 1, "{name}: generated");
        let x = start.player_chunk.0 * 32 + 1;
        let z = start.player_chunk.1 * 32 + 1;
        let (before, after) = runtime
            .world_thread(name)
            .unwrap()
            .set_block(x, 64, z, 5)
            .expect("set block");
        assert_ne!(before, 5, "{name}: original block");
        assert_eq!(after, 5, "{name}: new block");
        edits.push((name, x, 64, z));
    }

    assert_eq!(runtime.save_world("Alpha").expect("save Alpha"), 1);
    assert_eq!(runtime.stop_world("Alpha").expect("stop Alpha"), 0);
    assert_eq!(runtime.stop_world("Beta").expect("stop Beta"), 1);
    assert!(runtime.running_worlds().is_empty());
    assert_eq!(runtime.shutdown().expect("shutdown"), 0);

    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open assets");
    let assets = AssetRegistry::load_phase2(&pack).expect("load assets");
    let codec = ChunkCodec::new(&assets);
    let reopened = Universe::open(&root).expect("reopen universe");
    assert_eq!(reopened.list_worlds().len(), 2);
    for (name, x, y, z) in edits {
        let mut save = reopened.open_world(name).expect("open world");
        assert!(
            save.world_mut()
                .load_chunk(&codec, x >> 5, z >> 5)
                .expect("load chunk"),
            "{name}: saved chunk exists"
        );
        assert_eq!(save.world().get_block(x, y, z), 5, "{name}: edit persisted");
    }

    std::fs::remove_dir_all(root).ok();
}
