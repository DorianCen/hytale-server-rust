//! Boot a complete saved world: config, player, stored chunks and the player's
//! view-radius chunks.
//!
//! This composes the pieces the live server will own — [`WorldSave`] (world
//! config + player storage + [`World`]) and the asset-bound [`ChunkCodec`] — so
//! one command loads an existing save (or creates it), loads/creates a player,
//! loads stored chunks, generates missing chunks around the player, optionally
//! edits a block and saves everything.

use std::path::PathBuf;

use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::player_data::PlayerDataError;
use hytale_world::world::{chunk_coordinates_around, WorldError, WorldSave};
use hytale_world::world_config::{WorldConfigError, WorldUuid};
use thiserror::Error;

use crate::generation::{GenerationError, GeneratorOptions, WorldGenerator};

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
    /// Chunks stored on disk before demand loading.
    pub stored_chunks: usize,
    /// View radius used around the player chunk.
    pub view_radius: u32,
    /// Unique chunks requested (stored + demand set).
    pub requested_chunks: usize,
    /// Chunks read from region files.
    pub loaded_chunks: usize,
    /// Chunks generated because they were missing.
    pub generated_chunks: usize,
    /// Chunks written back (dirty ones).
    pub saved_chunks: usize,
    /// Block edit as `(x, y, z, before, after)`.
    pub block_edit: Option<(i32, i32, i32, i32, i32)>,
}

/// Errors from the saved-world boot.
#[derive(Debug, Error)]
pub enum WorldBootError {
    /// The generator boot failed.
    #[error(transparent)]
    Generation(#[from] GenerationError),
    /// The world/config/player layer failed.
    #[error(transparent)]
    World(#[from] WorldError),
    /// The config codec failed.
    #[error(transparent)]
    Config(#[from] WorldConfigError),
    /// A player file failed.
    #[error(transparent)]
    Player(#[from] PlayerDataError),
    /// A UUID argument is malformed.
    #[error("invalid UUID `{0}`")]
    InvalidUuid(String),
    /// The block edit targets a chunk that is not loaded or stored.
    #[error("cannot edit block ({x}, {y}, {z}): chunk ({chunk_x}, {chunk_z}) is not loaded")]
    ChunkNotLoaded {
        /// Block x.
        x: i32,
        /// Block y.
        y: i32,
        /// Block z.
        z: i32,
        /// Chunk x.
        chunk_x: i32,
        /// Chunk z.
        chunk_z: i32,
    },
}

/// Load or create the save, demand-load around the player, and save the result.
pub fn run(options: &WorldBootOptions) -> Result<WorldBootReport, WorldBootError> {
    let generator = WorldGenerator::open(&GeneratorOptions {
        assets_pack: options.assets_pack.clone(),
        assets_root: options.assets_root.clone(),
        world_dir: options.world_dir.clone(),
        workers: options.workers,
    })?;
    let codec = ChunkCodec::new(generator.assets());

    let world_uuid = match &options.world_uuid {
        Some(value) => value
            .parse()
            .map_err(|_| WorldBootError::InvalidUuid(value.clone()))?,
        None => WorldUuid::derive(
            options.seed,
            options.display_name.as_deref().unwrap_or("Default"),
        ),
    };
    let (mut save, config_created) = WorldSave::open(
        &options.save_dir,
        options.seed,
        options.display_name.clone(),
        world_uuid,
    )?;
    let generation_seed = save.config().seed as i32;

    let player_uuid = match &options.player_uuid {
        Some(value) => value
            .parse()
            .map_err(|_| WorldBootError::InvalidUuid(value.clone()))?,
        None => {
            let stored = save.players().players()?;
            stored.first().copied().unwrap_or_else(|| {
                WorldUuid::derive(
                    save.config().seed,
                    &format!(
                        "{}:player",
                        save.config().display_name.as_deref().unwrap_or("Default")
                    ),
                )
            })
        }
    };
    let (player, player_created) = save.load_or_create_player(player_uuid)?;
    // A real boot would hydrate more components here; save the holder so the
    // create path and the load path exercise the same writer.
    save.save_player(player_uuid, &player)?;

    let stored = save.world().storage().stored_chunk_coordinates()?;
    let stored_chunks = stored.len();
    let (center_x, center_z) = player
        .transform()
        .map(|transform| {
            (
                block_to_chunk(transform.position.x),
                block_to_chunk(transform.position.z),
            )
        })
        .unwrap_or((0, 0));

    let mut coordinates = stored;
    coordinates.extend(chunk_coordinates_around(
        center_x,
        center_z,
        options.view_radius,
    ));
    let demand = save.ensure_chunks(&codec, coordinates, |x, z| {
        generator
            .generate(generation_seed, x, z)
            .map_err(|error| WorldError::Generation(error.to_string()))
    })?;

    let block_edit = match options.set_block {
        Some((x, y, z, block_id)) => {
            let (chunk_x, chunk_z) = (x >> 5, z >> 5);
            if save.world().chunk(chunk_x, chunk_z).is_none() {
                return Err(WorldBootError::ChunkNotLoaded {
                    x,
                    y,
                    z,
                    chunk_x,
                    chunk_z,
                });
            }
            let before = save.world().get_block(x, y, z);
            save.world_mut().set_block(x, y, z, block_id)?;
            let after = save.world().get_block(x, y, z);
            Some((x, y, z, before, after))
        }
        None => None,
    };

    let saved_chunks = save.save_all(&codec)?;
    generator.shutdown();

    Ok(WorldBootReport {
        save_dir: options.save_dir.clone(),
        config_created,
        world_uuid: save.config().uuid.to_string(),
        seed: save.config().seed,
        player_uuid: player_uuid.to_string(),
        player_created,
        stored_chunks,
        view_radius: options.view_radius,
        requested_chunks: demand.requested,
        loaded_chunks: demand.loaded,
        generated_chunks: demand.generated,
        saved_chunks,
        block_edit,
    })
}

/// Floor a world coordinate to its chunk coordinate (negative-safe).
fn block_to_chunk(value: f64) -> i32 {
    (value.floor() as i32) >> 5
}
