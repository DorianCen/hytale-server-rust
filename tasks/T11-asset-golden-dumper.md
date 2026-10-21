# T11 — Java asset-packet golden dumper

**Status:** todo
**Agent fit:** medium (Java, one boot harness; needs the asset pack)
**Depends on:** T08
**Owns:** `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/AssetDumper.java`,
`testdata/golden/server/assets/`,
`tools/asset-golden-dumper/README.md`

## Goal

Produce the first server-side golden data: for the P0 asset stores, the exact
`generateInitPacket` bytes and the decoded asset index map from the real Java
asset pipeline.

## Read

- `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/RegistryDumper.java`
  (asset-pipeline boot pattern)
- `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/GoldenDumper.java`
- `CoreServer/.../asset/AssetRegistryLoader.java`
- `CoreServer/.../asset/HytaleAssetStore.java` (`sendAssets`,
  `packetGenerator.generateInitPacket`)
- `CoreServer/.../asset/packet/AssetPacketGenerator.java`
- `Protocol/.../io/PacketIO.java` (`serializeInto` / `computeSize` usage)
- `tools/asset-golden-dumper/asset-stores.json` from T08

## Work

1. Add an `AssetDumper` entry point to `tools/java-dumper` (main class
   `com.hypixel.hytale.tools.golden.AssetDumper`).
2. Boot the real asset pipeline exactly like `RegistryDumper`:
   - mount `Assets/Assets.zip` as a zip filesystem;
   - run `AssetRegistryLoader` with `common.parallelism=1` by default;
   - fail if any P0 store fails to load.
3. For each P0 store (from T08):
   - call `generateInitPacket(assetMap, assets)`;
   - serialize the packet payload with the protocol `serialize` path (no
     compression) to `testdata/golden/server/assets/<Store>.bin`;
   - write `index.json`: keys, numeric ids, counts, packet id, packet size,
     source pack.
4. Make output deterministic: sorted keys where order is not semantically
   meaningful, single-threaded decode, no timestamps/paths.
5. Add a `--check` mode that regenerates to a temp dir and diffs.
6. Document the exact command and prerequisites in the README.

## Out of scope

- No P1/P2/P3 stores yet.
- No Rust decoder.
- No modifications to `.hytale-source/`.

## Acceptance

```sh
ASSET_PACK=Assets/Assets.zip tools/java-dumper/run-assets.sh /tmp/assets-golden
# or the chosen entry point documented in the README
```

- P0 stores dump without errors.
- Two runs are byte-identical.
- `index.json` has the expected counts and ids.
- The files are small enough to commit (report sizes; if not, commit index +
  a manifest and keep the blobs in a generated artifact).

## Handoff

Report store counts, sizes, determinism result, and the exact command.
