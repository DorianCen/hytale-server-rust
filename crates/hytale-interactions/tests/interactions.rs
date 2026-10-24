use std::collections::BTreeMap;

use hytale_interactions::{
    execute_root, BlockInteractions, ExecutionContext, InteractionCatalog, WorldAction,
};
use hytale_protocol::generated::common::{GameMode, InteractionType};
use serde_json::{json, Value};

fn catalog(interactions: &[(&str, Value)], roots: &[(&str, Value)]) -> InteractionCatalog {
    InteractionCatalog::from_parts(
        interactions
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
        roots
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn walks_inline_and_referenced_door_nodes() {
    let catalog = catalog(
        &[("Door_Node", json!({"Type": "Door", "Horizontal": false}))],
        &[("Door_Root", json!({"Interactions": ["Door_Node"]}))],
    );
    let outcome = execute_root(&catalog, "Door_Root", &ExecutionContext::default());
    assert!(outcome.success);
    assert_eq!(
        outcome.actions,
        vec![WorldAction::Door { horizontal: false }]
    );
}

#[test]
fn condition_uses_failed_path_for_the_wrong_game_mode() {
    let catalog = catalog(
        &[
            (
                "Use_Node",
                json!({"Type": "UseBlock", "Failed": "Failed_Node"}),
            ),
            ("Failed_Node", json!({"Type": "OpenContainer"})),
        ],
        &[(
            "Root",
            json!({
                "Interactions": [{
                    "Type": "Condition",
                    "RequiredGameMode": "Creative",
                    "Next": "Use_Node",
                    "Failed": "Failed_Node"
                }]
            }),
        )],
    );
    let outcome = execute_root(
        &catalog,
        "Root",
        &ExecutionContext {
            game_mode: GameMode::Adventure,
            ..ExecutionContext::default()
        },
    );
    assert!(outcome.success);
    assert_eq!(outcome.actions, vec![WorldAction::OpenContainer]);
}

#[test]
fn replace_reads_interaction_vars_with_default_fallback() {
    let catalog = catalog(
        &[
            ("Open", json!({"Type": "OpenContainer"})),
            ("Default", json!({"Type": "Door"})),
        ],
        &[(
            "Root",
            json!({
                "Interactions": [{
                    "Type": "Replace",
                    "Var": "Action",
                    "DefaultValue": {"Interactions": ["Default"]}
                }]
            }),
        )],
    );
    let mut context = ExecutionContext::default();
    context
        .vars
        .insert("Action".to_string(), "Open".to_string());
    let outcome = execute_root(&catalog, "Root", &context);
    assert_eq!(outcome.actions, vec![WorldAction::OpenContainer]);

    context.vars.clear();
    let outcome = execute_root(&catalog, "Root", &context);
    assert_eq!(
        outcome.actions,
        vec![WorldAction::Door { horizontal: false }]
    );
}

#[test]
fn block_metadata_extracts_use_bench_container_and_state() {
    let block = json!({
        "Interactions": {"Use": "Door", "Primary": "Block_Primary"},
        "Bench": {"Type": "Crafting"},
        "State": {"Definitions": {"OpenDoorIn": {}, "OpenDoorOut": {}}}
    });
    let interactions =
        BlockInteractions::from_json("*Furniture_Door_State_Definitions_OpenDoorIn", &block);
    assert_eq!(interactions.root(InteractionType::Use), Some("Door"));
    assert!(interactions.bench);
    assert!(!interactions.container);
    assert!(interactions.is_door_open());

    let chest = json!({
        "Interactions": {"Use": "Open_Container"},
        "Container": {"Type": "Single"}
    });
    let chest = BlockInteractions::from_json("Furniture_Chest", &chest);
    assert!(chest.container);
    assert_eq!(chest.root(InteractionType::Use), Some("Open_Container"));
    assert_eq!(interactions.state.as_deref(), Some("OpenDoorIn"));
}

#[test]
fn container_root_dispatches_open_container() {
    let catalog = catalog(
        &[("Open_Container", json!({"Type": "OpenContainer"}))],
        &[(
            "Open_Container",
            json!({"Interactions": ["Open_Container"]}),
        )],
    );
    let outcome = execute_root(&catalog, "Open_Container", &ExecutionContext::default());
    assert!(outcome.success);
    assert_eq!(outcome.actions, vec![WorldAction::OpenContainer]);
}

#[test]
fn loads_the_real_root_and_interaction_stores_when_available() {
    let pack = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/Assets.zip");
    if !pack.is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = hytale_assets::AssetPack::open(pack).expect("open pack");
    let catalog = InteractionCatalog::load(&pack).expect("load interactions");
    assert!(catalog.root_count() > 500);
    assert!(catalog.interaction_count() > 1_000);
    assert!(catalog.root("Door").is_some());
    assert!(catalog.root("Open_Container").is_some());
    let door = catalog.interaction("Door").expect("Door interaction");
    assert_eq!(door.get("Type").and_then(Value::as_str), Some("Door"));
}
