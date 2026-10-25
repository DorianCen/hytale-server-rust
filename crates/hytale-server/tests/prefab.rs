//! T56: paste a real prefab through the world thread and persist its runtime
//! entities/block components.

use std::path::{Path, PathBuf};

use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::runtime::RuntimeEntityKind;

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
        "hytale-prefab-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn options(save_dir: PathBuf) -> WorldThreadOptions {
    WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir,
    )
}

fn open(save_dir: &Path) -> WorldThread {
    let (thread, _start) =
        WorldThread::spawn(options(save_dir.to_path_buf())).expect("world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    thread
}

#[test]
fn a_real_prefab_pastes_entities_and_components_and_survives_reload() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("site");
    let thread = open(&save_dir);

    let report = thread
        .place_prefab("Hunt.Generic.Site", 100, 200, 100, 90)
        .expect("paste Hunt.Generic.Site");
    assert_eq!(report.prefab, "Hunt.Generic.Site");
    assert_eq!(report.blocks, 10);
    assert_eq!(report.block_components, 1);
    assert_eq!(report.entities.len(), 3);

    // The trap block is at local (0, 1, 0); Rotation90 keeps it at the origin.
    let component = thread
        .block_component(100, 201, 100)
        .expect("read the block component")
        .expect("the Hunt_Trap component");
    assert_eq!(component.block_id, report_block_id(&thread, "Hunt_Trap"));
    assert_eq!(
        component
            .components
            .get("Components")
            .and_then(|value| value.get("BlockMapMarker"))
            .and_then(|value| value.get("Icon"))
            .and_then(|value| value.as_str()),
        Some("HuntSite.png")
    );

    // Entity local (-0.5, 1, 0.5) rotates to (0.5, 1, 0.5).
    let entities = thread.entities().expect("entities");
    assert_eq!(entities.len(), 3);
    let marker = entities
        .iter()
        .find(|entity| entity.prefab.as_deref() == Some("Hunt.Generic.Site"))
        .expect("a prefab entity");
    assert_eq!(marker.kind, RuntimeEntityKind::Prefab);
    assert!((marker.transform.position.x - 100.5).abs() < 1.0e-9);
    assert!((marker.transform.position.y - 201.0).abs() < 1.0e-9);
    assert!((marker.transform.position.z - 100.5).abs() < 1.0e-9);

    // Drop one entity through the lifecycle API.
    let despawned = thread
        .despawn_entity(marker.id)
        .expect("despawn entity")
        .expect("the marker exists");
    assert_eq!(despawned.id, marker.id);

    thread.save().expect("save the world");
    thread.shutdown().expect("shutdown");

    let thread = open(&save_dir);
    let entities = thread.entities().expect("reloaded entities");
    assert_eq!(entities.len(), 2, "the despawned entity stays gone");
    let components = thread.block_components().expect("reloaded components");
    assert_eq!(components.len(), 1);
    assert_eq!(components[0].0, (100, 201, 100));
    assert_eq!(
        components[0]
            .1
            .components
            .get("Components")
            .and_then(|value| value.get("BlockMapMarker"))
            .and_then(|value| value.get("Icon"))
            .and_then(|value| value.as_str()),
        Some("HuntSite.png")
    );
    thread
        .load_chunk(100 >> 5, 100 >> 5)
        .expect("load the chunk");
    assert_eq!(
        thread.get_block(100, 201, 100).expect("read block"),
        Some(components[0].1.block_id),
        "the pasted trap block survives too"
    );
    thread.shutdown().expect("shutdown reloaded world");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn item_entities_spawn_and_persist() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("items");
    let thread = open(&save_dir);
    let entity = thread
        .spawn_item_entity("Rock_Stone", 4, 2.5, 80.0, 3.5)
        .expect("spawn an item entity");
    assert_eq!(entity.kind, RuntimeEntityKind::Item);
    assert_eq!(entity.item_id(), Some("Rock_Stone"));
    assert_eq!(entity.item_quantity(), 4);
    assert_eq!(thread.entities().expect("entities").len(), 1);
    thread.shutdown().expect("shutdown");

    let thread = open(&save_dir);
    let entities = thread.entities().expect("reloaded entities");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].item_id(), Some("Rock_Stone"));
    assert_eq!(entities[0].item_quantity(), 4);
    thread.shutdown().expect("shutdown reloaded world");
    std::fs::remove_dir_all(save_dir).ok();
}

fn report_block_id(thread: &WorldThread, block: &str) -> i32 {
    thread
        .block_id_for_item(block)
        .expect("block lookup")
        .unwrap_or_else(|| panic!("{block} must resolve to a block id"))
}
