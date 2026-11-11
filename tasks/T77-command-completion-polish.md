# T77 — Command completion polish

**Status:** proposed (filed session 99)
**Package:** `hytale-commands`, `hytale-server`, docs
**Depends on:** T67b
**Agent fit:** medium

## Goal

Close the remaining command-completion polish: Java-compatible fuzzy suggestion
ranking, optional/override/subcommand token walking for the console, and
history/tab-completion UX. The advanced debug/editor/builder command set remains
out of scope (T61).

## Current gaps

- `CommandRegistry::suggest` filters prefix/substring only; Java
  `SuggestionUtil` adds a fuzzy-distance tier.
- The console `run_console` is a plain stdin loop with no completion or history
  integration.
- `CommandCompletion`-style token walking is absent: the registry cannot resolve
  the argument under the cursor through optional `--name[=value]`, suggestion
  override spans or subcommands.
- There is no regression for cursor/word-index edge cases.

## Deliverables

- Port `CommandCompletion` semantics into `hytale-commands`:
  - command-name completion and alias-aware resolution;
  - subcommand descent;
  - positional argument spans using `value_count`/parameter count;
  - optional `--name` and `--name=value` completion;
  - suggestion-override spans;
  - permission filtering at every step.
- Port `SuggestionUtil` ranking:
  - prefix, substring, then fuzzy-distance matches;
  - length/alphabetical tie-breaking and the 150-value network cap.
- Console UX:
  - add completion/history through a maintained line editor or an opt-in
    dependency; keep piped `--console`/non-terminal behavior prompt-free;
  - make completion failures non-fatal and testable without a TTY.
- Tests:
  - synthetic command tree covering multi-token args, overrides, optionals and
    subcommands;
  - fuzzy ranking cases;
  - console completion/history scripted test (no interactive terminal needed).
- Docs: update command task/board/NEXT_STEPS.

## Read first

- Java:
  - `CoreServer/.../command/system/CommandCompletion.java`
  - `.../command/system/suggestion/SuggestionUtil.java`
  - `.../console/ConsoleCommandCompleter.java`
  - `.../console/ConsoleModule.java`
  - `.../command/system/CommandManager.java` (`suggestCompletions`)
- Rust:
  - `crates/hytale-commands/src/{registry,console,builtins}.rs`
  - `crates/hytale-server/src/serve.rs` (`spawn_console`)
  - `tools/client-bot/tests/commands.rs` (ArgValues/console behavior)

## Acceptance

```sh
cargo test -p hytale-commands
cargo test -p hytale-server --lib
cargo test -p client-bot --test commands -- --ignored
./tools/test.sh fast
cargo clippy -p hytale-commands -p hytale-server --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Completion resolves command names, aliases, subcommands, optionals and
  multi-token arguments at the cursor.
- Fuzzy ranking matches Java's match tiers and cap behavior.
- Piped console output stays clean; interactive history/tab completion works
  when a terminal is available.
- Existing command tree, ArgValues and routing tests remain green.

## Notes

- The debug/editor/builder/cosmetics/prefab command set is not part of this
  packet.
- If adding a line-editor dependency is undesirable, implement the completion
  engine first and keep the history/TTY integration behind an opt-in feature.
