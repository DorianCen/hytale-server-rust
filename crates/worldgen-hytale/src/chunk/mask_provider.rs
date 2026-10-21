// Ported from: `.hytale-source/.../worldgen/chunk/MaskProvider.java`
// Ported from: `.hytale-source/.../worldgen/climate/ClimateMaskProvider.java`
//! Java `chunk.MaskProvider` — the seed-aware mask accessor of the zone
//! pattern generator (and, via [`MaskProvider::generate_unique_zones`], the
//! unique-zone mask painter).
//!
//! # Mask polymorphism (Rust adaptation of the Java subclass split)
//!
//! Java's `MaskProvider` is a **class**; `ClimateMaskProvider` **extends** it
//! and overrides `get`/`distance` to sample the procedural climate noise. A
//! world's `ZonePatternProvider.maskProvider` field is therefore either a
//! plain image `MaskProvider` or a `ClimateMaskProvider`, and the Java loaders
//! branch with `maskProvider instanceof ClimateMaskProvider`.
//!
//! The Rust port models that with an enum (rather than a trait object): the
//! variants keep the Java method surface (`get_x`/`get_y`/`get`/`distance`/
//! `in_bounds`/`get_fuzzy_zoom`/`generate_unique_zone_candidates`/
//! `generate_unique_zones`) dispatching internally, so the image variant
//! behaves exactly as the previous concrete struct did.
//!
//! * `Image(Rc<FuzzyZoom>)` — the Java `MaskProvider` (`Mask.png` branch).
//!   `Rc` (instead of a bare `FuzzyZoom`) is what makes [`MaskProvider`]
//!   `Clone`, which the loaders need to hand the provider to both the
//!   `ZonePatternProvider` and the per-seed generator. The extra allocation is
//!   load-time only.
//! * `Climate(Rc<ClimateMaskProvider>)` — the Java `ClimateMaskProvider`
//!   (`Mask.json` branch). Its `get`/`distance` need the scratch
//!   [`crate::climate::climate_noise::Buffer`] that Java reads from the
//!   per-chunk resource; the Rust port passes it explicitly through
//!   [`MaskProvider::get_climate`] / [`MaskProvider::distance_climate`].
//!   [`MaskProvider::get`] / [`MaskProvider::distance`] panic for this variant
//!   (documented) — exactly the Java method that would dereference the missing
//!   resource buffer.
//!
//! Java's `instanceof` checks become `match`es on this enum.
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;

use crate::climate::climate_mask_provider::ClimateMaskProvider;
use crate::climate::climate_noise::Buffer;
use crate::zone::unique::Unique;
use crate::zone::unique::UniqueCandidate;
use crate::zone::unique::UniqueEntry;
use crate::zoom::fuzzy_zoom::FuzzyZoom;

/// Java `chunk.MaskProvider` (the image variant) / `climate.ClimateMaskProvider`
/// (the climate variant).
#[derive(Clone)]
pub enum MaskProvider {
    /// Java `chunk.MaskProvider` — the `Mask.png` branch.
    Image(Rc<FuzzyZoom>),
    /// Java `climate.ClimateMaskProvider` — the `Mask.json` branch.
    Climate(Rc<ClimateMaskProvider>),
}

impl MaskProvider {
    /// Java `MaskProvider(FuzzyZoom fuzzyZoom)` — the image variant.
    pub fn new(fuzzy_zoom: FuzzyZoom) -> Self {
        Self::Image(Rc::new(fuzzy_zoom))
    }

    /// Java `ClimateMaskProvider` (the `Mask.json` branch).
    pub fn climate(provider: Rc<ClimateMaskProvider>) -> Self {
        Self::Climate(provider)
    }

    /// Java `boolean isClimate()` equivalent — whether this is the climate
    /// branch (the Java `instanceof ClimateMaskProvider` check).
    pub fn is_climate(&self) -> bool {
        matches!(self, Self::Climate(_))
    }

