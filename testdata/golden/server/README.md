# Server golden fixtures

Fixtures produced by the Java oracle tools for the CoreServer rewrite. They are
read-only inputs to Rust tests; regenerate with the tool named in each
subdirectory, never by hand.

| Directory | Producer | Status |
|---|---|---|
| `protocol/` | `tools/protocol-dumper` + `tools/protocol-gen` | session 3: inventory, connection IR, full `protocol.json` + generated Rust |
| `assets/` | `tools/java-dumper/run-assets.sh` | session 2: P0 store init packets + index |
| `chunks-pkts/` | `tools/java-dumper/run-chunks.sh` | session 3: seed-0 `SetChunk`/`SetColumn`/tint/environment/fluid payloads |
| `storage/` | `tools/java-dumper/run-storage.sh` | session 6: real Java world region files + BSON chunk documents; `hytale-storage` reads the container sample automatically |
| `world-config/` | `tools/java-dumper/run-world-config.sh` | session 9: real Java `WorldConfig` v4 document |
| `trace/` | `tools/packet-trace` | planned |

Keep fast fixtures committed. Full-pack runs belong behind an environment
variable or a separate script, never in the default `cargo test` path.
