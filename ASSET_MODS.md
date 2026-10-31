# Asset-only mods

Asset mods are data overlays: JSON assets, models, textures, prefabs and other
files under `Common/` or `Server/`. They are separate from Rust code plugins
(see [`PLUGINS.md`](PLUGINS.md)). The v1 server loads **one asset pack** — a
`.zip` or an extracted directory. Runtime overlay stacking is not implemented
yet; merge the mod into a pack before starting the server.

## Supported pack shapes

The server accepts either:

- a `.zip` containing `Common/`, `Server/` and optional `Schema/` trees; or
- an extracted directory with the same layout (`Assets/extracted` is the
  vanilla example).

`--assets-pack` selects the pack for asset decoding. `--assets-root` selects
the extracted tree used for prefabs, environments and tint gradients. For a
directory deployment both may point at the same directory.

The directory-pack path is covered by the asset-backed
`an_extracted_asset_pack_can_boot_a_world` regression test.

## Workflow: merge a mod into a directory pack

This is the recommended v1 workflow. It keeps the vanilla pack untouched and
makes the merge inspectable.

```sh
# 1. Create a writable copy of the vanilla extracted tree.
mkdir -p /srv/hytale/assets-modded
cp -a Assets/extracted/. /srv/hytale/assets-modded/

# 2. Overlay the mod's asset roots. Mod files replace same-path files.
cp -a MyMod/Common/. /srv/hytale/assets-modded/Common/
cp -a MyMod/Server/. /srv/hytale/assets-modded/Server/

# 3. Validate the merged pack before serving.
hytale-server --validate-assets --pack /srv/hytale/assets-modded

# 4. Run the server against the merged tree.
hytale-server serve \
  --universe /srv/hytale/universe \
  --world default \
  --assets-pack /srv/hytale/assets-modded \
  --assets-root /srv/hytale/assets-modded
```

If you must deploy a zip, build it after the merge:

```sh
(cd /srv/hytale/assets-modded && zip -q -r ../Assets-modded.zip .)
hytale-server --validate-assets --pack /srv/hytale/assets-modded/../Assets-modded.zip
```

## Mod file rules

- A mod may add a new asset key or replace an existing JSON file by path.
- `Parent` inheritance still applies after the merge; the parent must exist in
  the merged pack.
- Store ids are assigned deterministically by key after the merge. Do not hard
  code numeric ids from a different pack in gameplay data.
- References (`Model`, `Texture`, `BlockType`, tags and child-store ids) must
  resolve in the merged pack. `--validate-assets` checks the config burst and
  tag references; it does not prove every model/texture exists.
- Paths must stay inside the pack. Absolute paths, `..` components and drive
  prefixes are rejected by the VFS.
- A `manifest.json` in a mod is not interpreted by the Rust server's asset
  pipeline. It is ignored unless a future mod loader defines its semantics.
  Ship the files under `Common/`/`Server/` and merge by path.
- Code, Java classes, native libraries and plugin jars are not asset mods and
  are not loaded.

## Limitations

- No runtime mod stacking or load-order manifest yet; one merged pack is loaded.
- No hot reload/file watcher; restart to apply asset changes.
- The Rust port implements the stores listed by `--validate-assets`; an asset
  type that Java validates but Rust does not encode may still be ignored.
- Asset mods cannot add new Rust systems (NPC AI, commands, packets). Those
  require the code-plugin path or a server change.

## Troubleshooting

Run `--validate-assets` first. It reports:

- store counts and id ranges;
- config packet order and payload sizes;
- duplicate config packet ids;
- unresolved tag references;
- non-deterministic config encoding.

Malformed JSON, missing parents or unknown references fail validation instead of
being silently ignored. Keep the merged pack and the base pack revision
together in deployment notes so the mod can be re-applied after an asset update.
