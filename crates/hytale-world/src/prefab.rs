//! Paste a parsed prefab into a live [`World`].
//!
//! Java `PrefabUtil.paste` writes the rotated blocks/fluids, installs the
//! block's `components` holder, applies the support value and spawns the
//! prefab entities after rotating their position. The Rust port keeps the same
//! values, but stores block components and entities in the runtime stores from
//! [`crate::runtime`] instead of the ECS.
//!
//! The caller must have loaded every chunk the prefab touches; this function
//! reports a missing chunk instead of silently skipping it.

use hytale_math::fast_random::FastRandom;
use worldgen_hytale::prefab::buffer::prefab_buffer::PrefabBuffer;
use worldgen_hytale::prefab::buffer::prefab_buffer_call::PrefabBufferCallState;
use worldgen_hytale::prefab::prefab_rotation::PrefabRotation;
use worldgen_model::block_key::FillerBlockUtil;

use crate::runtime::{transform, RuntimeEntityId, RuntimeEntityKind};
use crate::world::{Result, World, WorldError};

/// What one paste produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrefabPasteReport {
    /// Prefab name or path used by the caller.
    pub prefab: String,
    /// Blocks written.
    pub blocks: usize,
    /// Fluid cells written (including empty cells that cleared a fluid).
    pub fluids: usize,
    /// Block-component documents installed.
    pub block_components: usize,
    /// Spawned runtime entity ids.
    pub entities: Vec<RuntimeEntityId>,
}

/// Paste `prefab` at `origin` with a quarter-turn rotation.
///
/// `prefab_name` is recorded on the spawned entities (the Java
/// `FromPrefab`/weight link). The random call context is seeded from `seed` so
/// a prefab with `%chance` blocks is deterministic.
pub fn paste_prefab(
    world: &mut World,
    prefab_name: &str,
    prefab: &PrefabBuffer,
    origin: (i32, i32, i32),
    rotation: PrefabRotation,
    seed: i64,
) -> Result<PrefabPasteReport> {
    let access = prefab.new_access();
    let call = PrefabBufferCallState::new(FastRandom::new(seed), rotation);
    let mut report = PrefabPasteReport {
        prefab: prefab_name.to_string(),
        ..PrefabPasteReport::default()
    };

    let mut paste_error = None;
    access.for_each(
        &mut |_, _, _, _| true,
        &mut |visit, _| {
            if paste_error.is_some() {
                return;
            }
            let x = origin.0 + visit.x;
            let y = origin.1 + visit.y;
            let z = origin.2 + visit.z;
            if world.chunk(x >> 5, z >> 5).is_none() {
                paste_error = Some(format!("chunk ({}, {}) is not loaded", x >> 5, z >> 5));
                return;
            }

            // Java writes the fluid for every visited cell, including the
            // empty cells that clear a fluid.
            match world.set_fluid(x, y, z, visit.fluid_id, visit.fluid_level) {
                Ok(true) => report.fluids += 1,
                Ok(false) => {}
                Err(error) => {
                    paste_error = Some(error.to_string());
                    return;
                }
            }

            // Java skips the block itself for filler cells but still stores
            // their components.
            if visit.filler == FillerBlockUtil::NO_FILLER {
                match world.set_block_full(x, y, z, visit.block_id, visit.rotation, visit.filler) {
                    Ok(true) => report.blocks += 1,
                    Ok(false) => {}
                    Err(error) => paste_error = Some(error.to_string()),
                }
            }

            if visit.support_value != 0 {
                world.set_support_value(x, y, z, visit.support_value);
            }

            if let Some(components) = &visit.entry.components {
                world.set_block_component(x, y, z, visit.block_id, components.clone());
                report.block_components += 1;
            }
        },
        None,
        &call,
    );

    if let Some(error) = paste_error {
        return Err(WorldError::PrefabPaste(error));
    }

    access.for_each_entity(rotation, |entity, position| {
        let world_position = [
            f64::from(origin.0) + position[0],
            f64::from(origin.1) + position[1],
            f64::from(origin.2) + position[2],
        ];
        let transform = transform(world_position, entity.rotation);
        // Java mutates the holder's `TransformComponent` to the paste position;
        // keep the preserved component document consistent with the runtime
        // transform so later presentation code can read either.
        let mut components = entity.components.clone();
        if let Some(transform) = components
            .get_mut("Transform")
            .and_then(serde_json::Value::as_object_mut)
        {
            let position = transform
                .entry("Position")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(position) = position.as_object_mut() {
                position.insert("X".to_string(), serde_json::json!(world_position[0]));
                position.insert("Y".to_string(), serde_json::json!(world_position[1]));
                position.insert("Z".to_string(), serde_json::json!(world_position[2]));
            }
        }
        let id = world.spawn_entity(
            RuntimeEntityKind::Prefab,
            Some(prefab_name.to_string()),
            transform,
            components,
        );
        report.entities.push(id);
    });

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::column::ChunkColumn;
    use crate::runtime::RuntimeEntity;
    use worldgen_hytale::prefab::buffer::prefab_buffer::{PrefabBuffer, PrefabEntity};
    use worldgen_hytale::prefab::buffer::prefab_buffer_block_entry::PrefabBufferBlockEntry;

    fn prefab_with_entity() -> PrefabBuffer {
        let mut entry = PrefabBufferBlockEntry::new(0, 5, "Rock_Stone");
        entry.components = Some(serde_json::json!({
            "Components": {"FarmingBlock": {"GrowthProgress": 0.0}}
        }));
        let mut builder = PrefabBuffer::new_builder();
        builder.set_anchor((0, 0, 0));
        builder.add_column(0, 0, vec![entry]);
        builder.set_entities(vec![PrefabEntity {
            position: [1.5, 1.0, 0.5],
            rotation: [0.0, 0.25, 0.0],
            components: serde_json::json!({"Transform": {"Position": {"X": 1.5}}}),
        }]);
        builder.build()
    }

    #[test]
    fn a_paste_rotates_blocks_components_and_entities() {
        let mut world = World::new(crate::world::WorldStorage::new(std::env::temp_dir().join(
            format!(
                "hytale-prefab-paste-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
        )));
        world.insert_chunk(ChunkColumn::new(0, 0));
        let report = paste_prefab(
            &mut world,
            "Test",
            &prefab_with_entity(),
            (0, 64, 0),
            PrefabRotation::Rotation90,
            1,
        )
        .expect("paste");

        assert_eq!(report.blocks, 1);
        assert_eq!(report.block_components, 1);
        assert_eq!(report.entities.len(), 1);
        // Rotation90 maps (x, z) to (z, -x): (0,0) for the block.
        assert_eq!(world.get_block(0, 64, 0), 5);
        assert!(world.block_component(0, 64, 0).is_some());
        let entity: &RuntimeEntity = world.entity(report.entities[0]).unwrap();
        // The entity's local (1.5, 0.5) rotates to (0.5, -1.5).
        assert!((entity.transform.position.x - 0.5).abs() < f64::EPSILON);
        assert!((entity.transform.position.z + 1.5).abs() < f64::EPSILON);
        assert_eq!(entity.transform.rotation.yaw, 0.25);
        assert_eq!(entity.prefab.as_deref(), Some("Test"));
        assert_eq!(
            entity
                .components
                .get("Transform")
                .and_then(|transform| transform.get("Position"))
                .and_then(|position| position.get("X"))
                .and_then(serde_json::Value::as_f64),
            Some(0.5)
        );
    }
}
