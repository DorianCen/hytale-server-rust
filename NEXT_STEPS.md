# Next steps — CoreServer rewrite, session 14

**Status:** sessions 1–13 landed. The wire layer (connection codegen, full
protocol codegen, registry, `ProtocolSettings`), QUIC mTLS transport, runtime
chunk sections, client-bot skeleton, P0 asset-packet goldens, Java chunk-packet
goldens, the runtime chunk packet encoders, the P0 asset registry and
Java-compatible BSON chunk storage are in place. Session 7 added the first
headless boot: it loads the asset pack, hands the generator the server's
in-memory registry ids, generates one chunk through `worldgen-server`, converts
it to a runtime `ChunkColumn`, saves a region blob and verifies the reload.
Session 8 added runtime world storage — load a saved chunk into a `World`, edit
a block, track dirty chunks and save back — plus the reverse Java reload gate:
the real Java codecs read the Rust-written region. Session 9 added the per-world
`config.json` codec (`WorldConfig` v4), preserving unknown fields, with a
Java-written fixture and a Java read-back probe. Session 10 added player saves
(`DiskPlayerStorageProvider`): the `EntityStore` holder document with typed
`UUID`/`Transform`/`WorldGenId`, opaque preservation of every other component,
and a Java read-back gate. Session 11 composed the three saved-world parts into
`WorldSave` + `hytale-server world-boot`, and added a whole-save Java gate
(`run-save-check.sh`). Session 12 added demand loading: `WorldSave::ensure_chunks`
loads stored chunks and generates missing ones around the player, with the shared
`WorldGenerator` boot now used by both `headless` and `world-boot`. Session 13
moved demand onto a dedicated `WorldThread`: a view-distance plan orders chunks
nearest-first, a view change cancels queued loads and unloads chunks that left,
and `Tick` emits the chunk events the network stream will consume. See
[`PORTING_HISTORY.md`](PORTING_HISTORY.md) and the status board in
[`tasks/README.md`](tasks/README.md).

**Try it:**

```sh
cargo run -p hytale-server -- headless --save target/headless-world --chunk 4,-5
cargo run -p hytale-server -- load-chunk --save target/headless-world --chunk 4,-5 \
  --set-block 129,250,-159,4
tools/java-dumper/run-reload-check.sh target/headless-world 4 -5 129 250 -159 Debug_Test_Block
cargo run -p hytale-server -- world-config \
  --input testdata/golden/server/world-config/config.json --save target/headless-world
tools/java-dumper/run-world-config.sh --probe target/headless-world/config.json
cargo run -p hytale-server -- player-data \
  --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
  --save target/headless-world
tools/java-dumper/run-player-data.sh --probe \
  target/headless-world/players/12345678-1234-5678-9abc-def012345678.json
cargo run -p hytale-server -- world-boot --save target/headless-world \
  --view-radius 1 --chunks-per-tick 4 --set-block 129,250,-159,4
tools/java-dumper/run-save-check.sh target/headless-world 4 -5 129 250 -159 Debug_Test_Block
```

**Next up:** finish the first vertical slice.

1. ~~P0 asset registry and typed stores (`BlockType`, `Fluid`, `Environment`,
   `Item`, …) with deterministic id assignment.~~ Done in session 5. P1 stores
   and the asset packet encoders are the remaining asset work; `BlockType`
   packet bytes still need the Java contained-asset load order.
2. ~~Runtime world storage: BSON chunk codecs on top of `IndexedStorageFile`,
   validated by a Java-written world fixture.~~ Chunks done in session 6;
   region → runtime `World` load/mutate/save and the Java reload gate done in
   session 8; `config.json`/`WorldConfig` done in session 9; player BSON
   (`DiskPlayerStorageProvider`) done in session 10; config + player + chunk boot
   (`WorldSave`/`world-boot`) and the whole-save Java gate done in session 11;
   missing-chunk generation and player-radius demand loading done in session 12;
   the world thread, view tracking, queued-cancellation and unload events done
   in session 13. Remaining: stream chunk events to a client on join.
3. ~~Consume `protocol.json` in `hytale-world` chunk packet encoders and compare
   against `chunks-pkts/**/*.bin` byte-for-byte.~~ Done in session 4. The
   `hytale-assets` packet encoders still need their `assets/*.bin` comparison.
4. ~~Wire `hytale-world` + `worldgen-server` into a headless boot that loads a
   world, generates one chunk, converts it to a runtime chunk, and saves a
   region blob.~~ Done in session 7. The boot loads the asset pack once, uses
   its registry ids for generation/storage and verifies the decoded save.
5. Extend `client-bot` from the connection handshake into auth/setup/game with
   the packet-trace fixture.
6. ~~Load saved chunks back into a world runtime (region → `ChunkColumn`) and
   add the mutate → Rust save → Java reload compatibility gate.~~ Done in
   session 8 (`World`/`WorldStorage`, `load-chunk`, `WorldStorageProbe`).

The completed worldgen port is archived under [`done/`](done/) and its
regression gate must stay green. The session-1 sections below remain as the
record of what was planned.

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
  bytes match a Java-produced fixture, and `hytale-world::chunk::storage`
  decodes the Java world-save fixture into the generated column.
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
