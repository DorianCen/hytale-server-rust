//! Spawn light-range tests.
//!
//! Ported from `LightRangePredicate`: ranges are percentages (`0..100`) and are
//! converted to packed light (`0..15`) with `round(value * 0.15)`. A full
//! `[0, 100]` range is not tested, matching Java's `isPartialRange` skip.

use std::collections::BTreeMap;

use hytale_world::world::LightSample;

/// Configured light ranges from `NPCSpawn.LightRanges`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LightRanges {
    tests: BTreeMap<LightChannel, (u8, u8)>,
}

/// One Java `LightType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LightChannel {
    /// Total (sunlight-adjusted sky vs block light).
    Light,
    /// Raw sky light.
    SkyLight,
    /// Sky light multiplied by the clock's sunlight factor.
    Sunlight,
    /// Red block light.
    RedLight,
    /// Green block light.
    GreenLight,
    /// Blue block light.
    BlueLight,
}

impl LightChannel {
    /// Parse a `LightRanges` key.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Light" => Some(Self::Light),
            "SkyLight" => Some(Self::SkyLight),
            "Sunlight" => Some(Self::Sunlight),
            "RedLight" => Some(Self::RedLight),
            "GreenLight" => Some(Self::GreenLight),
            "BlueLight" => Some(Self::BlueLight),
            _ => None,
        }
    }
}

impl LightRanges {
    /// Build from the world-spawn key/range map.
    pub fn from_map(ranges: &BTreeMap<String, [f64; 2]>) -> Self {
        let mut tests = BTreeMap::new();
        for (name, range) in ranges {
            let Some(channel) = LightChannel::from_name(name) else {
                continue;
            };
            let min = percentage_to_absolute(range[0]);
            let max = percentage_to_absolute(range[1]);
            if min == 0 && max == 15 {
                continue;
            }
            tests.insert(channel, (min, max));
        }
        Self { tests }
    }

    /// Whether no partial range remains.
    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }

    /// Java `LightRangePredicate.test`.
    pub fn matches(&self, sample: LightSample, sunlight_factor: f64) -> bool {
        self.tests.iter().all(|(channel, (min, max))| {
            let value = match channel {
                LightChannel::Light => sample.total(sunlight_factor),
                LightChannel::SkyLight => sample.sky,
                LightChannel::Sunlight => (f64::from(sample.sky) * sunlight_factor) as u8,
                LightChannel::RedLight => sample.red,
                LightChannel::GreenLight => sample.green,
                LightChannel::BlueLight => sample.blue,
            };
            value >= *min && value <= *max
        })
    }
}

fn percentage_to_absolute(value: f64) -> u8 {
    (value * 0.15).round().clamp(0.0, 15.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_and_full_ranges_follow_the_java_conversion() {
        let mut ranges = BTreeMap::new();
        ranges.insert("Light".to_string(), [0.0, 8.0]);
        ranges.insert("SkyLight".to_string(), [0.0, 100.0]);
        let light = LightRanges::from_map(&ranges);
        assert_eq!(light.tests.get(&LightChannel::Light), Some(&(0, 1)));
        assert!(!light.tests.contains_key(&LightChannel::SkyLight));

        let sample = LightSample {
            sky: 0,
            red: 1,
            green: 0,
            blue: 0,
        };
        assert!(light.matches(sample, 1.0));
        assert!(!light.matches(
            LightSample {
                sky: 15,
                ..LightSample::default()
            },
            1.0
        ));
    }

    #[test]
    fn sunlight_uses_the_clock_factor() {
        let mut ranges = BTreeMap::new();
        ranges.insert("Sunlight".to_string(), [80.0, 100.0]);
        let light = LightRanges::from_map(&ranges);
        let sample = LightSample {
            sky: 15,
            ..LightSample::default()
        };
        assert!(light.matches(sample, 0.9));
        assert!(!light.matches(sample, 0.5));
    }
}
