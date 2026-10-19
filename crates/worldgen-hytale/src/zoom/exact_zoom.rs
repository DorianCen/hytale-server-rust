// Ported from: `.hytale-source/.../worldgen/zoom/ExactZoom.java`
//! Java `zoom.ExactZoom` — nearest-neighbor sampling of a pixel map under a
//! zoom/offset transform, plus unique-zone scan/paint placement.

use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use procedural::noise::support::Vector2i;

use crate::unique_zone_placement::test_zone_fit_for_entry;
pub use crate::unique_zone_placement::{select_candidate_position, test_zone_fit};
use crate::zone::{Unique, UniqueCandidate, UniqueEntry};
use crate::zoom::pixel_distance_provider::PixelDistanceProvider;
use crate::zoom::pixel_provider::PixelProvider;

/// Java `zoom.ExactZoom`.
pub struct ExactZoom {
    /// Java `private final PixelProvider source`.
    pub source: PixelProvider,
    /// Java `private final PixelDistanceProvider distanceProvider`.
    pub distance_provider: PixelDistanceProvider,
    /// Java `private final double zoomX, zoomY`.
    pub zoom_x: f64,
    pub zoom_y: f64,
    /// Java `private final int offsetX, offsetY`.
    pub offset_x: i32,
    pub offset_y: i32,
}

impl ExactZoom {
    /// Java `ExactZoom(PixelProvider, double zoomX, double zoomY, int offsetX, int offsetY)`.
    pub fn new(
        source: PixelProvider,
        zoom_x: f64,
        zoom_y: f64,
        offset_x: i32,
        offset_y: i32,
    ) -> Self {
        let distance_provider = PixelDistanceProvider::new(source.clone());
        Self {
            source,
            distance_provider,
            zoom_x,
            zoom_y,
            offset_x,
            offset_y,
        }
    }

    /// Java `getDistanceProvider()`.
    pub fn get_distance_provider(&self) -> &PixelDistanceProvider {
        &self.distance_provider
    }

    /// Java `inBounds(double x, double y)`.
    #[allow(clippy::int_plus_one)] // faithful transcription of the Java comparison
    pub fn in_bounds(&self, x: f64, y: f64) -> bool {
        let mut x = x + self.offset_x as f64;
        let mut y = y + self.offset_y as f64;

        if x < 0.0 || y < 0.0 {
            return false;
        }
        x /= self.zoom_x;
        y /= self.zoom_y;

        self.source.width as i32 - 1 >= x as i32 && self.source.height as i32 - 1 >= y as i32
    }

    /// Java `generate(double x, double y)` — nearest-pixel sample.
    pub fn generate(&self, x: f64, y: f64) -> i32 {
        let mut x = x + self.offset_x as f64;
        let mut y = y + self.offset_y as f64;

        x /= self.zoom_x;
        y /= self.zoom_y;

        let px = math_util::floor(x).max(0).min(self.source.width as i32 - 1);
        let py = math_util::floor(y)
            .max(0)
            .min(self.source.height as i32 - 1);

        self.source.get_pixel(px, py)
    }

    /// Java `distanceToNextPixel(double x, double y)`.
    pub fn distance_to_next_pixel(&self, x: f64, y: f64) -> f64 {
        let mut x = x + self.offset_x as f64;
        let mut y = y + self.offset_y as f64;

        x /= self.zoom_x;
        y /= self.zoom_y;

        self.zoom_x
            * f64::sqrt(self.distance_provider.distance_sq_to_different_pixel(
                x,
                y,
                math_util::fast_floor_f64(x) as i32,
                math_util::fast_floor_f64(y) as i32,
            ))
    }

    /// Java `generateUniqueZones(Zone.UniqueCandidate[], FastRandom, List<Zone.Unique>)`.
    pub fn generate_unique_zones(
        &self,
        candidates: &[UniqueCandidate],
        random: &mut FastRandom,
        zones: &mut Vec<Unique>,
    ) -> ExactZoom {
        let mut source = self.source.clone();

        for candidate in candidates {
            let pos = select_candidate_position(candidate, &source, random);

            let pos = match pos {
                Some(p) => p,
                None => {
                    // Java: LogUtil warning "Failed to place unique zone: %s"
                    eprintln!(
                        "WARNING: Failed to place unique zone: {}",
                        candidate.zone.zone_id
                    );
                    continue;
                }
            };

            let radius = candidate.zone.radius;
            let radius2 = radius * radius;

            // Write the unique zone to the source image.
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy <= radius2 {
                        source.set_pixel(pos.x + dx, pos.y + dy, candidate.zone.color);
                    }
                }
            }

            // Record the placed unique zone scaled in world-space coords.
            // Java: (int) ((pos.x * zoomX) - offsetX) — double math, truncating cast.
            zones.push(Unique {
                zone_id: candidate.zone.zone_id,
                position: Vector2i::new(
                    ((pos.x as f64 * self.zoom_x) - self.offset_x as f64) as i32,
                    ((pos.y as f64 * self.zoom_y) - self.offset_y as f64) as i32,
                ),
            });
        }

        ExactZoom::new(
            source,
            self.zoom_x,
            self.zoom_y,
            self.offset_x,
            self.offset_y,
        )
    }

    /// Java `generateUniqueZoneCandidates(Zone.UniqueEntry[], int maxPositions)`.
    pub fn generate_unique_zone_candidates(
        &self,
        entries: &[UniqueEntry],
        max_positions: usize,
    ) -> Vec<UniqueCandidate> {
        let mut candidates = Vec::with_capacity(entries.len());

        for entry in entries {
            let radius = entry.radius;
            let search_radius = radius + entry.padding;

            let mut positions: Vec<Vector2i> = Vec::new();

            for iy in search_radius..(self.source.height as i32 - 1 - search_radius) {
                for ix in search_radius..(self.source.width as i32 - 1 - search_radius) {
                    if test_zone_fit_for_entry(entry, &self.source, ix, iy, search_radius) {
                        positions.push(Vector2i::new(ix, iy));
                    }
                }
            }

            let size = positions.len();
            if size == 0 {
                panic!(
                    "No parent matches found for unique zone entry: {}",
                    entry.zone_id
                );
            } else if size > max_positions {
                // Select every Nth position to reduce the number of candidates.
                let n = (size / max_positions).max(1);
                let count = size / n;

                let arr: Vec<Vector2i> = (0..count).map(|j| positions[j * n]).collect();
                candidates.push(UniqueCandidate {
                    zone: entry.clone(),
                    positions: arr,
                });
            } else {
                candidates.push(UniqueCandidate {
                    zone: entry.clone(),
                    positions,
                });
            }
        }

        candidates
    }

    /// Java `exportImage()` — returns the source pixel map (PNG rendering is a
    /// CLI concern, not part of the generator).
    pub fn export_pixels(&self) -> &PixelProvider {
        &self.source
    }
}
