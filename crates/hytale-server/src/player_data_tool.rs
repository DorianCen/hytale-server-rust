//! Load or create a player save file (`players/<uuid>.json`) with the
//! Java-compatible holder codec.
//!
//! The chunk/config tools write the other two parts of a saved world; this one
//! writes the player half and lets a Java probe read it back.

use std::path::PathBuf;

use hytale_world::player_data::{
    PlayerData, PlayerDataError, PlayerStorage, PlayerTransform, Position, Rotation,
};
use hytale_world::world_config::WorldUuid;
use thiserror::Error;

/// Where to read the player and what to use when creating a new one.
#[derive(Debug, Clone)]
pub struct PlayerDataToolOptions {
    /// Optional source player document to load and re-save.
    pub input: Option<PathBuf>,
    /// World save root; the file is written to `<save>/players/<uuid>.json`.
    pub save_dir: PathBuf,
    /// Player UUID (required when creating; optional override when loading).
    pub uuid: Option<String>,
    /// Position for a new player.
    pub position: Position,
    /// Rotation for a new player.
    pub rotation: Rotation,
    /// `WorldGenId` component for a new player.
    pub world_gen_id: i32,
}

/// What the player-data tool produced.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDataToolReport {
    /// Written file.
    pub path: PathBuf,
    /// Player UUID in canonical form.
    pub uuid: String,
    /// Position in the document.
    pub position: Position,
    /// Component count (known and preserved).
    pub component_count: usize,
    /// True when an existing document was loaded instead of created.
    pub loaded_existing: bool,
}

/// Errors from the player-data tool.
#[derive(Debug, Error)]
pub enum PlayerDataToolError {
    /// The player codec or store failed.
    #[error(transparent)]
    Player(#[from] PlayerDataError),
    /// A new player needs a UUID.
    #[error("creating a player requires --uuid")]
    MissingUuid,
    /// The requested UUID is malformed.
    #[error("invalid player UUID `{0}`")]
    InvalidUuid(String),
    /// The loaded document has no UUID component.
    #[error("player document {0} has no UUID component")]
    MissingDocumentUuid(PathBuf),
}

/// Load an existing player or create one, then save it into the world.
pub fn run(options: &PlayerDataToolOptions) -> Result<PlayerDataToolReport, PlayerDataToolError> {
    let (mut data, loaded_existing) = if let Some(input) = &options.input {
        let text = std::fs::read_to_string(input).map_err(|source| PlayerDataError::Io {
            path: input.clone(),
            source,
        })?;
        let data = PlayerData::from_json(&text).map_err(|source| PlayerDataError::Json {
            path: input.clone(),
            source,
        })?;
        (data, true)
    } else {
        let Some(value) = &options.uuid else {
            return Err(PlayerDataToolError::MissingUuid);
        };
        let uuid = value
            .parse()
            .map_err(|_| PlayerDataToolError::InvalidUuid(value.clone()))?;
        let mut data = PlayerData::new(
            uuid,
            PlayerTransform::new(options.position, options.rotation),
        );
        data.set_world_gen_id(options.world_gen_id);
        (data, false)
    };

    let uuid = match &options.uuid {
        Some(value) => {
            let uuid: WorldUuid = value
                .parse()
                .map_err(|_| PlayerDataToolError::InvalidUuid(value.clone()))?;
            if loaded_existing {
                data.set_uuid(uuid);
            }
            uuid
        }
        None => data.uuid().ok_or_else(|| {
            PlayerDataToolError::MissingDocumentUuid(
                options
                    .input
                    .clone()
                    .unwrap_or_else(|| options.save_dir.clone()),
            )
        })?,
    };

    let storage = PlayerStorage::for_save_dir(&options.save_dir);
    storage.save(uuid, &data)?;
    let position = data
        .transform()
        .map(|transform| transform.position)
        .unwrap_or(options.position);

    Ok(PlayerDataToolReport {
        path: storage.path_for(uuid),
        uuid: uuid.to_string(),
        position,
        component_count: data.component_count(),
        loaded_existing,
    })
}
