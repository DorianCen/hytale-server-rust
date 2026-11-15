# Creative-mode bugs — release-client session 2026-11-15

Observed with **Hytale release client 0.7.7**, server HEAD `ac0068d`
(`target/debug/hytale-server serve`, authenticated, world `T76`, default
provider, world config `GameMode: Creative`).

All four findings are fixed in the follow-up commit; the sections below keep the
original symptom/evidence and record the exact fix and regression test.

## 1. Creative inventory is empty

- **Symptom:** the player cannot give themselves blocks in creative; no creative
  item list is available in the inventory.
- **Cause:** the session sent `SetGameMode { Adventure }` unconditionally during
  join (`crates/hytale-server/src/session.rs`) and initialized the live game
  loop with `GameMode::Adventure`. The world config's `GameMode: Creative` was
  never read. The server also ignored the client's `SetCreativeItem`,
  `DropCreativeItem` and `SmartGiveCreativeItem` packets, so the creative item
  grid could not move an item into the inventory even when the mode was right.
- **Fix:** `WorldStartReport` now carries the mode resolved from
  `WorldConfig.game_mode`; the join `SetGameMode` and live loop use it. The game
  loop handles the three creative inventory packets with Java's
  `InventoryPacketHandler` semantics (max-stack clamp, slot merge/replace,
  hotbar/storage/backpack pickup, creative-only drop).
- **Regression tests:**
  `session::tests::creative_set_item_merges_a_slot_and_clamps_to_the_max_stack`,
  `session::tests::creative_give_packets_populate_the_inventory`, and the
  loopback `/gamemode creative` assertion in
  `tools/client-bot/tests/commands.rs`.

## 2. Creative break/place behaves like Adventure and desyncs

- **Symptom:** breaking a block in creative drops the item as in survival; a
  placed block exists on the server but not on the client. The client then walks
  into it while the server teleports the player to the block edge.
- **Evidence:** the placement trace contains records such as
  `{"block_key":"Soil_Dirt","client_rotation":false,...}` — the client did not
  send a `block_rotation`, and the server took the authored-placement path.
- **Cause:** the hardcoded `GameMode::Adventure` made the client and server
  disagree about the active mode. `handle_block_interaction` gates
  `quick_replace`/`quick_retype`/`no_physics`, held-item consumption and the
  break path on `game_mode == GameMode::Creative`, so the server behaved like
  survival while the client believed it was in creative.
- **Fix:** the join/live mode now comes from the world config. Creative breaks
  always clear the block and skip drop spawning (`BlockHarvestUtils` marks
  creative breaks `NO_DROP_ITEMS`); placement flags and held-item preservation
  now use the same creative state as the client.
- **Regression tests:** the world-config join test
  `creative_world_config_reaches_the_join_report_and_flight` plus the existing
  placement/rotation suites exercise the creative flags. A release-client
  re-run should confirm the prediction/`ServerSetBlock` reconciliation.
- **Follow-up if the re-run still desyncs:** capture the packet trace and
  compare the client's local prediction with the broadcast `ServerSetBlock`; the
  trace seam is already in place.

## 3. No water in generated terrain

- **Symptom:** the generated world contains no visible water.
- **Cause:** the generator did produce water (`worldgen-cli dump-chunk --seed 7
  --chunk 0,0` shows fluid ids 2/8). The client snapshot used
  `ChunkPacketBatch::initial_for_client`, which only sent `SetColumn`,
  `SetChunk` and `SetChunkEnvironments`. `SetFluids` was update-only, so a
  joining client never learned about water.
- **Fix:** `initial_for_client` now includes a `SetFluids` packet for every
  non-empty fluid section. The Java load-query subset (`initial`) and the golden
  manifest are unchanged; `full` keeps the same packet order.
- **Regression tests:**
  `chunk::stream::tests::client_initial_batch_carries_generated_fluids`, and the
  ignored loopback chunk test now asserts the stream carries `SetFluids`.
- **Follow-up:** if a specific world still has no visible water, compare its
  generated fluid ids with the registry mapping (`worldgen-cli dump-chunk`).

## 4. Creative flight is unavailable

- **Symptom:** the player cannot fly in creative.
- **Cause:** the join hardcode again; `MovementSettings.fly` also stayed
  `Disabled` (the world thread resolved it for Adventure).
- **Fix:** the world thread resolves `fly` like Java
  `MovementManager.deriveFly` (explicit config override wins, otherwise
  Creative = `Allowed`, Adventure = `Disabled`). `/gamemode` re-sends
  `UpdateMovementSettings` with the derived fly mode, matching
  `Player.setGameModeInternal`/`resetFly`.
- **Regression tests:**
  `creative_world_config_reaches_the_join_report_and_flight` and the
  `/gamemode creative` fly assertion in `tools/client-bot/tests/commands.rs`.

## Suggested re-test order

1. Join the release client with a `GameMode: Creative` world (or run
   `/gamemode creative`) and confirm the creative item grid and flight.
2. Break and place blocks; confirm no survival drops and that the client renders
   the placed block.
3. Look for water near generated lakes/oceans and confirm `SetFluids` arrives.
4. Re-run the T76 `RandomRotation` trace runbook in
   [`tasks/T76-placement-followups.md`](tasks/T76-placement-followups.md).

## Trace artifacts

- Placement trace: `target/trace/placements.jsonl`
- Server packet trace: `target/trace/packets.jsonl`
- Client log: latest
  `~/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/*_client.log`
