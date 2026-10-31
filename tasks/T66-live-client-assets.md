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
    `ParticleSpawner`, `MusicTransition`;
  - the two particle packets are byte-identical to the Java goldens
    (`ParticleSystem` 791 systems, `ParticleSpawner` 2439 spawners), covering
    particles, animation frames, attractors, collision, UV motion and the
    spawner-group tree;
  - `MusicTransition` carries the preloaded `EMPTY` rule plus the five authored
    transitions, expands `From`/`To` containers to indices, and encodes sync
    points, fades and computed specificity.
- Still pending real-client verification: release client
  `WorldLoadFinished` null (16:22 run). The code-side gaps still sent as
  placeholders are:
  - `Interaction`, `RootInteraction`, `UnarmedInteraction` (full action graph);
  - `MusicContainer`, `ConnectedBlockRuleSet`, `Stinger`, `Recipes`
    (polymorphic/asset-specific trees).
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
- `crates/hytale-assets` (all tests) passes with `HYTALE_TEST_ASSETS=1`.

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
