# Builtin module manifest goldens

`manifests.json` is the selected T60 module set copied verbatim (field names and
values) from the reference server manifests:

- `HytaleServer/builtin/{BlockTick,RandomTick,FallingBlocks,BlockPhysics,Fluid,Weather,EncounterManager}/src/main/resources/manifest.json`
- `HytaleServer/NPC/src/main/resources/manifest.json`

The entries are sorted by `Group:Name`. `crates/hytale-builtin/tests/parity.rs`
compares every compiled manifest against this file, so a manifest drift fails
the fast gate without the Java source tree or asset pack.

To re-baseline after a reference update, copy the eight manifests into this
array, keep the `Group:Name` sort, and rerun:

```sh
cargo test -p hytale-builtin
cargo test -p hytale-builtin --no-default-features
```
