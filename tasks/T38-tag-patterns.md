# T38 — TagPattern store and `UpdateTagPatterns`

**Status:** planned (session 25+)
**Package:** `hytale-assets`, `hytale-server`
**Depends on:** T35
**Agent fit:** medium

## Goal

Stream tag patterns and make pattern-expanded tags part of the same Rust-owned
tag registry as `ServerTags`.

## Deliverables

- `TagPattern` registry (`TagPatterns`, IndexedLookupTableAssetMap), including
  parent handling and deterministic id assignment.
- Java-compatible pattern expansion into tag indexes. Assets that use a pattern
  must see the expanded indexes in `BlockType`/`Item`/`Fluid`/`Environment`
  packets.
- `UpdateTagPatterns` encoder.
- `TagRegistry` must include pattern-defined tags and expose the same indexes to
  every packet and `ServerTags`.
- Golden test:
  - payload decodes and matches Java structure/semantics;
  - every emitted asset tag index resolves in `ServerTags`;
  - no index drift between `config_assets()` calls.

## Read first

- Java `TagPattern` / `TagPatternPacketGenerator` and tag-expansion code.
- `crates/hytale-assets/src/packets/mod.rs` (`TagRegistry`).
- `testdata/golden/server/assets/TagPattern.bin` from T35.

## Acceptance

```sh
cargo test -p hytale-assets
cargo test --workspace
cargo clippy -p hytale-assets --all-targets -- -D warnings
cargo fmt -p hytale-assets -- --check
```

- `UpdateTagPatterns` is part of the config packet set and sent before the
  block/item packets.
- All tag indexes resolve through `ServerTags`.
- P0 tests and the authenticated join stay green.

## Deviations

- Java’s global tag assignment order is still not reproduced; the Rust registry
  remains deterministic and self-consistent.
