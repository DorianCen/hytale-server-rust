# T71 — Interaction long tail

**Status:** done (session 100)
**Package:** `hytale-interactions`, `hytale-server`, `hytale-protocol`,
`tools/client-bot`
**Depends on:** T50, T60b, T66
**Agent fit:** medium

## Goal

Close the interaction payloads T66 left at their base form and make the
server-side interaction graph behaviorally match Java for the remaining common
client paths: chaining/memories/charging/wielding next maps, place-mode
selection, active-slot changes, and missing-id sentinel consistency.

The client already receives every protocol variant. This task owns the
*server* semantics and the remaining payload detail, not a new asset encoder.

## Current gaps

- `InteractionCatalog::resolve_node` and `executor.rs` traverse the common
  `Next`/`Interactions` slots, but `Chaining`, `Memories`, `Charging` and
  `Wielding` still lose their state-specific next maps where the client expects
  the full map.
- `PlaceModeSelect` is encoded but not applied as an authoritative place-mode
  decision.
- `ChangeActiveSlot` is encoded but the server path does not persist/ack the
  selected slot in every interaction chain.
- Missing-id sentinels are inconsistent across interaction/lookup paths: some
  use `0`, while Java uses `NOT_FOUND`/`-1` (or the reverse) for no block, no
  item, no interaction and no root.
- Charging/wielding are exercised by real consumable/shield assets but there is
  no scripted regression for the chain state transitions.

## Deliverables

- Extend the interaction catalog/executor:
  - preserve per-state `Next` maps for `Chaining`, `Memories`, `Charging` and
    `Wielding` instead of collapsing them to the base node;
  - route `Charging`/`Wielding` states and failed/interrupted branches through
    the same bounded executor;
  - apply `PlaceModeSelect` (`Default`, `Single`, `Line`, `Area`, ...) and
    `ChangeActiveSlot` as authoritative outcomes/effects;
  - retain interaction-chain state (`chain_id`, fork id, active slot) where the
    ack needs it.
- Audit missing-id sentinels against Java:
  - block/item/interaction/root ids in `hytale-interactions`, asset packet
    encoders and session chain acks;
  - document the canonical value per id space and add unit regressions.
- `client-bot` loopbacks:
  - charged consumable or charged-shot chain that reaches the charge branch;
  - shield/wielding branch;
  - place-mode/active-slot change observed by the server and a peer client.
- Docs/task board updates.

## Landed (session 100)

- `ExecutionContext` now carries `place_mode`, `memories_level`,
  `charge_value`/`charge_failed`, `chaining_index`/`flag_index` and
  `active_slot`.
- The executor selects Java's branch maps for `Chaining` (Next array and sorted
  Flags), `Charging`/`Wielding` (closest charge threshold, failed branch),
  `MemoriesCondition` (level map/failed), and `PlaceModeSelect` (creative mode
  only, with Default fallback). It emits `ChangeActiveSlot`.
- `PlaceBlockFlags` traversal is branch-aware, so `Block_Secondary` selects the
  creative Replace/Retype/Extrude/SurfaceDraw/FastPlace flags instead of always
  walking every child.
- Asset registration materializes `Charging.Next`, `Wielding.Next` and
  `MemoriesCondition.Next` inline children under Java's generated keys; the
  encoders now populate `chargedNext`/`memoriesNext`. `ChangeActiveSlot` uses
  Java's `Integer.MIN_VALUE` UNSET sentinel instead of slot 0.
- The session stores `SyncPlayerPreferences.place_mode`, passes it into the
  interaction/placement context, and applies `ChangeActiveSlot` to the hotbar
  (authoritative `SetActiveSlot` + held-item rebroadcast).
- Shared `ASSET_NOT_FOUND`/`INTERACTION_NOT_FOUND` constants document the
  indexed-asset sentinel and are covered by a distinct-from-zero regression.
- New ignored `client-bot/tests/interaction_long_tail.rs` loopback sends
  `SyncPlayerPreferences` with Replace mode, starts the active-slot root
  (`ChangeActiveSlot` 0 -> 1), Wielding and Charging roots through
  `overrideRootInteraction`, and a `Block_Secondary` chain that quick-replaces
  stone with the held soil block. It asserts the authoritative `SetActiveSlot`,
  the replaced block update and the server acks for every override root.

## Verification (session 100)

```sh
cargo test -p hytale-interactions                        # 20 passed
cargo test -p hytale-assets --test interaction_packets   # 7 passed
cargo test -p hytale-assets --test block_item_packets    # 24 passed
cargo test -p hytale-server --lib                         # 58 passed
cargo test -p client-bot --test block_interactions -- --ignored            # 2 passed
cargo test -p client-bot --test interaction_long_tail -- --ignored       # 1 passed
./tools/test.sh fast                                                     # 1646 passed, 30 skipped
cargo clippy -p hytale-assets -p hytale-interactions -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Read first

- Java:
  - `CoreServer/.../modules/interaction/interaction/config/client/ChainingInteraction.java`
  - `CoreServer/.../modules/interaction/interaction/config/entity/MemoriesInteraction.java`
  - `CoreServer/.../modules/interaction/interaction/config/entity/ChargingInteraction.java`
  - `CoreServer/.../modules/interaction/interaction/config/entity/WieldingInteraction.java`
  - `CoreServer/.../modules/interaction/interaction/config/client/PlaceModeSelectInteraction.java`
  - `CoreServer/.../modules/interaction/interaction/config/entity/ChangeActiveSlotInteraction.java`
  - `CoreServer/.../modules/interaction/InteractionModule.java`,
    `InteractionManager.java`
- Rust:
  - `crates/hytale-interactions/src/{catalog,executor,block,combat}.rs`
  - `crates/hytale-server/src/session.rs` (`block_interaction_from_chain`,
    `finish_interaction_chain`, `ack_interaction_update`)
  - `crates/hytale-assets/src/packets/mod.rs` interaction encoders
  - `tools/client-bot/tests/block_interactions.rs`

## Acceptance

```sh
cargo test -p hytale-interactions
cargo test -p hytale-server --lib
cargo test -p client-bot --test block_interactions -- --ignored
./tools/test.sh fast
cargo clippy -p hytale-interactions -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Real charged/wielding assets follow the correct state-specific next map.
- Place-mode/active-slot changes are authoritative and acked without desync.
- Every missing-id path has a documented sentinel and a regression.
- Existing door/place/break/container loopbacks stay green.

## Deviations

- The place-mode loopback uses the server's `overrideRootInteraction` path for
  `Block_Secondary`; the release client may instead carry the item root in a
  fork. A release-client trace is the next step if a live desync appears.
- Full server-side simulation of client `chargeValue`/`forkCounts` state is
  limited to branch selection; the executor remains a bounded graph walk rather
  than a persistent tick-by-tick interaction state machine.

## Notes

- Do not invent a new interaction payload shape. Use the existing generated
  variants and the Java source as the contract.
- Keep the executor bounded; long-tail nodes must stay inert rather than panic.
