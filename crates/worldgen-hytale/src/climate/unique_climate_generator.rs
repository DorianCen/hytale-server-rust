// Ported from: `.hytale-source/.../worldgen/climate/UniqueClimateGenerator.java`
//! Java `climate.UniqueClimateGenerator` — places the unique zones (Spawn,
//! Temple, …) into the climate mask via ring-spiral [`ClimateSearch`] scoring.
//!
//! Java wraps each placed position in a `CompletableFuture<Vector2i>`; the
//! searches are resolved synchronously by the Rust port (the parent-chain
//! `thenCompose` flattens to a sequential lookup), so positions are stored
//! resolved.

use std::collections::HashMap;
use std::rc::Rc;

use procedural::noise::support::Vector2i;

use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_noise::ClimateNoise;
use crate::climate::climate_search;
use crate::util::log_util;
use crate::zone::unique::UniqueCandidate;
use crate::zone::unique::UniqueEntry;
use crate::zone::zone::Zone;

/// Java `UniqueClimateGenerator.MAX_PARENT_DEPTH`.
const MAX_PARENT_DEPTH: usize = 10;
/// Java `UniqueClimateGenerator.DEFAULT_ORIGIN`.
const DEFAULT_ORIGIN: Vector2i = Vector2i { x: 0, y: 0 };

/// Java `UniqueClimateGenerator.Entry` (record).
#[derive(Clone, Debug)]
pub struct Entry {
    /// Java `@Nonnull String zone`.
    pub zone: String,
    /// Java `@Nonnull String parent` (Java default `""`).
    pub parent: String,
    /// Java `int color`.
    pub color: i32,
    /// Java `int radius`.
    pub radius: i32,
    /// Java `@Nonnull Vector2i origin`.
    pub origin: Vector2i,
    /// Java `int minDistance`.
    pub min_distance: i32,
    /// Java `int maxDistance`.
    pub max_distance: i32,
    /// Java `@Nonnull ClimateSearch.Rule rule`.
    pub rule: climate_search::Rule,
}

/// Java `UniqueClimateGenerator.Unique` (record).
#[derive(Clone, Debug)]
pub struct Unique {
    /// Java `int color`.
    pub color: i32,
    /// Java `int radius`.
    pub radius: i32,
    /// Java `int radius2` (`radius * radius`).
    pub radius2: i32,
    /// Java `@Nonnull CompletableFuture<Vector2i> position` (resolved).
    pub position: Vector2i,
}

impl Unique {
    /// Java `Unique(int color, int radius, CompletableFuture<Vector2i> position)`.
    pub fn new(color: i32, radius: i32, position: Vector2i) -> Self {
        Self {
            color,
            radius,
            radius2: radius.wrapping_mul(radius),
            position,
        }
    }

    /// Java `boolean contains(int x, int y)`.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let pos = self.position;
        let dx = x.wrapping_sub(pos.x);
        let dy = y.wrapping_sub(pos.y);
        // Bounds-check the deltas first as the length-squared calc may
        // overflow and incorrectly return true at large distances!
        dx >= -self.radius
            && dy >= -self.radius
            && dx <= self.radius
            && dy <= self.radius
            && dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) <= self.radius2
    }
}

/// Java `UniqueClimateGenerator`.
#[derive(Clone, Debug)]
pub struct UniqueClimateGenerator {
    /// Java `protected final Entry[] entries`.
    pub entries: Vec<Entry>,
    /// Java `protected final Unique[] zones`.
    pub zones: Vec<Unique>,
}

