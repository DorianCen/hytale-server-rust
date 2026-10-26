//! Real-pack NPC role/flock catalog tests.
//!
//! These load `Assets/extracted`; `./tools/test.sh assets` runs them and the
//! default fast tier skips them through `HYTALE_TEST_ASSETS=0`.

use std::path::PathBuf;

use hytale_npc::flock::{FlockCatalog, FlockSize};
use hytale_npc::role::{RoleCatalog, RoleType};
use serde_json::Value;

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

#[test]
fn the_real_role_store_loads_and_keeps_reference_documents() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the real role catalog");
    assert_eq!(catalog.len(), 1147);
    assert_eq!(catalog.duplicates().len(), 1);
    assert_eq!(catalog.duplicates()[0].0, "Test_Patrol_Path");

    let bear = catalog.get("Bear_Grizzly").expect("Bear_Grizzly");
    assert_eq!(bear.role_type, RoleType::Variant);
    assert_eq!(bear.reference.as_deref(), Some("Template_Predator"));
    assert_eq!(bear.appearance(), Some("Bear_Grizzly"));
    assert_eq!(bear.max_health(), Some(100.0));
    assert!(!bear.reference_chain(&catalog).is_empty());

    let template = catalog.get("Template_Predator").expect("Template_Predator");
    assert_eq!(template.role_type, RoleType::Abstract);
    assert_eq!(template.max_health(), Some(100.0));
    assert!(template
        .motion_controllers()
        .iter()
        .any(|controller| controller.get("Type").and_then(Value::as_str) == Some("Walk")));
}

#[test]
fn the_real_flock_store_loads() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog =
        FlockCatalog::load_root(&repo_root().join("Assets/extracted")).expect("load flocks");
    assert_eq!(catalog.len(), 8);

    let pack = catalog.get("Pack_Small").expect("Pack_Small");
    assert_eq!(pack.size, FlockSize::Range { size: [2, 3] });
    let group = catalog.get("Group_Large").expect("Group_Large");
    assert_eq!(
        group.size,
        FlockSize::Weighted {
            min_size: 5,
            weights: vec![60.0, 20.0, 20.0],
        }
    );
    assert_eq!(group.max_grow_size, 8);
}

#[test]
fn bear_grizzly_resolves_through_its_template() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the real role catalog");
    let resolved = catalog
        .resolve("Bear_Grizzly")
        .expect("resolve Bear_Grizzly")
        .expect("Bear_Grizzly exists");
    assert_eq!(resolved.appearance(), Some("Bear_Grizzly"));
    assert_eq!(resolved.max_health(), Some(100.0));
    assert_eq!(resolved.view_range(), Some(12.0));
    let walk = resolved
        .motion_controllers()
        .iter()
        .find(|controller| controller.get("Type").and_then(Value::as_str) == Some("Walk"))
        .expect("inherited Walk controller");
    assert_eq!(walk.get("MaxWalkSpeed").and_then(Value::as_f64), Some(6.0));
    assert_eq!(
        resolved.parameter("MaxSpeed").and_then(Value::as_f64),
        Some(6.0)
    );
}

#[test]
fn every_role_resolves_with_a_deterministic_rng() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the real role catalog");
    let mut failures = Vec::new();
    let mut rng = |low: f64, high: f64| low + (high - low) * 0.5;
    for role in catalog.iter() {
        if let Err(error) = catalog.resolve_with_rng(&role.key, &mut rng) {
            failures.push((role.key.clone(), error.to_string()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} roles failed to resolve: {failures:?}",
        failures.len()
    );
}
