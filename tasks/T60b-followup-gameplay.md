# T60b — T60 follow-up gameplay seams

**Status:** done (session 64)
**Package:** `hytale-builtin`, `hytale-entities`, `hytale-inventory`, `hytale-npc`, `hytale-world`, `hytale-server`
**Depends on:** T60
**Agent fit:** low

## Goal

Close the gameplay follow-ups recorded by T60: player health/combat and stat
replication, container `MoveItemStack` transactions, crafting bench windows and
recipe delivery, encounter spawn/presentation actions, the portal/instance
catalog/transport seam, and the NPC builder/ECS storage layer.

## Deliverables

- **Player health/combat**
  - `PlayerGameplay` owns a real `EntityGameplay` stat/effect map per connected
    player; NPC melee attacks against the high-bit player target id apply
    through the T53 resistance/health core.
  - `EntityStatsUpdate` deltas are queued in the world thread, drained by the
    session and broadcast to visible viewers; the stat map is keyed by stat
    index (Java `EntityStatMap.createInitUpdate`), with the entity id on the
    enclosing `EntityUpdate`. Runtime NPC spawn/update packets carry the
    per-entity `EntityStatsUpdate` snapshot.
  - Real `Server/Entity/Effects` assets can be applied to a player; their
    `DamageCalculator` DoT, durations, stat modifiers and `EntityEffectsUpdate`
    add/remove packets are ticked through the world thread.
  - Player death resets health/effects on respawn; the session broadcasts the
    resulting stat/effect updates.
  - Health persists in the player `EntityStats.Stats.Health` component and
    restores on reconnect.
- **Inventory/crafting**
  - `MoveItemStack` routing between player sections and the open block
    container, including same-container merge/swap and `UpdateWindow`/inventory
    resync packets.
  - Bench blocks open the Java `SetPage(Bench)`/`OpenWindow` pair with the real
    `BlockType.Bench` document and a mapped `WindowType`.
  - `RecipeCatalog` loads item-embedded and standalone salvage recipes;
    `UpdateRecipes`/`UpdateKnownRecipes` are sent on join.
  - `CraftRecipeAction` performs an atomic item/resource-type input
    check/consume, queues a bench timer job and grants the primary/output stacks
    when the timer expires; `CancelCraftingAction` refunds the queued inputs.
    Resource-type inputs resolve through the item `ResourceTypes` index.
- **Encounters**
  - `Spawn` actions execute once in the world thread and create live runtime
    NPCs.
  - Target-bound actions (`SetTargetNPCInvulnerable`, `ReleaseTarget`,
    `ApplyEntityEffect`, `ChangeTargetRole`) resolve their encounter target slot
    to the live NPC and update invulnerability, targeting, role/appearance or
    real effect state.
  - `SetEncounterWeather`/`ClearEncounterWeather` force/clear the world weather
    through the shared weather state; `SetEncounterBossBar`/`ClearEncounterBossBar`
    become `UpdateBossBar` packets bound to the resolved target entity,
    `SetEncounterAudioState` becomes a `SetAudioState` packet,
    `TriggerSpawners` forces matching/nearby spawn markers to pick and spawn
    their role, and `StartEncounterMusic`/`StopEncounterMusic` resolve the real
    `MusicContainer` index to `UpdateForcedMusic`.
  - `AdjustPortalTimer` updates the encounter's portal timer, exposed through
    `WorldThread::encounter_portal_remaining` and seeded with
    `set_encounter_portal_timer`.
  - `SpawnParticles`/`PlaySound` actions are translated into
    `SpawnParticleSystem`/`PlaySoundEvent3D` packets per session.
