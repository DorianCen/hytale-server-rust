# Server golden fixtures

Fixtures produced by the Java oracle tools for the CoreServer rewrite. They are
read-only inputs to Rust tests; regenerate with the tool named in each
subdirectory, never by hand.

| Directory | Producer | Status |
|---|---|---|
| `protocol/` | `tools/protocol-dumper` + `tools/protocol-gen` | session 3: inventory, connection IR, full `protocol.json` + generated Rust |
| `assets/` | `tools/java-dumper/run-assets.sh` | session 2: P0 store init packets + index |
| `chunks-pkts/` | `tools/java-dumper/run-chunks.sh` | session 3: seed-0 `SetChunk`/`SetColumn`/tint/environment/fluid payloads |
| `storage/` | Java world fixture (T11/T12) | planned; `hytale-storage` reads it automatically if present |
| `trace/` | `tools/packet-trace` | planned |

Keep fast fixtures committed. Full-pack runs belong behind an environment
variable or a separate script, never in the default `cargo test` path.
