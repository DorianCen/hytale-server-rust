# Benchmarks — CoreServer rewrite

The T63 section at the end is the first Rust CoreServer baseline. This file
also defines what to measure so later numbers stay comparable and are not
cherry-picked. The worldgen baseline is archived in
[`done/BENCHMARKS.md`](done/world-generation/BENCHMARKS.md).

---

## 1. Measurement rules

- Use release builds (`cargo build --release`, `--profile bench` where relevant).
- Report hardware (CPU, RAM, disk), OS, Rust version, and the exact command.
- Warm up caches where the workload is steady-state; say so.
- Record medians and p95, not just the best run.
- Benchmark the same asset pack and world seed for Java and Rust comparisons.
- Never put a 3.7 GB asset boot in the default test suite.

---

## 2. Planned measurements

### Boot

| Metric | Command | Notes |
|---|---|---|
| Asset pack index/scan | `hytale-server --validate-assets` | zip VFS + store discovery |
| P0 asset decode | `hytale-server --validate-assets` | block/fluid/environment/item |
| Full asset decode | `hytale-server --validate-assets` | all 54 stores |
| Asset packet pre-encode | `hytale-server --validate-assets` | cached init packets |
| World load | `hytale-server --world <name> --dry-run` | config + region scan |
| Time to ready | `hytale-server` | listener + universe ready |

### World generation

Reuse the existing worldgen benches and add the server conversion:

| Metric | Command | Notes |
|---|---|---|
| Chunk generation | `worldgen-cli bench` | existing baseline |
| Generated → runtime chunk | new bench in `hytale-world` | `from_generated_chunk` |
| Chunk packet encode | new bench in `hytale-world` | `SetChunk`/`SetColumn` |
| Chunk storage encode/decode | new bench in `hytale-world` (`chunk::storage`) | section blobs + zstd |
| Region read/write | new bench in `hytale-world` (`chunk::storage::RegionFile`) | 1024-blob file |

### Tick and network

| Metric | Command | Notes |
|---|---|---|
| Empty world tick | `hytale-server --bench-tick` | target: stable 30/60 TPS |
| 100 loaded chunks tick | synthetic world | chunk store + entity store |
| 1 player join | `client-bot join` | handshake + assets + spawn |
| 10/50/100 players | `client-bot load` | packet throughput, memory |
| Chunk stream throughput | `client-bot load` | chunks/s, bytes/s |
| Block edit round-trip | `client-bot edit` | client → world → clients |

### Memory

- Idle empty world.
- One player + view radius 6.
- 100 players + view radius 6.
- Full asset pack loaded (P0 vs all stores).
- Chunk cache at steady state.

---

## 3. Targets

These are acceptance guardrails, not promises; adjust after the first baseline.

- Boot with the default pack: no worse than the Java server on the same machine
  (measure both, report the ratio).
- Chunk generation: existing worldgen baseline must not regress.
- Generated → runtime conversion: small relative to generation.
- Tick: no unbounded allocations or lock contention in a 1-hour soak.
- Join: asset streaming must not block the world thread.
- Memory: document the per-chunk and per-player cost; no leak over 24h.

---

## 4. Reporting

Append a section here for each measured milestone:

```text
## <milestone> — <date>
Hardware:
Build:
Command:
Result:
Comparison:
Notes:
```

Do not compare numbers from different asset packs, view radii, or build
profiles.

## T63 — Rust CoreServer baseline — 2026-10-31

Hardware: AMD Ryzen 7 9800X3D (8 cores / 16 threads), 64 GiB RAM, Linux
`7.3.1-1-cachyos` x86_64.

Build: `rustc 1.99.1`, `cargo 1.99.1`, release build with the committed
`Assets/Assets.zip`, default seed, view radius 1 (9 chunks) and 4
chunks/world-tick. Commands:

```sh
tools/perf.sh
HYTALE_SOAK_CYCLES=1000 cargo test --release -p hytale-server --test soak -- --ignored --nocapture
```

### Probe results (`target/release/examples/perf`)

| Metric | Result |
|---|---:|
| Asset pack open | 0.002 ms |
| P0 registry decode | 1,271.4 ms |
| Phase-2 registry decode (includes P0) | 2,541.0 ms |
| Config asset encode | 68.4 ms |
| Config packets / payload | 47 packets / 10,116,796 B |
| World spawn (9 initial chunk requests) | 4,071.7 ms |
| Config cache cold / hit | 0.036 ms / 0.006 ms |
| Config cache shared (`Arc::ptr_eq`) | true |
| 200 ticks + simulation passes | 1,907.5 ms |
| Tick throughput (load + simulate) | 104.8 ticks/s |
| Load one chunk outside the view | 209.8 ms |
| Join chunk batch encode | 0.385 ms (21 packets, 298,878 B raw) |
| Join chunk batch zstd framing | 0.648 ms (20,896 B framed) |
| Block edit | 0.024 ms |
| Save 10 chunks | 48.7 ms |
| RSS after world spawn | 1.111 GiB |
| RSS after save | 1.592 GiB |

