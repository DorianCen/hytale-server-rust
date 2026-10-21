# Next steps — CoreServer rewrite, session 1

**Status:** the rewrite plan is frozen in [`plan.md`](plan.md). Session 1 has
landed: workspace scaffolding, protocol inventory/IR, wire primitives, the
`IndexedStorageFile` container, asset-store inventory, asset pack/`Parent`
resolution, and section palettes. See [`PORTING_HISTORY.md`](PORTING_HISTORY.md).
The completed worldgen port is archived under [`done/`](done/) and its
regression gate must stay green. The session-1 sections below remain as the
record of what was planned; use [`tasks/README.md`](tasks/README.md) for the
current status board.

**Session 1 goal:** stand up the workspace skeleton and prove the two highest-risk
foundations — **protocol codegen** and **Java-compatible storage** — on a small,
testable slice. Do not try to boot a server or load the full asset pack in session 1.

**Read first (in this order):**

1. [`plan.md`](plan.md) — scope, v1 contract, milestones.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate layout, runtime topology, invariants.
3. [`PORTING.md`](PORTING.md) — rules for porting and for subagent tasks.
4. [`GOLDEN_DATA.md`](GOLDEN_DATA.md) — the Java oracle and fixture layout.
5. [`tasks/README.md`](tasks/README.md) — task format and ownership rules.

The archived worldgen docs are only needed when touching the existing crates:
[`done/ARCHITECTURE.md`](done/ARCHITECTURE.md),
[`done/PORTING.md`](done/PORTING.md),
[`done/REMAINING.md`](done/REMAINING.md).

---

## Session 1 must-do tasks

| Task | Package | Deliverable | Agent fit |
|---|---|---|---|
| [T01](tasks/T01-workspace-scaffolding.md) | workspace | ten new crates wired into `Cargo.toml`; `cargo check/test/fmt/clippy` green | **high** — one file set, no Java |
| [T02](tasks/T02-protocol-inventory.md) | `tools/protocol-dumper` | `inventory.json`: every protocol class classified; packet ids/channels extracted; counts checked | **high** — script over generated sources |
| [T03](tasks/T03-connection-packet-ir.md) | `tools/protocol-dumper` | `connection.json` IR for the 13 `packets/connection` files, validated against `Connect` constants | **medium** — strict generated-source grammar |
| [T05](tasks/T05-wire-primitives.md) | `crates/hytale-protocol` | varint, fixed ints, UUID, nullable bitset, string/array helpers, framing, zstd; unit tests | **high** — isolated, known vectors |
| [T07](tasks/T07-indexed-storage-reader.md) | `crates/hytale-storage` | `IndexedStorageFile` read/write/remove/flush matching the Java header and blob layout | **medium** — one 1.1k-line Java file |
| [T08](tasks/T08-asset-store-inventory.md) | `tools/asset-golden-dumper` | `asset-stores.md` table: 54 stores, paths, codecs, map types, packet generators, P0–P3 priority | **high** — extraction from one Java file |

### Suggested parallel split

T01 must land first because it owns the workspace manifest. After that:

- **Track A — protocol:** T02 → T03 (same tool/crate; serialize these).
- **Track B — wire:** T05 (independent crate; can start with T01).
- **Track C — storage:** T07 (independent crate; can start with T01).
- **Track D — assets:** T08 (documentation/JSON only; can start with T01).

T04 starts after T03 and T05. Do not run two tasks that edit
`tools/protocol-dumper` at the same time; T02 and T03 share that crate.

## Session 1 stretch tasks (only if must-do is green)

| Task | Package | Deliverable | Agent fit |
|---|---|---|---|
| [T04](tasks/T04-connection-codegen.md) | `tools/protocol-gen` + `hytale-protocol` | generated connection packet types; `Connect`/`Ping`/`Pong`/`Disconnect` round-trip tests | **medium** — depends on T03 |
| [T06](tasks/T06-quic-transport-spike.md) | `crates/hytale-net` | quinn server + client with ALPN `hytale/3`, mTLS, client-cert capture | **low/medium** — needs stronger agent |
| [T09](tasks/T09-asset-parent-resolver.md) | `crates/hytale-assets` | zip/dir pack VFS + `Parent` merge resolver with cycle detection; fixture tests | **medium** — self-contained |
| [T10a](tasks/T10a-palette.md) | `crates/hytale-world` | `Empty`/`Byte`/`HalfByte`/`Short` palette get/set + serialize/deserialize | **high** — isolated format work |

Do **not** start T11–T14 (golden asset/chunk dumps, client bot, full protocol
codegen) until T01–T03/T05 are merged; they need the generated protocol types or
the asset pipeline. The chunk-section backlog (T10b/T10c) is in
[`tasks/README.md`](tasks/README.md).

---

## Definition of done for session 1

- `cargo check --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo fmt --all -- --check` are green.
- All existing worldgen goldens still pass:
  `cargo test -p worldgen-hytale -p worldgen-model -p worldgen-server`.
- `tools/protocol-dumper inventory.json` reports **389 packets / 445 structs /
  177 enums** (or documents every intentional discrepancy).
- `crates/hytale-protocol` has passing tests for the wire primitives and framing.
- `crates/hytale-storage` reads and writes a synthetic `IndexedStorageFile` whose
  bytes match a Java-produced fixture.
- `tools/asset-golden-dumper/asset-stores.md` lists all 54 stores and flags the
  P0/P1 subset.
- No Java source has been modified. No generated Rust file is hand-edited;
  regeneration is a command, not a copy/paste.

---

## Working rules for this session

- **One task per worktree/branch.** Tasks that touch `Cargo.toml` (T01, and any
  task adding dependencies) must land first or coordinate explicitly.
- **Do not read the whole protocol tree into context.** T02/T03 scripts process
  files; each task reads at most one packet family or one Java class.
- **Golden-first.** A port is not done until it matches a Java-produced fixture
  or a Java unit-test assertion. Never invent expected bytes.
- **Generated code is write-only.** Edit the IR/generator, rerun it, commit both.
- **No new dependencies outside `[workspace.dependencies]`** without a note in
  the task/PR. Keep the dependency surface small.
- **No `unsafe`** in new code unless the task explicitly requires FFI and the
  safety argument is documented.
- **Do not touch `done/`** except to fix a broken link; it is the archive.

---

## Regression gate (unchanged for the worldgen crates)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Optional, needs the 3.7 GB asset pack:
tools/java-dumper/run.sh
tools/java-dumper/check.sh
```

The Java dumper remains the oracle. The CoreServer rewrite adds new fixtures
under `testdata/golden/server/`; it does not change the existing worldgen
fixtures or their generation rules.

---

## After session 1

Session 2 should be the first vertical slice:

1. T04 connection codegen complete; T06 QUIC handshake working.
2. T09 asset resolver plus the P0 `BlockType`/`Fluid`/`Environment` stores.
3. T10b `BlockSection` + T10c `ChunkColumn::from_generated_chunk`.
4. T11 asset-packet golden dumper and T12 chunk-packet golden fixtures.
5. First goal: a Rust binary that loads a world, generates one chunk, converts
   it to a runtime chunk, and writes one Java-readable region blob.

Update [`PORTING_HISTORY.md`](PORTING_HISTORY.md) at the end of each session with
what landed, what was verified, and what moved.
