// Ported from: `.hytale-source/.../worldgen/zoom/FuzzyZoom.java`
//! Java `zoom.FuzzyZoom` — an `ExactZoom` with a fuzzy domain-warp on the
//! query coordinates (the mask provider path).

use hytale_math::fast_random::FastRandom;
use procedural::noise::random::ICoordinateRandomizer;
use std::rc::Rc;

use crate::zone::{Unique, UniqueCandidate, UniqueEntry};
use crate::zoom::exact_zoom::ExactZoom;
use crate::zoom::pixel_provider::PixelProvider;

/// Java `zoom.FuzzyZoom`.
pub struct FuzzyZoom {
    /// Java `private final ICoordinateRandomizer randomizer`.
    pub randomizer: Rc<dyn ICoordinateRandomizer>,
    /// Java `private final ExactZoom exactZoom`.
    pub exact_zoom: ExactZoom,
}

impl FuzzyZoom {
    /// Java `FuzzyZoom(ICoordinateRandomizer, PixelProvider source, double zoomX,
    /// double zoomY, int offsetX, int offsetY)`.
    pub fn new(
        randomizer: Rc<dyn ICoordinateRandomizer>,
        source: PixelProvider,
        zoom_x: f64,
        zoom_y: f64,
        offset_x: i32,
        offset_y: i32,
    ) -> Self {
        Self {
            randomizer,
            exact_zoom: ExactZoom::new(source, zoom_x, zoom_y, offset_x, offset_y),
        }
    }

    /// Java `FuzzyZoom(ICoordinateRandomizer, ExactZoom)`.
    pub fn from_exact_zoom(
        randomizer: Rc<dyn ICoordinateRandomizer>,
        exact_zoom: ExactZoom,
    ) -> Self {
        Self {
            randomizer,
            exact_zoom,
        }
    }

    /// Java `getX(int seed, double x, double y)` — fuzzy-warped coordinate.
    pub fn get_x(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.randomizer.random_double_x2(seed, x, y)
    }

    /// Java `getY(int seed, double x, double y)`.
    pub fn get_y(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.randomizer.random_double_y2(seed, x, y)
    }

    /// Java `generate(double x, double y)`.
    pub fn generate(&self, x: f64, y: f64) -> i32 {
        self.exact_zoom.generate(x, y)
    }

    /// Java `distance(double x, double y)`.
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        self.exact_zoom.distance_to_next_pixel(x, y)
    }

    /// Java `getExactZoom()`.
    pub fn get_exact_zoom(&self) -> &ExactZoom {
        &self.exact_zoom
    }

    /// Java `inBounds(double x, double y)`.
    pub fn in_bounds(&self, x: f64, y: f64) -> bool {
        self.exact_zoom.in_bounds(x, y)
    }

    /// Java `generateUniqueZoneCandidates(Zone.UniqueEntry[], int)`.
    pub fn generate_unique_zone_candidates(
        &self,
        entries: &[UniqueEntry],
        max_positions: usize,
    ) -> Vec<UniqueCandidate> {
        self.exact_zoom
            .generate_unique_zone_candidates(entries, max_positions)
    }

    /// Java `generateUniqueZones(Zone.UniqueCandidate[], FastRandom, List<Zone.Unique>)`.
    pub fn generate_unique_zones(
        &self,
        candidates: &[UniqueCandidate],
        random: &mut FastRandom,
        zones: &mut Vec<Unique>,
    ) -> FuzzyZoom {
        FuzzyZoom {
            randomizer: self.randomizer.clone(),
            exact_zoom: self
                .exact_zoom
                .generate_unique_zones(candidates, random, zones),
        }
    }
}

// Re-exposed for the zone package (Java: private statics shared by path).
