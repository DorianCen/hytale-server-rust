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
| `player-data/` | `tools/java-dumper/run-player-data.sh` | session 10: real Java `EntityStore` player holder |
| `fluid/` | `tools/java-dumper/run-fluid.sh` | session 41: Java fluid ticker/hitbox coverage |
| `entity/` | `tools/java-dumper/run-entity-gameplay.sh` | session 44/45: real Java `EntityStatType`/`EntityEffect`/`DamageCause` ids/fields plus legacy `Projectile` and `ProjectileConfig` maps |
| `npc/` | `tools/java-dumper/run-npc.sh` | session 50: Java `SearchDirections`, steering-force outputs and view-cone results |
| `trace/` | `hytale-server serve --trace` (`hytale-net::trace`) | session 36: normalized v1 lifecycle packet ids/channels/order/hashes + real-client capture runbook |

Keep fast fixtures committed. Full-pack runs belong behind an environment
variable or a separate script, never in the default `cargo test` path.
