# T66 — Live-client config-asset completeness

**Status:** in progress (session 70; real-client verification pending)
**Package:** `hytale-assets`, `hytale-server`, `client-bot`, docs
**Depends on:** T35, T39, T60b, T65
**Agent fit:** medium (Java generator reading + golden comparison; iterative
real-client verification is user-run)

## Goal

Make an unmodified release Hytale client pass `WorldLoadFinished` and reach
`Playing`. T65 made authentication and setup work; the client now fails inside
the `WorldLoadFinished` handler with a `NullReferenceException` because some
config-asset stores are empty or contain null fields where Java always sends
data.

This is an asset-packet completeness task, not an auth task.

## Known failures and gaps

- Fixed in the T65 follow-up: all required asset types are now initialized,
  empty collections are non-null, `AudioCategory`/`AudioState` carry real data,
  and block/item `tagIndexes` and block `interactions` are materialized.
- Landed in session 70:
  - items: `tool`, `weapon`, `armor`, `gliderConfig`, `trails`,
    `blockSelectorTool`, `builderToolData`, `itemAppearanceConditions`,
    `hudUI` and `ability` now carry parsed data (counts are asserted against
    the Java golden: tool 37, weapon 275, armor 122, glider 2,
    blockSelectorTool 2, builderTool 39, trails 5, appearance 117, hudUI 26,
    ability 24 in the current pack);
  - blocks: `states`, `bench`, `placementSettings` and `conditionalSounds` are
    populated; `conditionalSounds` materializes an entry even when AmbienceFX
    is still unresolved;
  - real stores replacing placeholders: `SoundSet`, `ResourceType`,
    `FieldcraftCategory`, `ReverbEffect`, `EqualizerEffect`, `BeamConfig`,
    `HitboxCollisionConfig`, `RepulsionConfig`, `AbilityKeyword`, `Trail`,
    `ModelVFX`, `EntityUIComponent`, `BlockSet`, `ParticleSystem`,
    `ParticleSpawner`, `MusicTransition`, `ConnectedBlockRuleSet`, `AmbienceFX`,
    `MusicContainer`, `Stinger`, `Recipes`, `Interaction`, `RootInteraction`,
    `UnarmedInteraction`;
  - the two particle packets are byte-identical to the Java goldens
    (`ParticleSystem` 791 systems, `ParticleSpawner` 2439 spawners), covering
    particles, animation frames, attractors, collision, UV motion and the
    spawner-group tree;
  - `MusicTransition` carries the preloaded `EMPTY` rule plus the five authored
    transitions, expands `From`/`To` containers to indices, and encodes sync
    points, fades and computed specificity;
  - `ConnectedBlockRuleSet` carries all three rule sets, including shapes
    inherited from `SimplePipe`, templated shape metadata, rotation groups and
    the face-tag/shape/And/Or/Not rule tree;
  - `AmbienceFX` carries the `Empty` preload plus conditions, sounds, ambient
    beds, sound effects, blocked indices and state writes for all 232 assets;
    both named and inline `MusicContainer` references resolve to real indices;
  - `MusicContainer` carries the 275 folder assets plus 399 inline child
    containers extracted from `Children`, `Layers`, `AmbienceFX` and `Stinger`
    documents, including random/sequence/horizontal/segment variants;
  - `Stinger` carries the `EMPTY` preload and both authored stingers with their
    container indices, categories, exclusion groups, volumes and ducking
    rules;
  - `Recipes` carries all 410 standalone crafting recipes with inputs,
    outputs, primary outputs, bench requirements, knowledge gates, time and
    memories level;
  - `Interaction`, `RootInteraction` and `UnarmedInteraction` stream the full
    ID graph: 2805 interaction nodes, 732 roots (including the `*Default_Swap`
    preload), inline `Next`/`Failed` children, root interaction lists and the
    `SwapFrom` default. Interaction bodies are type-specific for the ten most
    common nodes (`Simple`, `DamageEntity`, `Serial`, `Selector`,
    `StatsCondition`, `Charging`, `Wielding`, `Parallel`, `Condition`,
    `Projectile`, `EffectCondition`, `BlockCondition`, `Repeat`, `RemoveEntity`,
    `PlaceBlock`, `TriggerCooldown`, `DurabilityCondition`, `UseBlock`,
    `ChangeStat`, `ChangeState`, `ApplyEffect`, `ChangeBlock`,
    `GroundedCondition`, `MemoriesCondition`, `ClearEntityEffect`, `BreakBlock`,
    `PickBlock`, `FirstClick`, `ModifyInventory`, `ApplyForce`, `UseEntity`,
    `MovementCondition`, `DragPlaceBlock`, `ExtrudePlaceBlock`,
    `SurfaceDrawPlaceBlock`, `Replace`, `ChangeActiveSlot`); the remaining
    server-only long-tail node types map to the base form, matching Java's
    `SimpleInstantInteraction` packets.
