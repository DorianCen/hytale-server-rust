// Ported from: `.hytale-source/.../worldgen/zone/Zone.java` (nested records)
//! Java `zone.Zone` — the nested unique-zone value types.
//!
//! Ported faithfully from the Java nested records: [`UniqueEntry`] carries
//! the `Zone` handle (Java `UniqueEntry.zone`), [`UniqueCandidate`] pairs an
//! entry with its viable positions, and [`Unique`] is a placed unique
//! instance (Java wraps the position in a `CompletableFuture`; by the time
//! this type is produced the position is always resolved, so the Rust port
//! stores it directly).
//!
//! ⚠ The `Rc<Zone>` inside [`UniqueEntry`] does **not** create a reference
//! cycle: the zone owns its *unique prefab container* (the generators of the
//! zone), not its unique *entries* — the entries are built per seed by
//! [`crate::container::unique_prefab_container::UniquePrefabContainer::generate`]
//! and are not stored in the zone.

use std::rc::Rc;

use procedural::noise::support::Vector2i;

use crate::zone::zone::Zone;

/// Java `Zone.UniqueEntry` — a unique prefab definition.
#[derive(Clone)]
pub struct UniqueEntry {
    /// Java `Zone zone` — the zone the unique entry belongs to.
    pub zone: Rc<Zone>,
    /// Java `int color` — the color identifier of the unique entry.
    pub color: i32,
    /// Java `int[] parent` — parent colors that can spawn this entry.
    pub parent: Vec<i32>,
    /// Java `int radius` — min distance between two instances.
    pub radius: i32,
    /// Java `int padding` — padding around the unique entry.
    pub padding: i32,
}

impl UniqueEntry {
    /// Java `UniqueEntry(Zone zone, int color, int[] parent, int radius,
    /// int padding)`.
    pub fn new(zone: Rc<Zone>, color: i32, parent: Vec<i32>, radius: i32, padding: i32) -> Self {
        Self {
            zone,
            color,
            parent,
            radius,
            padding,
        }
    }

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

impl UniqueCandidate {
    /// Java `UniqueCandidate(UniqueEntry zone, Vector2i[] positions)`.
    pub fn new(zone: UniqueEntry, positions: Vec<Vector2i>) -> Self {
        Self { zone, positions }
    }
}

/// Java `Zone.Unique` — a placed unique instance.
#[derive(Clone)]
pub struct Unique {
    /// Java `Zone zone`.
    pub zone: Rc<Zone>,
    /// Java `CompletableFuture<Vector2i> position` (resolved).
    pub position: Vector2i,
}

impl Unique {
    /// Java `Unique(Zone zone, CompletableFuture<Vector2i> position)` —
    /// constructed with the resolved position.
    pub fn new(zone: Rc<Zone>, position: Vector2i) -> Self {
        Self { zone, position }
    }

    /// Java `Vector2i getPosition()` (`position.join()`).
    pub fn get_position(&self) -> Vector2i {
        self.position
    }
}
