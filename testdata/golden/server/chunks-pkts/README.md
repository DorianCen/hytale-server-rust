# Java chunk-packet golden data

Reference network payloads for two real Default-world chunks, produced by
`com.hypixel.hytale.tools.golden.ChunkPacketDumper`. This is the network
counterpart of `../../chunks.json`. Regenerate, never hand-edit.

The dumper boots the same real world generator as `ChunkDumper` (seed `0`,
`Assets/extracted/Server/World/Default`, `SeedString("ChunkGenerator")`, the
full `ChunkGeneratorJsonLoader.load()` and `ChunkGeneratorExecution.execute`),
converts the generated chunk into the server's `BlockChunk` / section model,
and calls the same packet builders CoreServer uses. Every payload is produced
by the packet object itself:

```java
byte[] payload = new byte[packet.computeSize()];
packet.serialize(MemorySegment.ofArray(payload), 0);
```

There is **no framing** (no packet id/length header) and **no zstd
compression**. The manifest carries the packet id, size and file name.

## Regenerate

Prerequisites: JDK 25, Maven 3.10+, the read-only Java source tree under
`.hytale-source/` (compiled once; `run.sh` rebuilds stale class trees), and the
base-game pack at `Assets/Assets.zip` (3.7 GB, not part of the source tree).

```sh
# write testdata/golden/server/chunks-pkts/
tools/java-dumper/run-chunks.sh

# write an arbitrary directory
tools/java-dumper/run-chunks.sh /tmp/chunks-golden

# determinism gate: two isolated runs + diff against this directory
tools/java-dumper/run-chunks.sh --check
```

The script forces `DUMPER_PARALLELISM=1`; the seed (`0`) and the chunk list
(`(0,0)`, `(4,-5)`) are fixed in the Java dumper.

## Coverage

| Chunk | Contents |
|---|---|
| `(0,0)` | `SetColumn`, `SetChunkTintmap`, 10 × `SetChunk`, 10 × `SetChunkEnvironments`, 1 × `SetFluids` |
| `(4,-5)` | `SetColumn`, `SetChunkTintmap`, 10 × `SetChunk`, 10 × `SetChunkEnvironments`, 1 × `SetFluids` |

`SetChunk` is emitted for all ten sections (32 blocks each, section Y `0..9`).
All-air sections serialize with `data = null`, exactly like CoreServer's
`BlockSection.getCachedChunkPacket`. `SetFluids` appears only for sections
whose `FluidSection` is non-empty (section Y `3` in both chunks here).

Total: **46 packets, 569,520 payload bytes** (manifest 13,619 B; about 704 KiB on
disk including filesystem block overhead).

## Files

```text
manifest.json
0_0/SetColumn_0_0.bin
0_0/SetChunkTintmap_0_0.bin
0_0/SetChunk_0_<sectionY>_0.bin
0_0/SetChunkEnvironments_0_<sectionY>_0.bin
0_0/SetFluids_0_<sectionY>_0.bin
4_-5/...                         (same names with chunk X=4, Z=-5)
```

Negative coordinates are kept verbatim in the directory and file names
(`4_-5`).

## manifest.json schema

```text
seed                         0
meta
  producer                   ChunkPacketDumper
  source                     seed 0 chunks (0,0) and (4,-5); same boot as ChunkDumper
  world                      Server/World/Default inside Assets/extracted
  loaderSeed                 "ChunkGenerator"
  packetOrder                per chunk: SetColumn, SetChunkTintmap, then per section
                             (ascending Y): SetChunk, SetChunkEnvironments, SetFluids
  payload                    computeSize + serialize, uncompressed, unframed
  sentAtInit                 which packet types belong to the initial load set
  deterministic              fixed seed/chunk list, single worker, no timestamps
chunks[]                     (fixed order: (0,0), (4,-5))
  chunk                      [x, z]
  packets[]                  (stable order described above)
    name                     SetChunk | SetColumn | SetChunkTintmap |
                             SetChunkEnvironments | SetFluids
    id                       packet id (131, 138, 133, 134, 136)
    file                     relative path to the .bin
    size                     payload byte length
    sentAtInit               true for the initial chunk/section load packet set
    section                  [sectionX, sectionY, sectionZ] (section packets only)
packetsNotSentAtInit[]       packet types captured from the chunk state but not
                             part of CoreServer's initial load query set, with reasons
```

## `sentAtInit` and the packet builders

The initial chunk/section load query systems registered by `LegacyModule` are:

| Packet | Builder | Sent at init? |
|---|---|---|
| `SetColumn` (138) | `BlockChunk.getCachedColumnPacket()` | yes |
| `SetChunk` (131) | `BlockSection.getCachedChunkPacket(x, y, z)` | yes (per section) |
| `SetChunkEnvironments` (134) | `EnvironmentSection.getCachedPacket(x, y, z)` | yes (per section) |
| `SetChunkTintmap` (133) | `BlockChunk.getCachedTintsPacket()` | no — `WorldNotificationHandler.updateChunkTints` |
| `SetFluids` (136) | `FluidSection.getCachedPacket()` | no — `ChunkSystems.ReplicateChanges` |

`SetChunkTintmap` and `SetFluids` are therefore still real Java payloads for
the same chunk state, but their manifest entries say `sentAtInit: false` and
`packetsNotSentAtInit` records why they are not part of the first send. A Rust
join-parity test should compare only `sentAtInit == true` entries; an encoder
test can also round-trip the other two.

`SetColumn` embeds `HeightmapColumn.serializeToInts()` and the column tint;
`SetChunkTintmap` embeds the same tint bytes. The dumper selects the legacy
heightmap backend (`IHeightmap.forLoader(null)`) and populates it with
`HeightmapColumn.onSectionResident(..., newlyGenerated=true)`, matching the
server's section-install path.

## Determinism

`tools/java-dumper/run-chunks.sh --check` runs the dumper twice into isolated
temporary directories, diffs the two trees, then diffs the result against this
directory. The fixture is byte-identical across runs. Sources of variation are
pinned: the asset pipeline runs with one common fork-join worker, generation is
single-threaded and seeded by `(seed, chunk)`, packet section order is explicit,
and the manifest contains no timestamps or absolute paths.

## Suggested Rust test

`worldgen-hytale/tests/chunks_packets_golden.rs` (follow-up): regenerate seed 0
chunks `(0,0)` and `(4,-5)` through the Rust chunk pipeline, build the same
packets, and compare each `computeSize`/encode result byte-for-byte with the
`.bin` files selected by the manifest. For the initial-join check, filter on
`sentAtInit == true`; for encoder coverage, also check `SetChunkTintmap` and
`SetFluids` (setting the fluid section coordinates to the manifest `section`).
