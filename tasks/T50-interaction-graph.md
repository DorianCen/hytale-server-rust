# T50 — Interaction graph and block interactions

**Status:** done (session 38)
**Package:** new `hytale-interactions`, `hytale-server`, `client-bot`
**Depends on:** T49
**Agent fit:** medium

## Goal

Load the Java root/interaction assets, walk the structural graph (chains,
conditions, variables, state/action nodes) and dispatch block `Use` interactions
before placement, with doors as the first visible consumer.

## Deliverables

- `hytale-interactions`:
  - load `Server/Item/Interactions` and `Server/Item/RootInteractions` with
    Java-style `Parent` inheritance from the base pack (single-pass
    `read_many`);
  - `InteractionCatalog` lookup for roots/nodes;
  - graph executor for `Condition`, `Replace`/interaction vars, `Serial`,
    `Parallel`, `Chaining`, `Repeat`, `Next`/`Failed` and inline nodes;
  - `WorldAction` outcomes for `Door`, `ChangeState`, `ChangeBlock`,
    `OpenContainer`, `OpenBench`, `UseBlock`, `BreakBlock` and `PlaceBlock`;
  - `BlockInteractions` extraction from `BlockType` JSON (`Interactions`,
    `Bench`, `Container`, state definitions and current state).
- `WorldThread`:
  - load the catalog once per world;
  - expose `block_type_json`, `set_block_state` (preserving rotation/filler) and
    `interaction_catalog`.
- `session`:
  - on right-click, resolve `BlockType.Interactions` (`Use`, then
    `Secondary`/`Primary`) and execute the root before falling back to
    T46 placement;
  - apply door toggles and `ChangeState` maps through `set_block_state` and
    broadcast the authoritative `ServerSetBlock`;
  - count block uses/state changes and keep the T46 break/place path green;
  - detect bench/container blocks as handled UI consumers.
- `client-bot`:
  - `use_block_after_join` scripted right-clicks;
  - door loopback: place a door, use it twice, assert placement → open →
    close/change state and server counters.
- Tests:
  - synthetic graph tests (door node, condition failure path, interaction vars,
    container root);
  - real pack catalog load (door/container roots, >500 roots, >1k nodes);
  - block metadata extraction;
  - ignored door loopback.

## Read first

- Java `InteractionModule.doMouseInteraction`, `RootInteraction`,
  `Interaction`, `ConditionInteraction`, `ReplaceInteraction`,
  `UseBlockInteraction`, `DoorInteraction`, `DoorBlockUtils`,
  `ChangeStateInteraction`, `OpenContainerInteraction`
- `Assets/extracted/Server/Item/{Interactions,RootInteractions}/**`
- `crates/hytale-server/src/{session,world_thread}.rs`
- `tools/client-bot/src/lib.rs`, `tests/block_interactions.rs`

## Acceptance

```sh
cargo test -p hytale-interactions
cargo test -p hytale-server
cargo test -p client-bot --test block_interactions -- --ignored
cargo test -p client-bot --test multi_join -- --ignored
cargo test -p client-bot --test inventory_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A placed door opens and then changes/closes state through the interaction
  graph, with authoritative block broadcasts.
- Condition/replace/vars and container roots are covered by scripts.
- T46 block place/break and T49 held-item consumption remain green.

## Deviations

- The executor ports the structural/block nodes needed by the first consumers,
  not every Java interaction type. Selectors, effects, sounds, cooldowns,
  projectiles, stats, client-chain sync and entity/weapon roots remain later
  gameplay work (T53/T55/T57).
- Bench/container blocks are detected and dispatched as handled UI actions, but
  the window packets/UI are T57.
- Door handling uses the base state/front heuristic and single-block state
  variants; double doors, filler/hitbox updates, soft-block breaking, door
  blocked checks and exact rotation matrices are not ported.
- `ChangeBlock` actions are parsed but not applied yet.
- The current right-click dispatch prioritises the block's `Use` root over the
  held item's `Secondary` item root, matching the observed door/container
  behavior; full item interaction root selection is future gameplay work.

## Follow-ups

- T51/T53/T55 add movement/combat/projectile interaction consumers.
- T56 adds block components and entity interaction hosts.
- T57 adds window/UI packets for containers/benches.