impl UniqueClimateGenerator {
    /// Java `UniqueClimateGenerator.EMPTY`.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            zones: Vec::new(),
        }
    }

    /// Java `UniqueClimateGenerator(Entry[] entries)`.
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            zones: Vec::new(),
        }
    }

    /// Java `UniqueClimateGenerator(Entry[] entries, Unique[] zones)`.
    pub fn new(entries: Vec<Entry>, zones: Vec<Unique>) -> Self {
        Self { entries, zones }
    }

    /// Java `int generate(int x, int y)`.
    pub fn generate(&self, x: i32, y: i32) -> i32 {
        for zone in &self.zones {
            if zone.contains(x, y) {
                return zone.color;
            }
        }
        -1
    }

    /// Java `Zone.UniqueCandidate[] getCandidates(Map<String, Zone> zoneLookup)`.
    pub fn get_candidates(&self, zone_lookup: &HashMap<String, Rc<Zone>>) -> Vec<UniqueCandidate> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let mut candidates = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let zone = match zone_lookup.get(&entry.zone) {
                Some(zone) => zone.clone(),
                None => panic!("Could not find zone: {}", entry.zone),
            };

            candidates.push(UniqueCandidate::new(
                UniqueEntry::new(zone, entry.color, Vec::new(), entry.radius, 0),
                Vec::new(),
            ));
        }

        candidates
    }

    /// Java `UniqueClimateGenerator apply(int seed, Zone.UniqueCandidate[] candidates, ClimateNoise noise, ClimateGraph graph, List<Zone.Unique> collector)`.
    pub fn apply(
        &self,
        seed: i32,
        candidates: &[UniqueCandidate],
        noise: &ClimateNoise,
        graph: &ClimateGraph,
        collector: &mut Vec<crate::zone::unique::Unique>,
    ) -> UniqueClimateGenerator {
        if candidates.len() != self.entries.len() {
            log_util::warn(&format!(
                "Mismatched unique climate generator candidates: expected {}, got {}",
                self.entries.len(),
                candidates.len()
            ));
            return self.clone();
        }

        let mut unique: Vec<Option<Unique>> = vec![None; candidates.len()];
        let mut lookup: HashMap<String, Unique> = HashMap::new();

        let mut it = 0;
        while it < MAX_PARENT_DEPTH && lookup.len() < unique.len() {
            it += 1;
            // Java indexes `entries[i]` / `unique[i]` in parallel; keep the loop.
            #[allow(clippy::needless_range_loop)]
            for i in 0..self.entries.len() {
                if unique[i].is_some() {
                    continue;
                }

                let entry = &self.entries[i];
                let parent = lookup.get(&entry.parent);

                if !entry.parent.is_empty() && parent.is_none() {
                    continue;
                }

                let position =
                    Self::find_zone_position(seed, DEFAULT_ORIGIN, entry, parent, noise, graph);

                let placed = Unique::new(entry.color, entry.radius, position);

                collector.push(crate::zone::unique::Unique::new(
                    candidates[i].zone.zone.clone(),
                    position,
                ));

                lookup.insert(entry.zone.clone(), placed.clone());
                unique[i] = Some(placed);
            }
        }

        UniqueClimateGenerator::new(
            self.entries.clone(),
            unique.into_iter().map(|u| u.unwrap()).collect(),
        )
    }

    /// Java `UniqueClimateGenerator apply(int seed, ClimateNoise noise, ClimateGraph graph)`.
    pub fn apply_without_collector(
        &self,
        seed: i32,
        noise: &ClimateNoise,
        graph: &ClimateGraph,
    ) -> UniqueClimateGenerator {
        let mut unique: Vec<Option<Unique>> = vec![None; self.entries.len()];
        let mut lookup: HashMap<String, Unique> = HashMap::new();

        let mut it = 0;
        while it < MAX_PARENT_DEPTH && lookup.len() < unique.len() {
            it += 1;
            // Java indexes `entries[i]` / `unique[i]` in parallel; keep the loop.
            #[allow(clippy::needless_range_loop)]
            for i in 0..self.entries.len() {
                if unique[i].is_some() {
                    continue;
                }

                let entry = &self.entries[i];
                let parent = lookup.get(&entry.parent);

                if !entry.parent.is_empty() && parent.is_none() {
                    continue;
                }

                let position =
                    Self::find_zone_position(seed, DEFAULT_ORIGIN, entry, parent, noise, graph);

                let placed = Unique::new(entry.color, entry.radius, position);
                lookup.insert(entry.zone.clone(), placed.clone());
                unique[i] = Some(placed);
            }
        }

        if lookup.len() < unique.len() {
            log_util::warn(&format!(
                "Could not resolve all unique climate zones, resolved {} out of {}",
                lookup.len(),
                unique.len()
            ));

            let mut new_entries = Vec::with_capacity(lookup.len());
            let mut new_unique = Vec::with_capacity(lookup.len());
            for (i, placed) in unique.into_iter().enumerate() {
                if let Some(placed) = placed {
                    new_entries.push(self.entries[i].clone());
                    new_unique.push(placed);
                }
            }

            return UniqueClimateGenerator::new(new_entries, new_unique);
        }

        UniqueClimateGenerator::new(
            self.entries.clone(),
            unique.into_iter().map(|u| u.unwrap()).collect(),
        )
    }

    /// Java `static CompletableFuture<Vector2i> findZonePosition(int seed, Vector2i origin, Entry entry, Unique parent, ClimateNoise noise, ClimateGraph graph)`.
    fn find_zone_position(
        seed: i32,
        origin: Vector2i,
        entry: &Entry,
        parent: Option<&Unique>,
        noise: &ClimateNoise,
        graph: &ClimateGraph,
    ) -> Vector2i {
        if let Some(parent) = parent {
            // Parent's position overrides and becomes the origin for this
            // search (Java `thenCompose` chain — synchronous here).
            let pos = parent.position;
            return Self::find_zone_position(seed, pos, entry, None, noise, graph);
        }

        let result = climate_search::search(
            seed,
            origin.x.wrapping_add(entry.origin.x),
            origin.y.wrapping_add(entry.origin.y),
            entry.min_distance,
            entry.max_distance,
            &entry.rule,
            noise,
            graph,
        );
        log_util::info(&format!(
            "Found location for unique zone '{}' -> {}",
            entry.zone,
            result.pretty()
        ));
        let (x, y) = result.position;
        Vector2i { x, y }
    }
}
