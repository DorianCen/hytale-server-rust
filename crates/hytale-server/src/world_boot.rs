//! Boot a complete saved world on its own thread: config, player, stored chunks
//! and the player's view-radius chunks.
//!
//! The heavy work is owned by [`WorldThread`] (save + generator + demand queue).
//! This module is the synchronous CLI/test adapter: it applies the initial view,
//! ticks until the queue is empty, applies an optional block edit and shuts the
//! thread down.

use std::path::PathBuf;

use thiserror::Error;

use crate::generation::GeneratorOptions;
use crate::world_thread::{LoadOutcome, WorldThread, WorldThreadError, WorldThreadOptions};

/// Paths and overrides for a saved-world boot.
#[derive(Debug, Clone)]
pub struct WorldBootOptions {
    /// `Assets/Assets.zip` (or an unpacked directory).
    pub assets_pack: PathBuf,
    /// Unpacked asset tree for prefabs, environments and tint gradients.
    pub assets_root: PathBuf,
    /// World-gen data folder (normally `testdata/Default`).
    pub world_dir: PathBuf,
    /// World save root (`config.json`, `chunks/`, `players/`).
    pub save_dir: PathBuf,
    /// Worldgen worker count.
    pub workers: usize,
    /// Radius of chunks loaded/generated around the player chunk.
    pub view_radius: u32,
    /// Maximum chunks loaded/generated per tick.
    pub chunks_per_tick: usize,
    /// Seed used only when a new config is created.
    pub seed: i64,
    /// Display name used only when a new config is created.
    pub display_name: Option<String>,
    /// World UUID used only when a new config is created.
    pub world_uuid: Option<String>,
    /// Player to load or create; defaults to the first stored player, then a
    /// UUID derived from the world seed/name.
    pub player_uuid: Option<String>,
    /// Optional block edit as `(x, y, z, block_id)` in world coordinates.
    pub set_block: Option<(i32, i32, i32, i32)>,
}

/// What the saved-world boot produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldBootReport {
    /// Save root.
    pub save_dir: PathBuf,
    /// True when this boot created `config.json`.
    pub config_created: bool,
    /// World UUID.
    pub world_uuid: String,
    /// World seed.
    pub seed: i64,
    /// Player UUID.
    pub player_uuid: String,
    /// True when this boot created the player file.
    pub player_created: bool,
    /// Chunks stored on disk at startup.
    pub stored_chunks: usize,
    /// View radius used around the player chunk.
    pub view_radius: u32,
    /// Chunks in the initial view plan.
    pub requested_chunks: usize,
    /// Chunks read from region files (view ticks + explicit edit chunk).
    pub loaded_chunks: usize,
    /// Chunks generated (view ticks + explicit edit chunk).
    pub generated_chunks: usize,
    /// Chunks written back (dirty ones).
    pub saved_chunks: usize,
    /// Block edit as `(x, y, z, before, after)`.
    pub block_edit: Option<(i32, i32, i32, i32, i32)>,
}

/// Errors from the saved-world boot.
#[derive(Debug, Error)]
pub enum WorldBootError {
    /// The world thread failed.
    #[error(transparent)]
    Thread(#[from] WorldThreadError),
}

/// Spawn the world thread, drain its demand queue and save the result.
pub fn run(options: &WorldBootOptions) -> Result<WorldBootReport, WorldBootError> {
    let (thread, start) = WorldThread::spawn(WorldThreadOptions {
        generator: GeneratorOptions {
            assets_pack: options.assets_pack.clone(),
            assets_root: options.assets_root.clone(),
            world_dir: options.world_dir.clone(),
            workers: options.workers,
        },
        save_dir: options.save_dir.clone(),
        seed: options.seed,
        display_name: options.display_name.clone(),
        world_uuid: options.world_uuid.clone(),
        player_uuid: options.player_uuid.clone(),
        view_radius: options.view_radius,
        chunks_per_tick: options.chunks_per_tick.max(1),
    })?;

    let mut loaded_chunks = 0;
    let mut generated_chunks = 0;
    loop {
        let tick = thread.tick()?;
        loaded_chunks += tick.loaded;
        generated_chunks += tick.generated;
        if tick.remaining == 0 {
            break;
        }
    }

    let block_edit = match options.set_block {
        Some((x, y, z, block_id)) => {
            match thread.load_chunk(x >> 5, z >> 5)? {
                LoadOutcome::Loaded => loaded_chunks += 1,
                LoadOutcome::Generated => generated_chunks += 1,
                LoadOutcome::AlreadyLoaded => {}
            }
            let (before, after) = thread.set_block(x, y, z, block_id)?;
            Some((x, y, z, before, after))
        }
        None => None,
    };

    let saved_chunks = thread.shutdown()?;

    Ok(WorldBootReport {
        save_dir: options.save_dir.clone(),
        config_created: start.config_created,
        world_uuid: start.world_uuid,
        seed: start.seed,
        player_uuid: start.player_uuid,
        player_created: start.player_created,
        stored_chunks: start.stored_chunks,
        view_radius: start.view_radius,
        requested_chunks: start.initial_loads,
        loaded_chunks,
        generated_chunks,
        saved_chunks,
        block_edit,
    })
}
