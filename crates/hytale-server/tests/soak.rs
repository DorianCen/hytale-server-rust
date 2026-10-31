//! T63 soak: repeated synthetic joins, edits, ticks and saves.
//!
//! The test keeps one world thread alive and cycles player registrations and
//! block edits through it, then checks that the view/registry are clean and
//! that resident memory did not grow past the configured allowance.
//!
//! ```sh
//! HYTALE_SOAK_CYCLES=100 cargo test -p hytale-server --test soak -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use hytale_protocol::io::Uuid;
use hytale_server::{
    player_channel_with_capacity, resident_bytes, GeneratorOptions, PlayerJoinRequest,
    PlayerRegistry, WorldThread, WorldThreadOptions,
};
use hytale_world::player_data::{PlayerAppearance, PlayerTransform, Position, Rotation};

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
    std::env::temp_dir().join(format!("hytale-soak-{}-{nonce}-{name}", std::process::id()))
}

fn transform() -> PlayerTransform {
    PlayerTransform::new(
        Position {
            x: 0.0,
            y: 200.0,
            z: 0.0,
        },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    )
}

fn cycles() -> usize {
    std::env::var("HYTALE_SOAK_CYCLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100)
        .max(1)
}

fn rss_limit_bytes() -> u64 {
    let mib = std::env::var("HYTALE_SOAK_RSS_LIMIT_MIB")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(64u64);
    mib.saturating_mul(1024 * 1024)
}

#[test]
#[ignore = "loads the asset pack and runs the soak budget; run explicitly with --ignored"]
fn repeated_joins_edits_and_saves_do_not_leak() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("save");
    let mut options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    options.view_radius = 1;
    options.chunks_per_tick = 8;

    let (thread, start) = WorldThread::spawn(options).expect("world thread");
    for _ in 0..32 {
        thread.tick().expect("initial load tick");
        while thread.try_event().is_some() {}
    }

    let registry = PlayerRegistry::new();
    let registry = std::sync::Arc::new(registry);
    let cycles = cycles();
    let started = Instant::now();
    let mut baseline = None;

    for cycle in 0..cycles {
        let uuid = Uuid::new(cycle as i64 + 1, 0x5000);
        let (tx, rx) = player_channel_with_capacity(64);
        let (session, _join) = registry
            .register(PlayerJoinRequest {
                uuid,
                username: format!("Soak{cycle}"),
                world_uuid: start.world_uuid.parse().expect("world uuid"),
                transform: transform(),
                appearance: PlayerAppearance::default(),
                skin: None,
                view_center: start.player_chunk,
                view_radius: 1,
                sender: tx,
            })
            .expect("register");

        let x = (cycle % 16) as i32;
        let z = ((cycle / 16) % 16) as i32;
        let block = 1 + (cycle % 8) as i32;
        thread.set_block(x, 201, z, block).expect("block edit");
        thread.tick().expect("tick");
        thread.simulate(0.05).expect("simulate");
        while thread.try_event().is_some() {}
        while rx.try_recv().is_ok() {}
        drop(session);

        if cycle == 2 {
            baseline = resident_bytes();
        }
        if (cycle + 1) % 10 == 0 {
            thread.save().expect("save");
        }
    }

    let elapsed = started.elapsed();
    assert!(registry.is_empty(), "every synthetic session unregisters");
    let loaded = thread.loaded_chunks().expect("loaded chunks");
    assert!(
        loaded.len() <= 9,
        "radius-1 view must not accumulate chunks: {loaded:?}"
    );
    assert_eq!(
        thread.get_block(0, 201, 0).expect("read edit"),
        Some(1) // cycle 0 wrote block 1 at (0, 201, 0)
    );
    assert!(
        thread
            .get_block(
                ((cycles - 1) % 16) as i32,
                201,
                (((cycles - 1) / 16) % 16) as i32,
            )
            .expect("read last edit")
            .is_some(),
        "the last edit is readable"
    );

    let final_rss = resident_bytes();
    if let (Some(baseline), Some(final_rss)) = (baseline, final_rss) {
        let growth = final_rss.saturating_sub(baseline);
        println!(
            "soak: {cycles} cycles in {:.2}s, rss {} -> {} (+{} bytes), limit {}",
            elapsed.as_secs_f64(),
            baseline,
            final_rss,
            growth,
            rss_limit_bytes()
        );
        assert!(
            growth <= rss_limit_bytes(),
            "resident memory grew by {growth} bytes (limit {})",
            rss_limit_bytes()
        );
    } else {
        println!(
            "soak: {cycles} cycles in {:.2}s (RSS unavailable on this platform)",
            elapsed.as_secs_f64()
        );
    }

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