- **Portals/instances**
  - `hytale_world::instance::PortalCatalog` loads all `Server/PortalTypes`
    documents (target instance, spawn, time limit, entry count, ephemeral
    items).
  - `InstanceCatalog` discovers instance saves; `PortalTransportPlan` is
    exposed through `WorldThread::portal_plan`.
  - `UniverseRuntime::open_portal_instance` creates/selects and starts the
    target instance world; `transfer_live_player`/`transfer_player_document`
    persist the live session state, move the complete player document
    (including inventory/stats) to the plan spawn and remove the source copy.
  - `PortalRouter` + `switch_session_world` perform the live handoff: the
    source view is removed, the target view is registered, the player registry
    entry switches world/transform/view, and the session sends `ClearChunks`,
    `JoinWorld` and the target chunk snapshot on the existing chunk stream.
  - `portal_return_point` records the source world/transform and ephemeral
    items on transfer; `return_from_instance` restores the source transform,
    strips ephemeral items and moves the document back. Instance entry counts
    are tracked, `AllowedEntryCount` is enforced, and
    `close_instance_if_empty` tears down empty instances for
    `CloseWhenEmpty` portals.
- **NPC builder/ECS**
  - `hytale_npc::builder` parses/evaluates the role `Instructions` graph
    (State/Any/Not/And/Or/Timer/Flag/Player/Target/Mob/Damage/Self/Stat/
    ChargeState/Block/Beacon/Path/InWater/InAir/Time/Weather/IsBusy/Alarm/
    Count/HasInteracted/CanInteract/IsBackingAway/InflictedDamage/FlockLeader/
    Animation/GameFlag/Leash/Light/Switch/DroppedItem/BlockChange/CanPlaceBlock/
    Kill/Nav/Eval/ReadPosition/BlockType/EntityEvent/InteractionContext/
    FlockCombatDamage/SearchRay/AdjustPosition/ChargeEntityCollisions/
    ChargeBlockCollisions/Random sensors, ordered actions, `Once`
    state), is attached to
    `NpcActor`, and the `NpcBlackboard`/actor apply state/flag/timer/beacon/path/
    target/invulnerability/stat/regeneration/effect actions, including nested
    `Sequence`/`ForEach`/`Random` actions and `SetAlarm`/`TimerContinue`.
  - `hytale_npc::motion::MotionController` resolves `Walk`, `Fly` and `Dive`
    controller limits, including horizontal/vertical speed, acceleration,
    climb/sink, altitude and dive-depth bounds; the ECS tick executes
    horizontal waypoint motion for Walk and direct 3D motion for Fly/Dive,
    clamping Fly altitude to the ground band and Dive depth to the water
    surface band.
  - `hytale_npc::ecs::NpcEcs` stores actor, blackboard and motion-controller
    components in a `bevy_ecs::World`; the world thread runs
    `tick_actor` (blackboard → actor/motion schedule) per NPC and syncs the
    resulting transform/blackboard/motion components. Builder `PlaySound` and
    `SpawnParticles` actions surface as per-session world packets,
    `DropItem` spawns runtime item entities, `PickUpItem` consumes nearby item
    entities, `PlaceBlock` writes world blocks, and
    `Spawn`/`TriggerSpawnBeacon`/`TriggerSpawners` create or force NPCs.

## Acceptance

