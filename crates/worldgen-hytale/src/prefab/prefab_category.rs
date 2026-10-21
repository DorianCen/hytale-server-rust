// Ported from: `.hytale-source/.../worldgen/prefab/PrefabCategory.java`
//! Java `prefab.PrefabCategory` (record).

use serde_json::Value;

use procedural::json::JsonLoaderBase;

/// Java `PrefabCategory(String name, int priority)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefabCategory {
    /// Java `String name`.
    pub name: String,
    /// Java `int priority`.
    pub priority: i32,
}

impl PrefabCategory {
    /// Java `PrefabCategory.FILENAME`.
    pub const FILENAME: &'static str = "PrefabCategories.json";
    /// Java `PrefabCategory.MIN_PRIORITY`.
    pub const MIN_PRIORITY: i32 = i32::MIN;
    /// Java `PrefabCategory.MAX_PRIORITY`.
    pub const MAX_PRIORITY: i32 = i32::MAX;
    /// Java `PrefabCategory.NONE`.
    pub fn none() -> Self {
        Self {
            name: "None".to_string(),
            priority: Self::MIN_PRIORITY,
        }
    }
    /// Java `PrefabCategory.UNIQUE`.
    pub fn unique() -> Self {
        Self {
            name: "Unique".to_string(),
            priority: Self::MAX_PRIORITY,
        }
    }

    /// Java `static void parse(JsonElement json, BiConsumer<String, PrefabCategory> consumer)`.
    pub fn parse(json: Option<&Value>, mut consumer: impl FnMut(&str, PrefabCategory)) {
        let Some(json) = json else {
            return;
        };
        let Some(obj) = json.as_object() else {
            return;
        };

        for (name, value) in obj {
            if !value.is_number() {
                panic!("Invalid prefab category priority for '{name}'. Must be an integer");
            }
            let priority = JsonLoaderBase::get_as_int(value);
            consumer(
                name,
                PrefabCategory {
                    name: name.clone(),
                    priority,
                },
            );
        }
    }
}
