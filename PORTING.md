# Porting conventions — CoreServer rewrite

Read this before writing any Rust for the server. The goal is **observable
parity** with the Java server, not a line-by-line translation. Worldgen has its
own archived conventions in [`done/PORTING.md`](done/PORTING.md); they still
apply to the existing crates.

---

## 1. Source roots

Java reference sources are read-only and live under:

```text
.hytale-source/hytale-shared-source/HytaleServer/
  CoreServer/src/main/java/com/hypixel/hytale/**
  AssetStore/src/main/java/com/hypixel/hytale/assetstore/**
  Codec/src/main/java/com/hypixel/hytale/codec/**
  Component/src/main/java/com/hypixel/hytale/component/**
  Common/src/main/java/com/hypixel/hytale/common/**
  Event/src/main/java/com/hypixel/hytale/event/**
  Logger/src/main/java/com/hypixel/hytale/logger/**
  Metrics/src/main/java/com/hypixel/hytale/metrics/**
  Registry/src/main/java/com/hypixel/hytale/registry/**
  Storage/src/main/java/com/hypixel/hytale/storage/**
  Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/**
  Protocol/src/main/java/com/hypixel/hytale/protocol/io/**
  WorldGenerator/src/main/java/com/hypixel/hytale/**
  builtin/**                 # post-v1
  NPC/**                     # post-v1
```

Asset schemas are under
`.hytale-source/hytale-shared-source/HytaleAssets/Schema/`.
Content is under `Assets/` (git-ignored, several GB).

Never copy Java files into the repository. Port behavior, cite the source path
in a `// Ported from:` comment.

---

## 2. Golden-first rule

A port is not done until it matches one of:

- a Java-produced golden fixture under `testdata/golden/`,
- a Java unit-test assertion,
- a packet trace captured from the Java server or a real client.

Do not invent expected bytes. If no oracle exists, write the Java-side dumper or
test first, then port. See [`GOLDEN_DATA.md`](GOLDEN_DATA.md).

---

## 3. Numeric and semantic fidelity

- Java `double` → `f64`, Java `float` → `f32`. Never change precision.
- Java `int`/`long` → `i32`/`i64` with wrapping arithmetic where Java overflows.
  Use `wrapping_add`/`wrapping_mul` in code that can overflow in debug builds.
- Java `>>>` → logical shift via `((x as u32) >> n) as i32`.
- Java `Math.*` → the same operation. If Java uses a custom approximation, port
  the approximation.
- Java `Random`/`FastRandom` → port the exact LCG; do not substitute `rand`.
- Integer division and `%` truncate toward zero in both languages; use
  `rem_euclid` only where Java uses `floorMod`.
- Java `char` is UTF-16; Hytale protocol strings are UTF-8 or ASCII with explicit
  bounds. Do not assume `len()` in bytes equals `len()` in Java chars.

For the server, “exact” also applies to:

- packet field order, offsets, nullability bits, and bounds;
- section palette byte layouts;
- BSON field names and value types;
- asset index assignment within the Rust pipeline;
- world tick ordering where observable by clients.

---

## 4. Protocol rules

- Generated files are **write-only**. Edit the IR or generator, rerun it, commit
  both. Do not hand-patch generated Rust.
- `protocol.json` is the contract. Any parser change must regenerate and pass the
  full golden suite.
- Keep `ProtocolSettings` (CRC, version, build) generated from the Java source.
- Add a round-trip test for every new packet family; add byte-level tests for
  packets used in the join path.
- Reject invalid frames early: negative/oversized length, unknown id, wrong
  direction/channel, oversized payload.
- Treat all client input as untrusted. Bound every string and array before
  allocation.

---

## 5. Asset rules

- Do not port the Java `Codec` library. Use the 120 JSON schemas, an intermediate
  JSON/BSON value, and serde types.
- Resolve `Parent` before typed decoding. Preserve per-store merge rules; a
  generic “deep merge” is wrong for stores that append/replace differently.
- Keep asset ids deterministic. Assign once, record the registry snapshot, use
  the same ids for worldgen, packets, and storage.
- Asset packet encoders must match Java byte-for-byte for the default pack.
- Validation mode must fail closed: unknown parent, cycle, missing required
  field, out-of-range index, duplicate key.
