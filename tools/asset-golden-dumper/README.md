# Asset golden dumper

This directory owns the Java-side asset oracle for the CoreServer rewrite.

## Current contents (T08)

- `extract_stores.py` — reads `AssetRegistryLoader.java` and writes the two
  inventory files.
- `asset-stores.json` — one entry per `HytaleAssetStore.builder(...)` call:
  class, asset path, key type, map type, codec, packet generator, load edges,
  preloaded assets, schema, priority, extension, and notes.
- `asset-stores.md` — the same inventory grouped by P0–P3 for review.

Regenerate both files with:

```sh
python3 tools/asset-golden-dumper/extract_stores.py
```

The script is deterministic: it walks the registrations in source order and
sorts the output by priority then class name. It never modifies
`.hytale-source/`.

## Planned dumper (T11)

T11 adds the Java program that boots the real `AssetRegistryLoader` for the
default pack and writes:

```text
testdata/golden/server/assets/
  index.json            per-store keys, ids, counts, source paths
  <store>.bin           Java generateInitPacket() bytes
  <store>.json          decoded assets for semantic diffs
```

Requirements for the dumper:

- Pin asset pack paths, load order, thread count, and id assignment so the
  output is byte-stable.
- Write one packet file per store with a packet generator (39 of the 54
  registrations at the time of T08).
- Record the protocol CRC/version used to encode the assets.
- Include the ignored-field manifest if any value depends on scheduling.

Full-pack runs are not part of the fast `cargo test` tier. The committed
inventory above is enough for store ordering, schema coverage, and the M2 work
queue.
