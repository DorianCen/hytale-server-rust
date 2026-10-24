# Tasks — CoreServer rewrite, phase 2

Each file is a self-contained work packet for one branch/worktree. Tasks are
sized so a fast subagent can finish them without loading the whole codebase into
context. The main agent owns sequencing and shared-file edits.

## Status board

| Task | Title | Depends on | Agent fit | Status |
|---|---|---|---|---|
| [T34](T34-archive-rebaseline.md) | Archive vertical slice and re-baseline docs | T33 | high | done (session 24) |
| [T35](T35-child-store-goldens.md) | Child-store golden dumper extension | T34 | medium | done (session 25) |
| [T36](T36-block-fluid-child-stores.md) | Block/fluid child stores + packets | T35 | medium | done (session 26) |
| [T37](T37-item-child-stores.md) | Item child stores + packets | T35 | medium | done (session 26) |
| [T38](T38-tag-patterns.md) | TagPattern + `UpdateTagPatterns` | T35 | medium | done (session 26) |
| [T39](T39-asset-wiring-validation.md) | Asset wiring, config cache, `--validate-assets` | T36, T37, T38 | medium | done (session 27) |
| [T40](T40-hytale-core-runtime.md) | `hytale-core` runtime | T34 | medium | planned |
| [T41](T41-universe-world-lifecycle.md) | Universe and world lifecycle | T40 | low/medium | planned |
| [T42](T42-persistent-listener.md) | Persistent listener and multi-session runtime | T39, T41 | low/medium | planned |
| T43 | World clock and `GameplayConfig` | T42 | medium | planned |
| T44 | Self-spawn equipment and cosmetics | T42 | medium | planned |
| T45 | Other-player entity tracking and broadcast | T42, T44 | low/medium | planned |
| T46 | Block place/break | T42 | medium | planned |
| T47 | Disconnect, reconnect and session teardown | T43–T46 | medium | planned |
| T48 | Real-client trace and v1 gate | T47 | low/medium | planned |
| T49 | Inventory, containers and transactions | T48 | low/medium | planned |
| T50 | Interaction graph and block interactions | T49 | low/medium | planned |
| T51 | Player movement physics and collision | T48 | low/medium | planned |
| T52 | Block simulation: ticks, fluids, support | T49, T51 | low/medium | planned |
| T53 | Entity stats, effects and combat basics | T48 | low/medium | planned |
| T54 | Time and weather | T43, T48 | medium | planned |
| T55 | Projectiles and traversal | T51, T53 | low/medium | planned |
| T56 | Prefab spawner and runtime entities | T45, T50 | low/medium | planned |
| T57 | UI basics and block-health presentation | T46, T53 | low/medium | planned |
| T58 | NPC core: AI, pathfinding, flocking | T48 | low/medium | planned |
| T59 | NPC spawning, combat and encounters | T58 | low/medium | planned |
| T60 | Builtin module runtime and selected modules | T58, T59 | low/medium | planned |
| T61 | Editor/builder/machinima/voice/cosmetics scope | T60 | low | planned |
| T62 | Security, fuzzing and rate limits | T48 | medium | planned |
| T63 | Performance, memory and soak | T48 | medium | planned |
| T64 | Packaging, docs and plugin/mod API | T60 | medium | planned |

Task files for T43–T64 are created when their milestone starts; the milestone
scope is in [`../plan.md`](../plan.md).

## Rules

1. **One task per branch/worktree.** Never edit files owned by another task.
2. **Shared files are coordinated.** `Cargo.toml`, `Cargo.lock`, `plan.md`,
   `NEXT_STEPS.md`, `ARCHITECTURE.md` and this board are main-agent-owned unless
   the task explicitly lists them.
3. **Read only what the task lists.** Use `rg`/`find` for discovery, then read
   the matches. Do not open thousands of generated Java files.
4. **Golden-first.** If the task needs expected bytes and no fixture exists, stop
   and extend the Java dumper instead of inventing values.
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
9. **Do not touch `done/`** except broken-link fixes; it is the archive.

## Subagent sizing guide

- **high**: isolated files, no Java reading beyond one small class, no network.
- **medium**: one Java class/generator or a bounded format port.
- **low**: multi-crate integration, TLS/QUIC behaviour, or debugging against a
  real client. Use the main agent or a stronger model.

If a task exceeds ~200k tokens of reading, split it. The task must name the
exact files so the subagent never has to guess.
