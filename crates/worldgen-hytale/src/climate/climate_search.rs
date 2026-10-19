// Ported from: `.hytale-source/.../worldgen/climate/ClimateSearch.java`
//! Java `climate.ClimateSearch` — ring-spiral scoring search for unique zone
//! positions matching climate rules.

use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_noise::ClimateNoise;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;

/// Java `ClimateSearch.STEP_SIZE`.
pub const STEP_SIZE: i32 = 100;
/// Java `ClimateSearch.DEFAULT_RADIUS`.
pub const DEFAULT_RADIUS: i32 = 5_000;
/// Java `ClimateSearch.MAX_RADIUS`.
pub const MAX_RADIUS: i32 = 10_000;
/// Java `ClimateSearch.PI2`.
pub const PI2: f64 = std::f64::consts::PI * 2.0;
/// Java `ClimateSearch.SEED_OFFSET`.
pub const SEED_OFFSET: i64 = 0x18273192381;
/// Java `ClimateSearch.TARGET_SCORE`.
pub const TARGET_SCORE: f64 = 0.75;

/// Java `ClimateSearch.Result`.
#[derive(Debug, Clone, PartialEq)]
pub struct Result {
    /// Java `Vector2i position`.
    pub position: (i32, i32),
    /// Java `double score`.
    pub score: f64,
    /// Java `long time_ms` (wall-clock search time — nondeterministic).
    pub time_ms: u128,
}

impl Result {
    /// Java `Result.pretty()`.
    pub fn pretty(&self) -> String {
        let score = self.score * 100.0;
        format!(
            "Position: {{{}, {}}}, Score: {:.2}%, Time: {}ms",
            self.position.0, self.position.1, score, self.time_ms
        )
    }
}

/// Java `ClimateSearch.Range`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    /// Java `public final double value`.
    pub value: f64,
    /// Java `public final double radius`.
    pub radius: f64,
    /// Java `public final double weight`.
    pub weight: f64,
}

impl Range {
    /// Java `Range.DEFAULT`.
    pub const DEFAULT: Range = Range {
        value: 0.0,
        radius: 1.0,
        weight: 0.0,
    };

    /// Java `Range(double value, double radius, double weight)`.
    pub fn new(value: f64, radius: f64, weight: f64) -> Self {
        Self {
            value,
            radius,
            weight,
        }
    }

    /// Java `double score(double value)`.
    pub fn score(&self, value: f64) -> f64 {
        let dif = self.radius.min((value - self.value).abs());
        1.0 - (dif / self.radius)
    }

    /// Java `boolean test(double value)`.
    pub fn test(&self, value: f64) -> bool {
        (value - self.value).abs() <= self.radius
    }
}

/// Java `ClimateSearch.Rule`.
pub struct Rule {
    /// Java `public final Range continent`.
    pub continent: Range,
    /// Java `public final Range temperature`.
    pub temperature: Range,
    /// Java `public final Range intensity`.
    pub intensity: Range,
    /// Java `public final Range fade`.
    pub fade: Range,
    /// Java `public transient final double sumWeight`.
    pub sum_weight: f64,
}

impl Rule {
    /// Java `Rule(Range, Range, Range, Range)`.
    pub fn new(continent: Range, temperature: Range, intensity: Range, fade: Range) -> Self {
        Self {
            sum_weight: continent.weight + temperature.weight + intensity.weight + fade.weight,
            continent,
            temperature,
            intensity,
            fade,
        }
    }

    /// Java `double score(double continent, double temperature, double intensity, double fade)`.
    pub fn score(&self, continent: f64, temperature: f64, intensity: f64, fade: f64) -> f64 {
        let mut sum_score = 0.0;
        sum_score += self.continent.score(continent) * self.continent.weight;
        sum_score += self.temperature.score(temperature) * self.temperature.weight;
        sum_score += self.intensity.score(intensity) * self.intensity.weight;
        sum_score += self.fade.score(fade) * self.fade.weight;
        sum_score / self.sum_weight
    }
}

/// Java `ClimateSearch.search(...)` — synchronous version (Java wraps in
/// `CompletableFuture.supplyAsync`; the Rust caller decides threading).
#[allow(clippy::too_many_arguments)] // faithful to the Java signature
pub fn search(
    seed: i32,
    cx: i32,
    cy: i32,
    start_radius: i32,
    search_radius: i32,
    rule: &Rule,
    noise: &ClimateNoise,
    graph: &ClimateGraph,
) -> Result {
    let start = std::time::Instant::now();

    let mut best_score = 0.0f64;
    let mut best_position = (cx, cy);

    let radius = search_radius.min(MAX_RADIUS);
    let mut rng = FastRandom::new(hash_util::hash2(seed as i64, SEED_OFFSET));

    let mut r = start_radius;
    while r <= radius {
        let steps = (PI2 * r as f64 / STEP_SIZE as f64).floor() as i32;
        let inc = PI2 / steps as f64;
        let off = PI2 * rng.next_double();

        for i in 0..steps {
            let t = i as f64 * inc + off;
            let x = cx + ((t.cos() * r as f64) as i32);
            let y = cy + ((t.sin() * r as f64) as i32);
            let score = collect(seed, x, y, noise, graph, rule);

            if score > best_score {
                best_position = (x, y);
                best_score = score;
            }
        }

        if best_score >= TARGET_SCORE {
            break;
        }

        r += STEP_SIZE;
    }

    Result {
        position: best_position,
        score: best_score,
        time_ms: start.elapsed().as_millis(),
    }
}

/// Java private static `collect(int seed, int x, int y, ClimateNoise, ClimateGraph, Rule)`.
fn collect(
    seed: i32,
    x: i32,
    y: i32,
    noise: &ClimateNoise,
    graph: &ClimateGraph,
    rule: &Rule,
) -> f64 {
    let xf = x as f64;
    let yf = y as f64;

    let continent = noise.continent.get(seed, xf, yf);
    if !rule.continent.test(continent) {
        return 0.0;
    }

    let temperature = noise.temperature.get(seed, xf, yf);
    if !rule.temperature.test(temperature) {
        return 0.0;
    }

    let intensity = noise.intensity.get(seed, xf, yf);
    if !rule.intensity.test(intensity) {
        return 0.0;
    }

    let fade = graph.get_fade_raw(temperature, intensity);
    if !rule.fade.test(fade) {
        return 0.0;
    }

    rule.score(continent, temperature, intensity, fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_scoring_is_symmetric_and_clamped() {
        let r = Range::new(0.5, 0.2, 1.0);
        assert!((r.score(0.5) - 1.0).abs() < 1e-12); // exact match → 1
        assert!((r.score(0.6) - 0.5).abs() < 1e-12); // half radius → 0.5
        assert!((r.score(0.7) - 0.0).abs() < 1e-12); // at radius → 0
        assert!(r.score(0.9) <= 0.0 || r.score(0.9).is_infinite()); // clamped negative
        assert!(r.test(0.7));
        assert!(!r.test(0.71));
    }

    #[test]
    fn default_range_matches_unit_band() {
        // DEFAULT = (value 0, radius 1): test() accepts |v| <= 1
        let r = Range::DEFAULT;
        assert!(r.test(-1.0));
        assert!(r.test(0.5));
        assert!(!r.test(-1.5));
        assert_eq!(r.weight, 0.0);
    }
}
