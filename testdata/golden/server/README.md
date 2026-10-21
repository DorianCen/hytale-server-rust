# Server golden fixtures

Fixtures produced by the Java oracle tools for the CoreServer rewrite. They are
read-only inputs to Rust tests; regenerate with the tool named in each
subdirectory, never by hand.

| Directory | Producer | Status |
|---|---|---|
| `protocol/` | `tools/protocol-dumper` | session 1: inventory + connection IR |
| `assets/` | `tools/asset-golden-dumper` (T11) | planned |
| `chunks-pkts/` | Java chunk packet dumper (T12) | planned |
| `storage/` | Java world fixture (T11/T12) | planned; `hytale-storage` reads it automatically if present |
| `trace/` | `tools/packet-trace` | planned |

Keep fast fixtures committed. Full-pack runs belong behind an environment
variable or a separate script, never in the default `cargo test` path.
