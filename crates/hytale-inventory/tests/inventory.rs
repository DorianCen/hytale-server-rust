//! Scripted inventory scenarios following the Java container semantics.

use std::path::PathBuf;

use hytale_inventory::{
    container_from_packet, ItemRules, ItemStack, PlayerInventory, Section, SimpleItemContainer,
};
use serde_json::Value;

struct Rules;

impl ItemRules for Rules {
    fn item_exists(&self, item_id: &str) -> bool {
        !item_id.is_empty()
    }

    fn max_stack(&self, item_id: &str) -> i32 {
        if item_id.contains("Sword") {
            1
        } else {
            100
        }
    }

    fn max_durability(&self, _item_id: &str) -> f64 {
        0.0
    }

    fn quality_index(&self, _item_id: &str) -> i32 {
        0
    }
}

fn stack(item_id: &str, quantity: i32) -> ItemStack {
    ItemStack::new(item_id, quantity, &Rules).expect("valid stack")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn adds_merge_partial_stacks_then_use_empty_slots() {
    let mut container = SimpleItemContainer::new(3);
    let first = container.add_item_stack(stack("Rock_Stone", 60), &Rules);
    assert_eq!(first.added, 60);
    assert!(first.remainder.is_none());
    assert_eq!(container.get_slot(0).unwrap().unwrap().quantity, 60);

    let second = container.add_item_stack(stack("Rock_Stone", 60), &Rules);
    assert_eq!(second.added, 60);
    assert!(second.remainder.is_none());
    assert_eq!(container.get_slot(0).unwrap().unwrap().quantity, 100);
    assert_eq!(container.get_slot(1).unwrap().unwrap().quantity, 20);
}

#[test]
fn splits_a_stack_into_a_target_slot() {
    let mut source = SimpleItemContainer::new(2);
    source.set_slot(0, Some(stack("Rock_Stone", 10))).unwrap();
    let mut destination = SimpleItemContainer::new(2);

    let outcome = source
        .move_to_slot(0, 4, &mut destination, 1, &Rules)
        .unwrap();
    assert_eq!(outcome.moved, 4);
    assert!(!outcome.swapped);
    assert_eq!(source.get_slot(0).unwrap().unwrap().quantity, 6);
    assert_eq!(destination.get_slot(1).unwrap().unwrap().quantity, 4);
}

#[test]
fn merges_into_a_partial_target_and_leaves_overflow_in_the_source() {
    let mut source = SimpleItemContainer::new(1);
    source.set_slot(0, Some(stack("Rock_Stone", 10))).unwrap();
    let mut destination = SimpleItemContainer::new(1);
    destination
        .set_slot(0, Some(stack("Rock_Stone", 95)))
        .unwrap();

    let outcome = source
        .move_to_slot(0, 10, &mut destination, 0, &Rules)
        .unwrap();
    assert_eq!(outcome.moved, 5);
    assert_eq!(outcome.remainder.unwrap().quantity, 5);
    assert_eq!(source.get_slot(0).unwrap().unwrap().quantity, 5);
    assert_eq!(destination.get_slot(0).unwrap().unwrap().quantity, 100);
}

#[test]
fn swaps_whole_incompatible_stacks_only() {
    let mut source = SimpleItemContainer::new(1);
    source
        .set_slot(0, Some(stack("Weapon_Longsword", 1)))
        .unwrap();
    let mut destination = SimpleItemContainer::new(1);
    destination
        .set_slot(0, Some(stack("Potion_Health", 3)))
        .unwrap();
    let outcome = source
        .move_to_slot(0, 1, &mut destination, 0, &Rules)
        .unwrap();
    assert!(outcome.swapped);
    assert_eq!(
        source.get_slot(0).unwrap().unwrap().item_id,
        "Potion_Health"
    );
    assert_eq!(
        destination.get_slot(0).unwrap().unwrap().item_id,
        "Weapon_Longsword"
    );

    let mut source = SimpleItemContainer::new(1);
    source.set_slot(0, Some(stack("Rock_Stone", 5))).unwrap();
    let mut destination = SimpleItemContainer::new(1);
    destination
        .set_slot(0, Some(stack("Potion_Health", 3)))
        .unwrap();
    let before_source = source.clone();
    let before_destination = destination.clone();
    let outcome = source
        .move_to_slot(0, 2, &mut destination, 0, &Rules)
        .unwrap();
    assert!(!outcome.changed());
    assert_eq!(source, before_source);
    assert_eq!(destination, before_destination);
}

#[test]
fn moves_within_one_container_merge_swap_and_leave_source() {
    let mut container = SimpleItemContainer::new(2);
    container
        .set_slot(0, Some(stack("Rock_Stone", 10)))
        .unwrap();
    let outcome = container.move_within(0, 4, 1, &Rules).unwrap();
    assert_eq!(outcome.moved, 4);
    assert_eq!(container.get_slot(0).unwrap().unwrap().quantity, 6);
    assert_eq!(container.get_slot(1).unwrap().unwrap().quantity, 4);

    let mut container = SimpleItemContainer::new(2);
    container.set_slot(0, Some(stack("Rock_Stone", 5))).unwrap();
    container
        .set_slot(1, Some(stack("Potion_Health", 2)))
        .unwrap();
    let outcome = container.move_within(0, 5, 1, &Rules).unwrap();
    assert!(outcome.swapped);
    assert_eq!(
        container.get_slot(0).unwrap().unwrap().item_id,
        "Potion_Health"
    );
    assert_eq!(
        container.get_slot(1).unwrap().unwrap().item_id,
        "Rock_Stone"
    );
}

#[test]
fn counts_and_removes_items_across_sections() {
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(0, Some(stack("Rock_Stone", 5)))
        .unwrap();
    inventory
        .storage
        .set_slot(3, Some(stack("Rock_Stone", 7)))
        .unwrap();
    inventory
        .storage
        .set_slot(4, Some(stack("Plant_Fiber", 2)))
        .unwrap();
    assert_eq!(inventory.count_item("Rock_Stone"), 12);
    assert_eq!(inventory.count_item("Missing"), 0);
    assert_eq!(inventory.remove_item("Rock_Stone", 8), 8);
    assert_eq!(inventory.count_item("Rock_Stone"), 4);
    assert_eq!(inventory.remove_item("Rock_Stone", 10), 4);
    assert_eq!(inventory.count_item("Rock_Stone"), 0);
    assert_eq!(inventory.count_item("Plant_Fiber"), 2);
}

#[test]
fn drop_removes_a_quantity_and_pickup_fills_existing_then_empty_slots() {
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(0, Some(stack("Rock_Stone", 5)))
        .unwrap();
    let dropped = inventory
        .drop_from_slot(Section::Hotbar, 0, 3)
        .unwrap()
        .expect("dropped stack");
    assert_eq!(dropped.quantity, 3);
    assert_eq!(
        inventory
            .hotbar
            .container
            .get_slot(0)
            .unwrap()
            .unwrap()
            .quantity,
        2
    );

    let remainder = inventory.pickup(stack("Rock_Stone", 110), &Rules);
    assert!(remainder.is_none());
    assert_eq!(
        inventory
            .hotbar
            .container
            .get_slot(0)
            .unwrap()
            .unwrap()
            .quantity,
        100
    );
    assert_eq!(
        inventory
            .hotbar
            .container
            .get_slot(1)
            .unwrap()
            .unwrap()
            .quantity,
        12
    );
}

#[test]
fn active_slots_and_held_items_follow_java_clamping() {
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(1, Some(stack("Rock_Stone", 5)))
        .unwrap();
    inventory
        .utility
        .container
        .set_slot(2, Some(stack("Potion_Health", 3)))
        .unwrap();
    assert!(inventory.set_active_slot(Section::Hotbar, 1));
    assert_eq!(inventory.right_hand().unwrap().item_id, "Rock_Stone");
    assert!(inventory.set_active_slot(Section::Utility, 2));
    assert_eq!(inventory.left_hand().unwrap().item_id, "Potion_Health");
    assert!(!inventory.set_active_slot(Section::Hotbar, 99));
    assert_eq!(inventory.hotbar.active_slot, Some(1));
}

#[test]
fn java_player_fixture_round_trips_through_the_inventory_components() {
    let path = repo_root()
        .join("testdata/golden/server/player-data/22345678-1234-5678-9abc-def012345678.json");
    let text = std::fs::read_to_string(path).expect("read Java player fixture");
    let data: Value = serde_json::from_str(&text).expect("parse fixture");
    let mut components = data
        .get("Components")
        .and_then(Value::as_object)
        .expect("Components")
        .clone();

    let inventory = PlayerInventory::from_components(&components).expect("load inventory");
    assert_eq!(inventory.hotbar.active_slot, Some(0));
    assert_eq!(
        inventory
            .hotbar
            .container
            .get_slot(0)
            .unwrap()
            .unwrap()
            .item_id,
        "Weapon_Longsword_Adamantite"
    );
    assert_eq!(
        inventory
            .utility
            .container
            .get_slot(0)
            .unwrap()
            .unwrap()
            .quantity,
        3
    );
    assert_eq!(
        inventory.armor.get_slot(1).unwrap().unwrap().item_id,
        "Armor_Adamantite_Chest"
    );

    inventory.write_components(&mut components);
    let reloaded = PlayerInventory::from_components(&components).expect("reload inventory");
    assert_eq!(reloaded, inventory);
    let hotbar = components
        .get("HotbarInventory")
        .and_then(|value| value.get("Inventory"))
        .expect("HotbarInventory.Inventory");
    assert_eq!(hotbar.get("Id").and_then(Value::as_str), Some("Simple"));
    assert_eq!(hotbar.get("Capacity").and_then(Value::as_i64), Some(9));
    assert_eq!(
        hotbar
            .get("Items")
            .and_then(|items| items.get("0"))
            .and_then(|item| item.get("Id"))
            .and_then(Value::as_str),
        Some("Weapon_Longsword_Adamantite")
    );
}

#[test]
fn legacy_inventory_component_is_used_as_a_fallback() {
    let source = r#"{
      "Components": {
        "Inventory": {
          "Armor": {"Id":"Simple","Capacity":4,"Items":{"0":{"Id":"Armor_Bronze_Head","Quantity":1}}},
          "HotBar": {"Id":"Simple","Capacity":9,"Items":{"3":{"Id":"Rock_Stone","Quantity":7}}},
          "Utility": {"Id":"Simple","Capacity":4,"Items":{"1":{"Id":"Potion_Health","Quantity":2}}},
          "ActiveHotbarSlot": 3,
          "ActiveUtilitySlot": 1
        }
      }
    }"#;
    let data: Value = serde_json::from_str(source).unwrap();
    let components = data
        .get("Components")
        .and_then(Value::as_object)
        .unwrap()
        .clone();
    let inventory = PlayerInventory::from_components(&components).unwrap();
    assert_eq!(inventory.hotbar.active_slot, Some(3));
    assert_eq!(inventory.right_hand().unwrap().item_id, "Rock_Stone");
    assert_eq!(inventory.left_hand().unwrap().item_id, "Potion_Health");
    assert_eq!(
        inventory.armor.get_slot(0).unwrap().unwrap().item_id,
        "Armor_Bronze_Head"
    );
}

#[test]
fn packet_sections_round_trip_to_a_container() {
    let mut container = SimpleItemContainer::new(2);
    container.set_slot(0, Some(stack("Rock_Stone", 5))).unwrap();
    let packet = container.to_packet();
    let decoded = container_from_packet(&packet).expect("decode packet container");
    assert_eq!(decoded, container);
}
