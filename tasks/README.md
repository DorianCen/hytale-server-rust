# Tasks — CoreServer rewrite

Each file is a self-contained work packet for one branch/worktree. Tasks are
sized so a fast subagent can finish them without loading the whole codebase into
context. The main agent owns sequencing and shared-file edits.

## Status board

Update the `Status:` line in the task file when you start/finish. Do not mark a
task done until its acceptance section is satisfied.

| Task | Title | Depends on | Agent fit | Status |
|---|---|---|---|---|
| [T01](T01-workspace-scaffolding.md) | Workspace crate scaffolding | — | high | done (session 1) |
| [T02](T02-protocol-inventory.md) | Protocol source inventory | T01 | high | done (session 1) |
| [T03](T03-connection-packet-ir.md) | Connection packet IR | T02 | medium | done (session 1) |
| [T04](T04-connection-codegen.md) | Connection packet codegen | T03, T05 | medium | done (session 2) |
| [T05](T05-wire-primitives.md) | Wire primitives + framing | T01 | high | done (session 1) |
| [T06](T06-quic-transport-spike.md) | QUIC transport spike | T01 | low/medium | done (session 2) |
| [T07](T07-indexed-storage-reader.md) | IndexedStorageFile reader/writer | T01 | medium | done (session 1) |
| [T08](T08-asset-store-inventory.md) | Asset store inventory | T01 | high | done (session 1) |
| [T09](T09-asset-parent-resolver.md) | Asset pack + Parent resolver | T01 | medium | done (session 1) |
| [T10a](T10a-palette.md) | Section palette types | T01 | high | done (session 1) |
| [T10b](T10b-block-section.md) | BlockSection + serialization | T10a | medium | done (session 2) |
| [T10c](T10c-chunk-column.md) | ChunkColumn + generated conversion | T10a, T10b | medium | done (session 2) |
| [T11](T11-asset-golden-dumper.md) | Java asset-packet golden dumper | T08 | medium | done (session 2) |
| [T12](T12-chunk-packet-goldens.md) | Java chunk-packet goldens | T10b | medium | done (session 3; Rust consumer in session 4) |
| [T13](T13-client-bot.md) | Rust client bot skeleton | T04, T06 | medium | done (session 2) |
| [T14](T14-full-protocol-codegen.md) | Full protocol codegen | T04 | medium | done (session 3) |
| [T15](T15-asset-registry.md) | P0 asset registry and typed stores | T08, T09 | medium | done (session 5) |
| [T16](T16-bson-world-storage.md) | BSON chunk storage + Java world fixture | T07, T10c, T15 | medium | done (session 6) |
| [T17](T17-headless-boot.md) | Headless boot: one chunk + region save | T15, T16 | medium | done (session 7) |
| [T18](T18-world-storage-runtime.md) | Runtime world storage + Java reload gate | T16, T17 | medium | done (session 8) |
| [T19](T19-world-config.md) | World config load/save + Java config gate | T18 | medium | done (session 9) |
| [T20](T20-player-data.md) | Player save BSON + Java player gate | T19 | medium | done (session 10) |
| [T21](T21-world-save-boot.md) | Complete saved-world boot + whole-save gate | T18, T19, T20 | medium | done (session 11) |
| [T22](T22-demand-chunks.md) | Generate missing chunks + demand-load | T21 | medium | done (session 12) |
| [T23](T23-world-thread.md) | World thread + view tracking + demand queue | T22 | medium | done (session 13) |
| [T24](T24-chunk-stream.md) | Chunk packet stream from world events | T23, T12 | medium | done (session 14) |
| [T25](T25-chunk-stream-transport.md) | Chunks QUIC transport + client-bot receiver | T24, T06 | medium | done (session 15) |
| [T26](T26-live-chunk-session.md) | Live chunk session bridge + client-bot stream mode | T23, T24, T25 | medium | done (session 16) |
| [T27](T27-incremental-view-updates.md) | Incremental view updates + unload packets | T26 | medium | done (session 17) |
| [T28](T28-p0-asset-packets.md) | P0 asset packet encoders (PhysicalMaterial + BlockHitboxes) | T15 | medium | done (session 18) |
| [T29](T29-dev-join.md) | Dev/insecure handshake + first bot join | T25, T26 | medium | done (session 19) |

## Rules

1. **One task per branch/worktree.** Never edit files owned by another task.
2. **Shared files are coordinated.** `Cargo.toml`, `Cargo.lock`, `ARCHITECTURE.md`
   and `NEXT_STEPS.md` are main-agent-owned unless the task explicitly lists them.
   T01 lands first; later tasks add dependencies by asking the main agent or by
   editing only their own crate's `Cargo.toml`.
3. **Read only what the task lists.** Use `rg`/`find` for discovery, then read the
   matches. Do not open 1,000 generated Java files.
4. **Golden-first.** If the task needs expected bytes and no fixture exists, stop
   and report the missing dumper instead of inventing values.
5. **Generated code is write-only.** Edit the IR/generator, rerun, commit both.
6. **No broad refactors.** Finish the acceptance section; report follow-ups.
7. **Gate before handoff:**

   ```sh
   cargo test -p <crate>
   cargo clippy -p <crate> --all-targets -- -D warnings
   cargo fmt -p <crate> -- --check
   ```

   For workspace-level tasks use `--workspace`.
8. **Report:** files changed, commands run, results, deviations, follow-ups.

## Subagent sizing guide

- **high**: isolated files, no Java reading beyond one small class, no network.
  Suitable for DeepSeek V4.1 Flash / GLM 5.3 Flash.
- **medium**: one Java class or one generated packet family, a parser, or a
  format port. Needs careful reading but still bounded.
- **low**: multi-crate integration, TLS/QUIC behavior, or debugging against a
  real client. Use the main agent or a stronger model.

If a task exceeds ~200k tokens of reading, split it. The task should name the
exact files so the subagent never has to guess.
