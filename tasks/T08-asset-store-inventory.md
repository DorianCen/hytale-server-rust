# T08 — Asset store inventory

**Status:** done (session 1)
**Agent fit:** high (extract a table from one Java file + schema directory)
**Depends on:** T01
**Owns:** `tools/asset-golden-dumper/asset-stores.md`,
`tools/asset-golden-dumper/asset-stores.json`,
`tools/asset-golden-dumper/README.md`

## Goal

Produce the definitive list of asset stores the server must load: Java class,
asset path, key type, map type, packet generator, load dependencies, and the
P0–P3 priority from the plan. This is the work queue for M2 and the input to the
asset-golden dumper.

## Read

- `CoreServer/.../asset/AssetRegistryLoader.java` (54 registrations; this is the
  main source)
- `CoreServer/.../asset/HytaleAssetStore.java` (builder methods only)
- `HytaleAssets/Schema/` file names (120 schemas)
- [`plan.md`](../plan.md) §5.2

Do not read the asset classes themselves. Extract registration metadata.

## Work

1. Create `tools/asset-golden-dumper/` with a README that explains it will
   produce Java asset fixtures (the dumper itself is T11).
2. Produce `asset-stores.json` with one entry per store:

   ```json
   {
     "class": "BlockType",
     "path": "Item/Items",
     "key": "String",
     "map": "BlockTypeAssetMap",
     "packetGenerator": "BlockTypePacketGenerator",
     "loadsBefore": ["..."],
     "loadsAfter": ["..."],
     "preLoadAssets": ["Empty", "Unknown", "..."],
     "schema": "BlockType.json",
     "priority": "P0",
     "notes": "embedded in Item assets via ContainedAssetCodec"
   }
   ```

3. Produce `asset-stores.md` as a readable table grouped by priority:

   | Priority | Store | Path | Schema | Packet | Notes |
   |---|---|---|---|---|---|

4. Mark P0/P1/P2/P3 using the plan. Flag:
   - stores with no schema;
   - stores whose data lives inside another asset (e.g. `BlockType` inside
     `Item`);
   - stores that are required by worldgen but not by the client join;
   - stores whose packet generator is absent or shared.
5. Verify the count: exactly the number of `HytaleAssetStore.builder(...)` calls
   in `AssetRegistryLoader.java` (expected 54; 39 have a `setPacketGenerator`).
   If a registration is conditional, note it. The four
   `TagPattern.CODEC.register(...)` calls are codec variants, not stores.

## Out of scope

- No asset decoding, no packet bytes, no Rust crate.
- No full-pack boot.

## Acceptance

- `jq length asset-stores.json` equals the registration count in the Java file.
- Every entry has class, path, map, packet (or `none`), schema (or `none`), and
  priority.
- `asset-stores.md` is generated from the JSON (or checked against it by a
  small script) so they cannot drift.
- No Java changes.

## Handoff

Report counts by priority, missing schemas, and the P0 list for session 2.