- Still pending real-client verification: release client
  `WorldLoadFinished` null (16:22 run). The code-side gaps still sent as
  placeholders are:
  - type-specific bodies for the remaining interaction node types (the ten
    most common are encoded; the long tail still uses the base form).

- `ModelAsset` has no packet generator; verify whether the missing
  `Debug_Model` model is a common-asset issue before changing packets.

## Deliverables

- Extend `AssetRegistry::config_assets` with real encoders, in dependency order,
  for every store Java sends with data. Use the Java `*PacketGenerator` /
  `toPacket()` classes and the committed goldens as the contract.
- Fill the omitted item fields (`tool`, `weapon`, `armor`, `gliderConfig`,
  `trails`, `blockSelectorTool`, `builderToolData`, `itemAppearanceConditions`,
  `hudUI`, `ability`).
- Fill the omitted block trees (`states`, `bench`, `connectedBlockRuleSet`,
  `placementSettings`, `conditionalSounds`), remapping Rust-owned ids through
  the registry/tag maps.
- Keep every collection Java materializes non-null; add presence assertions
  rather than relying on one-off client retries.
- Add a golden-presence gate per store: count and field-presence parity against
  `testdata/golden/server/assets/index.json` + `.bin`; byte parity where the
  store is not Rust-id-dependent.
- Document the user-run real-client smoke procedure in
  `testdata/golden/server/trace/README.md` or a dedicated runbook.

## Acceptance

- A release client gets past `WorldLoadFinished`, reaches `Playing`, and can
  move/edit in the test world.
- No required store that Java initializes with data is sent empty, and no
  always-non-null collection is sent as null.
- The golden-presence suite fails on any regression of the above.
- `./tools/test.sh fast`, asset tests, clippy and fmt are clean.

## Session 70 regression coverage

- `crates/hytale-assets/tests/block_item_packets.rs` now presence-checks every
  nested item tree against `Item.bin` and asserts the current pack counts.
- The same test checks `states`, `bench`, `placementSettings` and
  `conditionalSounds` presence against `BlockType.bin`.
- `simple_config_stores_are_no_longer_placeholders` decodes the real
  sound-set/resource/fieldcraft/reverb/equalizer/beam/hitbox/repulsion/
  ability-keyword/trail/model-VFX packets and compares their map lengths to
  the registry.
- `particle_packets_match_the_java_golden` byte-compares both particle
  packets with `testdata/golden/server/assets/*.bin`; `particle_stores_are_
  materialized` checks the map counts and that every system has spawners and
  every spawner has a particle definition.
- `music_transitions_are_materialized` checks the `EMPTY` preload, the five
  authored rules, container-index expansion, sync/fade fields and specificity.
- `connected_block_rule_sets_are_materialized` checks the three rule sets,
  inherited shapes, templated output and expanded rotation groups.
- `ambience_fx_is_materialized` checks the `Empty` preload, condition
  indices/patterns, sounds, ambient beds, blocked indices and state writes.
