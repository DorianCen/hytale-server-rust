# Performance baseline

Release-mode numbers recorded on the development machine. This is **not** a
reference-machine SLO — it is a reproducible baseline for spotting regressions
and for sizing the worker pool. Always benchmark `--release`: debug builds are
roughly an order of magnitude slower on prefab-heavy chunks.

## Machine / toolchain

| | |
|---|---|
| CPU | AMD Ryzen 7 9800X3D (8 cores / 16 threads) |
| RAM | 60 GiB |
| OS | Linux 7.2.10 (CachyOS) |
| Rust | rustc 1.97.1, cargo 1.97.1 |
| Profile | `--release` (workspace default) |
| World | `testdata/Default`, seed `0`, start chunk `(0,0)` |
| Assets | `Assets/extracted` (the full base-game pack) |

## Worker-pool throughput (`worldgen-cli bench`)

The grid is a square starting at `(0,0)`; `--workers` defaults to
`max(2, ceil(cores × 0.75))` = 12 on this machine.

| Chunks | Workers | Wall (s) | Chunks/s | Avg total (s) | caves (s) | prefabs (s) |
|---:|---:|---:|---:|---:|---:|---:|
| 16 | 12 | 18.04 | 0.89 | 1.119 | 0.139 | 0.969 |
| 16 | 1 | 2.01 | 7.97 | 0.121 | 0.024 | 0.088 |
| 144 | 1 | 51.20 | 2.81 | 0.324 | 0.265 | 0.040 |
| 144 | 12 | 22.92 | 6.28 | 0.154 | 0.032 | 0.112 |
| 576 | 12 | 68.68 | 8.39 | 0.109 | 0.057 | 0.040 |

**Reading the table**

- **Small workloads are cold-cache workloads.** Every worker owns its
  `PrefabStore`, cave cache, unique-prefab cache and core-data cache. With
  16 chunks on 12 workers each worker generates ~1 chunk, so it pays the full
  per-worker prefab/cave warm-up; that is why 12 workers are *slower* than 1
  worker at this size (18.0 s vs 2.0 s).
- Size the benchmark (and the server's warm-up) at **≥ 32 chunks per worker**.
  At 144 chunks the pool is ~2.2× faster than one worker; at 576 chunks it
  reaches 8.4 chunks/s (vs 2.8 chunks/s for one worker on 144 chunks).
- **Prefabs and caves dominate**: on the steady-state 576-chunk run they are
  0.040 s + 0.057 s of the 0.109 s average (the rest is tint/environment/
  blocks/water ≈ 0.012 s). Core-data tint/environment are ~0.0026 s.
- Scaling is below the 8 physical cores (12 workers → ~3× single-worker
  throughput at equal chunk counts) because of the cold per-worker caches and
  SMT; the exact ratio is workload-dependent.

Reproduce:

```sh
cargo build --release --workspace
./target/release/worldgen-cli bench --world testdata/Default --seed 0 --start 0,0 --chunks 576
./target/release/worldgen-cli bench --world testdata/Default --seed 0 --start 0,0 --chunks 144 --workers 1
```

## Criterion micro-benchmarks (`cargo bench -p worldgen-hytale`)

| Benchmark | Median |
|---|---:|
| `prefab_weights/random_lookup` | 1.282 ns |
| `prefab_weights/weighted_lookup_03` | 10.85 ns |
| `prefab_weights/weighted_lookup_05` | 11.52 ns |
| `prefab_weights/weighted_lookup_10` | 13.48 ns |
| `cache/sized_timeout_get_hit` (moka wrapper) | 91.68 ns |
| `cache/hash_map_get_hit` (baseline) | 6.256 ns |
| `cache/hash_map_get_or_insert` (baseline) | 6.209 ns |

The ported moka-backed sized/TTI cache is ~15× the raw `HashMap` lookup; the
world generator's core-data cache is a private twin with the same semantics,
so this is the expected order of magnitude for cached column lookups.

Reproduce:

```sh
cargo bench -p worldgen-hytale
```

## Notes for tuning

- The worker count is fixed at pool creation (`WorldGenOptions::workers`); the
  CLI `--workers` and `default_worker_count()` are the two knobs.
- Each worker loads the world and keeps its own caches (see
  `ARCHITECTURE.md` §5, "Shared caches"); the numbers above include that
  design, not a shared-cache alternative.
- Re-measure after touching the prefab paste path, the cave caches or the
  `moka` wrapper; those dominate the per-chunk time.
