// Ported from: `.hytale-source/.../worldgen/SeedStringResource.java`
//! Java `com.hypixel.hytale.server.worldgen.SeedStringResource` — the
//! resource every `SeedString` of a world load carries.
//!
//! The Java resource bundles
//!
//! * the biome-mask file cache (`FileMaskCache<IIntCondition>`),
//! * the block-mask registry (`BlockPlacementMaskRegistry`),
//! * the prefab loader (`WorldGenPrefabLoader`), and
//! * the **unique-name counters** (`Map<String, AtomicInteger> uniqueIds`).
//!
//! The caches and the prefab loader are unobservable (the cached values are
//! value-equal to the freshly loaded ones) and the prefab loader is Phase 5,
//! so the Rust port keeps the **unique-name counters** — they are *not*
//! unobservable: the generated names end up in the seed strings of the cave
//! node types (`CaveType#0`, `ChildCaveType#0`, …) and therefore in the
//! seeds of their noise layers.
//!
//! The counter is global for the whole world load (one resource instance),
//! so the numbers depend on the order in which the zones are loaded — the
//! same order the Java loader uses (the zone registry order).

use std::cell::RefCell;
use std::collections::HashMap;

/// Java `SeedStringResource`.
#[derive(Debug, Default)]
pub struct SeedStringResource {
    /// Java `private final Map<String, AtomicInteger> uniqueIds`.
    unique_ids: RefCell<HashMap<String, i32>>,
}

impl SeedStringResource {
    /// Java `SeedStringResource(PrefabStoreRoot, WorldGenConfig)` — the
    /// caches and the prefab loader are not ported, so only the unique-name
    /// counters are created.
    pub fn new() -> Self {
        Self {
            unique_ids: RefCell::new(HashMap::new()),
        }
    }

    /// Java `String getUniqueName(String prefix)` —
    /// `prefix + uniqueIds.computeIfAbsent(prefix, k -> new AtomicInteger(0))
    /// .getAndIncrement()`.
    ///
    /// The counter is per prefix and starts at `0`.
    pub fn get_unique_name(&self, prefix: &str) -> String {
        let mut unique_ids = self.unique_ids.borrow_mut();
        let counter = unique_ids.entry(prefix.to_string()).or_insert(0);
        let value = *counter;
        *counter += 1;
        format!("{prefix}{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counter_starts_at_zero_and_is_per_prefix() {
        let resource = SeedStringResource::new();
        assert_eq!(resource.get_unique_name("CaveType#"), "CaveType#0");
        assert_eq!(resource.get_unique_name("CaveType#"), "CaveType#1");
        assert_eq!(resource.get_unique_name("CaveType#"), "CaveType#2");
        // A different prefix has its own counter.
        assert_eq!(
            resource.get_unique_name("ChildCaveType#"),
            "ChildCaveType#0"
        );
        assert_eq!(resource.get_unique_name("CaveType#"), "CaveType#3");
    }

    #[test]
    fn two_resources_count_independently() {
        let first = SeedStringResource::new();
        let second = SeedStringResource::new();
        assert_eq!(first.get_unique_name("CaveType#"), "CaveType#0");
        assert_eq!(second.get_unique_name("CaveType#"), "CaveType#0");
    }
}
