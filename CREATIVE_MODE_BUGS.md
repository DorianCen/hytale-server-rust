# Creative-mode bugs — release-client session 2026-11-15

Observed with **Hytale release client 0.7.7**, server HEAD `ac0068d`
(`target/debug/hytale-server serve`, authenticated, world `T76`, default
provider, world config `GameMode: Creative`).

These are recorded for a follow-up fix; none are fixed by this document.

## 1. Creative inventory is empty

- **Symptom:** the player cannot give themselves blocks in creative; no creative
  item list is available in the inventory.
- **Likely area:** the session sends `SetGameMode { Adventure }` unconditionally
  during join (`crates/hytale-server/src/session.rs:1805`) and initializes the
  live game loop with `GameMode::Adventure`
  (`crates/hytale-server/src/session.rs:3239`). The world config's
  `GameMode: Creative` is never read into the join packet or live loop.
- **Next check:** compare the join packet sequence with Java
  `SetGameMode`/creative inventory initialization; verify whether the client
  expects a creative-mode item list or derives it from the item asset packets.

## 2. Creative break/place behaves like Adventure and desyncs

- **Symptom:** breaking a block in creative drops the item as in survival; a
  placed block exists on the server but not on the client. The client then walks
  into it while the server teleports the player to the block edge.
- **Evidence:** the placement trace contains records such as
  `{"block_key":"Soil_Dirt","client_rotation":false,...}` — the client did not
  send a `block_rotation`, and the server took the authored-placement path.
- **Likely area:** same hardcoded `GameMode::Adventure`
  (`session.rs:1805`, `session.rs:3239`). `handle_block_interaction` gates
  `quick_replace`/`quick_retype`/`no_physics` and held-item consumption on
  `game_mode == GameMode::Creative` (`session.rs:5961-6002`), so the server is
  in survival semantics while the client believes it is in creative. The
  missing client render is likely the client's local prediction rejecting the
  server placement after the mode mismatch.
- **Next check:** send the world's real game mode at join and on every
  `SetGameMode`; verify `ServerSetBlock` broadcast and client prediction with a
  release-client creative placement.

## 3. No water in generated terrain

- **Symptom:** the generated world contains no visible water.
- **Likely area:** world generation fluid sections / configured fluid ticker
  availability, or the selected default world seed region. The T76 observation
  is from a default-provider world created with `--seed 7`.
- **Next check:** probe the same seed/chunks with the Java `ChunkDumper` and
  compare fluid IDs/levels against the Rust `WorldGenerator`; check
  `World.json` fluid/ticker configuration and whether the environment/zone
  chosen by this world has water.

## 4. Creative flight is unavailable

- **Symptom:** the player cannot fly in creative.
- **Likely area:** same join `GameMode::Adventure` hardcode; the client enables
  creative flight only when it receives `SetGameMode { Creative }` and the
  matching movement/permission state.
- **Next check:** send the configured game mode at join; verify the client's
  creative movement mode and server movement validation reject/allow flight.

## Suggested fix order

1. Plumb `WorldConfig::game_mode` into join `SetGameMode` and the live
   `game_mode` state.
2. Re-test creative inventory, break drops, placement prediction and flight.
3. Separately investigate fluid generation with a Java/Rust chunk comparison.
4. Add a release-client creative regression (join + break + place + fly) and a
   fast server test for join `SetGameMode`.

## Trace artifacts

- Placement trace: `target/trace/placements.jsonl`
- Server packet trace: `target/trace/packets.jsonl`
- Client log: latest
  `~/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/*_client.log`
