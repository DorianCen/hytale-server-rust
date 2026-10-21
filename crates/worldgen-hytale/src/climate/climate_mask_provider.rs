// Ported from: `.hytale-source/.../worldgen/climate/ClimateMaskProvider.java`
//! Java `climate.ClimateMaskProvider` — the `MaskProvider` subclass that
//! samples the procedural climate noise instead of a zoomed mask image.
//!
//! Java inherits from `MaskProvider` (holding a discarded 1×1 `EMPTY_ZOOM`);
//! the Rust port keeps the same shape: a [`ClimateMaskProvider`] with a
//! `base` [`MaskProvider`] over an empty zoom.
//!
//! Java reads the scratch `ClimateNoise.Buffer` from the per-chunk resource
//! (`ChunkGenerator.getResource().climateBuffer`); the Rust port passes the
//! buffer explicitly (`&mut Buffer`) — see `GenContext`.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use procedural::noise::random::{EmptyCoordinateRandomizer, ICoordinateRandomizer};

use crate::chunk::mask_provider::MaskProvider;
use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_noise::{Buffer, ClimateNoise};
use crate::climate::climate_type::ClimateType;
use crate::climate::unique_climate_generator::UniqueClimateGenerator;
use crate::zone::unique::{Unique, UniqueCandidate};
use crate::zone::zone::Zone;
use crate::zoom::fuzzy_zoom::FuzzyZoom;
use crate::zoom::pixel_provider::PixelProvider;

/// Java `ClimateMaskProvider`.
pub struct ClimateMaskProvider {
    /// Java `super(EMPTY_ZOOM)` — the inherited `MaskProvider` over an empty
    /// 1×1 zoom (never meaningfully sampled).
    pub base: MaskProvider,
    /// Java `@Nonnull private final ICoordinateRandomizer randomizer`.
    randomizer: Rc<dyn ICoordinateRandomizer>,
    /// Java `@Nonnull private final ClimateNoise noise`.
    noise: Rc<ClimateNoise>,
    /// Java `@Nonnull private final ClimateGraph graph`.
    graph: Rc<ClimateGraph>,
    /// Java `@Nonnull private final UniqueClimateGenerator uniqueGenerator`.
    unique_generator: Rc<UniqueClimateGenerator>,
}

impl ClimateMaskProvider {
    /// Java `new ClimateMaskProvider(randomizer, noise, graph, uniqueGenerator)`.
    pub fn new(
        randomizer: Rc<dyn ICoordinateRandomizer>,
        noise: Rc<ClimateNoise>,
        graph: Rc<ClimateGraph>,
        unique_generator: Rc<UniqueClimateGenerator>,
    ) -> Self {
        Self {
            base: MaskProvider::new(Self::empty_zoom()),
            randomizer,
            noise,
            graph,
            unique_generator,
        }
    }

    /// Java `private ClimateMaskProvider(ClimateMaskProvider other, UniqueClimateGenerator uniqueGenerator)`.
    fn with_unique_generator(&self, unique_generator: Rc<UniqueClimateGenerator>) -> Self {
        Self {
            base: MaskProvider::new(Self::empty_zoom()),
            randomizer: self.randomizer.clone(),
            noise: self.noise.clone(),
            graph: self.graph.clone(),
            unique_generator,
        }
    }

    /// Java `static final FuzzyZoom EMPTY_ZOOM` — a 1×1 black-image zoom.
    fn empty_zoom() -> FuzzyZoom {
        // Java: `new PixelProvider(new BufferedImage(1, 1, TYPE_INT_RGB))` —
        // a single 0x000000 pixel.
        FuzzyZoom::new(
            Rc::new(EmptyCoordinateRandomizer),
            PixelProvider::from_rgb_image(1, 1, |_, _| 0),
            1.0,
            1.0,
            0,
            0,
        )
    }

    /// Java `ClimateGraph getGraph()`.
    pub fn get_graph(&self) -> &ClimateGraph {
        &self.graph
    }

    /// Java `@Override boolean inBounds(double x, double y)`.
    pub fn in_bounds(&self, _x: f64, _y: f64) -> bool {
        true
    }

    /// Java `@Override double getX(int seed, double x, double y)`.
    pub fn get_x(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.randomizer.random_double_x2(seed, x, y)
    }

    /// Java `@Override double getY(int seed, double x, double y)`.
    pub fn get_y(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.randomizer.random_double_y2(seed, x, y)
    }

    /// Java `@Override int get(int seed, double x, double y)` — the seed is
    /// passed through to the noise, the unique lookup uses it too (Java's
    /// unique check ignores the seed).
    pub fn get(&self, seed: i32, x: f64, y: f64, buffer: &mut Buffer) -> i32 {
        let unique = self
            .unique_generator
            .generate(math_util::floor(x), math_util::floor(y));

        if unique != -1 {
            return unique;
        }

        let id = self.noise.generate(seed, x, y, buffer, &self.graph);

        ClimateType::color(id, &self.graph)
    }

    /// Java `@Override double distance(double x, double y)` — the fade of the
    /// last sampled buffer.
    pub fn distance(&self, buffer: &Buffer) -> f64 {
        buffer.fade
    }

    /// Java `@Override MaskProvider generateUniqueZones(int seed, Zone.UniqueCandidate[] candidates, FastRandom random, List<Zone.Unique> collector)`.
    pub fn generate_unique_zones(
        &self,
        seed: i32,
        candidates: &[UniqueCandidate],
        _random: &mut FastRandom,
        collector: &mut Vec<Unique>,
    ) -> ClimateMaskProvider {
        if candidates.is_empty() {
            // Java returns `this` (as a MaskProvider).
            return self.with_unique_generator(self.unique_generator.clone());
        }

        let unique_generator = Rc::new(self.unique_generator.apply(
            seed,
            candidates,
            &self.noise,
            &self.graph,
            collector,
        ));
        self.with_unique_generator(unique_generator)
    }

    /// Java `@Override Zone.UniqueCandidate[] generateUniqueZoneCandidates(Zone.UniqueEntry[] entries, int maxPositions)`.
    pub fn generate_unique_zone_candidates(
        &self,
        _entries: &[crate::zone::unique::UniqueEntry],
        _max_positions: usize,
    ) -> Vec<UniqueCandidate> {
        // Java returns `Zone.UniqueCandidate.EMPTY_ARRAY`.
        Vec::new()
    }

    /// Java `Zone.UniqueCandidate[] getUniqueZoneCandidates(Map<String, Zone> zoneLookup)`.
    pub fn get_unique_zone_candidates(
        &self,
        zone_lookup: &std::collections::HashMap<String, Rc<Zone>>,
    ) -> Vec<UniqueCandidate> {
        self.unique_generator.get_candidates(zone_lookup)
    }
}