    /// Java `double getX(int seed, double x, double y)`.
    pub fn get_x(&self, seed: i32, x: f64, y: f64) -> f64 {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom.get_x(seed, x, y),
            Self::Climate(provider) => provider.get_x(seed, x, y),
        }
    }

    /// Java `double getY(int seed, double x, double y)`.
    pub fn get_y(&self, seed: i32, x: f64, y: f64) -> f64 {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom.get_y(seed, x, y),
            Self::Climate(provider) => provider.get_y(seed, x, y),
        }
    }

    /// Java `int get(int seed, double x, double y)` — the image variant. The
    /// seed is unused, exactly as in Java (the fuzzy zoom generates without
    /// it). Panics for the climate variant: use [`Self::get_climate`].
    pub fn get(&self, _seed: i32, x: f64, y: f64) -> i32 {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom.generate(x, y),
            Self::Climate(_) => panic!(
                "MaskProvider::get is only valid for the image mask; use get_climate for ClimateMaskProvider"
            ),
        }
    }

    /// Java `ClimateMaskProvider.get(int seed, double x, double y)` — the
    /// climate variant. The buffer is the Java `ChunkGenerator` resource
    /// scratch (`climateBuffer`), passed explicitly by the Rust port.
    pub fn get_climate(&self, seed: i32, x: f64, y: f64, buffer: &mut Buffer) -> i32 {
        match self {
            Self::Climate(provider) => provider.get(seed, x, y, buffer),
            Self::Image(_) => panic!(
                "MaskProvider::get_climate is only valid for the climate mask (ClimateMaskProvider)"
            ),
        }
    }

    /// Java `double distance(double x, double y)` — the image variant.
    /// Panics for the climate variant: use [`Self::distance_climate`].
    pub fn distance(&self, x: f64, y: f64) -> f64 {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom.distance(x, y),
            Self::Climate(_) => panic!(
                "MaskProvider::distance is only valid for the image mask; use distance_climate for ClimateMaskProvider"
            ),
        }
    }

    /// Java `ClimateMaskProvider.distance(double x, double y)` — the climate
    /// variant (the fade of the last sampled buffer).
    pub fn distance_climate(&self, buffer: &Buffer) -> f64 {
        match self {
            Self::Climate(provider) => provider.distance(buffer),
            Self::Image(_) => panic!(
                "MaskProvider::distance_climate is only valid for the climate mask (ClimateMaskProvider)"
            ),
        }
    }

    /// Java `boolean inBounds(double x, double y)`.
    pub fn in_bounds(&self, x: f64, y: f64) -> bool {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom.in_bounds(x, y),
            Self::Climate(provider) => provider.in_bounds(x, y),
        }
    }

    /// Java `FuzzyZoom getFuzzyZoom()`.
    pub fn get_fuzzy_zoom(&self) -> &FuzzyZoom {
        match self {
            Self::Image(fuzzy_zoom) => fuzzy_zoom,
            Self::Climate(provider) => provider.base.get_fuzzy_zoom(),
        }
    }

    /// Java `Zone.UniqueCandidate[] generateUniqueZoneCandidates(
    /// Zone.UniqueEntry[] entries, int maxPositions)`.
    pub fn generate_unique_zone_candidates(
        &self,
        entries: &[UniqueEntry],
        max_positions: usize,
    ) -> Vec<UniqueCandidate> {
        match self {
            Self::Image(fuzzy_zoom) => {
                fuzzy_zoom.generate_unique_zone_candidates(entries, max_positions)
            }
            Self::Climate(provider) => {
                provider.generate_unique_zone_candidates(entries, max_positions)
            }
        }
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
        match self {
            Self::Image(fuzzy_zoom) => Self::Image(Rc::new(
                fuzzy_zoom.generate_unique_zones(candidates, random, zones),
            )),
            Self::Climate(provider) => Self::Climate(Rc::new(
                provider.generate_unique_zones(seed, candidates, random, zones),
            )),
        }
    }

    /// The climate mask provider (the `Mask.json` branch).
    pub fn as_climate(&self) -> Option<&Rc<ClimateMaskProvider>> {
        match self {
            Self::Climate(provider) => Some(provider),
            Self::Image(_) => None,
        }
    }

    /// The image mask's fuzzy zoom (the `Mask.png` branch).
    pub fn as_image(&self) -> Option<&Rc<FuzzyZoom>> {
        match self {
            Self::Image(fuzzy_zoom) => Some(fuzzy_zoom),
            Self::Climate(_) => None,
        }
    }
}
