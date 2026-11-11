# T67 — Command framework and in-game commands

**Status:** done (session 91 framework; session 94 console, permissions, chat and targets)
**Package:** `hytale-commands`, `hytale-server`, `hytale-core`, `tools/client-bot`
**Depends on:** T42, T48
**Agent fit:** medium

## Goal

Replace the empty `hytale-commands` stub with the Java command framework and wire
it into the live server, so the console and in-game chat can execute commands.
Today `ChatMessage` is ignored and `CommandTreeSync` is never sent, which is why
`/help`, `/tp`, `/give`, `/stop` and the rest do not work in a vanilla client.

The port should follow Java `CoreServer/.../command/**` and
`CoreServer/.../console/command/**`, but scope is the framework plus the core
built-ins needed by the current server — not every debug/editor command.

## Landed (session 91)

- `hytale-commands` now has a tokenizer (quotes/escapes), command registry with
  aliases, argument specs, sender/permission model, `CommandInvocation` and
  runtime `CommandEffect`s, plus a `CommandTreeSync` builder.
- Built-ins: `help`, `tp`, `give`, `gamemode`, `save`, `stop`, `whereami`,
  `whoami`, `who`, `kill`, `respawn`, `time`, `version`.
- `ChatMessage` command lines are decoded and dispatched in the live game loop.
  `CommandTreeSync` is sent after the join HUD, matching Java's
  `GamePacketHandler.registered0`.
- Effects are applied on the session: authoritative teleport, inventory give +
  resync, game-mode packet, damage/respawn, player+world save, and a listener
  shutdown signal for `/stop`.
- `client-bot` can send commands and records `CommandTreeSync`/`ServerMessage`/
  teleports. The ignored `commands` loopback test covers `/help`, `/tp`, `/give`
  and `/save`.

## Landed (session 94) — console, permissions, chat and targets

- **Permissions:** `hytale-core::permissions` adds `PermissionSet` (exact,
  `*` and `prefix.*` matching) and `PermissionRegistry` (default set, named
  groups, per-player grants and operators). `CommandSender` now resolves a
  permission set, and the command tree/help filter and dispatch use it. The
  server CLI accepts repeatable `--op <name>`; unconfigured players get no
  permissions, while the tooling/test default registry stays permissive.
  Acting on another player requires `server.commands.target`; `stop`, `say`,
  `kick`, `world`, `maxplayers` and `maxviewradius` carry their own
  `server.commands.*` permissions.
- **Console:** `hytale-commands::run_console` and the `ConsoleEffects` trait;
  the listener starts a stdin console when stdin is a terminal (also
  `--console` / `--no-console`). Console `save`, `stop`, `say`, `kick` and
  `world` map to the world thread, player registry and shutdown signal; player
  effects report that they are unavailable from the console.
- **Chat:** plain (non-command) `ChatMessage` lines are relayed to every player
  as `Name: message`; `/say` broadcasts an operator message. The game-loop
  report tracks `chat_messages`.
- **Player targets:** `/tp`, `/give`, `/gamemode`, `/kill` and `/respawn` accept
  a player name (self when omitted). Remote effects are queued as
  `players::ControlAction`s and applied by the target session's own loop
  (authoritative teleport correction, inventory resync, game-mode/state
  packets, disconnect), so no writer is shared across sessions.
- **New commands:** `/kick <player> [reason]`, `/world [name]` (query, or a
  live world switch through `PortalRouter` in listener mode),
  `/maxplayers` and `/maxviewradius` (queries).
- **Tests:** 24 `hytale-commands` unit tests (including the console runner and
  targeting/permission cases), 55 `hytale-server` library tests (permission
  resolution and control/chat fan-out), and two ignored `client-bot` loopbacks:
  the single-bot command round trip (`/help`, `/tp`, `/give`, `/save`, `/say`,
  `/world`, limits, `/who`, `/whereami`, `/gamemode`, unknown-command errors and
  plain chat) and a two-bot operator test (targeted give/teleport/gamemode,
  cross-session control actions, and the operator-only `/stop` tree entry).

## Read first

- Java: `CoreServer/src/main/java/com/hypixel/hytale/server/core/command/system/`
  `CommandManager.java`, `CommandRegistry.java`, `Tokenizer.java`,
  `ParserContext.java`, `ParseResult.java`, `CommandContext.java`,
  `CommandSender.java`, `CommandTreeBuilder.java`, `CommandBase.java` and
  `exceptions/`.
- Java: `CoreServer/src/main/java/com/hypixel/hytale/server/core/console/command/`
  `SayCommand.java`, plus `console/ConsoleSender.java`/`ConsoleModule.java` for
  the console sender.
