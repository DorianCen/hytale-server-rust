# Storage fixtures

## Container sample

`java-sample.region.bin` was produced by the real Java
`com.hypixel.hytale.storage.IndexedStorageFile` using the module `target/classes`
trees and Maven jars, so it exercises the actual container implementation, not a
reimplementation.

Shape:

- magic `HytaleIndexedStorage`, version 1, 16 blobs, 64-byte segments;
- **big-endian** header, index table, and blob headers (Java `ByteBuffer`
  default);
- blob 0: empty; blob 1: `"hello java indexed storage"`; blob 5: 1,000 bytes
  from a fixed LCG seed.

Regenerate with a JDK plus the compiled Hytale modules:

```java
IndexedStorageFile.open(path, 16, 64,
    StandardOpenOption.CREATE_NEW, StandardOpenOption.READ, StandardOpenOption.WRITE);
file.setCompressionLevel(3);
file.writeBlob(0, ByteBuffer.allocate(0));
file.writeBlob(1, ByteBuffer.wrap("hello java indexed storage".getBytes(UTF_8)));
// blob 5: state = state * 6364136223846793005 + 1442695040888963407; byte = state >>> 33
file.force(false);
```

`crates/hytale-storage/tests/indexed_storage.rs` reads this fixture and checks
the decoded blobs. Do not edit the `.bin` by hand.

## Java world save

`java-world/` is a real Java world save produced by `StorageDumper`:

- seed 0 chunks `(0,0)` and `(4,-5)`;
- `chunks/0.0.region.bin` and `chunks/0.-1.region.bin` are
  `IndexedStorageFile`s with the default 1024 blobs and 4096-byte segments;
- blob index `ChunkUtil.indexColumn(x & 31, z & 31)`;
- each blob is `ChunkStore.REGISTRY.serialize(holder)` written with
  `BsonUtil.writeToBytes`.

The BSON root is `{"Components": ...}` with `WorldChunk`, `BlockChunk`,
`ChunkColumn` (`Sections` array with `Block`/`Fluid`/`Environment` components)
and `HeightmapColumn` (legacy backend). `BlockChunk.Data` is `needsPhysics`
followed by the tint palette. Block and fluid palettes store asset names;
environment storage stores environment names. See
`crates/hytale-world/src/chunk/storage.rs` for the codec.

Regenerate and verify determinism:

```sh
tools/java-dumper/run-storage.sh
tools/java-dumper/run-storage.sh --check
```

`crates/hytale-world/tests/chunk_storage.rs` decodes both chunks with the golden
registry and compares every block, filler, rotation, fluid level, environment,
the tint and the legacy heightmap with the same Rust-generated chunk. It also
round-trips a runtime column through the BSON codec and an on-disk region file.
