//! Java `ISpawnProvider` codec subset.
//!
//! The world config and portal overrides store a provider map
//! (`{"Id":"Global","SpawnPoint":{...}}`). This module parses the common
//! `Global`, `Individual` and `FitToHeightMap` forms and resolves the
//! per-player spawn without inventing a new JSON shape.

use serde_json::Value;

use crate::player_data::{PlayerTransform, Position, Rotation};
use crate::world_config::WorldUuid;

/// Java `HeightmapColumn.NO_HEIGHT`.
pub const NO_HEIGHT: i32 = i32::MIN;

/// One configured spawn provider.
#[derive(Debug, Clone, PartialEq)]
pub enum SpawnProvider {
    /// One static spawn point for every player.
    Global(PlayerTransform),
    /// A list selected deterministically from the player UUID.
    Individual(Vec<PlayerTransform>),
    /// Another provider whose sentinel `Y` is fitted to the column heightmap.
    FitToHeightMap(Box<SpawnProvider>),
}

impl SpawnProvider {
    /// Parse a Java provider map. Unknown/missing ids return `None`.
    pub fn from_value(value: &Value) -> Option<Self> {
        let id = value
            .get("Id")
            .or_else(|| value.get("Type"))
            .and_then(Value::as_str)?;
        match id {
            "Global" => Some(Self::Global(transform(value.get("SpawnPoint")?)?)),
            "Individual" => {
                let points = value
                    .get("SpawnPoints")?
                    .as_array()?
                    .iter()
                    .filter_map(transform)
                    .collect::<Vec<_>>();
                (!points.is_empty()).then_some(Self::Individual(points))
            }
            "FitToHeightMap" => Some(Self::FitToHeightMap(Box::new(Self::from_value(
                value.get("SpawnProvider")?,
            )?))),
            _ => None,
        }
    }

    /// Every configured spawn point, used by world defaults and diagnostics.
    pub fn spawn_points(&self) -> Vec<PlayerTransform> {
        match self {
            Self::Global(point) => vec![*point],
            Self::Individual(points) => points.clone(),
            Self::FitToHeightMap(inner) => inner.spawn_points(),
        }
    }

    /// Resolve the provider's raw spawn for one player. The result can still
    /// carry [`NO_HEIGHT`] in `Y` for a `FitToHeightMap` provider; callers use
    /// [`Self::resolve_for`] when they have a heightmap.
    pub fn spawn_point(&self, uuid: WorldUuid) -> Option<PlayerTransform> {
        match self {
            Self::Global(point) => Some(*point),
            Self::Individual(points) => {
                if points.is_empty() {
                    return None;
                }
                let index = (hash_uuid(uuid) as i32).rem_euclid(points.len() as i32) as usize;
                Some(points[index])
            }
            Self::FitToHeightMap(inner) => inner.spawn_point(uuid),
        }
    }

    /// Resolve one player's spawn and fit a `FitToHeightMap` provider.
    pub fn resolve_for(
        &self,
        uuid: WorldUuid,
        mut height: impl FnMut(i32, i32) -> Option<i32>,
    ) -> Option<PlayerTransform> {
        let mut point = self.spawn_point(uuid)?;
        if matches!(self, Self::FitToHeightMap(_)) || point.position.y == f64::from(NO_HEIGHT) {
            let x = point.position.x.floor() as i32;
            let z = point.position.z.floor() as i32;
            point.position.y = f64::from(height(x, z).map(|height| height + 1).unwrap_or(1));
        }
        Some(point)
    }

    /// Java `isWithinSpawnDistance`: every point is checked, with a strict
    /// `< distance²` comparison.
    pub fn is_within_spawn_distance(&self, position: &Position, distance: f64) -> bool {
        let distance_squared = distance * distance;
        self.spawn_points().iter().any(|point| {
            let dx = position.x - point.position.x;
            let dy = position.y - point.position.y;
            let dz = position.z - point.position.z;
            dx * dx + dy * dy + dz * dz < distance_squared
        })
    }
}

fn transform(value: &Value) -> Option<PlayerTransform> {
    let position = Position {
        x: value.get("X")?.as_f64()?,
        y: value.get("Y")?.as_f64()?,
        z: value.get("Z")?.as_f64()?,
    };
    let rotation = Rotation {
        pitch: degrees(value.get("Pitch")),
        yaw: degrees(value.get("Yaw")),
        roll: degrees(value.get("Roll")),
    };
    Some(PlayerTransform::new(position, rotation))
}

fn degrees(value: Option<&Value>) -> f32 {
    value
        .and_then(Value::as_f64)
        .map(|degrees| degrees.to_radians() as f32)
        .unwrap_or(0.0)
}

/// Java `HashUtil.hashUuid`.
fn hash_uuid(uuid: WorldUuid) -> i64 {
    let bytes = uuid.as_bytes();
    let most = i64::from_be_bytes(bytes[0..8].try_into().expect("eight bytes"));
    let least = i64::from_be_bytes(bytes[8..16].try_into().expect("eight bytes"));
    mix_pair(least, most)
}

fn mix(value: i64) -> i64 {
    let mut value = value as u64;
    value = ((value >> 30) ^ value).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = ((value >> 27) ^ value).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value >> 31) ^ value) as i64
}

fn mix_pair(first: i64, second: i64) -> i64 {
    let mut value = first as u64;
    value = ((mix(first) as u64 >> 30) ^ value).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= mix(second) as u64 >> 31;
    value as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn uuid(value: &str) -> WorldUuid {
        value.parse().expect("uuid")
    }

    #[test]
    fn parses_the_real_portal_override_and_world_provider_shapes() {
        let global = json!({
            "Id": "Global",
            "SpawnPoint": {"X": 0.5, "Y": 149, "Z": 290.5}
        });
        let provider = SpawnProvider::from_value(&global).expect("global provider");
        let point = provider.spawn_point(uuid("00000000-0000-0000-0000-000000000001"));
        assert_eq!(point.expect("point").position.y, 149.0);

        let individual = json!({
            "Id": "Individual",
            "SpawnPoints": [
                {"X": 1, "Y": 70, "Z": 2},
                {"X": 3, "Y": 71, "Z": 4}
            ]
        });
        let provider = SpawnProvider::from_value(&individual).expect("individual provider");
        let first = provider
            .spawn_point(uuid("00000000-0000-0000-0000-000000000001"))
            .expect("first");
        let again = provider
            .spawn_point(uuid("00000000-0000-0000-0000-000000000001"))
            .expect("again");
        assert_eq!(first, again, "UUID selection is deterministic");
        assert!([1.0, 3.0].contains(&first.position.x));

        let fit = json!({
            "Id": "FitToHeightMap",
            "SpawnProvider": {
                "Id": "Global",
                "SpawnPoint": {"X": 4, "Y": -2147483648, "Z": 7}
            }
        });
        let provider = SpawnProvider::from_value(&fit).expect("fit provider");
        let resolved = provider
            .resolve_for(uuid("00000000-0000-0000-0000-000000000001"), |_, _| {
                Some(90)
            })
            .expect("fitted point");
        assert_eq!(resolved.position.y, 91.0);
    }

    #[test]
    fn spawn_distance_matches_java_strict_comparison() {
        let provider = SpawnProvider::Global(PlayerTransform::new(
            Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        ));
        assert!(provider.is_within_spawn_distance(
            &Position {
                x: 3.0,
                y: 4.0,
                z: 0.0
            },
            5.1
        ));
        assert!(!provider.is_within_spawn_distance(
            &Position {
                x: 3.0,
                y: 4.0,
                z: 0.0
            },
            5.0
        ));
    }
}