A second run with view radius 0 measured 134.8 ticks/s, a 12.8 ms save of 2
chunks and 1.557 GiB RSS after save. The 8-chunk difference is roughly 4.5 MiB
per loaded chunk on this workload (includes the world's chunk/entity caches,
not just the column bytes).

### Soak result

`HYTALE_SOAK_CYCLES=1000` (register player → edit block → tick/simulate → save
every 10 cycles → unregister) completed in 1.09 s. Resident memory moved from
1,128,251,392 B to 1,128,673,280 B: **+421,888 B over 1,000 cycles**, below the
64 MiB regression ceiling. The radius-1 view held at most 9 chunks and the
player registry was empty at the end.

The same test at 100 cycles grew +602,112 B. Memory is dominated by the
resident asset registry (~503 MiB) and world caches (~1.1 GiB at radius 1), not
per-cycle session state.

### Java comparison

Java: HytaleServer `0.0.0-dev` revision `387dc7e0aa08a887d3e013de7ec1752b8ae9e53e`,
Temurin JDK 25.0.4, same host. Reproduce with `tools/java-baseline.sh`; raw
logs are under `target/perf/java-*`.

| Probe | Rust | Java | Notes |
|---|---:|---:|---|
| Asset validation | 9.92 s / 1.34 GiB peak RSS | 42.9–43.6 s / 4.83–6.00 GiB peak RSS | Rust validates 27 stores and encodes 47 config packets twice; Java validates every loaded store (100+ types, including interactions/NPC/model assets). Not equal scope; the Java JVM heap peak varies between runs. |
| Boot to ready (zip pack) | `WorldThread::spawn` 4.07 s / 1.11 GiB RSS | 4.09–4.10 s to `Hytale Server Booted`; 4.73–4.77 s process / 2.78–3.44 GiB peak RSS | Java boot includes all plugins and a fresh universe; Rust has the implemented world systems only, radius-1 view and 4 chunks/tick. |

The Java validation path also reported the expected `Plugin.Instance.Discovery`
deprecation warnings and exited 0. The Rust validation was run against the same
extracted `HytaleAssets` tree and now passes with the current 47-packet config
burst.

These numbers should be compared only against the same pack, seed, hardware and
view radius. No worldgen benchmark regressed: the existing worldgen criterion
targets are unchanged.

### Notes

- `world.tick_per_second` includes one `simulate(0.05)` pass per tick and is a
  batch throughput guardrail, not a wall-clock TPS measurement.
- The config cache is encoded once per world and shared by sessions; cold/hit
  numbers are for the same `Arc<ConfigAssets>`.
- The soak harness is `crates/hytale-server/tests/soak.rs`; the probe is
  `crates/hytale-server/examples/perf.rs`; both are excluded from the default
  `fast` tier.
- The probe is a single warm-cache run; consecutive runs on this host varied by
  less than 2% for decode, tick and save. Median/p95 capture is a follow-up.
- Rust `WorldThread::spawn` loads the phase-2 registry itself; the probe also
  measures P0/phase-2 decode separately for reporting, so the probe's total
  wall time is not the boot time. Use the `world.spawn` row for boot.

## T72 — Concurrent player load — 2026-11-11

Hardware: AMD Ryzen 7 9800X3D (8 cores / 16 threads), 60 GiB usable RAM,
Linux `7.3.3-1-cachyos` x86_64.

Build: `rustc 1.99.1`, release build with the committed `Assets/Assets.zip`,
embedded loopback listener, default view radius 1 and 4 chunks/world-tick.
Join latency is QUIC connect + setup + world-join snapshot; it excludes the
post-join observation window. `peak` is the highest concurrent session count
observed by the listener. RSS is the process hosting the listener.

```sh
cargo build --release -p client-bot
./target/release/client-bot --load --players 10  --load-window 3  --report target/perf/load-release-10.json
./target/release/client-bot --load --players 50  --stagger-ms 100 --load-window 15 --report target/perf/load-release-50.json
./target/release/client-bot --load --players 100 --stagger-ms 200 --load-window 45 --report target/perf/load-release-100.json
```

