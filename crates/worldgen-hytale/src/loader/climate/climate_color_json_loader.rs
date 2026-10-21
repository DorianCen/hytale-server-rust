// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateColorJsonLoader.java`
//! Java `loader.climate.ClimateColorJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::climate_color::ClimateColor;
use crate::loader::color_util::ColorUtil;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ClimateColorJsonLoader.Constants.KEY_COLOR`.
pub const KEY_COLOR: &str = "Color";
/// Java `ClimateColorJsonLoader.Constants.KEY_SHORE`.
pub const KEY_SHORE: &str = "Shore";
/// Java `ClimateColorJsonLoader.Constants.KEY_OCEAN`.
pub const KEY_OCEAN: &str = "Ocean";
/// Java `ClimateColorJsonLoader.Constants.KEY_SHALLOW_OCEAN`.
pub const KEY_SHALLOW_OCEAN: &str = "ShallowOcean";

/// Java `ClimateColorJsonLoader`.
pub struct ClimateColorJsonLoader {
    /// Java `extends JsonLoader<K, ClimateColor>`.
    pub base: JsonLoaderBase,
    /// Java `@Nullable private final ClimateColor parent`.
    parent: Option<ClimateColor>,
}

impl ClimateColorJsonLoader {
    /// Java `ClimateColorJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, ClimateColor parent)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        parent: Option<ClimateColor>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            parent,
        }
    }

    /// Java `ClimateColor load()`.
    pub fn load(&self) -> ClimateColor {
        ClimateColor::new(
            self.load_color(KEY_COLOR, ClimateColor::UNSET),
            self.load_color(KEY_SHORE, self.parent_shore()),
            self.load_color(KEY_OCEAN, self.parent_ocean()),
            self.load_color(KEY_SHALLOW_OCEAN, self.parent_shallow_ocean()),
        )
    }

    /// Java `int loadColor(String key, int defaultColor)`.
    pub fn load_color(&self, key: &str, default_color: i32) -> i32 {
        if self.base.has(key) {
            let color = self.base.must_get_string(key, None);
            return ColorUtil::hex_string(&color);
        }
        default_color
    }

    fn parent_shore(&self) -> i32 {
        self.parent.map(|p| p.shore).unwrap_or(ClimateColor::UNSET)
    }

    fn parent_ocean(&self) -> i32 {
        self.parent.map(|p| p.ocean).unwrap_or(ClimateColor::UNSET)
    }

    fn parent_shallow_ocean(&self) -> i32 {
        self.parent
            .map(|p| p.shallow_ocean)
            .unwrap_or(ClimateColor::UNSET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value, parent: Option<ClimateColor>) -> ClimateColorJsonLoader {
        ClimateColorJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json), parent)
    }

    #[test]
    fn unset_defaults_without_parent() {
        let color = loader(serde_json::json!({}), None).load();
        assert_eq!(color.land, ClimateColor::UNSET);
        assert_eq!(color.shore, ClimateColor::UNSET);
        assert_eq!(color.ocean, ClimateColor::UNSET);
        assert_eq!(color.shallow_ocean, ClimateColor::UNSET);
    }

    #[test]
    fn parent_supplies_the_missing_slots() {
        let parent = ClimateColor::new(1, 2, 3, 4);
        let color = loader(serde_json::json!({ "Color": "#ff0000" }), Some(parent)).load();
        assert_eq!(color.land, 0xFF0000);
        assert_eq!(color.shore, 2);
        assert_eq!(color.ocean, 3);
        assert_eq!(color.shallow_ocean, 4);
    }

    #[test]
    fn hex_colors_are_parsed() {
        let color = loader(
            serde_json::json!({
                "Color": "#00ff00",
                "Shore": "#123456",
                "Ocean": "#abcdef",
                "ShallowOcean": "#010203"
            }),
            None,
        )
        .load();
        assert_eq!(color.land, 0x00FF00);
        assert_eq!(color.shore, 0x123456);
        assert_eq!(color.ocean, 0xABCDEF);
        assert_eq!(color.shallow_ocean, 0x010203);
    }
}