- `music_containers_and_stingers_are_materialized` checks the 674-container
  map, all five variants, inline child/layer resolution, sequence/horizontal
  children, stinger container indices and inline `AmbienceFX` music.
- `recipes_are_materialized` checks all 410 standalone recipes and the
  salvage/flower recipe fields end to end.
- `interaction_graph_is_materialized` checks the 2805-node interaction map,
  732 roots, no dangling `next`/`failed`/root indices, the default unarmed
  root and the chain-hook root. It also counts and validates the typed bodies,
  including `Serial`/`Parallel` children, `Projectile.Config`, `Select` fail
  mode, `Repeat`, `TriggerCooldown`, `PlaceBlock`, `ModifyInventory`,
  `ApplyForce` and the condition/change nodes.
- `crates/hytale-assets` (all tests) passes with `HYTALE_TEST_ASSETS=1`.

## 2026-10-31 release-client crash

The authenticated release client reached `SettingUp -> Playing`, received every
config store, and then crashed while preparing the world. The log's only
asset warning was:

```text
Missing model asset:  for block Debug_Model
```

`Debug_Model` is a Java `preLoadAssets` block. The Rust preload omitted its
`CustomModel`/`CustomModelTexture` fields, so the client received an empty model
path and crashed after `PrepareWorldMap`.

Fixed in this slice:

- `Debug_Model` now preloads
  `Blocks/_Debug/Model.blockymodel` and `Blocks/_Debug/Texture.png`.
- `Debug_Cube` now carries the six `BlockTextures/_Debug/*.png` faces.
- `UpdateTranslations` now sends the real `Server/Languages/<language>` catalog
  instead of an empty map, using Java's `fallback.lang` language fallback.
- Regressions: `debug_block_preloads_carry_models_and_textures` and
  `translations_carry_server_catalog_and_language_fallback`.

### Second run: interaction id `NOT_FOUND` self-loop

The client was rerun with the Debug_Model fix. The model warning disappeared but
the client still died at the same point. It is a native SIGSEGV, not a managed
exception: `coredumpctl` shows the main thread's stack repeating
`HytaleClient+0xa561fd` hundreds of times (stack exhaustion), and walking out of
the recursion lands in the interaction resolver (`HytaleClient+0xa55e60`), whose
strings include "Interaction index {0} is out of range or references an
unpopulated slot; skipping malformed reference".

Cause: Java sends `AssetMapWithIndexes.NOT_FOUND` (`i32::MIN`) for an absent
interaction reference, but the Rust encoders used `0`. Interaction id `0` is the
preloaded `*Change_Active_Slot`, which has no `Next`/`Failed`, so the payload
made it point at itself and the client followed the chain forever.

Fixed in this slice:

- `INTERACTION_NOT_FOUND` (`i32::MIN`) for every interaction reference:
  `Next`/`Failed`/`Blocked`, `Click`/`Held`, all eight `MovementCondition`
  directions, fork/ground/collision next, serial/parallel arrays, root children
  and the unarmed `SwapFrom` binding.
- Inline `Interactions` array elements are registered as contained interaction
  assets at any depth, matching Java's `CHILD_ASSET_CODEC_ARRAY`. Before this,
  most inline children resolved to `0`.
- `DamageEntity.Blocked` and the `MovementCondition` directions now come from
  JSON instead of hardcoded `0`.
- Regression: `crates/hytale-assets/tests/interaction_packets.rs`
  (`absent_interaction_references_use_not_found`).

After the containment fix only 2 references stay unresolved (root interaction
names used inside an interaction array); they are sent as `NOT_FOUND` and the
client skips them. A key-level walk of all 5,758 interaction nodes found no
cycles.

## Client/server version comparison

