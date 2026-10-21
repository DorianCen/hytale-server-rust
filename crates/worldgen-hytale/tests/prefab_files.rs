//! Spot checks of the **real** `.prefab.json` files of the base-game asset
//! pack (`Assets/extracted/Server/Prefabs`).
//!
//! The tests read the files through the real prefab store and check the
//! values the Java deserializer produces: the anchor, the block count, the
//! column count and the extents of the prefab (relative to the anchor).

use std::path::PathBuf;
use std::rc::Rc;

use worldgen_hytale::prefab::prefab_rotation::PrefabRotation;
use worldgen_hytale::prefab::prefab_store::PrefabStore;
use worldgen_model::registry::Registries;

/// The root of the extracted base-game asset pack.
fn assets_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../Assets/extracted",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

/// Whether the base-game asset pack is available (it is not part of the
/// source tree — see `.gitignore`).
fn assets_available() -> bool {
    assets_root().join("Server/Prefabs").is_dir()
}

/// The prefab store of the base-game asset pack.
fn store() -> PrefabStore {
    PrefabStore::new(
        assets_root().join("Server/Prefabs"),
        Rc::new(Registries::load()),
    )
}

/// The number of blocks of a prefab (the sum of the column block counts).
fn block_count(prefab: &worldgen_hytale::prefab::buffer::prefab_buffer::PrefabBuffer) -> usize {
    let mut count = 0;
    for (_, column) in prefab.columns.iter() {
        count += column.get_block_count();
    }
    count
}

/// The number of columns of a prefab.
fn column_count(prefab: &worldgen_hytale::prefab::buffer::prefab_buffer::PrefabBuffer) -> usize {
    prefab.columns.size() as usize
}

#[test]
fn the_cave_formation_prefab_is_parsed() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let store = store();
    let path = assets_root().join(
        "Server/Prefabs/Cave/Formations/Rock_Sandstone/Ceiling/Rock_Sandstone_Ceiling_Formation_001.prefab.json",
    );
    let prefab = store.load(&path);
    let access = prefab.new_access();

    // The anchor of the file.
    assert_eq!(access.get_anchor_x(), 1);
    assert_eq!(access.get_anchor_y(), 20);
    assert_eq!(access.get_anchor_z(), 1);

    // 265 blocks in 21 columns.
    assert_eq!(block_count(&prefab), 265);
    assert_eq!(column_count(&prefab), 21);

    // The extents are relative to the anchor.
    assert_eq!(access.get_min_x(PrefabRotation::Rotation0), -2);
    assert_eq!(access.get_max_x(PrefabRotation::Rotation0), 2);
    assert_eq!(access.get_min_y(), -17);
    assert_eq!(access.get_max_y(), 7);
    assert_eq!(access.get_min_z(PrefabRotation::Rotation0), -2);
    assert_eq!(access.get_max_z(PrefabRotation::Rotation0), 2);

    // The per-column height map: the centre column reaches from -16 to 6.
    let min_y = access.get_min_y_at(PrefabRotation::Rotation0, 0, 0);
    let max_y = access.get_max_y_at(PrefabRotation::Rotation0, 0, 0);
    assert!(min_y <= max_y, "the column must have a height range");
    assert_eq!(min_y, -16, "the lowest block of the centre column");
    assert_eq!(max_y, 6, "the highest block of the centre column");
    // A column without blocks reports -1 (the Java value).
    assert_eq!(access.get_min_y_at(PrefabRotation::Rotation0, 5, 5), -1);
    assert_eq!(access.get_max_y_at(PrefabRotation::Rotation0, 5, 5), -1);

    // The rotated bounds swap the axes.
    assert_eq!(access.get_min_x(PrefabRotation::Rotation90), -2);
    assert_eq!(access.get_max_x(PrefabRotation::Rotation90), 2);
    assert_eq!(access.get_min_z(PrefabRotation::Rotation90), -2);
    assert_eq!(access.get_max_z(PrefabRotation::Rotation90), 2);
}

#[test]
fn the_amber_trees_are_parsed() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let store = store();
    /// The prefab file, its block count, its column count and its extents.
    type Case = (&'static str, usize, usize, (i32, i32, i32), (i32, i32, i32));
    let cases: [Case; 3] = [
        (
            "Server/Prefabs/Trees/Amber/Stage_0/Amber_Stage0_001.prefab.json",
            86,
            43,
            (-4, -1, -4),
            (4, 8, 4),
        ),
        (
            "Server/Prefabs/Trees/Amber/Stage_1/Amber_Stage1_001.prefab.json",
            372,
            141,
            (-7, -2, -9),
            (7, 16, 7),
        ),
        (
            "Server/Prefabs/Trees/Amber/Stage_2/Amber_Stage2_001.prefab.json",
            1342,
            353,
            (-11, -4, -13),
            (12, 28, 11),
        ),
    ];

    for (relative, blocks, columns, min, max) in cases {
        let path = assets_root().join(relative);
        let prefab = store.load(&path);
        let access = prefab.new_access();
        assert_eq!(block_count(&prefab), blocks, "{relative}");
        assert_eq!(column_count(&prefab), columns, "{relative}");
        assert_eq!(
            (
                access.get_min_x(PrefabRotation::Rotation0),
                access.get_min_y(),
                access.get_min_z(PrefabRotation::Rotation0)
            ),
            min,
            "{relative}"
        );
        assert_eq!(
            (
                access.get_max_x(PrefabRotation::Rotation0),
                access.get_max_y(),
                access.get_max_z(PrefabRotation::Rotation0)
            ),
            max,
            "{relative}"
        );
    }
}

#[test]
fn the_prefab_names_resolve_through_the_store() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let store = store();
    // The dotted name is the path of the prefab file.
    let paths = store
        .resolve("Cave.Formations.Rock_Sandstone.Ceiling.Rock_Sandstone_Ceiling_Formation_001");
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("Rock_Sandstone_Ceiling_Formation_001.prefab.json"));

    // A wildcard name matches every prefab of the folder.
    let paths = store.resolve("Cave.Formations.Rock_Sandstone.Ceiling.*");
    assert!(!paths.is_empty());
    for path in paths.iter() {
        assert!(path
            .to_string_lossy()
            .contains("Cave/Formations/Rock_Sandstone/Ceiling/"));
    }
}

#[test]
fn an_unknown_prefab_name_resolves_to_nothing() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let store = store();
    assert!(store.resolve("Does.Not.Exist").is_empty());
}

#[test]
fn a_prefabs_version_is_validated() {
    // A prefab with an old version is rejected (the Java message).
    let dir = std::env::temp_dir().join("worldgen-prefab-version-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Old.prefab.json");
    std::fs::write(
        &path,
        r#"{"version": 7, "anchorX": 0, "anchorY": 0, "anchorZ": 0, "blocks": []}"#,
    )
    .unwrap();

    let store = PrefabStore::new(dir.clone(), Rc::new(Registries::load()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.load(&path)));
    let payload = match result {
        Ok(_) => panic!("the old prefab version must be rejected"),
        Err(payload) => payload,
    };
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| "<non-string panic>".to_string());
    assert_eq!(message, "Prefab version is too old: 7");
}
