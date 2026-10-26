//! Deterministic spawn-position search over a runtime world.
//!
//! Java's full `SpawningContext`/`SpawnPosition*` machinery works in 32³
//! sections, flood-fills candidate spans and validates NPC collision models.
//! This slice keeps the same movement-mode contract and search bounds but scans
//! loaded block columns in a deterministic near-first order:
//!
//! * walk — solid floor, air at feet and head;
//! * wade — solid floor, shallow fluid at feet, air at head;
//! * underwater walk — solid floor, fluid at feet and head;
//! * dive — fluid at feet and head without a floor;
//! * fly — air at feet and head.
//!
//! Block-set and fluid-tag stores are not ported yet; callers can still pass
//! an explicit allowed-floor block set or require the feet block to be fluid.

use std::collections::BTreeSet;

use hytale_world::collision::Vec3;
use hytale_world::world::World;

use crate::light::LightRanges;
use crate::navigation::BlockPos;

/// Java `MovementMode` subset used by spawning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnMovementMode {
    /// Dry solid ground.
    Walk,
    /// Shallow fluid over solid ground.
    Wade,
    /// Ground under deep fluid.
    UnderwaterWalk,
    /// Fully submerged without a floor requirement.
    Dive,
    /// Air.
    Fly,
}

/// A validated spawn position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnPosition {
    /// World position of the NPC's feet.
    pub position: Vec3,
    /// Block containing the feet.
    pub block: BlockPos,
    /// Movement mode that accepted the position.
    pub mode: SpawnMovementMode,
}

/// Search settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnSearchOptions {
    /// Horizontal radius in blocks.
    pub radius: i32,
    /// Vertical search range around the hint (`y - vertical_range ..= y + vertical_range`).
    pub vertical_range: i32,
    /// Movement mode to validate.
    pub mode: SpawnMovementMode,
    /// Optional allowed floor block ids (`RoleSpawnParameters.SpawnBlockSet`).
    pub allowed_floor: Option<BTreeSet<i32>>,
    /// Require a non-empty fluid at the feet.
    pub require_fluid: bool,
    /// Allowed fluid ids (`RoleSpawnParameters.SpawnFluidTag` resolved through
    /// [`crate::fluid_tags::FluidTagIndex`]).
    pub allowed_fluids: Option<BTreeSet<i32>>,
    /// Optional `NPCSpawn.LightRanges` constraints.
    pub light_ranges: LightRanges,
    /// World clock sunlight factor (`0..1`).
    pub sunlight_factor: f64,
}

impl Default for SpawnSearchOptions {
    fn default() -> Self {
        Self {
            radius: 8,
            vertical_range: 16,
            mode: SpawnMovementMode::Walk,
            allowed_floor: None,
            require_fluid: false,
            allowed_fluids: None,
            light_ranges: LightRanges::default(),
            sunlight_factor: 1.0,
        }
    }
}

/// Deterministic spawn-position search.
#[derive(Debug)]
pub struct SpawnPositionSearch<'a, F>
where
    F: Fn(i32) -> bool,
{
    world: &'a World,
    is_solid: F,
}