The old and new Java checkouts share the same protocol (CRC `3908406161`,
build `258`) and the Rust asset pack matches the old checkout byte-for-byte.
The 0.7.5 delta is cosmetic/Halloween assets plus server-side security and
packet-handler validation, not a wire-protocol change. See
[`JAVA_VERSION_DIFF.md`](../JAVA_VERSION_DIFF.md).

## User-run real-client smoke

The client is never launched by the agent. After deploying this revision:

1. Build the server (`cargo build --release -p hytale-server`).
2. Start it with `--auth-mode authenticated` and the same asset pack.
3. Connect the release client and let it reach `SettingUp`/`WorldLoadFinished`.
4. If it still fails, capture the client log around `WorldLoadFinished` and the
   server trace (`serve --trace <path>`); the next asset gap is then data-driven
   from the named access rather than guessed.
5. On success, record `Playing`, spawn, movement and a block edit in the trace
   fixture runbook.

## Notes

- Do not automate or run the Hytale client from the agent. The user runs the
  client and supplies logs; the agent only changes the server and reads logs.
- Prefer data-driven comparison against the committed goldens over guessing at
  the null from stack offsets.
- If a store cannot be encoded yet, document the exact divergence and the
  client symptom rather than silently sending a placeholder.

### Third run: `Weather.Fog` NullReferenceException

After the interaction sentinel fix the client no longer crashes. It runs every
`Prepare*` step and then throws a managed `NullReferenceException` while
handling `WorldLoadFinished` (`HytaleClient+0x9db009`):

```text
movss 0x8(%rdi)
```

`rdi` is element field `+0xa0`; the element is the client's weather-store entry
and the object is a 24-byte `NearFar` (`float Near`, `float Far`). Java's
`Weather.toPacket()` always sets `packet.fog = new NearFar(fogDistance[0],
fogDistance[1])` with `DEFAULT_FOG_DISTANCE = {-96, 1024}`. The Rust encoder
used `Weather::default()` for everything except id/tags/stars/moons, so every
weather had `fog: null`, and the client's clone dereferences it without a null
check.

Fixed in this slice:

- `Weather.Fog` is populated from `FogDistance`, defaulting to `{-96, 1024}`.
- `Weather.ScreenEffect` is forwarded too.
- Regression: `block_item_packets::weather_fog_is_always_sent` (also checks the
  inherited `Creative_Hub` value and a non-default `Forgotten_Temple` range).

Remaining known issue from the same run: the client logs a non-fatal
`Failed to prepare the item icon atlas; item icons are unavailable` warning.
Investigate after the world join succeeds.

### Fourth run: chunk loading stalled on absent sections

After the Weather fix the client reached `JoinWorld`, `OnSetupComplete`,
`PrepareJoiningWorld` and `StartJoiningWorld`, then received no further packets
and timed out. The server had streamed 81 columns (`SetColumn` + 810
`SetChunk` + 810 `SetChunkEnvironments`).

Cause: sections with no stored data were sent as `SetChunk` with `data = null`.
Java's `ChunkTracker` marks those with `SetAbsentChunks`; its own comment says
that otherwise "the client sees a never-arriving neighbour and stalls meshing of
the section adjacent to a column's terrain top / bottom". The client never
finished chunk tracking and never sent `ClientReady(readyForGameplay)`.

Fixed in this slice:

- `ChunkPacketBatch::initial_for_client` / `full_for_client` emit
  `SetAbsentChunks` for data-less sections; the raw `initial`/`full` batches stay
  byte-identical to the Java dumper fixtures.
- `session.rs` now sends `UpdateSunSettings` and `UpdatePostFxSettings` from the
  world config, and `UpdateWorldMapSettings` before the chunk snapshot, matching
  Java's `onFinishPlayerJoining`.
- Regressions: `chunk::stream::tests::client_batches_mark_absent_sections`; the
  chunk golden test still passes.

Known follow-up: the saved player transform is `(0, 0, 0)`. A real surface spawn
(Java spawn provider) is still needed so the player does not start in the ground.
