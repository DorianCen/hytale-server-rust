# Benchmarks — CoreServer rewrite

No Rust CoreServer measurements exist yet. This file defines what to measure so
the first numbers are comparable and not cherry-picked. The worldgen baseline is
archived in [`done/BENCHMARKS.md`](done/BENCHMARKS.md).

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
| Chunk storage encode/decode | new bench in `hytale-storage` | section blobs + zstd |
| Region read/write | new bench in `hytale-storage` | 1024-blob file |

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
