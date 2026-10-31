//! T63 performance probe.
//!
//! Run from the repository root in release mode:
//!
//! ```sh
//! cargo run --release -p hytale-server --example perf
//! ```
//!
//! Optional environment overrides:
//! `HYTALE_PERF_ASSETS_PACK`, `HYTALE_PERF_ASSETS_ROOT`,
//! `HYTALE_PERF_WORLD_DATA`, `HYTALE_PERF_SAVE`, `HYTALE_PERF_TICKS`,
//! `HYTALE_PERF_VIEW_RADIUS`.
//!
//! The output is tab-separated (`metric<TAB>value<TAB>unit`) so
//! `tools/perf.sh` can archive it without a benchmark harness dependency.

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_server::{
    peak_resident_bytes, resident_bytes, GeneratorOptions, WorldThread, WorldThreadOptions,
};
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;

fn env_path(name: &str, fallback: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn report_ms(name: &str, start: Instant) {
    println!("{name}\t{:.3}\tms", start.elapsed().as_secs_f64() * 1000.0);
}

fn report_value(name: &str, value: impl std::fmt::Display, unit: &str) {
    println!("{name}\t{value}\t{unit}");
}

fn report_memory(label: &str) {
    if let Some(bytes) = resident_bytes() {
        report_value(&format!("memory.{label}.rss"), bytes, "bytes");
    }
    if let Some(bytes) = peak_resident_bytes() {
        report_value(&format!("memory.{label}.peak_rss"), bytes, "bytes");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets_pack = env_path("HYTALE_PERF_ASSETS_PACK", "Assets/Assets.zip");
    let assets_root = env_path("HYTALE_PERF_ASSETS_ROOT", "Assets/extracted");
    let world_data = env_path("HYTALE_PERF_WORLD_DATA", "testdata/Default");
    let save_dir = env_path("HYTALE_PERF_SAVE", "target/perf-world");
    let ticks = std::env::var("HYTALE_PERF_TICKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200usize)
        .max(1);
    let view_radius = std::env::var("HYTALE_PERF_VIEW_RADIUS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1u32);

    report_value("config.assets_pack", assets_pack.display(), "path");
    report_value("config.ticks", ticks, "count");
    report_value("config.view_radius", view_radius, "chunks");
    report_memory("start");

    let open_start = Instant::now();
    let pack = AssetPack::open(&assets_pack)?;
    report_ms("assets.pack_open", open_start);

    let p0_start = Instant::now();
    let p0 = AssetRegistry::load_p0(&pack)?;
    report_ms("assets.load_p0", p0_start);
    report_value("assets.p0.blocks", p0.block_types.len(), "count");
    report_value("assets.p0.items", p0.items.len(), "count");
    report_memory("p0");

    let phase2_start = Instant::now();
    let registry = AssetRegistry::load_phase2(&pack)?;
    report_ms("assets.load_phase2", phase2_start);
    report_value("assets.phase2.blocks", registry.block_types.len(), "count");
    report_value("assets.phase2.items", registry.items.len(), "count");

    let config_start = Instant::now();
    let config = registry.config_assets()?;
    report_ms("assets.config_assets", config_start);
    report_value("assets.config_packets", config.packets.len(), "count");
    report_value(
        "assets.config_payload_bytes",
        config
            .packets
            .iter()
            .map(|packet| packet.payload.len())
            .sum::<usize>(),
        "bytes",
    );
    report_memory("assets");

    let mut options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack,
            assets_root,
            world_dir: world_data,
            workers: 1,
        },
        save_dir.clone(),
    );
    options.view_radius = view_radius;
    options.chunks_per_tick = 4;

    let world_start = Instant::now();
    let (thread, start) = WorldThread::spawn(options)?;
    report_ms("world.spawn", world_start);
    report_value("world.stored_chunks", start.stored_chunks, "count");
    report_value("world.initial_loads", start.initial_loads, "count");
    report_memory("world_spawn");

    let cache_start = Instant::now();
    let first = thread.config_asset_packets()?;
    let cache_hit_start = Instant::now();
    let second = thread.config_asset_packets()?;
    report_ms("world.config_cache_cold", cache_start);
    report_ms("world.config_cache_hit", cache_hit_start);
    report_value(
        "world.config_cache_shared",
        std::sync::Arc::ptr_eq(&first, &second) as u8,
        "bool",
    );

    let tick_start = Instant::now();
    for _ in 0..ticks {
        black_box(thread.tick()?);
        black_box(thread.simulate(0.05)?);
    }
    let tick_elapsed = tick_start.elapsed();
    report_ms("world.tick_batch", tick_start);
    report_value("world.tick_count", ticks, "count");
    report_value(
        "world.tick_per_second",
        ticks as f64 / tick_elapsed.as_secs_f64(),
        "ticks/s",
    );
    while thread.try_event().is_some() {}

    // Force a chunk outside the configured view so the probe measures real
    // load/generation rather than a cached chunk.
    let (chunk_x, chunk_z) = (start.player_chunk.0 + 8, start.player_chunk.1 + 8);
    let chunk_start = Instant::now();
    let _ = thread.load_chunk(chunk_x, chunk_z)?;
    report_ms("world.load_one_chunk", chunk_start);

    let packets_start = Instant::now();
    let batch = thread.chunk_packets(chunk_x, chunk_z, false)?;
    report_ms("chunk.packet_encode_join", packets_start);
    report_value("chunk.packet_count", batch.packets.len(), "count");
    report_value("chunk.payload_bytes", batch.payload_bytes(), "bytes");

    let frames_start = Instant::now();
    let frames = batch.frames(DEFAULT_COMPRESSION_LEVEL)?;
    report_ms("chunk.frame_encode_zstd", frames_start);
    report_value(
        "chunk.frame_bytes",
        frames.iter().map(Vec::len).sum::<usize>(),
        "bytes",
    );

    let edit_start = Instant::now();
    let _ = thread.set_block(4, 200, 4, 1)?;
    report_ms("world.block_edit", edit_start);

    let save_start = Instant::now();
    let saved = thread.save()?;
    report_ms("world.save", save_start);
    report_value("world.saved_chunks", saved, "count");
    report_memory("world_saved");

    thread.shutdown()?;
    report_memory("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
    Ok(())
}