impl<'a, F> SpawnPositionSearch<'a, F>
where
    F: Fn(i32) -> bool + Copy,
{
    /// Wrap a loaded world with a block-id solidity rule.
    pub fn new(world: &'a World, is_solid: F) -> Self {
        Self { world, is_solid }
    }

    /// Find the nearest valid position to `hint`, or `None`.
    pub fn find(&self, hint: Vec3, options: &SpawnSearchOptions) -> Option<SpawnPosition> {
        let hint_block = BlockPos::new(
            hint.x.floor() as i32,
            hint.y.floor() as i32,
            hint.z.floor() as i32,
        );
        let radius = options.radius.clamp(0, 128);
        let vertical_range = options.vertical_range.clamp(-128, 128);
        for (dx, dz) in candidate_offsets(radius) {
            let x = hint_block.x + dx;
            let z = hint_block.z + dz;
            if !self.loaded(x, z) {
                continue;
            }
            let mut best: Option<SpawnPosition> = None;
            let mut best_distance = f64::MAX;
            for dy in -vertical_range..=vertical_range {
                let y = hint_block.y + dy;
                if !self.accepts(x, y, z, options) {
                    continue;
                }
                let distance = {
                    let px = f64::from(x) + 0.5 - hint.x;
                    let py = f64::from(y) - hint.y;
                    let pz = f64::from(z) + 0.5 - hint.z;
                    px * px + py * py + pz * pz
                };
                if distance < best_distance {
                    best_distance = distance;
                    best = Some(SpawnPosition {
                        position: Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5),
                        block: BlockPos::new(x, y, z),
                        mode: options.mode,
                    });
                }
            }
            if best.is_some() {
                return best;
            }
        }
        None
    }

    fn loaded(&self, x: i32, z: i32) -> bool {
        self.world.chunk(x >> 5, z >> 5).is_some()
    }

    fn accepts(&self, x: i32, y: i32, z: i32, options: &SpawnSearchOptions) -> bool {
        let floor = self.world.get_block(x, y - 1, z);
        let floor_solid = (self.is_solid)(floor);
        let feet = self.world.get_block(x, y, z);
        let head = self.world.get_block(x, y + 1, z);
        let feet_solid = (self.is_solid)(feet);
        let head_solid = (self.is_solid)(head);
        let feet_fluid_id = self.world.get_fluid_id(x, y, z);
        let feet_fluid = feet_fluid_id != 0;
        let head_fluid = self.world.get_fluid_id(x, y + 1, z) != 0;

        match options.mode {
            SpawnMovementMode::Walk => {
                if !floor_solid || feet_solid || head_solid {
                    return false;
                }
            }
            SpawnMovementMode::Wade => {
                if !floor_solid || feet_solid || head_solid || !feet_fluid {
                    return false;
                }
            }
            SpawnMovementMode::UnderwaterWalk => {
                if !floor_solid || feet_solid || head_solid || !feet_fluid || !head_fluid {
                    return false;
                }
            }
            SpawnMovementMode::Dive => {
                if feet_solid || head_solid || !feet_fluid || !head_fluid {
                    return false;
                }
            }
            SpawnMovementMode::Fly => {
                if feet_solid || head_solid {
                    return false;
                }
            }
        }

        if let Some(allowed) = &options.allowed_fluids {
            if !allowed.contains(&feet_fluid_id) {
                return false;
            }
        } else if options.require_fluid && !feet_fluid {
            return false;
        }
        if let Some(allowed) = &options.allowed_floor {
            if !allowed.contains(&floor) {
                return false;
            }
        }
        if !options.light_ranges.is_empty() {
            let Some(sample) = self.world.light_sample(x, y, z) else {
                return false;
            };
            if !options
                .light_ranges
                .matches(sample, options.sunlight_factor)
            {
                return false;
            }
        }
        true
    }
}

