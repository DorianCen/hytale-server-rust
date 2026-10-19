// Ported from: `.hytale-source/.../worldgen/chunk/MaskProvider.java`
//! Java `chunk.MaskProvider` — a thin delegation wrapper over
//! [`FuzzyZoom`]: the seed-aware mask accessor of the zone pattern
//! generator (and, via [`MaskProvider::generate_unique_zones`], the
//! unique-zone mask painter).
use hytale_math::fast_random::FastRandom;

use crate::zone::unique::Unique;
use crate::zone::unique::UniqueCandidate;
use crate::zone::unique::UniqueEntry;
use crate::zoom::fuzzy_zoom::FuzzyZoom;

/// Java `chunk.MaskProvider`.
pub struct MaskProvider {
    /// Java `protected final FuzzyZoom fuzzyZoom`.
    fuzzy_zoom: FuzzyZoom,
}

impl MaskProvider {
    /// Java `MaskProvider(FuzzyZoom fuzzyZoom)`.
    pub fn new(fuzzy_zoom: FuzzyZoom) -> Self {
        Self { fuzzy_zoom }
    }

    /// Java `double getX(int seed, double x, double y)`.
    pub fn get_x(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.fuzzy_zoom.get_x(seed, x, y)
    }

    /// Java `double getY(int seed, double x, double y)`.
    pub fn get_y(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.fuzzy_zoom.get_y(seed, x, y)
    }

    /// Java `int get(int seed, double x, double y)` — the seed is unused,
    /// exactly as in Java (the fuzzy zoom generates without it).
    pub fn get(&self, _seed: i32, x: f64, y: f64) -> i32 {
        self.fuzzy_zoom.generate(x, y)
    }

    /// Java `double distance(double x, double y)`.
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        self.fuzzy_zoom.distance(x, y)
    }

    /// Java `boolean inBounds(double x, double y)`.
    pub fn in_bounds(&self, x: f64, y: f64) -> bool {
        self.fuzzy_zoom.in_bounds(x, y)
    }

    /// Java `FuzzyZoom getFuzzyZoom()`.
    pub fn get_fuzzy_zoom(&self) -> &FuzzyZoom {
        &self.fuzzy_zoom
    }

    /// Java `Zone.UniqueCandidate[] generateUniqueZoneCandidates(
    /// Zone.UniqueEntry[] entries, int maxPositions)`.
    pub fn generate_unique_zone_candidates(
        &self,
        entries: &[UniqueEntry],
        max_positions: usize,
    ) -> Vec<UniqueCandidate> {
        self.fuzzy_zoom
            .generate_unique_zone_candidates(entries, max_positions)
    }

    /// Java `MaskProvider generateUniqueZones(int seed,
    /// Zone.UniqueCandidate[] candidates, FastRandom random,
    /// List<Zone.Unique> zones)` — paints the placed unique zones into the
    /// mask and returns a provider over the zoomed mask.
    pub fn generate_unique_zones(
        &self,
        seed: i32,
        candidates: &[UniqueCandidate],
        random: &mut FastRandom,
        zones: &mut Vec<Unique>,
    ) -> MaskProvider {
        let _ = seed;
        MaskProvider::new(
            self.fuzzy_zoom
                .generate_unique_zones(candidates, random, zones),
        )
    }
}
