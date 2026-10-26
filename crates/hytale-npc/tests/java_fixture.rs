//! Java NPC navigation/steering fixture gate.
//!
//! The fixture is produced by `tools/java-dumper/run-npc.sh` from the real
//! `SearchDirections`, `SteeringForcePursue/Evade/Rotate` and
//! `NPCPhysicsMath.isInViewCone` classes.

use std::path::PathBuf;

use hytale_npc::combat::{DamageCalculator, DamageType};
use hytale_npc::navigation::search_directions;
use hytale_npc::perception::is_in_view_cone;
use hytale_npc::steering::{
    Steering, SteeringForceEvade, SteeringForcePursue, SteeringForceRotate,
};
use hytale_world::collision::Vec3;
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> Value {
    let path = repo_root().join("testdata/golden/server/npc/npc.json");
    let bytes = std::fs::read(&path).expect("read the NPC fixture");
    serde_json::from_slice(&bytes).expect("NPC fixture JSON")
}

fn vector(value: &Value) -> Vec3 {
    let values = value.as_array().expect("vector array");
    Vec3::new(
        values[0].as_f64().expect("x"),
        values[1].as_f64().expect("y"),
        values[2].as_f64().expect("z"),
    )
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

#[test]
fn search_directions_match_the_java_record() {
    let fixture = fixture();
    let cases = fixture["searchDirections"]
        .as_object()
        .expect("searchDirections object");
    for (name, expected) in cases {
        let (is_2d, diagonal, projected) = match name.as_str() {
            "walkNoDiagonal" => (true, false, [false, true, false]),
            "walkDiagonal" => (true, true, [false, true, false]),
            "planeXyNoDiagonal" => (true, false, [false, false, true]),
            "planeYzNoDiagonal" => (true, false, [true, false, false]),
            "flyNoDiagonal" => (false, false, [false, false, false]),
            "flyDiagonal" => (false, true, [false, false, false]),
            other => panic!("unexpected fixture case {other}"),
        };
        let actual = search_directions(is_2d, diagonal, projected);
        let directions = expected["directions"].as_array().expect("directions");
        let distances = expected["distances"].as_array().expect("distances");
        let inverses = expected["inverseDirections"]
            .as_array()
            .expect("inverseDirections");
        assert_eq!(actual.len(), directions.len(), "{name}");
        for (index, direction) in actual.iter().enumerate() {
            let expected_delta = vector(&directions[index]);
            assert!(
                close(direction.delta.x, expected_delta.x),
                "{name}[{index}] x"
            );
            assert!(
                close(direction.delta.y, expected_delta.y),
                "{name}[{index}] y"
            );
            assert!(
                close(direction.delta.z, expected_delta.z),
                "{name}[{index}] z"
            );
            assert!(
                close(direction.distance, distances[index].as_f64().unwrap()),
                "{name}[{index}] distance"
            );
            assert_eq!(
                direction.inverse,
                inverses[index].as_u64().unwrap() as usize,
                "{name}[{index}] inverse"
            );
        }
        match expected["normalDirections"].as_array() {
            Some(normals) => {
                assert_eq!(
                    expected["normalsPerDirection"].as_u64().unwrap(),
                    1,
                    "{name} normalsPerDirection"
                );
                assert_eq!(actual.len(), normals.len(), "{name} normals length");
                for (index, normal) in normals.iter().enumerate() {
                    assert_eq!(
                        actual[index].normal,
                        Some(normal.as_u64().unwrap() as usize),
                        "{name}[{index}] normal"
                    );
                }
            }
            None => {
                assert_eq!(
                    expected["normalsPerDirection"].as_u64().unwrap(),
                    0,
                    "{name} normalsPerDirection"
                );
                assert!(actual.iter().all(|direction| direction.normal.is_none()));
            }
        }
    }
}

#[test]
fn steering_forces_match_the_java_output() {
    let fixture = fixture();
    let cases = fixture["steering"].as_object().expect("steering object");

    for case in cases["pursue"].as_array().expect("pursue cases") {
        let mut force = SteeringForcePursue::new(
            case["stopDistance"].as_f64().unwrap(),
            case["slowdownDistance"].as_f64().unwrap(),
        );
        force.set_falloff(case["falloff"].as_f64().unwrap());
        force.set_component_selector(Vec3::new(1.0, 1.0, 1.0));
        force.set_positions(vector(&case["self"]), vector(&case["target"]));
        let mut steering = Steering::default();
        let result = force.compute(&mut steering);
        assert_eq!(result, case["result"].as_bool().unwrap(), "pursue result");
        assert_eq!(
            steering.has_translation(),
            case["hasTranslation"].as_bool().unwrap()
        );
        assert!(close(steering.translation().x, case["x"].as_f64().unwrap()));
        assert!(close(steering.translation().y, case["y"].as_f64().unwrap()));
        assert!(close(steering.translation().z, case["z"].as_f64().unwrap()));
    }

    for case in cases["evade"].as_array().expect("evade cases") {
        let mut force = SteeringForceEvade::new(
            case["slowdownDistance"].as_f64().unwrap(),
            case["stopDistance"].as_f64().unwrap(),
        );
        force.set_falloff(case["falloff"].as_f64().unwrap());
        force.set_direction_hint(case["directionHint"].as_f64().unwrap() as f32);
        force.set_adhere_to_direction_hint(case["adhereToDirectionHint"].as_bool().unwrap());
        force.set_component_selector(Vec3::new(1.0, 1.0, 1.0));
        force.set_positions(vector(&case["self"]), vector(&case["target"]));
        let mut steering = Steering::default();
        let result = force.compute(&mut steering);
        assert_eq!(result, case["result"].as_bool().unwrap(), "evade result");
        assert_eq!(
            steering.has_translation(),
            case["hasTranslation"].as_bool().unwrap()
        );
        assert!(close(steering.translation().x, case["x"].as_f64().unwrap()));
        assert!(close(steering.translation().y, case["y"].as_f64().unwrap()));
        assert!(close(steering.translation().z, case["z"].as_f64().unwrap()));
    }

    for case in cases["rotate"].as_array().expect("rotate cases") {
        let mut force = SteeringForceRotate::default();
        force.set_desired_heading(case["desiredHeading"].as_f64().unwrap() as f32);
        force.set_heading(case["heading"].as_f64().unwrap() as f32);
        force
            .set_tolerance((case["toleranceDegrees"].as_f64().unwrap() as f32).to_radians() as f64);
        let mut steering = Steering::default();
        let result = force.compute(&mut steering);
        assert_eq!(result, case["result"].as_bool().unwrap(), "rotate result");
        assert_eq!(
            steering.has_yaw(),
            case["hasYaw"].as_bool().unwrap(),
            "rotate hasYaw"
        );
        assert!((steering.yaw() - case["yaw"].as_f64().unwrap() as f32).abs() < 1e-6);
    }
}

#[test]
fn view_cone_cases_match_java() {
    let fixture = fixture();
    for case in fixture["perception"].as_array().expect("perception cases") {
        let viewer = vector(&case["viewer"]);
        let direction = vector(&case["viewDirection"]);
        let cos = case["cosHalfAngle"].as_f64().unwrap() as f32;
        let object = vector(&case["object"]);
        assert_eq!(
            is_in_view_cone(viewer, direction, cos, object),
            case["result"].as_bool().unwrap(),
            "{viewer:?} {direction:?} {cos} {object:?}"
        );
    }
}

#[test]
fn builder_expressions_match_the_java_engine() {
    use hytale_npc::expr::{evaluate, EvalContext, Value};

    let fixture = fixture();
    let vars = std::collections::BTreeMap::from([
        ("a".to_string(), Value::Number(2.0)),
        ("b".to_string(), Value::Number(9.0)),
        ("name".to_string(), Value::String("x".to_string())),
        ("flag".to_string(), Value::Bool(true)),
        ("values".to_string(), Value::NumberArray(vec![1.0, 2.0])),
        (
            "names".to_string(),
            Value::StringArray(vec!["x".to_string(), "y".to_string()]),
        ),
        ("flags".to_string(), Value::BoolArray(vec![true, false])),
        ("emptyValues".to_string(), Value::NumberArray(Vec::new())),
        ("emptyNames".to_string(), Value::StringArray(Vec::new())),
    ]);
    let mut context = EvalContext::new(&vars);
    for case in fixture["expressions"].as_array().expect("expressions") {
        let expression = case["expression"].as_str().expect("expression");
        let actual = evaluate(expression, &mut context)
            .unwrap_or_else(|error| panic!("{expression}: {error}"));
        assert_eq!(
            actual.type_name(),
            case["type"].as_str().unwrap(),
            "{expression}"
        );
        match actual {
            Value::Number(number) => {
                assert!(
                    (number - case["number"].as_f64().unwrap()).abs() < 1e-9,
                    "{expression}"
                );
            }
            Value::Bool(boolean) => {
                assert_eq!(boolean, case["boolean"].as_bool().unwrap(), "{expression}");
            }
            Value::String(string) => {
                assert_eq!(string, case["string"].as_str().unwrap(), "{expression}");
            }
            Value::NumberArray(numbers) => {
                let expected: Vec<f64> = case["numbers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_f64().unwrap())
                    .collect();
                assert_eq!(numbers, expected, "{expression}");
            }
            Value::StringArray(strings) => {
                let expected: Vec<String> = case["strings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap().to_string())
                    .collect();
                assert_eq!(strings, expected, "{expression}");
            }
            Value::BoolArray(booleans) => {
                let expected: Vec<bool> = case["booleans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_bool().unwrap())
                    .collect();
                assert_eq!(booleans, expected, "{expression}");
            }
            Value::EmptyArray => panic!("unexpected empty array for {expression}"),
        }
    }
}

#[test]
fn damage_ranges_match_the_java_damage_calculator() {
    let fixture = fixture();
    let cases = fixture["damageRanges"]
        .as_array()
        .expect("damageRanges array");
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let damage_type = match case["type"].as_str().expect("damage type") {
            "DPS" => DamageType::Dps,
            "ABSOLUTE" => DamageType::Absolute,
            other => panic!("unexpected damage type {other}"),
        };
        let base_damage = case["baseDamage"]
            .as_array()
            .expect("base damage")
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    format!("Cause{}", index + 1),
                    value.as_f64().expect("base damage value") as f32,
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let calculator = DamageCalculator {
            damage_type,
            base_damage,
            random_percentage_modifier: case["randomPercentageModifier"]
                .as_f64()
                .expect("random modifier") as f32,
            damage_class: None,
            source: None,
        };
        let duration = case["duration"].as_f64().expect("duration");
        let range = calculator.compute_range(duration);
        let expected_min = case["min"].as_f64().expect("min") as f32;
        let expected_max = case["max"].as_f64().expect("max") as f32;
        assert!(
            (range[0] - expected_min).abs() < 1e-5,
            "{name}: min {} != {expected_min}",
            range[0]
        );
        assert!(
            (range[1] - expected_max).abs() < 1e-5,
            "{name}: max {} != {expected_max}",
            range[1]
        );
    }
}
