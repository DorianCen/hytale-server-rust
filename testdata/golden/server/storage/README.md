# Storage fixtures

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
