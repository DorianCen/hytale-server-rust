# T67 — Command framework and in-game commands

**Status:** in progress (session 91; core framework and built-ins landed)
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

## Deviations

- There is no permission provider yet; the authenticated in-game player is
  currently treated as the server operator so `/stop` and future protected
  commands are usable. The framework already supports permission-filtered trees
  and denial.
- Plain (non-command) chat is ignored for now; command chat is the first slice.
- `/tp` and `/give` target the sender only; player-name targeting and cross-
  session mutation need the live registry lookup.

## Follow-ups

- Add `kick`, `world`/universe selection, `maxplayers`, `maxviewradius`, the
  console stdin sender and player-name `tp`/`give` targets.
- Wire the real permissions provider, op/group management and per-command
  permissions instead of the temporary operator fallback.
- Broadcast plain chat and user/message formatting events.
- The full Java command set (`debug/**`, editor/builder, cosmetics, prefab and
  singleplayer commands) stays out of scope for this packet.
- Command history and tab-completion UI polish stay later.
