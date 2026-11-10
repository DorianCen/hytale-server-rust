# Parity audit program

**Status:** proposed, not started
**Scope:** Rust server/runtime crates vs the Java CoreServer/reference behavior
**Goal:** find real behavioral divergences and performance cliffs systematically,
without turning the audit into a rewrite.

## Short answer

Yes, audit the Rust rewrite against Java — but do it as a **behavior-driven,
risk-based parity program**, not a line-by-line review of every Rust file.

A literal “read everything and compare” pass is expensive and low-signal. Much
of the Java source is client, editor, builder-tool, machinima or voice code that
is intentionally out of scope. Internal structure also does not need to match:
what matters is that the same inputs produce the same observable outputs and the
same failure behavior.

The right unit of review is therefore a **behavior/contract**, not a file.

## What “parity” means here

For each behavior, compare the things a client, save file, trace or operator can
observe:

| Contract | Examples | Oracle |
|---|---|---|
| Wire bytes | packet payloads, packet order, channel, nullability | Java packet encoders, committed `.bin` goldens, real-client traces |
| Asset bytes | config-asset packet payloads, ids/indexes, field presence | `tools/java-dumper`, `testdata/golden/server/assets` |
| Save bytes | player/chunk/world BSON, region files, metadata | Java storage dumpers, round-trip probes |
| Gameplay state | blocks, inventories, stats/effects, NPCs, containers, recipes | scenario tests against `WorldThread` and Java reference dumps |
| Error behavior | invalid input rejection, disconnect reason, rate limits | fuzz/abuse tests, Java validation behavior |
| Timing/memory | boot, join, chunk streaming, save, ticks, soak | `tools/perf.sh`, `tools/java-baseline.sh`, criterion |

Internal algorithm choice, data layout and module boundaries are **not** part of
parity unless they change one of the above.

## Non-goals

- Porting or reviewing the Hytale client, editor, AssetEditor, builder tools,
  machinima or voice relay.
- Forcing Rust code to mirror Java class/method structure.
- Re-benchmarking every function.
- Treating a different but equivalent algorithm as a bug.
- Auditing generated protocol codec bodies by hand; they are write-only output.

## Existing infrastructure

Most of the machinery already exists:

- `tools/java-dumper` — deterministic Java goldens for assets, registry, chunks,
  fluids, storage, player data, NPC/entity gameplay, world config and traces.
- `testdata/golden/server/**` — committed asset/store/chunk/storage/trace
  fixtures.
- `testdata/golden/server/trace/` + `tools/client-bot` — packet order/channel
  contract replay.
- `tools/test.sh` tiers (`unit`, `fast`, `integration`, `assets`, `slow`,
  `e2e`).
- `tools/perf.sh` — release probe and soak with RSS ceilings.
- `tools/java-baseline.sh` — Java asset validation and offline boot timing/RSS.
- `tools/security.sh` — fuzz targets, dependency audit and abuse gates.
- `CONNECTION_DEBUG.md` — the real-client failure log and diagnostic recipes.

The missing piece is a **coverage matrix**, a consistent audit template and a
backlog of confirmed divergences.

## Audit model

Each row in the matrix is one behavior:

| Field | Meaning |
|---|---|
| ID | Stable id, e.g. `INV-042` |
| Behaviour | One sentence, input → output |
| Java reference | Class/method or Java dumper output |
| Rust reference | Crate/module/function |
| Observable | Packet bytes, state, save bytes, error, timing |
| Existing coverage | Unit/golden/trace/scenario/fuzz test or none |
| Status | `covered`, `unverified`, `divergence`, `intentional`, `n/a` |
| Severity | `S0`–`S3` |
| Notes / task | Finding link or follow-up task |

Status rules:

- **covered** — a test proves the observable contract.
- **unverified** — behavior exists but no oracle/test yet.
- **divergence** — confirmed difference; needs a failing test then a fix.
- **intentional** — documented architectural or scope difference.
- **n/a** — Java behavior is out of scope.

Severity:

- **S0** — data loss/corruption, crash, security or protocol break.
- **S1** — wrong gameplay state, visible desync, save incompatibility.
- **S2** — edge-case mismatch, missing validation, minor UI/feedback gap.
- **S3** — internal/performance/style only, no observable mismatch.

