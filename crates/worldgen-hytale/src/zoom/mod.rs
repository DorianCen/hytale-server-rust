// Ported from: `.hytale-source/.../worldgen/zoom/*.java`
//! Java `worldgen.zoom` package — pixel-map zooming primitives.
//!
//! | Java class              | Rust module                      |
//! |-------------------------|----------------------------------|
//! | `PixelProvider`         | [`pixel_provider`]               |
//! | `PixelDistanceProvider` | [`pixel_distance_provider`]      |
//! | `ExactZoom`             | [`exact_zoom`]                   |
//! | `FuzzyZoom`             | [`fuzzy_zoom`]                   |

pub mod exact_zoom;
pub mod fuzzy_zoom;
pub mod pixel_distance_provider;
pub mod pixel_provider;

pub use exact_zoom::ExactZoom;
pub use fuzzy_zoom::FuzzyZoom;
pub use pixel_distance_provider::PixelDistanceProvider;
pub use pixel_provider::PixelProvider;
