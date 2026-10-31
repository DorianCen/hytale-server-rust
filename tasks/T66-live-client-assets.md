# T66 — Live-client config-asset completeness

**Status:** planned (paused after T65)
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
- Still failing: release client `WorldLoadFinished` null (16:22 run).
- Golden presence gaps measured against `testdata/golden/server/assets`:
  - items: `tool` (34), `weapon` (275), `armor` (120), `gliderConfig` (2),
    `trails` (5), `blockSelectorTool` (2), `builderToolData` (9),
    `itemAppearanceConditions` (98), `hudUI` (8), `ability` (24);
  - blocks: `states` (4017), `connectedBlockRuleSet` (2171),
    `placementSettings` (1081), `bench` (56), `conditionalSounds` (3);
  - required stores currently empty/placeholder: `ParticleSystem`,
    `ParticleSpawner`, `AmbienceFX`, `BlockSet`, `Interaction`,
    `RootInteraction`, `UnarmedInteraction`, `SoundSet`, `MusicContainer`,
    `ReverbEffect`, `EqualizerEffect`, `ConnectedBlockRuleSet`,
    `MusicTransition`, `Stinger`, `BeamConfig`, `AbilityKeyword`,
    `ResourceType`, `FieldcraftCategory`, `Trail`, `ModelVFX`,
    `EntityUIComponent`, `HitboxCollisionConfig`, `RepulsionConfig`, `Recipes`.
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

## Notes

- Do not automate or run the Hytale client from the agent. The user runs the
  client and supplies logs; the agent only changes the server and reads logs.
- Prefer data-driven comparison against the committed goldens over guessing at
  the null from stack offsets.
- If a store cannot be encoded yet, document the exact divergence and the
  client symptom rather than silently sending a placeholder.