- Do not read the 3.7 GB pack in unit tests. Use tiny fixtures; reserve full-pack
  tests for the golden suite.

---

## 6. Storage rules

- `IndexedStorageFile` must match the Java header, blob index table, segment
  chain, and zstd payloads exactly.
- Chunk BSON uses the Java component names and `Data` byte arrays. Section
  storage writes block, fluid and environment palette keys as asset names, not
  runtime ids, and uses big-endian keys for fillers. The chunk holder codec lives
  in `hytale-world::chunk::storage`.
- The legacy heightmap backend is the storage-supported backend; `Tree` returns
  a documented `Unsupported` error until its fields are ported.
- Never silently migrate or rewrite a world on load. Read-only mode must not
  create files, locks, or indexes.
- Atomicity: write blob, update index, flush in the same order as Java. Test
  crash/recovery behavior.
- Java-written worlds are the compatibility gate. Do not declare storage done
  based only on Rust round-trips. The committed
  `testdata/golden/server/storage/java-world/` fixture is the current read gate;
  the mutate → Java-reload half is still open.

---

## 7. Chunk rules

- `worldgen-model::GeneratedChunk` is the generation buffer. Do not add storage
  or network concerns to it.
- Runtime `ChunkColumn`/sections own palettes, lighting, heightmaps, entities,
  and block components.
- `from_generated_chunk` is a one-way copy at the generation boundary.
- Section serialization is shared between network and storage; one implementation,
  two consumers.
- Palette selection must be deterministic and match Java's thresholds. A
  different palette type is a different byte stream even if the logical blocks
  match.

---

## 8. ECS and threading rules

- Use `bevy_ecs` for entities; use the custom chunk store for chunks.
- All world state is mutated on the world thread. Async tasks send messages;
  they do not touch `Store`/`World` directly.
- No blocking I/O on the world thread. Storage and generation complete
  asynchronously and are installed on the tick.
- Use `Arc` for immutable registries/assets; avoid global locks in hot paths.
- Do not promise Java `Holder`/`Ref`/`ComponentRegistry` API parity. Preserve
  behavior, expose Rust-native APIs.

---

## 9. Code style

- Rust 2021, `cargo fmt`, `cargo clippy --all-targets -- -D warnings`.
- `thiserror` for library errors; no `anyhow` in library crates.
- `tracing` for logs; no `println!` in library code.
- No `unsafe` unless the task explicitly requires FFI and documents the safety
  argument.
- Public APIs are deterministic and allocation-light in hot paths.
- Keep modules small and named after the Java class/package they replace where a
  direct mapping exists; cite the source path.
- New dependencies go in `[workspace.dependencies]` first.

---

## 10. Subagent task rules

Tasks are in [`tasks/`](tasks/README.md). To keep a task inside a 200k context
and safe to run in parallel:

- **One task, one branch/worktree.** Do not edit files owned by another task.
- **Read only the files listed in the task.** If the task needs a broad search,
  use `rg`/`find` and read the matches, not the whole tree.
- **Bounded outputs.** Each task names its files and acceptance tests. Do not
  refactor neighboring code.
- **No generated-code hand edits.** No editing `Cargo.lock` by hand.
- **Stop at the acceptance criteria.** Report follow-ups instead of expanding
  scope.
- **Run the gate** for the touched crate:
  `cargo test -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D warnings`,
  `cargo fmt -p <crate> -- --check` (or `--all` for workspace tasks).
- **Report** what changed, what was verified, and what is still open. Update
  [`PORTING_HISTORY.md`](PORTING_HISTORY.md) only for landed work.

Tasks marked **high** are suitable for fast/cheap subagents: isolated files,
clear inputs, small output. Tasks marked **medium** need a careful agent that can
read one Java class or a generated packet family. Tasks marked **low** should be
done by the main agent or a stronger model.

---

## 11. Definition of done

For every task:

1. Code compiles with zero warnings.
2. New behavior has a unit test or golden test.
3. No existing worldgen golden regressed.
4. Generated artifacts are reproducible (`protocol-gen --check` where relevant).
5. Public APIs are documented with `///`.
6. The task file’s acceptance section is satisfied.
7. `PORTING_HISTORY.md` is updated only when the work is merged.