/// Horizontal candidate offsets ordered by squared distance, then `(dx, dz)`.
fn candidate_offsets(radius: i32) -> Vec<(i32, i32)> {
    let mut offsets = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            offsets.push((dx, dz));
        }
    }
    offsets.sort_by_key(|(dx, dz)| (dx * dx + dz * dz, *dx, *dz));
    offsets
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use hytale_world::chunk::column::ChunkColumn;
    use hytale_world::chunk::section::light::{LightData, LIGHT_BLOCK_COUNT};
    use hytale_world::world::WorldStorage;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hytale-npc-spawn-position-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    fn flat_world(name: &str) -> (World, PathBuf) {
        let dir = temp_dir(name);
        let mut world = World::new(WorldStorage::new(dir.clone()));
        let mut column = ChunkColumn::new(0, 0);
        for x in 0..32 {
            for z in 0..32 {
                column.set_block(x, 0, z, 1, 0, 0).unwrap();
            }
        }
        world.insert_chunk(column);
        (world, dir)
    }

    #[test]
    fn walk_finds_the_nearest_flat_position() {
        let (world, dir) = flat_world("walk");
        let search = SpawnPositionSearch::new(&world, |block| block != 0);
        let position = search
            .find(
                Vec3::new(15.5, 1.0, 15.5),
                &SpawnSearchOptions {
                    radius: 2,
                    vertical_range: 1,
                    mode: SpawnMovementMode::Walk,
                    ..SpawnSearchOptions::default()
                },
            )
            .expect("flat floor");
        assert_eq!(position.block, BlockPos::new(15, 1, 15));
        assert_eq!(position.position, Vec3::new(15.5, 1.0, 15.5));
        assert_eq!(position.mode, SpawnMovementMode::Walk);

        let with_block_set = SpawnSearchOptions {
            radius: 1,
            vertical_range: 1,
            mode: SpawnMovementMode::Walk,
            allowed_floor: Some(BTreeSet::from([2])),
            require_fluid: false,
            allowed_fluids: None,
            ..SpawnSearchOptions::default()
        };
        assert!(search
            .find(Vec3::new(15.5, 1.0, 15.5), &with_block_set)
            .is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn light_ranges_use_the_global_light_sample() {
        let (mut world, dir) = flat_world("light");
        let values = vec![15u16 << 12; LIGHT_BLOCK_COUNT];
        world
            .chunk_mut(0, 0)
            .expect("chunk")
            .section_mut(1)
            .expect("section")
            .block
            .set_global_light(LightData::from_values(values, 0));

        let search = SpawnPositionSearch::new(&world, |block| block != 0);
        let mut bright = BTreeMap::new();
        bright.insert("SkyLight".to_string(), [90.0, 100.0]);
        let options = SpawnSearchOptions {
            radius: 1,
            vertical_range: 1,
            mode: SpawnMovementMode::Walk,
            light_ranges: LightRanges::from_map(&bright),
            ..SpawnSearchOptions::default()
        };
        assert!(search.find(Vec3::new(4.5, 1.0, 4.5), &options).is_some());

        let mut dark = BTreeMap::new();
        dark.insert("Light".to_string(), [0.0, 8.0]);
        let options = SpawnSearchOptions {
            light_ranges: LightRanges::from_map(&dark),
            ..options
        };
        assert!(search.find(Vec3::new(4.5, 1.0, 4.5), &options).is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn water_and_fly_modes_validate_their_medium() {
        let (mut world, dir) = flat_world("water");
        {
            let column = world.chunk_mut(0, 0).expect("chunk");
            column.set_fluid(4, 1, 4, 1, 8).unwrap();
            column.set_fluid(4, 2, 4, 1, 8).unwrap();
        }
        let search = SpawnPositionSearch::new(&world, |block| block != 0);
        let options = |mode| SpawnSearchOptions {
            radius: 1,
            vertical_range: 1,
            mode,
            ..SpawnSearchOptions::default()
        };
        assert!(search
            .find(Vec3::new(4.5, 1.0, 4.5), &options(SpawnMovementMode::Wade))
            .is_some());
        assert!(search
            .find(
                Vec3::new(4.5, 1.0, 4.5),
                &options(SpawnMovementMode::UnderwaterWalk)
            )
            .is_some());
        assert!(search
            .find(Vec3::new(4.5, 1.0, 4.5), &options(SpawnMovementMode::Dive))
            .is_some());
        assert!(search
            .find(Vec3::new(4.5, 5.0, 4.5), &options(SpawnMovementMode::Fly))
            .is_some());

        let allowed = SpawnSearchOptions {
            radius: 1,
            vertical_range: 1,
            mode: SpawnMovementMode::Wade,
            allowed_fluids: Some(BTreeSet::from([1])),
            ..SpawnSearchOptions::default()
        };
        assert!(search.find(Vec3::new(4.5, 1.0, 4.5), &allowed).is_some());
        let wrong = SpawnSearchOptions {
            allowed_fluids: Some(BTreeSet::from([2])),
            ..allowed
        };
        assert!(search.find(Vec3::new(4.5, 1.0, 4.5), &wrong).is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
