# T12 — Java chunk-packet goldens

**Status:** todo
**Agent fit:** medium (Java; one world boot and packet serialization)
**Depends on:** T10b (format understanding), existing chunk goldens
**Owns:** `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/ChunkPacketDumper.java`,
`testdata/golden/server/chunks-pkts/`

## Goal

Capture the exact network payloads Java sends for a handful of chunks, so the
Rust runtime chunk and packet encoder can be byte-compared. This is the network
counterpart to `chunks.json`.

## Read

- `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/ChunkDumper.java`
  (world boot)
- `CoreServer/.../world/chunk/BlockChunk.java` (`getCachedColumnPacket`,
  `getCachedTintsPacket`)
- `CoreServer/.../world/chunk/section/BlockSection.java` (`serializeForPacket`)
- `CoreServer/.../world/chunk/section/FluidSection.java`
- `CoreServer/.../world/chunk/section/EnvironmentSection.java`
- `Protocol/.../io/PacketIO.java` (serialize without compression)
- `testdata/golden/chunks.json` (source chunk coordinates)

## Work

1. Add a `ChunkPacketDumper` entry point.
2. Reuse the real world boot from `ChunkDumper` for seed `0` and chunks `(0,0)`
   and `(4,-5)` (add more only if cheap).
3. After the chunk is fully generated/loaded, emit:
   - `SetChunk` payload per section (x, y, z, data);
   - `SetColumn` payload;
   - `SetChunkTintmap` payload;
   - `SetChunkEnvironments` payload (if sent at init);
   - `SetFluids` payload (if sent at init).

   Use the packet object’s `computeSize`/`serialize` path, not a hand-rolled
   encoder.
4. Write a manifest:

   ```json
   {
     "seed": 0,
     "chunk": [0, 0],
     "packets": [
       { "name": "SetChunk", "id": 131, "file": "0_0/SetChunk_0_0_0.bin", "size": 1234 }
     ]
   }
   ```

5. Make it deterministic: no timestamps, stable chunk order, no compressed
   payloads (store uncompressed to make byte diffs readable).
6. Add a check mode that regenerates and diffs.

## Out of scope

- No Rust tests yet (follow-up).
- No client join trace.
- No compressed packet variants.

## Acceptance

- Two runs are byte-identical.
- Every blob has a manifest entry with packet id and size.
- Sizes are reasonable to commit; if not, commit the manifest and a small
  subset, and document the full-run command.

## Handoff

Report chunk coverage, total size, and the Rust test that should consume it.
