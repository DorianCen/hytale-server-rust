# T01 — Workspace crate scaffolding

**Status:** done (session 1)
**Agent fit:** high (fast/cheap subagent; no Java reading)
**Depends on:** none
**Owns:** `Cargo.toml`, `crates/hytale-*/Cargo.toml`, `crates/hytale-*/src/lib.rs`,
`crates/hytale-server/src/main.rs`

## Goal

Add the planned CoreServer crates to the workspace with correct names,
dependency direction, and empty documented modules. Nothing else.

## Read

- `Cargo.toml`
- `crates/worldgen-server/Cargo.toml` (style reference)
- `crates/worldgen-model/src/lib.rs` (doc/style reference)
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §1

Do **not** read the Java sources for this task.

## Work

1. Add these members to the workspace:

   ```text
   crates/hytale-protocol
   crates/hytale-assets
   crates/hytale-core
   crates/hytale-storage
   crates/hytale-world
   crates/hytale-entities
   crates/hytale-net
   crates/hytale-auth
   crates/hytale-commands
   crates/hytale-server
   ```

2. Create each crate with `version.workspace`, `edition.workspace`,
   `license.workspace`, and a one-paragraph `//!` module doc naming its
   responsibility. `hytale-server` is a binary; the rest are libraries.

3. Dependency direction (keep it acyclic):

   | Crate | Internal deps |
   |---|---|
   | `hytale-protocol` | none |
   | `hytale-core` | none |
   | `hytale-assets` | `hytale-protocol`, `hytale-core` |
   | `hytale-storage` | `hytale-core` |
   | `hytale-world` | `hytale-protocol`, `hytale-assets`, `hytale-storage`, `worldgen-model`, `worldgen-hytale`, `worldgen-server` |
   | `hytale-entities` | `hytale-world`, `hytale-core` |
   | `hytale-net` | `hytale-protocol`, `hytale-core` |
   | `hytale-auth` | `hytale-core` |
   | `hytale-commands` | `hytale-core` |
   | `hytale-server` | all of the above + existing worldgen crates |

4. Add only these workspace dependencies for now (other tasks add their own):

   ```toml
   bytes = "1"
   bevy_ecs = "0.16"
   bson = "2"
   quinn = "0.11"
   rcgen = "0.13"
   rustls = "0.23"
   tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time", "sync", "io-util"] }
   tracing = "0.1"
   tracing-subscriber = "0.3"
   zip = "2"
   zstd = "0.13"
   ```

   If a version does not resolve, use the current compatible release and note it
   in the handoff. Do not add unused dependencies to individual crates.

5. `hytale-server` `main.rs` prints `hytale-server <version>` and exits 0. No
   argument parsing yet.

6. Add `#![deny(clippy::all)]` to library crates only if the existing crates do;
   otherwise match their style.

## Out of scope

- No protocol parser, no QUIC, no asset code, no storage code.
- No changes to `crates/worldgen-*`.
- No `Cargo.lock` hand edits.

## Acceptance

```sh
cargo metadata --no-deps --format-version 1 >/dev/null
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p hytale-server
```

All green; `cargo run -p hytale-server` prints the version.

## Handoff

Report the crate list, dependency edges, any version substitutions, and the
commands run.
