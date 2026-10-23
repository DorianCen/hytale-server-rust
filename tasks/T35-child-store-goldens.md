# T35 — Child-store golden dumper extension

**Status:** done (session 25)
**Package:** `tools/asset-golden-dumper`, `tools/java-dumper`, `testdata/golden/server/assets`
**Depends on:** T34
**Agent fit:** medium

## Goal

Extend the Java asset oracle from P0 to the P1 stores and the P2 dependency
closure that feeds their indexes. Everything after this task must be
golden-first; do not hand-write child-store payload expectations.

## Deliverables

- Replace `AssetDumper`’s single `PRIORITY = "P0"` selector with a deterministic
  store selector (for example `--priority P0,P1` and/or an explicit `--stores`
  list), preserving the current default `P0` output.
- Add dependency-closure selection driven by the `loadsBefore`/`loadsAfter`
  edges in `tools/asset-golden-dumper/asset-stores.json`.
- Dump, with `index.json` metadata and `<Store>.bin` where a packet generator
  exists:
  - **P1:** `BlockBreakingDecal`, `BlockParticleSet`, `BlockSet`,
    `BlockSoundSet`, `ItemCategory`, `ItemQuality`.
  - **P2 closure:** `FluidFX`, `ItemSoundSet`, `SoundEvent`, `TagPattern`,
    `ParticleSystem`, `ParticleSpawner`, `ItemReticleConfig`,
    `ItemPlayerAnimations`.
  - Any additional decode dependency in that closure (for example audio
    categories/sets). Stores without a packet generator still get keys, ids and
    counts in `index.json`.
- Extend `tools/java-dumper/run-assets.sh` with the mode/arguments needed to
  write and check the new set; `--check` must still compare the committed tree.
- Keep the map canonicalization and single-worker determinism rules.
- Update `tools/asset-golden-dumper/README.md`,
  `tools/java-dumper/README.md` and
  `testdata/golden/server/assets/README.md` with the new selection and store
  list.
- Record any store that still fails to decode (`loadFailures`) instead of
  inventing payloads; document known gaps.

## Read first

- `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/AssetDumper.java`
- `tools/asset-golden-dumper/asset-stores.json` and `asset-stores.md`
- `testdata/golden/server/assets/README.md`
- The Java packet generators named in the inventory.

## Acceptance

```sh
tools/java-dumper/run-assets.sh --check
```

- Two isolated dumps are byte-identical.
- Fresh output matches the committed tree.
- Existing P0 `.bin` files and P0 `index.json` entries are unchanged.
- Every new `.bin` decodes with the generated Rust codec.
- `index.json` entries have the right counts, ids, packet ids/sizes and hashes.

## Deviations

- The default set is 26 stores (21 payloads, ~12.6 MiB) rather than only the
  literal P1 list; the extra roots and their closure are required by T36–T38.
- `BlockSet` has a packet generator but needs `BlockSetModule`, which the
  asset-only boot does not install. Its index entry is complete and carries
  `packetError` instead of a `.bin`.
- `GameplayConfig` has no loaded assets in the canonical boot (`Default` fails
  validation on a missing camera effect); it is recorded with count 0.
- The phase-2 regeneration re-baselined the `BlockType`/`Environment`/`Fluid`/
  `Item` payload hashes. Sizes are unchanged and the Rust semantic gates pass;
  the committed bytes are the canonical single-worker output.
- Stores that cannot boot in the canonical dumper (missing plugin codec
  variants, prefab/AssetModule gaps) are recorded, not faked.
- The dumper remains single-threaded for the committed data.
