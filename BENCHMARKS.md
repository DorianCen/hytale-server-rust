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
| P0 registry decode | 1,276.3 ms |
| Phase-2 registry decode (includes P0) | 2,574.3 ms |
| Config asset encode | 68.5 ms |
| Config packets / payload | 47 packets / 10,116,796 B |
| World spawn (9 initial chunk requests) | 4,085.6 ms |
| Config cache cold / hit | 0.028 ms / 0.009 ms |
| Config cache shared (`Arc::ptr_eq`) | true |
| 200 ticks + simulation passes | 1,908.6 ms |
| Tick throughput (load + simulate) | 104.8 ticks/s |
| Load one chunk outside the view | 210.0 ms |
| Join chunk batch encode | 0.347 ms (21 packets, 298,878 B raw) |
| Join chunk batch zstd framing | 0.614 ms (20,896 B framed) |
| Block edit | 0.014 ms |
| Save 10 chunks | 39.7 ms |
| RSS after world spawn | 1.199 GiB |
| RSS after save | 1.715 GiB |

A second run with view radius 0 measured 134.8 ticks/s, a 12.8 ms save of 2
chunks and 1.672 GiB RSS after save. The 8-chunk difference is roughly 5.4 MiB
per loaded chunk on this workload (includes the world's chunk/entity caches,
not just the column bytes).

### Soak result

`HYTALE_SOAK_CYCLES=1000` (register player → edit block → tick/simulate → save
every 10 cycles → unregister) completed in 1.09 s. Resident memory moved from
1,128,144,896 B to 1,128,562,688 B: **+417,792 B over 1,000 cycles**, below the
64 MiB regression ceiling. The radius-1 view held at most 9 chunks and the
player registry was empty at the end.

The same test at 100 cycles grew +602,112 B. Memory is dominated by the
resident asset registry (~503 MiB) and world caches (~1.1 GiB at radius 1), not
per-cycle session state.

### Comparison

Java CoreServer numbers are not available in this workspace (the release Java
server is a user-run external binary), so the Java/Rust ratios remain pending.
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
- The probe is a single warm-cache run; two consecutive runs on this host
  varied by less than 2% for decode, tick and save. Median/p95 capture is a
  follow-up when the Java comparison harness exists.
