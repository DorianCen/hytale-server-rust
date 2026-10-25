//! Per-world weather state and hourly environment forecasts.
//!
//! Ported from `builtin/Weather/resources/WeatherResource`,
//! `builtin/Weather/systems/WeatherSystem`,
//! `asset/type/environment/config/WeatherForecast` and
//! `asset/type/weather/config/Weather`. Weather only changes on an in-game hour
//! boundary unless `WorldConfig.ForcedWeather` is set; this module owns the
//! deterministic selection and the effective index per environment.

use std::collections::BTreeMap;

use hytale_assets::AssetRegistry;
use hytale_math::fast_random::FastRandom;
use serde_json::Value;

/// One weighted `WeatherForecast` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherWeight {
    /// Weather asset id.
    pub weather_id: String,
    /// Resolved weather index.
    pub weather_index: i32,
    /// Relative weight.
    pub weight: f64,
}

/// One environment's 24-hour forecast table.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentForecast {
    /// Seed key (`WeatherForecastSeed`, defaulting to the environment id).
    pub seed_key: String,
    /// Hour (`0..24`) to forecast entries.
    pub hours: [Vec<WeatherWeight>; 24],
}

impl EnvironmentForecast {
    /// Parse `Environment.WeatherForecasts` and its seed key with a weather-id
    /// resolver (the registry in production, a small map in tests).
    pub fn from_value(
        environment: &str,
        value: &Value,
        weather_id: impl Fn(&str) -> Option<i32>,
    ) -> Self {
        let mut hours: [Vec<WeatherWeight>; 24] = std::array::from_fn(|_| Vec::new());
        if let Some(forecasts) = value.get("WeatherForecasts").and_then(Value::as_object) {
            for (hour, entries) in forecasts {
                let Ok(hour) = hour.parse::<usize>() else {
                    continue;
                };
                if hour >= 24 {
                    continue;
                }
                let Some(entries) = entries.as_array() else {
                    continue;
                };
                for entry in entries {
                    let Some(weather_name) = entry.get("WeatherId").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(weather_index) = weather_id(weather_name) else {
                        continue;
                    };
                    hours[hour].push(WeatherWeight {
                        weather_id: weather_name.to_string(),
                        weather_index,
                        weight: entry.get("Weight").and_then(Value::as_f64).unwrap_or(1.0),
                    });
                }
            }
        }
        if hours.iter().all(Vec::is_empty) {
            // Java `Environment.DEFAULT_WEATHER_FORECAST`.
            let unknown = weather_id("Unknown").unwrap_or(0);
            for entries in &mut hours {
                entries.push(WeatherWeight {
                    weather_id: "Unknown".to_string(),
                    weather_index: unknown,
                    weight: 1.0,
                });
            }
        } else {
            // Java's codec requires all 24 keys; a malformed/missing hour falls
            // back to the nearest configured hour so the state stays total.
            for hour in 0..24 {
                if !hours[hour].is_empty() {
                    continue;
                }
                let source = (0..24)
                    .map(|offset| (hour + offset) % 24)
                    .find(|candidate| !hours[*candidate].is_empty());
                if let Some(source) = source {
                    hours[hour] = hours[source].clone();
                }
            }
        }
        Self {
            seed_key: value
                .get("WeatherForecastSeed")
                .and_then(Value::as_str)
                .unwrap_or(environment)
                .to_string(),
            hours,
        }
    }

    /// Java `WeightedMap.get(Random)`.
    pub fn select(&self, hour: i32, random_value: f64) -> Option<&WeatherWeight> {
        let entries = &self.hours[hour.rem_euclid(24) as usize];
        let sum: f64 = entries.iter().map(|entry| entry.weight.max(0.0)).sum();
        if entries.is_empty() || sum <= 0.0 {
            return entries.first();
        }
        const EPSILON: f64 = 0.99999;
        let mut weight_sum = random_value.min(EPSILON) * sum;
        for entry in entries {
            weight_sum -= entry.weight.max(0.0);
            if weight_sum <= 1.0 - EPSILON {
                return Some(entry);
            }
        }
        entries.last()
    }
}

/// Per-world weather state (`WeatherResource`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeatherState {
    forced_weather: Option<i32>,
    previous_forced_weather: Option<i32>,
    environment_weather: BTreeMap<i32, i32>,
    previous_hour: Option<i32>,
}

impl WeatherState {
    /// Create state with an optional forced weather index.
    pub fn new(forced_weather: Option<i32>) -> Self {
        Self {
            forced_weather,
            previous_forced_weather: forced_weather,
            environment_weather: BTreeMap::new(),
            previous_hour: None,
        }
    }

    /// Set/replace the forced weather (`None` clears it).
    pub fn set_forced_weather(&mut self, forced_weather: Option<i32>) {
        self.previous_forced_weather = self.forced_weather;
        self.forced_weather = forced_weather.filter(|index| *index != 0);
    }