## Risk map

Audit in this order unless a live-client bug changes the priority.

| Area | Crates | Why it is risky | Existing evidence | First action |
|---|---|---|---|---|
| Protocol/asset encoding | `hytale-protocol`, `hytale-assets` | wire bytes and field presence break the client instantly | strongest golden coverage; T66 closed several null/root/group gaps | audit remaining sentinels, nullable fields, ordering and failure paths |
| Inventory/containers/crafting | `hytale-inventory`, server inventory paths | stateful, many transaction/merge/overflow edges | unit tests + fixtures | differential tests for stacks, swaps, drops, overflow, crafting refunds |
| Interactions/placement | `hytale-interactions`, `hytale-server` | forked/nested chains, roots, rotation, placement rules | real-client break/pickup/place now works | audit fork/parallel/failure paths and placement fidelity |
| World/chunk/player storage | `hytale-storage`, `hytale-world` | data loss, migrations, corrupt input | Java storage goldens and round-trips | corrupt/partial input and version/migration paths |
| Worldgen | `worldgen-hytale`, `worldgen-model`, `procedural` | determinism and seed/version drift | strong seed/chunk goldens | audit uncovered generators and benchmark cold boot |
| Combat/stats/effects/NPC | `hytale-entities`, `hytale-npc`, `hytale-server` | complex state machines, timing, persistence | entity/NPC goldens and scenarios | scenario parity for damage, effects, death, spawn, encounters |
| Movement/collision | `hytale-world`, `hytale-server` | correction floods and client/server divergence | real-client movement stable | observable parity for validation and correction behavior |
| Auth/net/security | `hytale-auth`, `hytale-net`, `hytale-server` | handshake failures, abuse, rate limits | live auth and fuzz gates | negative-path and concurrency audits |
| Commands/builtins | `hytale-commands`, `hytale-builtin` | feature completeness and permissions | core framework tests | completeness matrix, not byte parity |
| Server orchestration | `hytale-server` | lifecycle, sessions, world switching, saves | loopback/E2E traces | failure-path and shutdown/save audits |

## Phase plan

### Phase 0 — Scope and target version

- Pick the reference version to audit against. The repository contains older
  Java goldens/source while the live client is 0.7.7; document the decision in
  `JAVA_VERSION_DIFF.md`.
- Freeze the non-goals so the audit does not chase client/editor code.
- Create one doc per crate or a single matrix with per-crate sections.

### Phase 1 — Inventory (roughly 1–2 sessions)

- List every public behavior per crate from the Rust API and the Java reference.
- Fill the matrix with `covered` / `unverified` / `intentional` / `n/a`.
- Mark existing goldens/traces/tests against rows.
- Produce a ranked backlog of `unverified` + high-severity rows.

This phase is mostly reading and mapping; it should not change runtime code.

### Phase 2 — Differential gaps

For each high-priority `unverified` row:

- add a Java dumper fixture/field dump, or
- add a packet/save/trace golden, or
- add a deterministic scenario test against the Rust world thread, or
- add a property/fuzz test for invalid input.

Confirm the current behavior first. Only then fix divergences.

### Phase 3 — Module audits

Work top-down through the risk map. For each area:

1. Audit behavior rows.
2. Add missing oracles/tests.
3. Fix confirmed S0–S2 divergences.
4. Record intentional differences.
5. Update the matrix and docs.

### Phase 4 — Performance baselines

Only benchmark paths that are hot or risky. See the benchmark policy below.

### Phase 5 — Ongoing gate

- New features must add a matrix row and a test/golden.
- A release check reports the number of `unverified` high-severity rows.
- Re-run the parity matrix when the reference Java/client version changes.

## Pilot

Start with a focused pilot before committing to the full program:

1. `hytale-inventory` + container/crafting transactions.
2. `hytale-interactions` + placement/break behavior.
3. `hytale-world` storage/chunk round-trips.

These are central, stateful and have clear observable outputs. If the pilot
finds little, stop or narrow the program. If it finds real bugs, reuse the same
template for the remaining crates.

