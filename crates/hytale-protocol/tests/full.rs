//! Round-trips for non-trivial generated field shapes.
//!
//! The default-value tests in `registry.rs` cover every packet but leave
//! variable fields empty. These cases exercise the byte layouts that only
//! appear with real content: struct/map/array combinations, non-string map
//! keys, unions, bit-packed booleans, fixed arrays and JOML vectors.

use std::collections::BTreeMap;

use hytale_protocol::generated::assets::UpdateUnarmedInteractions;
use hytale_protocol::generated::common::{
    AssetIconProperties, AssetInfo, AssetPath, Cloud, ColorAlpha, InteractionType, ItemArmor,
    ItemLibrary, Modifier, ResistanceModifier, UpdateType,
};
use hytale_protocol::generated::wire::Wire;
use hytale_protocol::generated::world::SetColumn;
use hytale_protocol::io::{Reader, Writer};

fn roundtrip<T>(value: &T) -> T
where
    T: Wire + PartialEq + std::fmt::Debug,
{
    let mut writer = Writer::new();
    value.encode(&mut writer).expect("encode");
    assert_eq!(
        writer.len(),
        value.compute_size().expect("compute size"),
        "computed size must match encoded size"
    );

    let mut reader = Reader::new(writer.as_slice());
    let decoded = T::decode(&mut reader).expect("decode");
    assert!(reader.is_empty(), "decode must consume exactly one value");
    assert_eq!(&decoded, value);
    decoded
}

#[test]
fn bit_packed_booleans_and_nullable_structs() {
    let value = AssetInfo {
        path: Some(AssetPath {
            pack: Some("base".to_string()),
            path: Some("Server/Blocks.json".to_string()),
        }),
        old_path: None,
        is_deleted: true,
        is_new: true,
        last_modification_date: 1_700_000_000,
        last_modification_username: Some("steve".to_string()),
    };
    roundtrip(&value);
}

#[test]
fn maps_of_arrays_of_structs() {
    let mut stats = BTreeMap::new();
    stats.insert(
        7,
        vec![Modifier {
            target: Default::default(),
            calculation_type: Default::default(),
            amount: 1.5,
        }],
    );
    let mut resistance = BTreeMap::new();
    resistance.insert(
        "Physical".to_string(),
        vec![ResistanceModifier {
            calculation_type: Default::default(),
            amount: 0.25,
        }],
    );
    let armor = ItemArmor {
        armor_slot: Default::default(),
        cosmetics_to_hide: Some(vec![Default::default()]),
        stat_modifiers: Some(stats),
        base_damage_resistance: 0.5,
        damage_resistance: Some(resistance),
        damage_enhancement: None,
        damage_class_enhancement: None,
        movement_settings: None,
    };
    roundtrip(&armor);
}

#[test]
fn numeric_and_enum_map_keys() {
    let mut interactions = BTreeMap::new();
    interactions.insert(InteractionType::Primary, 3);
    interactions.insert(InteractionType::Secondary, 4);
    roundtrip(&UpdateUnarmedInteractions {
        r#type: UpdateType::Init,
        interactions: Some(interactions),
    });
}

#[test]
fn float_keyed_maps_are_ordered_pairs() {
    let cloud = Cloud {
        texture: Some("Clouds/Default".to_string()),
        speeds: Some(vec![(0.5, 1.0), (1.5, 2.0)]),
        colors: Some(vec![(0.25, ColorAlpha::default())]),
    };
    roundtrip(&cloud);
}

#[test]
fn arrays_of_maps() {
    let mut first = BTreeMap::new();
    first.insert(1, "one".to_string());
    first.insert(2, "two".to_string());
    let library = ItemLibrary {
        items: None,
        block_map: Some(vec![BTreeMap::new(), first]),
    };
    roundtrip(&library);
}

#[test]
fn fixed_array_and_variable_byte_array() {
    let mut heightmap = vec![0i32; 1024];
    heightmap[0] = 64;
    heightmap[1023] = -1;
    let column = SetColumn {
        x: 4,
        z: -5,
        heightmap,
        tintmap: vec![1, 2, 3, 4],
    };
    roundtrip(&column);
}

#[test]
fn joml_vectors_are_three_and_two_float_components() {
    let value = AssetIconProperties {
        scale: 1.0,
        translation: Some(hytale_protocol::generated::wire::Vector2f { x: 1.0, y: 2.0 }),
        rotation: Some(hytale_protocol::generated::wire::Vector3f {
            x: 3.0,
            y: 4.0,
            z: 5.0,
        }),
    };
    roundtrip(&value);
}