- Rust: `crates/hytale-commands/src/lib.rs`,
  `crates/hytale-protocol/src/generated/interface_.rs` (`ChatMessage`,
  `CommandTreeSync`) and `crates/hytale-server/src/session.rs` for the packet
  loop.

## Deliverables

- `hytale-commands` foundation:
  - command registry with names, aliases, descriptions, usage and subcommands;
  - tokenizer and argument parser (`Tokenizer`, `ParserContext`, `ParseResult`,
    `MatchResult`, `AbbreviationMap` behavior);
  - command context/sender abstraction for console, player and target-world
    commands, including deferred execution on the owning world thread;
  - permission checks (`hytale-core` permission holder/query seam) and the
    Java error types (`CommandException`, `NoPermissionException`,
    `SenderTypeException`).
- Protocol wiring:
  - decode and dispatch inbound `ChatMessage` command lines, with `/` handling
    and non-command chat ignored or relayed consistently;
  - build and send `CommandTreeSync` from the live registry after join/config;
  - send `ServerMessage`/`ChatType` responses for success, usage and errors.
- Built-in commands:
  - server: `stop`, `save`, `kick`, `who`, `maxplayers`, `maxviewradius`, `version`;
  - player/world: `tp`, `give`, `gamemode`, `kill`, `respawn`, `whereami`,
    `whoami`;
  - utility: `help` (tree-aware), `world`/`universe` selection and `time`;
  - console equivalents and a console sender over stdin where the existing
    server process supports it.
- Suggestion/completion data for the tree and player-name/item-id arguments.
- Tests and coverage:
  - unit tests for tokenizing, quoting, aliases, subcommands, suggestions and
    permission denial;
  - a loopback `client-bot` scenario that sends `/help`, `/tp` and `/give`,
    receives `CommandTreeSync` + `ServerMessage`, and verifies the world/player
    state changed;
  - a serialized `CommandTreeSync` fixture for the built-in tree, checked for
    names/aliases/usage/subcommands and stable ordering;
  - console command test for `stop`/`save`.
- Docs: update `README.md`, `ARCHITECTURE.md`, `NEXT_STEPS.md` and this task file
  when the framework lands.

## Acceptance

```sh
./tools/test.sh fast
cargo test -p hytale-commands
cargo clippy -p hytale-commands -p hytale-server --all-targets -- -D warnings
cargo fmt -p hytale-commands -p hytale-server -- --check
```

- A vanilla client receives a non-empty `CommandTreeSync` and `/help` lists the
  registered commands.
- `/tp`, `/give`, `/gamemode` and `/kill` change the live player state through
  the world thread, and `/save` persists it.
- Unknown commands, bad arguments and missing permissions return a clear
  `ServerMessage` instead of being silently ignored.
- `client-bot` covers the command round trip and the existing join/edit gates
  stay green.

## Verification (session 94)

```sh
cargo test -p hytale-commands                      # 24 passed
cargo test -p hytale-server --lib                  # 55 passed
cargo test -p client-bot --test commands -- --ignored   # 2 passed
./tools/test.sh fast                               # 1605 passed, 26 skipped
cargo clippy -p hytale-commands -p hytale-server -p hytale-core --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Deviations

- Argument **value** suggestions (Java's per-process random `argTypeId` registry
  and suggestion-sync flow) are not ported. The tree carries the argument kinds
  and names (`Player`, `Item`, `GameMode`, ...) so the client can use its own
  completion providers; dynamic player/item value lists remain a follow-up.
- `/maxplayers` and `/maxviewradius` report the configured values; changing them
  at runtime is rejected because the handshake `ServerInfo`/view ceiling is
  captured at startup. Set them with the CLI and restart.
- `/world` switching requires listener mode (a `PortalRouter`); single-connection
  tools report that world switching is unavailable. Console `/world <name>`
  starts/resolves the world but has no live player to move.
- The `CommandTreeSync` coverage is a stable in-code snapshot of names, usage,
  arguments and permission filtering (plus a protocol round-trip test) rather
  than a committed binary `.bin` fixture; it catches ordering and field drift
  without adding another generated artifact.
- A world switch keeps the source world's saved player document so returning to
  it later restores the previous position, matching the portal-return model
  rather than deleting the source record.
- The full Java command set (`debug/**`, editor/builder, cosmetics, prefab and
  singleplayer commands) stays out of scope for this packet.

## Follow-ups

- Dynamic argument-value suggestions and the Java `argTypeId` sync packet.
- Runtime `maxplayers`/`maxviewradius` changes if the listener gains a shared
  atomic limit; today the semaphore/`ServerInfo` are fixed at startup.
- Command history, tab-completion UI polish and the remaining Java command set.
- Message formatting events/rich text for chat (the current relay uses raw text).
