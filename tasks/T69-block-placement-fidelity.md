# T69 — Block placement fidelity

**Status:** done (session 96; rotation, setblock, neighbour simulation and persistence)
**Package:** `hytale-server`, `hytale-commands`, `hytale-world`, `tools/client-bot`
**Depends on:** T46, T50, T52, T56, T66
**Agent fit:** low/medium

## Goal

Close the placement-fidelity gaps left after the real-client block loop (T66):
preserve the client's block rotation, make placement drive neighbour/support
simulation, expose an authoritative `/setblock` admin path, and prove that the
stored block id/rotation survives save/reload.

The wire protocol and asset schemas do not change. Placement continues to use
the client's target cell/face; the server now keeps the rotation the client
predicted instead of resetting it to zero.

## Landed scope

1. **Rotation preservation**
   - `SyncInteractionChain` placement data carries
     `InteractionSyncData.block_rotation` (yaw/pitch/roll). Encode it as the
     Java `RotationTuple` index (`roll*16 + pitch*4 + yaw`) and set it through
     `World::set_block_full`.
   - The authoritative `ServerSetBlock` broadcast and the interaction-chain ack
     carry the same rotation/placed block.
   - `/setblock` accepts an explicit rotation.
2. **Authoritative `/setblock`**
   - `setblock <x> <y> <z> <block> [rotation]`, where `<block>` is a numeric
     block id or a block type key and rotation is `0/90/180/270` or a raw
     `0..63` tuple index.
   - Replaces the target even when occupied, broadcasts the edit, and is gated by
     `server.commands.setblock`.
3. **Neighbour and physics updates**
   - `set_block_full` already records the changed cell, so the block simulation
     schedules the 3×3×3 neighbourhood on the next 50 ms tick (support loss,
     fluid, falling blocks). Regression tests cover support removal and a
     falling block caused by a placed/removed neighbour.
4. **Persistence**
   - Block id and rotation go through the existing chunk section codec; a
     save/reopen test proves both survive.
5. **Tests**
   - Unit tests for rotation parsing/packing and the `/setblock` command.
   - World-thread tests for rotated placement, neighbour support/falling, and
     save/reload.
   - The ignored client-bot placement scenario still passes.

## Deliberate scope limit

`ConnectedBlockRuleSet` visual/state-variant recomputation is **not** in this
slice. The server already streams the rule sets to the client (T66) and stores
whatever state variant the client resolves; server-side mutation of connected
block ids belongs with the interaction long tail (memories/charging/connect
updates). This task covers the authoritative placement cell, rotation,
neighbour simulation, admin path and persistence.

## Read first

- `crates/hytale-server/src/session.rs`: `block_interaction_from_chain`,
  `parse_block_edit`, `handle_block_interaction`, `PlacedBlock`,
  `patched_interaction_data`.
- `crates/hytale-server/src/world_thread.rs`: `Command::PlaceBlock`,
  `Command::SetBlock`, `place_block`, `set_block`, `start`/`run`.
- `crates/hytale-world/src/world.rs`: `set_block`, `set_block_full`,
  `get_rotation`.
- `crates/hytale-world/src/simulation.rs`: changed-block scheduling and support.
- `crates/hytale-commands/src/builtins.rs` and
  `crates/hytale-server/src/session.rs` command effects.

## Acceptance

```sh
cargo test -p hytale-server --lib
cargo test -p hytale-commands
cargo test -p hytale-world
cargo test -p client-bot --test commands -- --ignored
./tools/test.sh fast
cargo clippy -p hytale-server -p hytale-commands -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A client placement with yaw/pitch/roll stores and broadcasts that rotation.
- `/setblock` places a block by id/key with an explicit rotation and reports
  unknown keys/out-of-range rotations.
- Removing a supporting neighbour makes an unsupported block fall/break on the
  next simulation tick.
- Save/reopen restores the block id and rotation.
- Existing join/edit/chunk loops stay green.

## Deviations

- The client's `blockRotation` is authoritative when present; without it the
  server stores rotation `0`. Java's `RotationYawPlacementOffset`,
  `RandomRotation` and client prediction simulation fallbacks are not
  recomputed server-side.
- `/setblock` accepts a block id or block type key plus an explicit rotation;
  there is no separate `Block[State]` syntax (state variants are their own block
  keys and already resolve).
- `ConnectedBlockRuleSet` server-side state-variant mutation is deliberately
  out of scope (see above); the client-visible rule sets remain T66's packets.
- Block-entity metadata beyond the existing container documents and state
  variants is unchanged; no new block component codec landed.
- The legacy `client-bot` block-interaction fixture was stale after the spawn
  resolver/player-data changes: it now keeps `spawn_search = false`, targets the
  loaded spawn chunk (`-1,112,-1`), and checks world-map settings instead of a
  map image the bot never reads from the dedicated `WorldMap` stream.

## Verification (session 96)

```sh
cargo test -p hytale-server --lib                  # 57 passed
cargo test -p hytale-commands                      # 27 passed
cargo test -p hytale-world                         # all suites passed (incl. placement 2)
cargo test -p client-bot --test block_interactions -- --ignored   # 2 passed
cargo test -p client-bot --test commands -- --ignored             # 2 passed (setblock/rotation)
./tools/test.sh fast                               # 1618 passed, 28 skipped
cargo clippy -p hytale-server -p hytale-commands -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Follow-ups

- The deferred placement work is filed as
  [T69b](T69b-block-placement-followups.md): server-side
  `ConnectedBlockRuleSet` state variants, `quickReplace`/`quickRetype`/
  `noPhysics`, and block-entity components beyond containers.
- `RandomRotation`/`RotationYawPlacementOffset` parity if a client scenario
  exposes it.