| Players | Joined | Join p50 | Join p95 | Join max | Peak | Accepted | Rejected | Teardown | Chunks | Packets | Bytes | Edits | Sim ticks | Tick/s | RSS | Peak RSS |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 10 | 10 | 813.1 ms | 913.8 ms | 913.8 ms | 10 | 10 | 0 | 0 | 90 | 2,340 | 25.4 MiB | 20 | 415 | 103.3 | 1,634.7 MiB | 1,634.7 MiB |
| 50 | 50 | 753.6 ms | 1,333.8 ms | 1,496.0 ms | 49 | 50 | 0 | 1 | 441 | 11,466 | 124.4 MiB | 98 | 12,125 | 583.4 | 2,416.5 MiB | 2,416.5 MiB |
| 100 | 100 | 513.9 ms | 791.4 ms | 1,257.5 ms | 99 | 100 | 0 | 0 | 900 | 23,399 | 253.9 MiB | 200 | 74,934 | 1,145.7 | 3,181.5 MiB | 3,181.5 MiB |

Notes:

- All runs joined every bot with zero rejected connections, zero duplicate
  identities, zero queue overflows, zero rate-limit disconnects and zero idle
  timeouts. `Teardown` counts sessions whose client closed while the server was
  still writing (`STOP_SENDING`), which the harness classifies separately from
  unexpected session errors.
- At 100 players the join ramp needs a wider stagger and observation window;
  with the default 25 ms stagger the asset/config burst produced a long tail and
  early bots left before the last joined. `--stagger-ms 200 --load-window 45`
  keeps 99 sessions concurrent at peak. The 50-player run peaked at 49.
- `Tick/s` is the sum of every session's `simulation_ticks` divided by wall
  time. Each session currently drives the shared world tick, so it measures
  aggregate simulation load rather than an independent world TPS.
- The ignored debug gate (`cargo test -p client-bot --test load -- --ignored`)
  is a correctness check, not a baseline: debug worldgen/asset decode makes the
  10-bot join latency much larger. It asserts full joins, zero rejected/queued
  overflow, chunk counters, block edits, ticks and RSS.

## T73 — Universe restart procedure — 2026-11-15

A listener-owned universe is a directory:

```text
<universe-root>/
  universe.json              # default_world + world UUID -> provider map
  <WorldName>/config.json    # per-world seed/display name/provider config
  <WorldName>/players/*.json # complete player documents
  <WorldName>/chunks/        # region files flushed on graceful shutdown
  Portals_Portals_Goblins/   # portal instance worlds use sanitized names
```

Restart procedure used by `client-bot/tests/universe_restart.rs`:

```sh
# Start the primary and any world sessions may switch to.
hytale-server serve --universe <root> --world Default \
  --prestart-world Portals_Portals_Goblins --bind 127.0.0.1:5520

# Send /stop (or Ctrl-C), then run the same command again. The listener
# reopens universe.json, reloads the same world directories and player files,
# and clients can rejoin Default or use `/world <name>` for the second world.
```

- A graceful shutdown waits for sessions, closes connections, saves every
  running world through `WorldThread::shutdown` and drops the listener's
  `PlayerRegistry`/`WorldThread` handles; a restart starts from clean state.
- `--prestart-world` avoids paying the first-start/worldgen cost inside a
  session. Without it `/world <name>` starts the catalogued world on demand.
- Portal transfer/return rewrites the player document in the target world and
  deletes the source copy; the returned document, transform, inventory and
  chosen world are what persist. Portal return points and entry counts are
  in-memory session state (same as Java's live instance bookkeeping).
- The verified test uses a fresh ephemeral port for the second listener; the
  universe root, world names/UUIDs, `universe.json` and save directories are
  unchanged.

## Surface-only generation mode — 2026-11-22

Hardware: same dev box as the T63/T72 rows.
Build: release, default features.
Command:

```sh
cargo run --release -p worldgen-cli --bin surface_only_probe -- testdata/Default 0
```

Result (64 chunks at the origin, one generator, serial, seed 0):

| Mode | Region (s) | Per chunk (s) |
|---|---:|---:|
| Full pipeline | 20.6–21.2 | 0.32–0.33 |
| `surface_only` | 3.82–3.84 | 0.060 |

Comparison: **5.4× on the cold run and 5.5× on an immediate warm rerun**
(verified in both orders — the delta is the mode, not cache warmth). The
full pipeline's own numbers match the T04/T63 chunk-path costs.

Notes:
- The mode skips the cave and prefab populators (per-request
  `WorldGenRequest::surface_only`, default false). Every skipped stage
  runs its own re-seeded stream, so the remaining voxels are
  bit-identical to the full pipeline outside carved/placed space; the
  golden parity test stays byte-identical and no `GENERATOR_VERSION` bump
  applies.
- Surface fidelity on Default seed 0 chunk (0,0): 847/1024 top-block
  columns match the full pipeline and 1013/1024 top-fluid columns; every
  differing column is the chunk's biome prefab (pinned by
  `tests/surface_only.rs`).
- Consumers: map-summary/placeholder queries only (the explorer's coarse
  fill passes). `WorldGenStats` reports zero for the skipped stages —
  averages across mixed modes must not read that as "caves became free".