## Per-crate audit procedure

1. Read the Rust public API and the corresponding Java classes.
2. Enumerate behaviors, including error and boundary cases.
3. For each behavior, write the observable contract and the oracle.
4. Check whether an existing test/golden already proves it.
5. Add the smallest differential fixture or scenario test.
6. Run it; classify the result.
7. For a divergence: add a failing regression test, fix, re-run the wider tier.
8. For an intentional difference: record why, and what would break if changed.
9. For a hot path: add a benchmark and record a baseline.
10. Update the matrix and any task file.

## Matrix template

```markdown
| ID | Behaviour | Java reference | Rust reference | Observable | Coverage | Status | Sev | Notes |
|---|---|---|---|---|---|---|---|---|
| INV-001 | Add stack merges into an existing partial stack before empty slots | `SimpleItemContainer.addItemStack` | `hytale-inventory::container` | slot map after add | `inventory::adds_merge_partial_stacks...` | covered | — | — |
| INV-002 | Move into a full slot swaps only when compatible | `InventoryUtils.moveItemStack` | `hytale-inventory::player` | slot maps | none | unverified | S1 | candidate |
```

## Differential test recipes

- **Packet/asset parity:** decode the Rust payload and the Java `.bin` golden,
  compare fields (and byte-compare where ids are not Rust-owned).
- **Storage parity:** write with Rust, read with the Java probe (and reverse),
  compare semantically; serialize twice for determinism.
- **Scenario parity:** drive `WorldThread` with the same scripted inputs as a
  Java dumper scenario; compare block/inventory/stat/NPC state.
- **Trace parity:** replay a captured lifecycle and assert packet order,
  channel, direction and normalized payload hashes.
- **Invalid input:** property/fuzz tests with expected rejection, no panic and
  bounded resource use.
- **Nondeterminism:** normalize map order, timestamps, UUIDs and thread
  scheduling before comparison.

## Benchmark policy

Benchmarks must answer a question; do not benchmark everything.

- **Criterion** for pure, repeatable code: asset decode, chunk encode/decode,
  worldgen stages, interaction graph build, inventory operations, packet
  codecs.
- **`tools/perf.sh`** for server-level join/tick/save/chunk and soak behavior.
- **`tools/java-baseline.sh`** for comparable Java validation/boot timing and
  RSS.
- Record hardware, build profile, asset pack and commit with every baseline.
- Use the same workload and normalization rules for Java/Rust comparisons.
- Treat a benchmark as a regression gate only after it has a stable baseline.

## Findings and backlog

Use the matrix as the source of truth and create a task only for a confirmed or
high-risk `unverified` row. Each finding should include:

- behavior id and severity;
- reproduction/fixture path;
- Java and Rust references;
- observed vs expected;
- whether it is a bug, an intentional divergence, or missing coverage;
- acceptance test that proves the fix.

## Caveats

- **Version drift:** Java source/goldens and the live client may be different
  versions. Pick and record the audit target.
- **Intentional divergences:** single-player stored-player join, simplified
  placement/physics, unsupported editor features, etc. Classify them, don’t
  “fix” them accidentally.
- **Nondeterminism:** network timing, hash order and parallel worldgen need
  normalization.
- **Generated code:** protocol codecs are generated; audit inputs and goldens,
  not the generated bodies.
- **Performance is contextual:** a slower internal algorithm is fine if the
  observable latency/throughput is acceptable; measure end-to-end paths.

## Definition of done

- **Per behavior:** observable contract has an oracle and a passing test, or is
  documented as intentional/n/a with a reason.
- **Per crate:** no unclassified public behavior; high-severity `unverified`
  rows have tasks.
- **Program:** the matrix is current, the pilot and prioritized audits are
  complete, and new work adds rows/tests by default.

## First three sessions

1. Create the matrix skeleton for `hytale-inventory`, `hytale-interactions` and
   `hytale-storage`; fill `covered`/`unverified` from existing tests.
2. Add 3–5 differential tests for the highest-risk unverified rows in each.
3. Fix the first confirmed divergences, add regressions, and decide whether to
   continue with the remaining crates.
