//! World-thread view changes: demand ticks, unloads and events.

use std::path::PathBuf;

use hytale_server::{WorldEvent, WorldThread, WorldThreadOptions};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-world-thread-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn ticks_the_view_and_unloads_chunks_that_leave_it() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("view");
    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    // Defaults: radius 0 around the player chunk, one chunk per tick.
    let (thread, start) = WorldThread::spawn(options).expect("spawn the world thread");
    assert_eq!(start.initial_loads, 1);

    let mut generated = 0;
    loop {
        let tick = thread.tick().expect("tick");
        generated += tick.generated;
        if tick.remaining == 0 {
            break;
        }
    }
    assert_eq!(generated, 1);
    assert_eq!(
        thread.try_event(),
        Some(WorldEvent::ChunkLoaded {
            x: start.player_chunk.0,
            z: start.player_chunk.1,
            generated: true,
        })
    );

    // Move one chunk away: the old chunk leaves the view and is saved/unloaded,
    // and the new center is queued instead.
    let view = thread
        .set_view(start.player_chunk.0 + 1, start.player_chunk.1, 0)
        .expect("set view");
    assert_eq!(view.queued, 1);
    assert_eq!(view.unloaded, 1);

    let tick = thread.tick().expect("tick the new view");
    assert_eq!(tick.generated, 1);
    assert_eq!(tick.remaining, 0);

    let mut unloaded = false;
    let mut loaded_new = false;
    while let Some(event) = thread.try_event() {
        match event {
            WorldEvent::ChunkUnloaded { x, z } => {
                unloaded = true;
                assert_eq!((x, z), start.player_chunk);
            }
            WorldEvent::ChunkLoaded { x, z, generated } => {
                assert!(generated);
                if (x, z) == (start.player_chunk.0 + 1, start.player_chunk.1) {
                    loaded_new = true;
                }
            }
        }
    }
    assert!(unloaded);
    assert!(loaded_new);

    assert!(thread.shutdown().expect("shutdown") >= 1);
    std::fs::remove_dir_all(save_dir).ok();
}