```sh
cargo test -p hytale-entities
cargo test -p hytale-inventory
cargo test -p hytale-npc
cargo test -p hytale-world
cargo test -p hytale-server --test builtin_runtime
cargo test -p hytale-server --test npc_runtime
cargo test -p hytale-server --test npc_encounter
cargo test -p hytale-server --test portal_transfer
cargo test -p hytale-server --test world_thread
./tools/test.sh fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- `apply_recipe_consumes_inputs_and_returns_outputs` covers the atomic
  inventory transaction; `resource_type_recipe_consumes_matching_items` and
  `asset_backed_resource_recipe_plans_from_the_real_catalog` cover resource
  inputs; `craft_queue_waits_for_the_bench_timer_and_cancels` covers timer/queue
  behavior.
- `resource_type_index_maps_real_items` and the real `Bench_Furnace` resource
  recipe test gate the item/`ResourceTypes` index.
- `player_effects` proves a real `Burn` effect applies DoT through the world
  thread, expires with `EntityEffectsUpdate`, and lethal damage followed by
  respawn restores max health and clears effects.
- `a_real_encounter_spawn_action_creates_a_runtime_npc` proves a real Coffer
  encounter's `Spawn` action creates a live NPC; packet-builder tests cover the
  particle/sound actions and target-bound actions resolve to live NPC state.
- `portal_transfer` proves `Goblin_Breach` creates/selects
  `Portals_Portals_Goblins`, moves the saved player to the plan spawn and
  removes the source document.
- `client-bot/tests/portal_session.rs` proves a connected client triggers the
  live handoff, the server session reports the instance world and the player
  document lands in the target world.
- `./tools/test.sh fast`: 1,512 passed, 18 skipped.
- `player_snapshot_round_trips_a_session_transform` proves health persistence;
  the block-container test proves live container read/write.
- `hytale-npc` builder tests parse the real Goblin Scrapper instruction tree and
  fire `Once` sensors once; ECS tests cover component round trips and the
  gameplay-tick query; `npc_ecs` runs a real Rat through the blackboard/motion
  ECS schedule.
- `./tools/test.sh slow`: 818 passed, 2 skipped.

## Deviations

- Resource-type item inputs, bench timers/queues and cancellation are
  supported; `known_for`/`for_bench` filter known recipes, and
  `CraftingKnowledge` gates knowledge/memories/bench tier/augment tags, is
  learned on a successful craft and is persisted in the player's `Crafting`
  component. The bench window payload carries `benchId`, `categories`,
  `tierLevel`, `augmentTags` and tier-filtered `recipeIds`.
- Portal transport produces a complete plan/catalog, moves the saved player
  document and performs the live session handoff (`ClearChunks`/`JoinWorld`,
  registry/view/chunk-stream switch); `portal_return_point`/
  `return_from_instance` restore the source transform and strip ephemeral
  items, instance entry counts enforce `AllowedEntryCount`, and
  `close_instance_if_empty` honors the portal's `CloseWhenEmpty` policy before
  tearing down an empty instance. Deeper instance removal/adventure policy
  remains follow-up work.
- The builder/blackboard/motion-controller state is ECS-scheduled through the
  world thread; Walk/Fly/Dive controllers execute their speed/acceleration and
  vertical limits. Damage/Self/Stat/ChargeState/Block/Beacon/Path conditions
  and Beacon/ResetPath actions are evaluated; rarer action-condition types
  remain pass-through/unknown. Target/role/invulnerability actions
  (`ReleaseTarget`, `SetMarkedTarget`, `SetInvulnerable`) update the
  blackboard/actor state and gate NPC damage; `SetStat`, `SetHealthRegen`,
  `ApplyEntityEffect`, `Attack`, `Despawn`/`Remove`/`Die`, `Appearance`,
  `Notify`, `OverrideAttitude`, `StorePosition`, `SetLeashPosition`,
  `PlayAnimation`, `DisplayName`, `SetInteractable`, `Crouch`,
  `SetBlockToPlace`, `FlockState`/`FlockTarget`, `MakePath`, `ResetInstructions`
  and `ResetPath` update the actor/blackboard state; `ResetBlockSensors`,
  `ResetSearchRays`, `SignalTaggedVolumes`, `ToggleStateEvaluator`,
  `OpenBarterShop`, `CompleteTask` and `StartObjective` now mutate their
  blackboard state. `AddToHostileTargetMemory`,
  `FlockBeacon`, `CombatAbility`, `LockOnInteractionTarget`, `PickUpItem`,
  `Inventory`, `Mount` populate the blackboard memory/beacon/ability/equipment
  slots; `PickUpItem` consumes nearby item entities into the blackboard
  inventory and `Inventory` equips it. `EntityHitInteraction`/
  `BlockHitInteraction` execute their real root interaction graph and apply
  block state/place/break actions. Role parameters resolve
  `{ "Compute": ... }` action fields for world actions (`Spawn`/`DropItem`).
  Nested instruction/sensor `Reference` components are resolved (including
  `Modify` parameter overrides) before parsing, so referenced detection/combat
  graphs contribute concrete sensors and actions.
- ECS systems currently call actor methods directly; a fully data-oriented
  Java-style schedule is follow-up work.
- Encounter target actions update invulnerability/role/target/effect state;
  spawner triggers, boss-bar/audio/weather/music presentation and portal-timer
  propagation are wired. `SpawnInteraction` creates a server-only host entity
  with a TTL and executes its interaction graph for both encounter and NPC
  builder actions; every action/sensor type observed in the base roles is now
  handled, with only unrecognized future node types preserved inert.

## Verified (session 64)

- `cargo test -p hytale-world` and `cargo test -p hytale-npc` pass.
- Targeted server tests pass:
  - `player_snapshot_round_trips_a_session_transform` (health persistence).
  - `block_containers_open_with_their_block_capacity` (container transaction).
  - `npcs_attack_a_target_through_the_runtime_tick` (player damage/stat queue).
  - `a_real_encounter_spawn_action_creates_a_runtime_npc`.
  - `encounter_target_actions_update_a_live_npc` (target-bound actions).
  - `npc_spawn_updates_carry_entity_stats_keyed_by_stat_index`,
    `stat_packet_targets_the_entity_and_keys_stats_by_index` and
    `npc_spawn_updates_carry_active_entity_effects` (per-entity stats/effects).
  - `invulnerable_actors_ignore_damage` (NPC invulnerability gate),
    `set_stat_actions_change_the_actor_stat_map` and
    `set_health_regen_action_gates_natural_regeneration` (builder actions).
  - `returning_from_an_instance_restores_the_source_and_strips_ephemeral_items`
    and `an_instance_entry_limit_is_enforced` (portal return/entry limits).
  - `a_real_portal_drain_encounter_updates_the_portal_timer` and
    `music_containers_resolve_real_playlists_and_tracks` (encounter/audio).
  - `real_water_time_weather_alarm_and_count_sensors_parse_and_evaluate` and
    `real_interaction_animation_flock_and_game_flag_sensors_parse` (sensors).
  - `a_real_npc_builder_place_block_action_changes_the_world`,
    `a_real_npc_builder_spawn_action_creates_npcs` and
    `a_nearby_item_entity_can_be_picked_up` (builder world actions, item
    pickup and the ECS blackboard merge fix).
  - `damaging_an_npc_updates_its_entity_stat_snapshot` (NPC damage → runtime
    entity stat snapshot/update).
  - `a_real_interaction_root_applies_block_actions` (real interaction graph
    execution + block action application).
  - `returning_from_an_instance_restores_the_source_and_strips_ephemeral_items`
    also asserts the `CloseWhenEmpty=false` gate and an explicit teardown.
  - `blackboard::tests::presentation_flock_path_and_place_actions_apply`,
    `blackboard::tests::nested_alarm_and_timer_continue_actions_apply`,
    `blackboard::tests::nested_timeout_action_flattens_and_applies` and
    `blackboard::tests::reset_signal_task_and_shop_actions_apply`
    (ordered/nested builder actions and previously-inert actions).
  - `player_data::tests::crafting_knowledge_round_trips_and_preserves_components`
    and `player_snapshot_round_trips_a_session_transform` (crafting progression
    persistence across a world restart).
  - `crafting::tests::known_recipes_gate_on_learned_ids_and_memories_level`,
    `crafting::tests::bench_filter_selects_recipes_for_the_window_type`,
    `crafting::tests::crafting_knowledge_gates_knowledge_memories_tiers_and_augments`
    and `session::tests::window_types_map_to_recipe_bench_types`
    (knowledge/memories, tier/augment and bench-window progression).
  - `session::tests::bench_window_data_applies_tier_and_category_contract`
    (bench id/categories/tier/augment-tag/recipe-id window payload),
    `motion::tests::turn_speed_clamps_yaw_changes` (non-walk turn limit),
    `npc_ecs::real_fly_and_dive_roles_clamp_altitude_and_depth` (real Fly/Dive
    altitude/dive band limits) and
    `builder::tests::referenced_sensor_components_expand_into_concrete_sensors`
    (nested instruction/sensor `Reference` resolution).
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` are clean.

## Follow-ups

- Crafting UI presentation beyond the landed bench payload contract.
- Death-screen/configured-spawn respawn policy and effect conditions.
- Unrecognized future builder node types are preserved inert (none are present
  in the shipped base roles); non-locomotion motion-controller extras remain.
- Deeper instance removal/adventure policy (return, entry limits,
  `CloseWhenEmpty` and interaction hosts are landed).
