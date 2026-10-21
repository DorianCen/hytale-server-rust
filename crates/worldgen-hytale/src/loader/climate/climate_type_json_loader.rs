// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateTypeJsonLoader.java`
//! Java `loader.climate.ClimateTypeJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::climate::climate_color::ClimateColor;
use crate::climate::climate_point::ClimatePoint;
use crate::climate::climate_type::ClimateType;
use crate::loader::climate::climate_color_json_loader::ClimateColorJsonLoader;
use crate::loader::climate::climate_point_json_loader::ClimatePointJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ClimateTypeJsonLoader.Constants.KEY_NAME`.
pub const KEY_NAME: &str = "Name";
/// Java `ClimateTypeJsonLoader.Constants.KEY_POINTS`.
pub const KEY_POINTS: &str = "Points";
/// Java `ClimateTypeJsonLoader.Constants.KEY_CHILDREN`.
pub const KEY_CHILDREN: &str = "Children";
/// Java `ClimateTypeJsonLoader.Constants.KEY_ISLAND`.
pub const KEY_ISLAND: &str = "Island";

/// Java `ClimateTypeJsonLoader`.
pub struct ClimateTypeJsonLoader {
    /// Java `extends JsonLoader<K, ClimateType>`.
    pub base: JsonLoaderBase,
    /// Java `@Nullable private final ClimateType parent`.
    parent: Option<Rc<ClimateType>>,
}

impl ClimateTypeJsonLoader {
    /// Java `ClimateTypeJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, ClimateType parent)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        parent: Option<Rc<ClimateType>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            parent,
        }
    }

    /// Java `ClimateType load()`.
    pub fn load(&self) -> Rc<ClimateType> {
        let name = self.load_name();
        let color = ClimateColorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            self.parent.as_ref().map(|p| p.color),
        )
        .load();
        let island = self.load_island_color(color);
        let points = self.load_climate_points();
        // Java: the children are loaded with a *parent* climate type that
        // carries this type's color / island color (the children inherit the
        // unset color slots from it).
        let parent = Rc::new(ClimateType::new(
            name.clone(),
            color,
            island,
            points.clone(),
            Vec::new(),
        ));
        let children = self.load_children(parent);
        Rc::new(ClimateType::new(name, color, island, points, children))
    }

    /// Java `String loadName()`.
    pub fn load_name(&self) -> String {
        self.base.must_get_string(KEY_NAME, None)
    }

    /// Java `ClimateColor loadIslandColor(ClimateColor color)`.
    pub fn load_island_color(&self, color: ClimateColor) -> ClimateColor {
        if self.base.has(KEY_ISLAND) {
            return ClimateColorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_ISLAND),
                self.parent.as_ref().map(|p| p.island),
            )
            .load();
        }
        color
    }

    /// Java `ClimatePoint[] loadClimatePoints()`.
    pub fn load_climate_points(&self) -> Vec<ClimatePoint> {
        let array = self
            .base
            .must_get_array(KEY_POINTS, Some(serde_json::json!([])));
        let Some(items) = array.as_array() else {
            // `must_get_array` already enforces the type.
            unreachable!()
        };
        let mut points = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            points.push(self.load_point(index, item));
        }
        points
    }

    /// Java `ClimateType[] loadChildren(ClimateType parent)`.
    pub fn load_children(&self, parent: Rc<ClimateType>) -> Vec<Rc<ClimateType>> {
        let array = self
            .base
            .must_get_array(KEY_CHILDREN, Some(serde_json::json!([])));
        let Some(items) = array.as_array() else {
            unreachable!()
        };
        let mut children = Vec::with_capacity(items.len());
        for item in items.iter() {
            let Some(child_json) = item.as_object() else {
                // Java: `childrenArr.get(i).getAsJsonObject()` — a
                // ClassCastException when the element is not an object.
                panic!("Expected a JSON object for the climate type child");
            };
            children.push(
                ClimateTypeJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(Value::Object(child_json.clone())),
                    Some(parent.clone()),
                )
                .load(),
            );
        }
        children
    }

    /// Java `ClimatePoint loadPoint(int index, JsonElement pointsJson)`.
    pub fn load_point(&self, index: usize, points_json: &Value) -> ClimatePoint {
        let loader = ClimatePointJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(points_json.clone()),
        );
        match loader.try_load() {
            Ok(point) => point,
            // Java: `throw error(t, "Invalid climate point at index: %d", index)`.
            Err(_) => panic!("Invalid climate point at index: {index}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    fn loader(json: Value) -> ClimateTypeJsonLoader {
        ClimateTypeJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json), None)
    }

    #[test]
    fn loads_a_climate_type() {
        let climate = loader(serde_json::json!({
            "Name": "Zone 1",
            "Color": "#00ff00",
            "Points": [{ "Temperature": 0.5, "Intensity": 0.5 }]
        }))
        .load();

        assert_eq!(climate.name, "Zone 1");
        assert_eq!(climate.color.land, 0x00FF00);
        assert_eq!(climate.points.len(), 1);
        assert_eq!(climate.points[0].temperature, 0.5);
        assert_eq!(climate.points[0].intensity, 0.5);
        assert_eq!(climate.points[0].modifier, 1.0);
        assert!(climate.children.is_empty());
        // Without an "Island" section the island color mirrors the color.
        assert_eq!(climate.island.land, 0x00FF00);
    }

    #[test]
    fn children_inherit_the_parent_color_slots() {
        let climate = loader(serde_json::json!({
            "Name": "Zone 1",
            "Color": "#00ff00",
            "Children": [
                { "Name": "Tier 1", "Color": "#78ff27" }
            ]
        }))
        .load();

        assert_eq!(climate.children.len(), 1);
        let child = &climate.children[0];
        assert_eq!(child.name, "Tier 1");
        assert_eq!(child.color.land, 0x78FF27);
        // The unset slots are inherited from the parent climate type.
        assert_eq!(child.color.shore, climate.color.shore);
        assert_eq!(child.color.ocean, climate.color.ocean);
        assert_eq!(child.color.shallow_ocean, climate.color.shallow_ocean);
    }

    #[test]
    fn invalid_point_is_wrapped() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(serde_json::json!({
                "Name": "Zone 1",
                "Points": [{ "Temperature": "hot" }]
            }))
            .load()
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert_eq!(message, "Invalid climate point at index: 0");
    }
}
