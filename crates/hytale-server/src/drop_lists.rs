//! `Server/Drops` (`ItemDropList`) loading and weighted rolling.
//!
//! Block `Gathering` drops reference these lists by id. Java's
//! `ItemDropList` is a tree of `Single`/`Multiple`/`Choice`/`Droplist`/`Empty`
//! containers; the client only needs the resulting item stacks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hytale_math::fast_random::FastRandom;
use serde_json::Value;

/// All loaded `ItemDropList` assets keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct DropListCatalog {
    lists: BTreeMap<String, DropContainer>,
}

impl DropListCatalog {
    /// Load `<assets_root>/Server/Drops/**/*.json`.
    pub fn load(assets_root: &Path) -> Result<Self, String> {
        let root = assets_root.join("Server/Drops");
        if !root.is_dir() {
            return Ok(Self::default());
        }
        let mut files = Vec::new();
        walk(&root, &mut files)?;
        files.sort();
        let mut lists = BTreeMap::new();
        for path in files {
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            let json: Value = serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?;
            let Some(container) = json.get("Container").and_then(parse_container) else {
                continue;
            };
            lists.entry(id.to_string()).or_insert(container);
        }
        Ok(Self { lists })
    }

    /// Roll one drop list. Returns `(item_id, quantity)` pairs.
    pub fn roll(&self, id: &str, rng: &mut FastRandom) -> Vec<(String, i32)> {
        let mut out = Vec::new();
        if let Some(container) = self.lists.get(id) {
            self.roll_container(container, rng, &mut BTreeSet::new(), &mut out);
        }
        out
    }

    fn roll_container(
        &self,
        container: &DropContainer,
        rng: &mut FastRandom,
        visiting: &mut BTreeSet<String>,
        out: &mut Vec<(String, i32)>,
    ) {
        match &container.kind {
            ContainerKind::Single {
                item_id,
                quantity_min,
                quantity_max,
            } => {
                let span = (*quantity_max - *quantity_min + 1).max(1);
                let quantity = *quantity_min + rng.next_int_bound(span);
                if quantity > 0 {
                    out.push((item_id.clone(), quantity));
                }
            }
            ContainerKind::Multiple {
                containers,
                min_count,
                max_count,
            } => {
                let span = (*max_count - *min_count).max(0);
                let count = *min_count
                    + if span > 0 {
                        rng.next_int_bound(span + 1)
                    } else {
                        0
                    };
                for _ in 0..count {
                    for child in containers {
                        if child.weight >= rng.next_double() * 100.0 {
                            self.roll_container(child, rng, visiting, out);
                        }
                    }
                }
            }
            ContainerKind::Choice {
                containers,
                rolls_min,
                rolls_max,
            } => {
                let span = (*rolls_max - *rolls_min).max(0);
                let rolls = *rolls_min
                    + if span > 0 {
                        rng.next_int_bound(span + 1)
                    } else {
                        0
                    };
                let total: f64 = containers.iter().map(|child| child.weight.max(0.0)).sum();
                if total <= 0.0 {
                    return;
                }
                for _ in 0..rolls {
                    let mut pick = rng.next_double() * total;
                    for child in containers {
                        pick -= child.weight.max(0.0);
                        if pick <= 0.0 {
                            self.roll_container(child, rng, visiting, out);
                            break;
                        }
                    }
                }
            }
            ContainerKind::Droplist { id } => {
                if !visiting.insert(id.clone()) {
                    return;
                }
                if let Some(nested) = self.lists.get(id) {
                    self.roll_container(nested, rng, visiting, out);
                }
                visiting.remove(id);
            }
            ContainerKind::Empty => {}
        }
    }
}

#[derive(Debug, Clone)]
struct DropContainer {
    weight: f64,
    kind: ContainerKind,
}

#[derive(Debug, Clone)]
enum ContainerKind {
    Single {
        item_id: String,
        quantity_min: i32,
        quantity_max: i32,
    },
    Multiple {
        containers: Vec<DropContainer>,
        min_count: i32,
        max_count: i32,
    },
    Choice {
        containers: Vec<DropContainer>,
        rolls_min: i32,
        rolls_max: i32,
    },
    Droplist {
        id: String,
    },
    Empty,
}

fn parse_container(value: &Value) -> Option<DropContainer> {
    let object = value.as_object()?;
    let weight = object
        .get("Weight")
        .and_then(Value::as_f64)
        .unwrap_or(100.0);
    let kind = match object
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or("Single")
    {
        "Single" => {
            let item = object.get("Item")?.as_object()?;
            let item_id = item.get("ItemId").and_then(Value::as_str)?.to_string();
            ContainerKind::Single {
                item_id,
                quantity_min: integer(item.get("QuantityMin"), 1),
                quantity_max: integer(item.get("QuantityMax"), 1),
            }
        }
        "Multiple" => ContainerKind::Multiple {
            containers: containers(object.get("Containers"))?,
            min_count: integer(object.get("MinCount"), 1),
            max_count: integer(object.get("MaxCount"), 1),
        },
        "Choice" => ContainerKind::Choice {
            containers: containers(object.get("Containers"))?,
            rolls_min: integer(object.get("RollsMin"), 1),
            rolls_max: integer(object.get("RollsMax"), 1),
        },
        "Droplist" => ContainerKind::Droplist {
            id: object
                .get("DroplistId")
                .and_then(Value::as_str)?
                .to_string(),
        },
        _ => ContainerKind::Empty,
    };
    Some(DropContainer { weight, kind })
}

fn containers(value: Option<&Value>) -> Option<Vec<DropContainer>> {
    Some(
        value?
            .as_array()?
            .iter()
            .filter_map(parse_container)
            .collect(),
    )
}

fn integer(value: Option<&Value>, default: i32) -> i32 {
    value
        .and_then(Value::as_i64)
        .map_or(default, |value| value as i32)
}

fn walk(directory: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hytale-drop-lists-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn single_and_multiple_containers_roll_items() {
        let root = temp_root("single");
        let drops = root.join("Server/Drops");
        std::fs::create_dir_all(&drops).expect("drops dir");
        std::fs::write(
            drops.join("Test.json"),
            r#"{
              "Container": {
                "Type": "Multiple",
                "Containers": [
                  {
                    "Type": "Single",
                    "Weight": 100,
                    "Item": { "ItemId": "Rock_Stone", "QuantityMin": 2, "QuantityMax": 3 }
                  }
                ]
              }
            }"#,
        )
        .expect("write drop list");
        let catalog = DropListCatalog::load(&root).expect("load");
        let mut rng = FastRandom::new(7);
        let rolled = catalog.roll("Test", &mut rng);
        assert_eq!(rolled.len(), 1);
        assert_eq!(rolled[0].0, "Rock_Stone");
        assert!((2..=3).contains(&rolled[0].1));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn nested_droplist_references_do_not_recurse_forever() {
        let root = temp_root("nested");
        let drops = root.join("Server/Drops");
        std::fs::create_dir_all(&drops).expect("drops dir");
        std::fs::write(
            drops.join("A.json"),
            r#"{"Container":{"Type":"Droplist","DroplistId":"B"}}"#,
        )
        .expect("write A");
        std::fs::write(
            drops.join("B.json"),
            r#"{"Container":{"Type":"Droplist","DroplistId":"A"}}"#,
        )
        .expect("write B");
        let catalog = DropListCatalog::load(&root).expect("load");
        let mut rng = FastRandom::new(1);
        assert!(catalog.roll("A", &mut rng).is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}
