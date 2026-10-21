# Porting history — CoreServer rewrite

Append-only log of landed sessions. Do not read this for planning; use
[`NEXT_STEPS.md`](NEXT_STEPS.md) for current work. Read this when debugging a
regression or verifying why a design choice was made.

The worldgen port history is archived in
[`done/PORTING_HISTORY.md`](done/PORTING_HISTORY.md).

Format:

```text
## Session N — <short title>
- Goal:
- Landed:
- Verified:
- Deviations:
- Follow-ups:
```

---

## Session 0 — planning and documentation (2026-10-21)

- **Goal:** define the CoreServer rewrite scope and prepare session 1.
- **Landed:**
  - `plan.md` — full rewrite plan, milestones M0–M7.
  - `ARCHITECTURE.md` — crate layout, runtime topology, invariants, mapping table.
  - `PORTING.md` — server porting conventions and subagent rules.
  - `REMAINING.md` — deferred scope.
  - `NEXT_STEPS.md` — session 1 must-do/stretch tasks.
  - `GOLDEN_DATA.md` — oracle/fixture strategy.
  - `BENCHMARKS.md` — planned measurement matrix.
  - `tasks/` — task specs T01–T14.
  - Moved the completed worldgen docs to `done/`; fixed links that pointed back
    into `testdata/` and `tools/`.
- **Verified:** no code changed; `git status` shows only documentation moves and
  additions.
- **Deviations:** none.
- **Follow-ups:** session 1 starts with T01, T02, T03, T05, T07, T08.

---

## Session 1 — workspace, protocol, wire, storage, assets (2026-10-21)

- **Goal:** workspace scaffolding, protocol inventory/IR, wire primitives,
  `IndexedStorageFile`, asset-store inventory.
- **Landed:**
  - T01: ten `hytale-*` crates wired into the workspace; `hytale-server` prints
    its version. No unused per-crate dependencies beyond the planned internal
    graph.
  - T02/T03: `tools/protocol-dumper` with `inventory` and `connection` modes.
    Checked-in fixtures in `testdata/golden/server/protocol/`.
  - T05: `hytale-protocol::io` (varint, little-endian primitives, big-endian
    UUIDs, bounded strings/arrays, nullable bitsets) and `::frame` (8-byte
    header, zstd with declared-size checks).
  - T07: `hytale-storage::indexed_storage_file` — v1 header, blob index table,
    first-fit segment allocation, zstd blobs, flush/remove/reopen.
  - T08: `tools/asset-golden-dumper/asset-stores.{json,md}` plus the
    deterministic `extract_stores.py` generator (54 stores, 39 packet
    generators, P0=8/P1=8/P2=21/P3=17).
  - Stretch T09: zip/directory asset VFS, `Parent` resolver with cycle/missing
    errors and per-store merge hooks, stable store topological order.
  - Stretch T10a: `Palette` (`Empty`/`HalfByte`/`Byte`/`Short`) with Java
    promotion thresholds, nibble packing, packet layout (LE), and storage layout
    (BE + key codec); demotion compacts internal ids.
- **Verified:**
  - `cargo test --workspace` green, including the existing worldgen goldens.
  - `cargo clippy --workspace --all-targets -- -D warnings` green.
  - `cargo fmt --all -- --check` green.
  - Protocol inventory parses 389 packets / 443 generated structs / 175 top-level enums;
    the 445/177 `ProtocolSettings` delta is documented in `meta.discrepancies`.
  - `Connect` IR matches the Java constants and field order; connection IR has
    8 packets and 5 enums.
- **Deviations:**
  - T02 classifies generated interfaces and constant/registry helpers as
    `other` instead of forcing them into `struct`; this is why the struct count
    is 443 rather than the declared 445.
  - The storage task sketch said little-endian, but the Java oracle is
    big-endian (`ByteBuffer` default). A real Java-produced fixture,
    `testdata/golden/server/storage/java-sample.region.bin`, is committed and
    the port/tests use big-endian; this was caught by the fixture, not by a Rust
    round-trip.
- **Follow-ups:**
  - T04 connection codegen is next; `connection.json` is its input. Nested
    `HostAddress`/`InstantData` are simple, while `FormattedMessage`/`PlayerSkin`
    need either recursive struct parsing or a curated common-struct set.
  - T06 QUIC transport spike is still open.
  - Session 2: T04, T09 asset stores from the inventory, T10b `BlockSection`,
    T10c `ChunkColumn::from_generated_chunk`, then T11/T12 goldens.
