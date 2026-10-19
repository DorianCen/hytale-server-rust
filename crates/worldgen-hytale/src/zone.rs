// Ported from: `.hytale-source/.../worldgen/zone/Zone.java` (PARTIAL)
//! Java `zone.Zone` — the unique-zone value types needed by the `zoom`
//! package.
//!
//! ⚠ TODO(phase2): the full `Zone` class (id/name/discoveryConfig/
//! caveGenerator/biomePatternGenerator/uniquePrefabContainer) is ported with
//! the zone package. Only the nested records used by `ExactZoom` live here
//! for now.

use procedural::noise::support::Vector2i;

/// Java `Zone.UniqueEntry` — a unique prefab definition.
#[derive(Clone)]
pub struct UniqueEntry {
    /// Java `Zone zone` — identifier of the zone the entry belongs to.
    /// (Full `Zone` arrives with the zone package; the id suffices for the
    /// zoom-phase placement algorithms, which only read radius/padding/color.)
    pub zone_id: i32,
    /// Java `int color`.
    pub color: i32,
    /// Java `int[] parent` — parent colors that can spawn this entry.
    pub parent: Vec<i32>,
    /// Java `int radius` — min distance between two instances.
    pub radius: i32,
    /// Java `int padding`.
    pub padding: i32,
}

impl UniqueEntry {
    /// Java `UniqueEntry.matchesParent(int color)`.
    #[allow(clippy::manual_contains)] // faithful transcription of the Java loop
    pub fn matches_parent(&self, color: i32) -> bool {
        self.parent.iter().any(|&p| p == color)
    }
}

/// Java `Zone.UniqueCandidate` — an entry plus its viable positions.
#[derive(Clone)]
pub struct UniqueCandidate {
    /// Java `UniqueEntry zone`.
    pub zone: UniqueEntry,
    /// Java `Vector2i[] positions`.
    pub positions: Vec<Vector2i>,
}

/// Java `Zone.Unique` — a placed unique instance.
///
/// Java wraps the position in a `CompletableFuture` (async zone placement);
/// by the time this type is produced here the position is always resolved,
/// so the Rust port stores it directly.
#[derive(Clone)]
pub struct Unique {
    /// Java `Zone zone`.
    pub zone_id: i32,
    /// Java `CompletableFuture<Vector2i> position` (resolved).
    pub position: Vector2i,
}
