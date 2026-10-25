//! `--validate-assets` report and determinism gate.

use std::path::PathBuf;

use hytale_server::asset_validation::validate_assets;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn asset_pack() -> PathBuf {
    repo_root().join("Assets/Assets.zip")
}

fn assets_disabled() -> bool {
    matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    )
}

#[test]
fn default_pack_validates_and_reports_every_config_packet() {
    if assets_disabled() || !asset_pack().is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let report = validate_assets(&asset_pack()).expect("validate the default pack");
    assert!(
        report.failures.is_empty(),
        "validation failures: {:?}",
        report.failures
    );
    assert_eq!(report.packets.len(), 21);
    assert_eq!(
        report
            .packets
            .iter()
            .map(|packet| packet.id)
            .collect::<Vec<_>>(),
        vec![41, 87, 65, 42, 63, 83, 61, 84, 40, 44, 45, 43, 55, 57, 56, 52, 72, 51, 85, 47, 54]
    );
    assert!(report.packets.iter().all(|packet| packet.bytes > 0));
    assert!(report.tag_count > 0);
    assert!(report.checked_tag_indexes > 0);
    assert_eq!(report.stores.len(), 27);
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "Item" && store.count == 4008));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "EntityStatType" && store.count == 13));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "EntityEffect" && store.count == 260));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "DamageCause" && store.count == 20));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "Projectile" && store.count == 86));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "ProjectileConfig" && store.count == 188));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "GameplayConfig" && store.count == 8));
    assert!(report
        .stores
        .iter()
        .any(|store| store.name == "TagPattern" && store.count == 196));
}

#[test]
fn validation_is_deterministic() {
    if assets_disabled() || !asset_pack().is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let first = validate_assets(&asset_pack()).expect("first validation");
    let second = validate_assets(&asset_pack()).expect("second validation");
    assert_eq!(first, second);
}
