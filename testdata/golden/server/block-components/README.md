# Block-component holder golden

`hunt-site.json` is the real Java `ChunkStore.REGISTRY.serialize` output for the
`Hunt_Trap` block-component holder in
`Assets/extracted/Server/Prefabs/Hunt/Generic/Site.prefab.json`.

The prefab deserializer used by the chunk goldens deliberately skips block
component holders. `tools/java-dumper/run-block-components.sh` compiles a small
dumper against the installed release `HytaleServer.jar`, registers the real
`BlockMapMarker` component codec and deserializes/reserializes the prefab
`components` document through the canonical registry.

Regenerate:

```sh
tools/java-dumper/run-block-components.sh
```

`HYTALE_SERVER_JAR` can point at a different release jar. The Rust consumer is
`crates/hytale-server/tests/prefab.rs`; it asserts the persisted opaque
`BlockHolder` document equals this golden verbatim.
