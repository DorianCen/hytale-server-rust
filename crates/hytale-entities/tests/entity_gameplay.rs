//! Real-pack stat asset checks.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_entities::damage::DamageCause;
use hytale_entities::stats::{EntityStatType, StatResetBehavior};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Option<AssetRegistry> {
    let pack = repo_root().join("Assets/Assets.zip");
    if !pack.is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(pack).expect("open the pack");
    Some(AssetRegistry::load_p0(&pack).expect("load P0"))
}

#[test]
fn entity_stat_ids_and_fields_match_the_real_store() {
    let Some(registry) = registry() else {
        return;
    };
    assert_eq!(registry.entity_stat_id("Unknown"), Some(0));
    let health_id = registry.entity_stat_id("Health").expect("Health stat");
    assert!(health_id > 0);
    let health = registry
        .entity_stat_types
        .get("Health")
        .expect("Health record");
    let health = EntityStatType::from_value("Health", health_id, &health.value).expect("decode");
    assert_eq!(health.initial_value, 100.0);
    assert_eq!((health.min, health.max), (0.0, 100.0));
    assert_eq!(health.reset_behavior, StatResetBehavior::MaxValue);
    assert!(health.shared);
    assert_eq!(health.regenerating.len(), 2);

    let stamina_id = registry.entity_stat_id("Stamina").expect("Stamina stat");
    let stamina = registry
        .entity_stat_types
        .get("Stamina")
        .expect("Stamina record");
    let stamina =
        EntityStatType::from_value("Stamina", stamina_id, &stamina.value).expect("decode");
    assert_eq!(
        (stamina.initial_value, stamina.min, stamina.max),
        (10.0, -4.0, 10.0)
    );
    assert_eq!(stamina.reset_behavior, StatResetBehavior::InitialValue);
}

#[test]
fn damage_cause_inheritance_matches_the_chain() {
    let physical = DamageCause {
        id: "Physical".to_string(),
        durability_loss: true,
        stamina_loss: true,
        ..DamageCause::default()
    };
    let slashing = DamageCause {
        id: "Slashing".to_string(),
        inherits: Some("Physical".to_string()),
        ..DamageCause::default()
    };
    let causes = hytale_entities::damage::DamageCauses::new([physical, slashing]);
    assert!(causes.is_or_inherits("Slashing", "Physical"));
    assert!(causes.is_or_inherits("Slashing", "Slashing"));
    assert!(!causes.is_or_inherits("Physical", "Slashing"));
}
