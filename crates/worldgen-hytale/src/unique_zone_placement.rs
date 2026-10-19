// Ported from: `.hytale-source/.../worldgen/zoom/ExactZoom.java` (private statics)
//! Unique-zone placement helpers shared by `ExactZoom` and (later) the
//! `ZonePatternGenerator`. Java declares them as private statics on
//! `ExactZoom`/`ZonePatternGenerator`; the Rust port shares them here.

use hytale_math::fast_random::FastRandom;
use procedural::noise::support::Vector2i;

use crate::zone::{UniqueCandidate, UniqueEntry};
use crate::zoom::pixel_provider::PixelProvider;

/// Java `ExactZoom.testZoneFit(Zone.UniqueEntry, PixelProvider, int x, int y, int radius)`.
pub fn test_zone_fit_for_entry(
    entry: &UniqueEntry,
    source: &PixelProvider,
    x: i32,
    y: i32,
    radius: i32,
) -> bool {
    let radius2 = radius * radius;

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius2
                && !entry.matches_parent(source.get_pixel(x + dx, y + dy))
            {
                return false;
            }
        }
    }

    true
}

/// Java `selectCandidatePosition` — picks a random viable position, checking
/// for overlaps with previously placed unique zones. Java mutates the
/// candidate's array in place (swap-to-back); the Rust port mirrors that
/// behavior on the caller-owned `positions` slice.
///
/// Returns the selected position, or `None` if no viable position was found.
pub fn select_candidate_position(
    candidate: &UniqueCandidate,
    source: &PixelProvider,
    random: &mut FastRandom,
) -> Option<Vector2i> {
    let search_radius = candidate.zone.radius + candidate.zone.padding;

    let mut positions = candidate.positions.clone();
    let mut len = positions.len();

    while len > 0 {
        let index = random.next_int_bound(len as i32) as usize;
        let pos = positions[index];

        // Verify that the position is still valid (not overlapping another
        // unique zone).
        if !test_zone_fit_for_entry(&candidate.zone, source, pos.x, pos.y, search_radius) {
            // Swap position to the back and reduce the search space.
            let back = positions[len - 1];
            positions[len - 1] = pos;
            positions[index] = back;
            len -= 1;
            continue;
        }

        return Some(pos);
    }

    None
}

/// Alias mirroring the Java `testZoneFit` static name for the
/// entry-based overload.
pub fn test_zone_fit(
    entry: &UniqueEntry,
    source: &PixelProvider,
    x: i32,
    y: i32,
    radius: i32,
) -> bool {
    test_zone_fit_for_entry(entry, source, x, y, radius)
}
