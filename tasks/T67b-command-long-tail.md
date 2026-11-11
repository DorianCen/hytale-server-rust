# T67b — Command long tail

**Status:** done (session 99)
**Package:** `hytale-commands`, `hytale-server`, `tools/client-bot`
**Depends on:** T67
**Agent fit:** medium

## Goal

Close the command-framework follow-ups T67 deliberately left open: dynamic
argument-value suggestions with Java's `argTypeId` request/response flow,
runtime `maxplayers`/`maxviewradius` changes, and formatted chat. The remaining
Java debug/editor/builder command set stays out of scope (T61).

## Landed

- **Argument suggestions:**
  - `CommandArgInfo.argTypeId` is populated for every built-in argument kind and
    the tree uses Java's `getSuggestionValueCount` values (game modes 4, dynamic
    types `-1`).
  - `CommandRegistry::suggest` resolves `hytale:player`, `hytale:item`,
    `hytale:game_mode` and `hytale:coordinates` from live context/assets. Player
    and item values are prefix/substring filtered, capped at Java's
    `MAX_WIRE_SUGGESTIONS` (150), and carry per-value continuation flags.
  - The session answers `ArgValuesRequest` with `ArgValuesResponse`, validates
    the Java 128/256-byte id/partial limits, rate-limits per type to one request
    per 50 ms, and records request/suggestion counters.
  - Player joins/leaves broadcast `ArgCacheInvalidation` for the player type so
    clients refetch the online-player list.
- **Runtime limits:**
  - New `hytale-server::RuntimeLimits` holds atomics shared by every session and
    the console. `/maxplayers <amount>` updates `ServerInfo.maxPlayers` for new
    joins and rejects joins above the limit; `/maxviewradius <radius>` validates
    Java's `1..=32` range, updates the handshake/view clamp, and queues a control
    action that re-clamps every live session and sends the new `ViewRadius`.
  - Queries (`/maxplayers`, `/maxviewradius`) read the shared values. The console
    applies the same effects.
- **Formatted chat:**
  - `hytale-commands::message` builds translated messages
    (`server.chat.playerMessage`, `server.chat.broadcastMessage`) with raw-text
    fallbacks, colours and a Java `Message.parse`-compatible JSON subset
    (`RawText`, `MessageId`, `Params`, `MessageParams`, `Children`, styles,
    `Color`, `Link`, `Image`).
  - `PlayerMessage::Chat` now carries a `FormattedMessage`; plain chat keeps its
    translation id and `/say` applies Java's cyan colour. `/say { ... }` keeps
    rich styles instead of flattening the payload.

## Tests

- `hytale-commands`: 31 unit tests cover tree arg ids, value filtering, the JSON
  message parser, formatted `/say`, runtime limit effects and console routing.
- `hytale-server`: 58 library tests, including `RuntimeLimits` clamping and
  formatted/argument-cache broadcasts.
- `client-bot/tests/commands.rs` drives the real loopback server and verifies
  arg-value responses for player/item/game-mode, player-cache invalidation,
  formatted player/say messages, `/say` JSON styling, runtime limits and the
  existing command round trip.

## Acceptance

```sh
./tools/test.sh fast
cargo test -p hytale-commands
cargo test -p hytale-server --lib
cargo test -p client-bot --test commands -- --ignored
cargo clippy -p hytale-commands -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Verification (session 99)

```sh
./tools/test.sh fast                                   # 1637 passed, 29 skipped
cargo test -p hytale-commands                          # 31 passed
cargo test -p hytale-server --lib                      # 58 passed
cargo test -p client-bot --test commands -- --ignored  # 2 passed (61.19 s)
cargo clippy -p hytale-commands -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Deviations

- Java assigns a random UUID per argument type. Rust uses stable descriptive ids
  (`hytale:player`, ...), which are equivalent as opaque client cache keys.
- Item suggestions use ordered prefix/substring tiers, matching the client's
  visible filtering. Java's final fuzzy-distance tier is not ported.
- Formatted chat covers Java's built-in JSON codec and translation keys; plugin
  `PlayerChatEvent` formatter hooks are not present because the Rust server has
  no Java plugin event bus.
- `/maxplayers` does not disconnect sessions that are already connected when the
  limit is lowered, matching Java's config update behavior.
- The debug/editor/builder/cosmetics/prefab command set remains out of scope
  (T61).

## Follow-ups

- Add fuzzy-distance ranking and Java `SuggestionProvider` overrides if a
  release-client completion trace exposes a difference.
- Command history/tab-completion polish belongs to the console UI, not the
  protocol path.
