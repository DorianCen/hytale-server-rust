//! ECS-backed NPC actor storage.
//!
//! The world thread stores live NPCs in a small `bevy_ecs::World`. Each entity
//! owns a stable [`RuntimeEntityId`] component and an [`NpcActor`] component;
//! the id index lets the runtime commands address actors the same way the
//! previous map did. Behavior graphs can add components/systems without
//! changing the command surface.

use std::collections::BTreeMap;

use bevy_ecs::prelude::{Component, Entity, World};
use hytale_world::runtime::RuntimeEntityId;
use hytale_world::world::World as VoxelWorld;

use crate::blackboard::NpcBlackboard;
use crate::combat::{NpcActor, NpcTarget, NpcTickReport};
use crate::motion::MotionController;

/// Stable runtime id attached to one NPC entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct NpcEntityId(pub RuntimeEntityId);

/// NPC actor component.
#[derive(Debug, Component)]
pub struct NpcActorComponent(pub NpcActor);

/// NPC blackboard component.
#[derive(Debug, Component)]
pub struct NpcBlackboardComponent(pub NpcBlackboard);

/// NPC motion-controller component.
#[derive(Debug, Component)]
pub struct NpcMotionComponent(pub MotionController);

/// One NPC ECS store.
#[derive(Debug, Default)]
pub struct NpcEcs {
    world: World,
    ids: BTreeMap<RuntimeEntityId, Entity>,
}

impl NpcEcs {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert/replace one actor and return its stable id.
    pub fn insert(&mut self, id: RuntimeEntityId, actor: NpcActor) -> RuntimeEntityId {
        if let Some(entity) = self.ids.get(&id).copied() {
            if let Some(mut component) =
                self.world.entity_mut(entity).get_mut::<NpcActorComponent>()
            {
                component.0 = actor;
            }
            return id;
        }
        let blackboard = actor.blackboard.clone();
        let motion = actor.motion.clone();
        let entity = self
            .world
            .spawn((
                NpcEntityId(id),
                NpcActorComponent(actor),
                NpcBlackboardComponent(blackboard),
                NpcMotionComponent(motion),
            ))
            .id();
        self.ids.insert(id, entity);
        id
    }

    /// One actor.
    pub fn get(&self, id: &RuntimeEntityId) -> Option<&NpcActor> {
        let entity = *self.ids.get(id)?;
        self.world
            .get::<NpcActorComponent>(entity)
            .map(|component| &component.0)
    }

    /// One mutable actor.
    pub fn get_mut(&mut self, id: &RuntimeEntityId) -> Option<&mut NpcActor> {
        let entity = *self.ids.get(id)?;
        let component = self.world.get_mut::<NpcActorComponent>(entity)?;
        Some(&mut component.into_inner().0)
    }

    /// Remove one actor.
    pub fn remove(&mut self, id: &RuntimeEntityId) -> Option<NpcActor> {
        let entity = self.ids.remove(id)?;
        let actor = self
            .world
            .entity_mut(entity)
            .take::<NpcActorComponent>()
            .map(|component| component.0);
        self.world.despawn(entity);
        actor
    }

    /// Number of live actors.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether there is no live actor.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Run the actor/blackboard/motion schedule for one NPC.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_actor<F>(
        &mut self,
        id: RuntimeEntityId,
        dt: f64,
        world: &VoxelWorld,
        is_solid: F,
        targets: &[NpcTarget],
        random_unit: f64,
    ) -> Option<NpcTickReport>
    where
        F: Fn(i32) -> bool + Copy,
    {
        let entity = *self.ids.get(&id)?;
        let motion = self
            .world
            .get::<NpcMotionComponent>(entity)
            .map(|component| component.0.clone())?;
        let (report, blackboard, motion) = {
            let actor = self
                .world
                .get_mut::<NpcActorComponent>(entity)
                .map(|component| component.into_inner())?;
            let actor = &mut actor.0;
            // The actor's blackboard is authoritative: the world thread writes
            // environment state (water/air/light/placement flags) into it before
            // each tick. The ECS blackboard component is a mirror written back
            // below, not a source that may clobber those updates.
            actor.motion = motion;
            let mut report = actor.tick_behavior(dt, world, is_solid, targets, random_unit);
            if report.attack.is_none() {
                let (moved, moved_distance) = actor.advance_motion(dt, world, is_solid);
                report.moved = moved;
                report.moved_distance = moved_distance;
            }
            (report, actor.blackboard.clone(), actor.motion.clone())
        };
        if let Some(mut component) = self
            .world
            .entity_mut(entity)
            .get_mut::<NpcBlackboardComponent>()
        {
            component.0 = blackboard;
        }
        if let Some(mut component) = self
            .world
            .entity_mut(entity)
            .get_mut::<NpcMotionComponent>()
        {
            component.0 = motion;
        }
        Some(report)
    }

    /// Advance gameplay stat/effect timers for every live actor.
    pub fn tick_gameplay(&mut self, dt: f32) {
        let mut query = self.world.query::<&mut NpcActorComponent>();
        for mut component in query.iter_mut(&mut self.world) {
            component.0.tick_gameplay(dt);
        }
    }

    /// Stable ids in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &RuntimeEntityId> {
        self.ids.keys()
    }

    /// Every actor with its stable id in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = (&RuntimeEntityId, &NpcActor)> {
        self.ids.iter().filter_map(|(id, entity)| {
            self.world
                .get::<NpcActorComponent>(*entity)
                .map(|component| (id, &component.0))
        })
    }

    /// Stable ids as an owned vector (mutation-safe iteration).
    pub fn ids(&self) -> Vec<RuntimeEntityId> {
        self.ids.keys().copied().collect()
    }

    /// Underlying ECS world (component/system extension seam).
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable underlying ECS world.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl FromIterator<(RuntimeEntityId, NpcActor)> for NpcEcs {
    fn from_iter<T: IntoIterator<Item = (RuntimeEntityId, NpcActor)>>(iter: T) -> Self {
        let mut store = Self::new();
        for (id, actor) in iter {
            store.insert(id, actor);
        }
        store
    }
}

impl<'a> IntoIterator for &'a NpcEcs {
    type Item = (&'a RuntimeEntityId, &'a NpcActor);
    type IntoIter = Box<dyn Iterator<Item = (&'a RuntimeEntityId, &'a NpcActor)> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.ids.iter().filter_map(move |(id, entity)| {
            self.world
                .get::<NpcActorComponent>(*entity)
                .map(|component| (id, &component.0))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_store_has_no_ids() {
        let mut store = NpcEcs::new();
        assert!(store.is_empty());
        assert!(store.get(&999).is_none());
        assert_eq!(store.ids(), Vec::<RuntimeEntityId>::new());
        assert_eq!(store.len(), 0);
        store.tick_gameplay(0.05);
    }

    #[test]
    fn component_round_trip_uses_the_actor_component() {
        let mut world = World::new();
        let entity = world.spawn(NpcEntityId(7)).id();
        assert_eq!(world.get::<NpcEntityId>(entity), Some(&NpcEntityId(7)));
        world.despawn(entity);
        assert!(world.get_entity(entity).is_err());
    }
}
