# hytale-server-rust

[![CI](https://github.com/DorianCen/hytale-server-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/DorianCen/hytale-server-rust/actions/workflows/ci.yml)

A from-scratch Rust reimplementation of the Hytale dedicated server
(`CoreServer`), aiming for **observable parity** with the Java server:

- loads unmodified Hytale asset packs;
- speaks the same wire protocol as the vanilla client (QUIC, same packet
  layouts, same framing and compression);
- reads and writes vanilla `IndexedStorage` saves;
- runs worlds, chunks, players, NPCs and gameplay systems in Rust.

It is not a wrapper around the Java server and does not ship or load Java
plugins. The project is an independent port, not affiliated with or endorsed by
Hypixel Studios.

---

## Status

The port is past the "can a client join?" stage. A real **Hytale 0.7.7 release
client** authenticates through the live Hytale OAuth/session service, receives
the full config burst, loads terrain, spawns on the surface, moves, sees the HUD,
breaks blocks, picks up drops and places blocks. Protocol compatibility is
proven, not assumed: the client and the Rust server use the same protocol CRC
(`-386561135`, `3908406161` unsigned) and build number (`258`).

T66 is complete (session 93). Sessions 71–93 closed the join/edit path through
surface spawn, movement, player model, stats, block/item interactions, the full
interaction variant audit, item icons, block groups, drops/pickup and
placement-chain reconciliation. Every encoded item icon resolves through the
streamed common-asset index, and the sole icon-less `Unknown` item matches
Java. Basic in-game commands are implemented (`help`, `tp`, `give`, `gamemode`,
`save`, `stop`, `who`, ...); [T67](tasks/T67-commands.md) tracks the remaining
console, permission, chat and advanced-command work.

### Milestones

| Milestone | Scope | Status |
|---|---|---|
| Vertical slice | Protocol codegen, transport, asset registry, world storage, chunk stream, authenticated join | Archived |
| M0 | Archive the slice and re-baseline the docs | Done |
| M2 | Asset foundation: P0/P1 stores, child stores, goldens, `--validate-assets` | Done |
| M3 | Runtime, persistent universe and world lifecycle | Done |
| M4 | v1 client join, multiplayer tracking and block edits | Done |
| M5 | Core gameplay parity: inventory, interactions, movement, fluids, stats, weather, projectiles, prefabs, UI basics | Done; T57b closed, residual presentation gaps deferred |
| M6 | Builtins and NPCs: AI/pathfinding, spawning, combat, encounters, modules | Done; T66 live-client interaction loop complete; T67 commands in progress |
| M7 | Security/fuzzing, performance/soak, packaging, plugin and mod API | Done |

### Implemented and covered by tests

- **Join and world load** — live auth, setup handshake, common assets,
  translations, the 47-packet config burst, `JoinWorld`, paced chunk snapshot,
  `ClientReady` and `ClientTeleport`.
- **Players** — surface spawn resolution, first-person model (camera, hitbox,
  animation sets, attachments), equipment/skin, stats, effects, movement
  states, HUD, other-player tracking and reconnect.
- **World** — generated and saved chunks, Java-ordered chunk batches, block
  edit/place/break handling, block health, fluids, ticks, support rules, time
  and weather, prefab/runtime entities, world-map images and containers.
- **Gameplay** — inventory and container transactions, the interaction graph,
  block interactions, projectiles/traversal core, NPC roles, spawning,
  pathfinding, steering, combat and encounters, portals, and the builtin module
  runtime.
- **Persistence** — Java-compatible world `config.json`, chunk region files and
  player documents, verified by Java read-back gates.
- **Tooling** — a scripted `client-bot`, protocol/asset dumpers, Java golden
  comparison, test tiers, security/perf gates and reproducible release archives.

At the time of writing, the local gates are **1,577 fast-tier tests**, **22
block/item golden tests**, a clean
`cargo clippy --workspace --all-targets -- -D warnings`, and
`--validate-assets` reporting **0 failures**.

---

## Caveats

- **Assets are required and not included.** The server needs a Hytale asset
  pack (`Assets.zip` or an extracted tree) from a Hytale installation you own.
  The pack is several GB and is git-ignored.
- **Live-client parity is verified for the block loop.** Join, spawn, movement,
  block breaking, item drops, pickup and block placement are confirmed against
  the release client (T66, session 93). Remaining close-out work is placement
  fidelity, long-tail interaction payloads, multi-player/persistence smoke and
  the T67 command follow-ups.
- **Commands are basic.** `ChatMessage` command lines and `CommandTreeSync` now
  work for `help`, `tp`, `give`, `gamemode`, `save`, `stop`, `whereami`,
  `whoami`, `who`, `kill`, `respawn`, `time` and `version`. Console stdin, real
  permissions/op groups, plain-chat broadcast, player-name targets and the
  advanced command set remain under [T67](tasks/T67-commands.md).
- **One stored player identity.** The join currently uses the world's stored
  player record rather than necessarily the authenticated UUID. Per-join
  identity and the full Java `SpawnProvider` behavior remain future work.
- **No Java plugins.** The plugin API is Rust-native and compile-time linked
  (`hytale_builtin::BuiltinModule`); there is no classloader, jar loading or
  dynamic-library ABI.
- **Asset mods are merged, not stacked.** v1 loads one pack. Overlay mods into a
  directory pack before starting the server; runtime overlay stacking is not
  implemented.
- **Gameplay depth is partial by design.** Core systems exist, but many edge
  cases and full UI flows are deferred. [`REMAINING.md`](REMAINING.md) is the
  authoritative list.
- **Protocol versions are pinned.** The generated codecs and constants match
  Hytale build `258`. A different client build requires regenerating the
  protocol, not editing constants by hand.
- **Goldens were produced from the 0.7.2 asset pack.** The 0.7.5 protocol is
  identical, but refreshing `Assets/Assets.zip` to 0.7.5 content is still
  recommended; see [`JAVA_VERSION_DIFF.md`](JAVA_VERSION_DIFF.md).

---

## Quick start

### Prerequisites

- Linux, macOS or Windows with a stable Rust toolchain and a C toolchain.
- The Hytale asset pack from your installation (not distributed here).
- A Hytale account if you want a release client to join.

### 1. Assets

Place the pack at the default paths:

```text
Assets/Assets.zip       # zip pack used for asset decoding
Assets/extracted/       # extracted tree used for prefabs, environments, gradients
```

For example:

```sh
mkdir -p Assets
cp /path/to/Hytale/Assets.zip Assets/Assets.zip
unzip -q Assets/Assets.zip -d Assets/extracted
```

`--assets-pack` and `--assets-root` can point elsewhere. A directory pack can
serve as both.

### 2. Build and validate

```sh
cargo build --release --locked -p hytale-server -p client-bot
./target/release/hytale-server --validate-assets
```

Validation checks every enabled asset store and the encoded P0 config burst.

### 3. Create a world

```sh
./target/release/hytale-server universe --root target/universe create RealTest
./target/release/hytale-server universe --root target/universe default RealTest
```

`serve` also creates a `default` world automatically when the universe is
empty.

### 4. Run the server

For a release client, use live authentication:

```sh
./target/release/hytale-server serve \
  --universe target/universe \
  --world RealTest \
  --auth-mode authenticated \
  --trace target/trace/RealTest.jsonl
```

The server prints a device-flow URL and code on first use:

```text
To authenticate this server, open https://oauth.accounts.hytale.com/oauth2/device/verify?user_code=...
and enter code ...
```

Authorize it once; the session is cached at
`~/.config/hytale-server-rust/auth.json` (or under `$XDG_CONFIG_HOME`; override
the location with `HYTALE_AUTH_CACHE`). To skip the device flow, provide
`HYTALE_SERVER_SESSION_TOKEN` and `HYTALE_SERVER_IDENTITY_TOKEN` in the
environment.

The default bind is `127.0.0.1:5520` over **UDP**. For a client on another
machine, bind a reachable address and open UDP 5520:

```sh
./target/release/hytale-server serve \
  --universe target/universe \
  --world RealTest \
  --auth-mode authenticated \
  --bind 0.0.0.0:5520
```

### 5. Connect

Launch the Hytale client normally, add the server address
`127.0.0.1:5520` (or your host and port), and join. The server must be running
in `authenticated` mode for the release client.

### Authentication modes

| Mode | Use case | Behavior |
|---|---|---|
| `insecure` | Local bots, tests, protocol work | No authentication; not for release clients |
| `local` | Offline development and `client-bot --auth-join` | Ed25519 JWT authority; prints a generated key |
| `authenticated` | Real release clients | Live Hytale OAuth device flow and session service |

### Without the game client

`client-bot` is a scripted client for development:

```sh
# Terminal 1 — dev/insecure join demo
./target/release/hytale-server chunk-serve --handshake \
  --save target/dev-join-smoke --bind 127.0.0.1:5523 --view-radius 0

# Terminal 2
./target/release/client-bot --host 127.0.0.1:5523 --dev-join \
  --view-radius 0 --timeout 60
```

See [`tools/client-bot/README.md`](tools/client-bot/README.md) for the chunk
stream, dev join and authenticated join scenarios.

---

## Development

### Tests

`cargo test --workspace` is the correctness gate; use the tiers for the inner
loop:

```sh
./tools/test.sh unit          # in-crate unit tests, no asset pack
./tools/test.sh fast          # unit + non-asset integration tests
./tools/test.sh integration   # all non-E2E integration tests
./tools/test.sh assets        # asset/golden packages
./tools/test.sh slow          # worldgen and world-boot suites
./tools/test.sh e2e           # ignored loopback/auth/trace tests, serial
./tools/test.sh all           # full workspace gate
```

CI runs formatting, clippy and the `fast` tier. The asset/golden and E2E tiers
need the pack and loopback UDP. Details and measured timings are in
[`TESTING.md`](TESTING.md).

Other gates:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
HYTALE_FUZZ_ITERATIONS=100000 ./tools/security.sh
tools/perf.sh
tools/release.sh
```

`tools/release.sh` writes a deterministic archive and SHA-256 checksum under
`dist/`; see [`PACKAGING.md`](PACKAGING.md).

### Porting rules

This is a parity port, not a clean-room redesign:

- match Java behavior and packet bytes, citing the Java source in
  `// Ported from:` comments;
- do not invent expected bytes — every port needs a Java golden, a Java test
  assertion or a captured trace;
- keep numeric and semantic fidelity (Java `double` → `f64`, overflow
  semantics, iteration order);
- never copy Java sources into the repository.

Read [`PORTING.md`](PORTING.md) before contributing code and
[`GOLDEN_DATA.md`](GOLDEN_DATA.md) for the oracle layout.

---

## Project layout

| Path | Contents |
|---|---|
| `crates/hytale-protocol` | Generated packet/struct codecs, framing, protocol constants |
| `crates/hytale-assets` | Asset registry, deterministic ids, golden-verified packet encoders |
| `crates/hytale-net` | QUIC transport, framing, packet trace |
| `crates/hytale-auth` | Insecure, local EdDSA and live OAuth/session authentication |
| `crates/hytale-core` | Config, logging, scheduler, event bus, module runtime |
| `crates/hytale-world` / `hytale-storage` | Worlds, chunks, generation hooks, Java-compatible saves |
| `crates/hytale-server` | `hytale-server` binary, sessions, world threads, chunk streaming |
| `crates/hytale-entities` / `hytale-inventory` / `hytale-interactions` | Gameplay models and systems |
| `crates/hytale-npc` / `hytale-builtin` / `hytale-commands` | NPC runtime, builtin modules, command framework |
| `crates/worldgen-*` / `procedural` | World generation engine and CLI |
| `tools/` | `client-bot`, protocol/asset dumpers, test/security/perf/release scripts |
| `testdata/golden/` | Java-produced packet and save goldens |
| `deploy/` | Hardened systemd unit and deployment layout |

---

## Documentation

| Document | Purpose |
|---|---|
| [`plan.md`](plan.md) | Milestones, tasks and acceptance criteria |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crate layout, runtime topology, Java→Rust mapping |
| [`NEXT_STEPS.md`](NEXT_STEPS.md) | Current work and known gaps |
| [`REMAINING.md`](REMAINING.md) | Deferred scope (what v1 deliberately does not do) |
| [`PORTING.md`](PORTING.md) / [`PORTING_HISTORY.md`](PORTING_HISTORY.md) | Porting conventions and session history |
| [`TESTING.md`](TESTING.md) | Test tiers, timings, E2E and CI |
| [`PARITY_AUDIT.md`](PARITY_AUDIT.md) | Proposed behavior-driven Java↔Rust parity audit program |
| [`CONNECTION_DEBUG.md`](CONNECTION_DEBUG.md) | Live release-client diagnosis log and evidence |
| [`GOLDEN_DATA.md`](GOLDEN_DATA.md) | Java oracle and golden fixture layout |
| [`BENCHMARKS.md`](BENCHMARKS.md) | Performance and memory baselines |
| [`PACKAGING.md`](PACKAGING.md) | Release artifacts and deployment |
| [`PLUGINS.md`](PLUGINS.md) | Rust-native plugin API |
| [`ASSET_MODS.md`](ASSET_MODS.md) | Asset-only mod workflow |
| [`JAVA_VERSION_DIFF.md`](JAVA_VERSION_DIFF.md) | 0.7.2 → 0.7.5 protocol and asset comparison |
| [`tasks/README.md`](tasks/README.md) | Task board and per-task work packets |
| [`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md) | Archived first vertical slice |

---

## Legal

The Rust source in this repository is licensed under the MIT license, as
declared in [`Cargo.toml`](Cargo.toml). Hytale assets, the Hytale client and the
Hytale name are the property of Hypixel Studios; they are not covered by that
license and are not distributed with this project. You must own a Hytale
installation to use the asset pack.
