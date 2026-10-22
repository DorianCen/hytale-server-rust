//! A complete saved-world bundle: config, players and chunks together.

use std::path::PathBuf;

use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::palette::NameLookup;
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::player_data::{PlayerTransform, Position, Rotation};
use hytale_world::world::{WorldSave, WorldStorage};
use hytale_world::world_config::{ConfigObject, WorldConfig, WorldUuid};

struct TestNames;

impl NameLookup for TestNames {
    fn name_of(&self, id: i32) -> Option<&str> {
        Some(Box::leak(format!("id_{id}").into_boxed_str()))
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        name.strip_prefix("id_")?.parse().ok()
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-world-save-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn uuid() -> WorldUuid {
    "12345678-1234-5678-9abc-def012345678".parse().unwrap()
}

fn column(x: i32, z: i32) -> ChunkColumn {
    let mut column = ChunkColumn::new(x, z);
    column.set_block(1, 64, 1, 5, 0, 0).expect("set block");
    column
}

#[test]
fn boots_a_complete_save_and_reopens_it() {
    let dir = temp_dir("bundle");
    let names = TestNames;
    let codec = ChunkCodec::from_lookups(&names, &names, &names);

    {
        let (mut save, created) =
            WorldSave::open(&dir, 7, Some("Bundle".to_string()), uuid()).expect("open the save");
        assert!(created);
        assert_eq!(save.config().seed, 7);
        assert_eq!(save.config().uuid, uuid());
        assert!(save.config_path().is_file());

        // Give the config a spawn point so a new player starts there.
        save.config_mut().spawn_provider = Some(ConfigObject::new([
            ("Id", serde_json::Value::String("Global".to_string())),
            (
                "SpawnPoint",
                serde_json::json!({"X": 10.5, "Y": 80.0, "Z": -3.5}),
            ),
        ]));

        let (player, created) = save
            .load_or_create_player("00000000-0000-0000-0000-000000000004".parse().unwrap())
            .expect("create the player");
        assert!(created);
        assert_eq!(
            player.transform(),
            Some(PlayerTransform::new(
                Position {
                    x: 10.5,
                    y: 80.0,
                    z: -3.5
                },
                Rotation {
                    pitch: 0.0,
                    yaw: 0.0,
                    roll: 0.0
                }
            ))
        );

        save.world_mut().insert_chunk(column(4, -5));
        assert_eq!(save.save_all(&codec).expect("save all"), 1);
    }

    {
        let (mut save, created) = WorldSave::open(&dir, 0, None, uuid()).expect("reopen");
        assert!(!created);
        let coordinates = save
            .world()
            .storage()
            .stored_chunk_coordinates()
            .expect("list chunks");
        assert_eq!(coordinates, vec![(4, -5)]);
        assert!(save.world_mut().load_chunk(&codec, 4, -5).expect("load"));
        assert_eq!(save.world().get_block(4 * 32 + 1, 64, -5 * 32 + 1), 5);

        let (player, created) = save
            .load_or_create_player("00000000-0000-0000-0000-000000000004".parse().unwrap())
            .expect("load the player");
        assert!(!created);
        assert_eq!(player.transform().unwrap().position.x, 10.5);
        assert_eq!(save.players().players().unwrap().len(), 1);
    }

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_new_save_without_a_spawn_starts_at_the_origin() {
    let dir = temp_dir("origin");
    let (mut save, _) = WorldSave::open(&dir, 0, None, uuid()).expect("open");
    let (player, created) = save
        .load_or_create_player("00000000-0000-0000-0000-000000000005".parse().unwrap())
        .expect("create");
    assert!(created);
    let transform = player.transform().expect("transform");
    assert_eq!(transform.position.x, 0.0);
    assert_eq!(transform.position.y, 0.0);
    assert_eq!(transform.position.z, 0.0);

    // The bundle exposes the same storage handle as a plain World.
    let world = hytale_world::world::World::new(WorldStorage::new(&dir));
    assert_eq!(world.storage().save_dir(), dir);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn reads_a_config_created_elsewhere() {
    let dir = temp_dir("existing-config");
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = WorldConfig::new(uuid(), 99, Some("Existing".to_string()));
    config.is_pvp_enabled = true;
    config
        .save(WorldConfig::path_in(&dir))
        .expect("write config");

    let (save, created) = WorldSave::open(&dir, 0, None, uuid()).expect("open");
    assert!(!created);
    assert_eq!(save.config().seed, 99);
    assert!(save.config().is_pvp_enabled);
    assert_eq!(save.config().display_name.as_deref(), Some("Existing"));
    std::fs::remove_dir_all(dir).ok();
}
