//! Boot a complete saved world: config, player and stored chunks.
//!
//! This composes the pieces the live server will own — [`WorldSave`] (world
//! config + player storage + [`World`]) and the asset-bound [`ChunkCodec`] — so
//! one command loads an existing save (or creates it), loads/creates a player,
//! loads every stored chunk, optionally edits a block and saves everything.

use std::path::PathBuf;

use hytale_assets::{AssetError, AssetPack, AssetRegistry};
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::player_data::PlayerDataError;
use hytale_world::world::{WorldError, WorldSave};
use hytale_world::world_config::{WorldConfigError, WorldUuid};
use thiserror::Error;

/// Paths and overrides for a saved-world boot.
#[derive(Debug, Clone)]
pub struct WorldBootOptions {
    /// `Assets/Assets.zip` (or an unpacked directory) used for the codec.
    pub assets_pack: PathBuf,
    /// World save root (`config.json`, `chunks/`, `players/`).
    pub save_dir: PathBuf,
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
    /// Chunks stored on disk before loading.
    pub stored_chunks: usize,
    /// Chunks loaded into the runtime world.
    pub loaded_chunks: usize,
    /// Chunks written back (dirty ones).
    pub saved_chunks: usize,
    /// Block edit as `(x, y, z, before, after)`.
    pub block_edit: Option<(i32, i32, i32, i32, i32)>,
}

/// Errors from the saved-world boot.
#[derive(Debug, Error)]
pub enum WorldBootError {
    /// The asset pack failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
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
    /// The block edit targets a chunk that is not stored.
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

/// Load or create the save, load every stored chunk, and save the result.
pub fn run(options: &WorldBootOptions) -> Result<WorldBootReport, WorldBootError> {
    let pack = AssetPack::open(&options.assets_pack)?;
    let assets = AssetRegistry::load_p0(&pack)?;
    let codec = ChunkCodec::new(&assets);

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

    let coordinates = save.world().storage().stored_chunk_coordinates()?;
    let stored_chunks = coordinates.len();
    let mut loaded_chunks = 0;
    for &(x, z) in &coordinates {
        if save.world_mut().load_chunk(&codec, x, z)? {
            loaded_chunks += 1;
        }
    }

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

    Ok(WorldBootReport {
        save_dir: options.save_dir.clone(),
        config_created,
        world_uuid: save.config().uuid.to_string(),
        seed: save.config().seed,
        player_uuid: player_uuid.to_string(),
        player_created,
        stored_chunks,
        loaded_chunks,
        saved_chunks,
        block_edit,
    })
}