    /// Current forced weather index.
    pub fn forced_weather(&self) -> Option<i32> {
        self.forced_weather
    }

    /// Whether the forced weather changed since the last call.
    pub fn consume_forced_weather_change(&mut self) -> bool {
        if self.previous_forced_weather == self.forced_weather {
            return false;
        }
        self.previous_forced_weather = self.forced_weather;
        true
    }

    /// Update every environment forecast when the in-game hour changes.
    ///
    /// Returns `true` when a new hour was selected.
    pub fn update(
        &mut self,
        hour: i32,
        game_time_seconds: i64,
        world_seed: i64,
        forecasts: &BTreeMap<i32, EnvironmentForecast>,
    ) -> bool {
        if self.previous_hour == Some(hour) {
            return false;
        }
        self.previous_hour = Some(hour);
        if self.forced_weather.is_some() {
            return false;
        }
        for (&environment, forecast) in forecasts {
            let seed = hytale_math::hash_util::hash3(
                world_seed,
                java_string_hash(&forecast.seed_key),
                game_time_seconds / 3_600,
            );
            let mut random = FastRandom::new(seed);
            if let Some(weather) = forecast.select(hour, random.next_double()) {
                self.environment_weather
                    .insert(environment, weather.weather_index);
            }
        }
        true
    }

    /// Effective weather for an environment (`ForcedWeather` wins).
    pub fn effective_weather(&self, environment: i32) -> Option<i32> {
        self.forced_weather
            .or_else(|| self.environment_weather.get(&environment).copied())
    }

    /// Current environment map.
    pub fn environment_weather(&self) -> &BTreeMap<i32, i32> {
        &self.environment_weather
    }

    /// Current in-game hour last applied.
    pub fn hour(&self) -> Option<i32> {
        self.previous_hour
    }
}

/// Java `String.hashCode`.
fn java_string_hash(text: &str) -> i64 {
    let mut hash = 0i64;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i64::from(unit));
    }
    hash as i32 as i64
}

/// Build every environment forecast from the registry's raw environment store.
pub fn environment_forecasts(registry: &AssetRegistry) -> BTreeMap<i32, EnvironmentForecast> {
    registry
        .environments
        .iter()
        .filter_map(|record| {
            let id = record.id?;
            Some((
                id,
                EnvironmentForecast::from_value(&record.key, &record.value, |weather| {
                    registry.weather_id(weather)
                }),
            ))
        })
        .collect()
}

/// Java `WeatherResource.getWeatherIndexForEnvironment` and
/// `getEffectiveWeatherIndex` combined.
pub fn weather_for_environment(state: &WeatherState, environment: i32) -> i32 {
    state.effective_weather(environment).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn forecast() -> EnvironmentForecast {
        let values = json!({
            "WeatherForecastSeed": "seed",
            "WeatherForecasts": {
                "0": [{"WeatherId": "A", "Weight": 1}, {"WeatherId": "B", "Weight": 3}],
                "1": [{"WeatherId": "C", "Weight": 1}]
            }
        });
        let ids = BTreeMap::from([("Unknown", 0), ("A", 1), ("B", 2), ("C", 3)]);
        EnvironmentForecast::from_value("Env", &values, |weather| ids.get(weather).copied())
    }

    #[test]
    fn weighted_selection_uses_the_java_epsilon_rule() {
        let forecast = forecast();
        assert_eq!(forecast.select(0, 0.0).unwrap().weather_id, "A");
        assert_eq!(forecast.select(0, 0.20).unwrap().weather_id, "A");
        assert_eq!(forecast.select(0, 0.30).unwrap().weather_id, "B");
        assert_eq!(forecast.select(0, 0.999).unwrap().weather_id, "B");
        assert_eq!(forecast.select(1, 0.5).unwrap().weather_id, "C");
    }

    #[test]
    fn weather_changes_only_on_the_hour_and_forced_weather_wins() {
        let forecast = forecast();
        let forecasts = BTreeMap::from([(7, forecast)]);
        let mut state = WeatherState::new(None);
        assert!(state.update(0, 0, 1, &forecasts));
        assert!(!state.update(0, 1, 1, &forecasts));
        let first = state.effective_weather(7).expect("weather");
        assert!(state.update(1, 3_600, 1, &forecasts));
        assert!(state.effective_weather(7).is_some());
        state.set_forced_weather(Some(2));
        assert_eq!(state.effective_weather(7), Some(2));
        assert!(state.consume_forced_weather_change());
        state.set_forced_weather(None);
        assert_ne!(state.effective_weather(7), Some(2));
        assert_eq!(state.hour(), Some(1));
        let _ = first;
    }
}
