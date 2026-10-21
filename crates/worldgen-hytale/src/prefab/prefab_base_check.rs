// Ported from: `.hytale-source/.../worldgen/prefab/PrefabBaseCheck.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.PrefabBaseCheck` — the
//! additional height checks around the base of a prefab.

use serde_json::Value;

use procedural::json::JsonLoaderBase;

/// Java `public class PrefabBaseCheck`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrefabBaseCheck {
    /// Java `protected float baseScale` — the size of the prefab's base as a
    /// proportion of its horizontal dimensions.
    pub base_scale: f32,
    /// Java `protected int prefabSize` — the minimum size of the prefab the
    /// check applies to.
    pub prefab_size: i32,
    /// Java `protected int heightTolerance` — the allowed terrain height
    /// difference around the base.
    pub height_tolerance: i32,
}

impl Default for PrefabBaseCheck {
    fn default() -> Self {
        Self {
            base_scale: Self::DEFAULT_BASE_SCALE,
            prefab_size: Self::DEFAULT_PREFAB_SIZE,
            height_tolerance: Self::DEFAULT_HEIGHT_TOLERANCE,
        }
    }
}

impl PrefabBaseCheck {
    /// Java `public static final String KEY_BASE_SCALE` = `"BaseScale"`.
    pub const KEY_BASE_SCALE: &'static str = "BaseScale";
    /// Java `public static final String KEY_PREFAB_SIZE` = `"PrefabSize"`.
    pub const KEY_PREFAB_SIZE: &'static str = "PrefabSize";
    /// Java `public static final String KEY_HEIGHT_TOLERANCE` =
    /// `"HeightTolerance"`.
    pub const KEY_HEIGHT_TOLERANCE: &'static str = "HeightTolerance";
    /// Java `public static final float DEFAULT_BASE_SCALE` = `1.0f`.
    pub const DEFAULT_BASE_SCALE: f32 = 1.0;
    /// Java `public static final int DEFAULT_PREFAB_SIZE` = `20`.
    pub const DEFAULT_PREFAB_SIZE: i32 = 20;
    /// Java `public static final int DEFAULT_HEIGHT_TOLERANCE` = `2`.
    pub const DEFAULT_HEIGHT_TOLERANCE: i32 = 2;

    /// Java `public float getBaseScale()`.
    pub fn get_base_scale(&self) -> f32 {
        self.base_scale
    }

    /// Java `public int getPrefabSize()`.
    pub fn get_prefab_size(&self) -> i32 {
        self.prefab_size
    }

    /// Java `public int getHeightTolerance()`.
    pub fn get_height_tolerance(&self) -> i32 {
        self.height_tolerance
    }

    /// Java `public static PrefabBaseCheck[] loadAll(@Nonnull final
    /// JsonLoader<T, ?> parent, @Nullable final JsonElement element)`.
    ///
    /// The checks are sorted by their prefab size (descending) so that the
    /// first check that is "greater than or equal" to the prefab size wins.
    pub fn load_all(element: Option<&Value>) -> Vec<PrefabBaseCheck> {
        let Some(Value::Array(array)) = element else {
            return Vec::new();
        };
        if array.is_empty() {
            return Vec::new();
        }
        let mut checks: Vec<PrefabBaseCheck> = array
            .iter()
            .map(|element| {
                let mut check = PrefabBaseCheck::default();
                if let Some(value) = element.get(Self::KEY_BASE_SCALE) {
                    check.base_scale = JsonLoaderBase::get_as_double(value) as f32;
                }
                if let Some(value) = element.get(Self::KEY_PREFAB_SIZE) {
                    check.prefab_size = JsonLoaderBase::get_as_int(value);
                }
                if let Some(value) = element.get(Self::KEY_HEIGHT_TOLERANCE) {
                    check.height_tolerance = JsonLoaderBase::get_as_int(value);
                }
                check
            })
            .collect();
        // Java: `Arrays.sort(checks, Comparator.comparingInt(PrefabBaseCheck::getPrefabSize).reversed())`
        checks.sort_by_key(|check| std::cmp::Reverse(check.prefab_size));
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_match_the_java_constants() {
        let check = PrefabBaseCheck::default();
        assert_eq!(check.get_base_scale(), 1.0);
        assert_eq!(check.get_prefab_size(), 20);
        assert_eq!(check.get_height_tolerance(), 2);
    }

    #[test]
    fn the_checks_are_sorted_by_their_prefab_size() {
        let json = serde_json::json!([
            {"BaseScale": 0.5, "PrefabSize": 10, "HeightTolerance": 1},
            {"BaseScale": 1.0, "PrefabSize": 30, "HeightTolerance": 3}
        ]);
        let checks = PrefabBaseCheck::load_all(Some(&json));
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].get_prefab_size(), 30);
        assert_eq!(checks[1].get_prefab_size(), 10);
        assert_eq!(checks[0].get_height_tolerance(), 3);
    }

    #[test]
    fn an_absent_array_yields_no_checks() {
        assert!(PrefabBaseCheck::load_all(None).is_empty());
        assert!(PrefabBaseCheck::load_all(Some(&serde_json::json!({}))).is_empty());
    }
}
